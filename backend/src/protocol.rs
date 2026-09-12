use crate::{
    Config, Engine, Position, Scope,
    cells::{CellRegion, Coordinate, NearbyCell},
    gnss::GnssConfig,
    motion::Motion,
    operators::Operators,
    record::Recording,
    route::{Playback, Route, RouteState},
    telephony::{TelephonyConfig, TelephonyFrame, DetectedSubscription, validate_detected},
    wifi::WifiConfig,
};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const VERSION: u32 = 1;
pub const MAX_FRAME: u64 = 65_536;

#[derive(Deserialize)]
struct Request {
    version: u32,
    #[serde(flatten)]
    command: Command,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn route_start(points: &[(f64, f64)], speed: f64) -> String {
        json!({"version":1,"op":"start_route","scope":{"mode":"apps","packages":["example.selected"]},
            "route":{"points":points.iter().map(|(lat, lon)| Position::new(*lat, *lon)).collect::<Vec<_>>(), "speed":speed}}).to_string()
    }

    fn static_start() -> String {
        json!({"version":1,"op":"start","config":{"position":Position::new(31.0, 121.0),
            "scope":{"mode":"apps","packages":["example.selected"]}}}).to_string()
    }

    fn record_point(latitude: f64, longitude: f64, seconds: f64) -> String {
        json!({"version":1,"op":"record_point","position":Position::new(latitude, longitude),"seconds":seconds}).to_string()
    }

    /// 便捷包装：`Control::handle` 收 `&str`，测试里直接传引用更好读。
    fn send(control: &mut Control, line: String) -> Response {
        control.handle(&line)
    }

    #[test]
    fn recording_collects_points_and_hands_them_over_for_replay() {
        let mut control = Control::default();
        assert!(control.handle(r#"{"version":1,"op":"record_start"}"#).ok);
        // 静止不动的重复采样被丢弃，但仍然计入 skipped。
        assert!(send(&mut control, record_point(31.0, 121.0, 0.0)).ok);
        assert!(send(&mut control, record_point(31.0, 121.0, 0.5)).ok);
        assert!(send(&mut control, record_point(31.0, 121.0, 1.0)).ok);
        assert!(send(&mut control, record_point(31.0005, 121.0, 1.5)).ok);

        let progress = control.handle(r#"{"version":1,"op":"status"}"#).state.recording.unwrap();
        assert_eq!(progress.points, 2);
        assert_eq!(progress.skipped, 2);
        assert!(!progress.full);
        assert!((progress.seconds - 1.5).abs() < 1e-9);
        // 录制不产生输出：会话仍然是停止的。
        assert!(!control.handle(r#"{"version":1,"op":"status"}"#).state.requested_active);

        let response = control.handle(r#"{"version":1,"op":"record_stop"}"#);
        assert!(response.ok);
        let track = response.state.recorded.expect("the recording must be handed back to the panel");
        assert_eq!(track.points.len(), 2);
        assert!(response.state.recording.is_none());
        // 交付一次就取走：重复取要被拒绝，避免界面反复弹同一条成品。
        let taken = control.handle(r#"{"version":1,"op":"record_take"}"#);
        assert!(taken.ok);
        assert!(taken.state.recorded.is_none(), "the same take must not appear again after it is taken");
        let again = control.handle(r#"{"version":1,"op":"record_take"}"#);
        assert!(!again.ok);
        assert!(again.error.unwrap().contains("no recorded route"));
        // 录下来的点可以直接当路线回放。
        let route = Route { points: track.points, speed: 5.0, repeat_count: 1, repeat_delay: 0.0 };
        assert!(Playback::new(route, Instant::now()).is_ok());
    }

    #[test]
    fn recording_and_simulation_refuse_to_run_together() {
        let mut control = Control::default();
        // 正在模拟时不能开始录制：否则会把合成位置录成轨迹。
        assert!(send(&mut control, static_start()).ok);
        let refused = control.handle(r#"{"version":1,"op":"record_start"}"#);
        assert!(!refused.ok);
        assert!(refused.error.unwrap().contains("stop the simulation"));
        assert!(refused.state.recording.is_none());

        // 反过来也一样：录制中不能开始模拟。
        assert!(control.handle(r#"{"version":1,"op":"stop"}"#).ok);
        assert!(control.handle(r#"{"version":1,"op":"record_start"}"#).ok);
        let blocked = send(&mut control, static_start());
        assert!(!blocked.ok);
        assert!(blocked.error.unwrap().contains("stop recording"));
        assert!(!blocked.state.requested_active);
    }

    #[test]
    fn stopping_without_points_reports_an_error_and_leaves_nothing_behind() {
        let mut control = Control::default();
        assert!(control.handle(r#"{"version":1,"op":"record_start"}"#).ok);
        let response = control.handle(r#"{"version":1,"op":"record_stop"}"#);
        assert!(!response.ok);
        assert!(response.error.unwrap().contains("nothing was recorded"));
        // 没有半成品：既不在录，也没有成品交给面板。
        assert!(response.state.recording.is_none());
        assert!(response.state.recorded.is_none());
        // 再来一次仍然是"没在录"，不会因为上一次失败而卡住。
        assert!(!send(&mut control, record_point(31.0, 121.0, 0.0)).ok);
    }

    #[test]
    fn discarding_a_recording_clears_both_progress_and_result() {
        let mut control = Control::default();
        assert!(control.handle(r#"{"version":1,"op":"record_start"}"#).ok);
        assert!(send(&mut control, record_point(31.0, 121.0, 0.0)).ok);
        let discarded = control.handle(r#"{"version":1,"op":"record_discard"}"#);
        assert!(discarded.ok);
        assert!(discarded.state.recording.is_none());
        assert!(discarded.state.recorded.is_none());

        // 丢弃后可以重新开一段。
        assert!(control.handle(r#"{"version":1,"op":"record_start"}"#).ok);
        assert!(send(&mut control, record_point(32.0, 121.0, 0.0)).ok);
        assert!(control.handle(r#"{"version":1,"op":"record_stop"}"#).ok);
    }

    #[test]
    fn points_sent_without_recording_are_rejected() {
        let mut control = Control::default();
        let response = send(&mut control, record_point(31.0, 121.0, 0.0));
        assert!(!response.ok);
        assert!(response.error.unwrap().contains("no route recording"));
    }

    #[test]
    fn repeated_route_wait_can_pause_resume_and_stop_without_waiting_for_the_delay() {
        let mut control = Control::default();
        let now = Instant::now();
        let mut request: serde_json::Value =
            serde_json::from_str(&route_start(&[(0.0, 0.0), (0.0, 0.001)], 10.0)).unwrap();
        request["route"]["repeat_count"] = json!(3);
        request["route"]["repeat_delay"] = json!(10);
        assert!(control.handle_at(&request.to_string(), now).ok);
        let paused = control.handle_at(
            r#"{"version":1,"op":"pause_route"}"#,
            now + Duration::from_secs(12),
        );
        assert!(paused.ok);
        let paused = serde_json::to_value(paused).unwrap();
        assert_eq!(paused["state"]["route"]["completed"], false);
        assert_eq!(paused["state"]["route"]["lap"], 1);
        assert_eq!(paused["state"]["config"]["position"]["speed"], 0.0);
        let later = serde_json::to_value(control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            now + Duration::from_secs(100),
        ))
        .unwrap();
        assert_eq!(paused["state"]["route"], later["state"]["route"]);
        assert!(
            control
                .handle_at(
                    r#"{"version":1,"op":"resume_route"}"#,
                    now + Duration::from_secs(100)
                )
                .ok
        );
        let moving = serde_json::to_value(control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            now + Duration::from_secs(110),
        ))
        .unwrap();
        assert_eq!(moving["state"]["route"]["lap"], 2);
        assert_eq!(moving["state"]["config"]["position"]["speed"], 10.0);
        let waiting = serde_json::to_value(control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            now + Duration::from_secs(121),
        ))
        .unwrap();
        assert!(
            waiting["state"]["route"]["waiting_seconds"]
                .as_f64()
                .unwrap()
                > 0.0
        );
        let stopped = control.handle_at(
            r#"{"version":1,"op":"stop"}"#,
            now + Duration::from_secs(121),
        );
        assert!(stopped.ok);
        assert!(!stopped.state.requested_active);
        assert!(stopped.state.route.is_none());
    }

    #[test]
    fn repeats_use_total_play_count_and_finish_even_after_a_long_tick_gap() {
        let mut control = Control::default();
        let now = Instant::now();
        let mut request: serde_json::Value =
            serde_json::from_str(&route_start(&[(0.0, 0.0), (0.0, 0.001)], 10.0)).unwrap();
        request["route"]["repeat_count"] = json!(3);
        request["route"]["repeat_delay"] = json!(0);
        assert!(control.handle_at(&request.to_string(), now).ok);
        let second = serde_json::to_value(control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            now + Duration::from_secs(24),
        ))
        .unwrap();
        assert_eq!(second["state"]["route"]["lap"], 3);
        assert_eq!(second["state"]["route"]["completed"], false);
        let end = serde_json::to_value(control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            now + Duration::from_secs(34),
        ))
        .unwrap();
        assert_eq!(end["state"]["route"]["completed"], true);
        assert_eq!(end["state"]["route"]["lap"], 3);
        assert_eq!(end["state"]["route"]["waiting_seconds"], 0.0);
        assert_eq!(end["state"]["config"]["position"]["longitude"], 0.001);
        assert_eq!(end["state"]["config"]["position"]["speed"], 0.0);
        let later = serde_json::to_value(control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            now + Duration::from_secs(86400),
        ))
        .unwrap();
        assert_eq!(end["state"]["route"], later["state"]["route"]);
    }

    #[test]
    fn invalid_repeat_settings_leave_the_current_route_running() {
        let mut control = Control::default();
        let now = Instant::now();
        let base = route_start(&[(0.0, 0.0), (0.0, 0.001)], 10.0);
        assert!(control.handle_at(&base, now).ok);
        for (key, value) in [
            ("repeat_count", json!(0)),
            ("repeat_count", json!(1.5)),
            ("repeat_count", json!(10001)),
            ("repeat_delay", json!(-1)),
            ("repeat_delay", json!(86401)),
        ] {
            let mut request: serde_json::Value = serde_json::from_str(&base).unwrap();
            request["route"][key] = value;
            let response = control.handle_at(&request.to_string(), now);
            assert!(!response.ok);
            assert!(response.state.requested_active);
            assert_eq!(response.state.route.unwrap().plan.repeat_count, 1);
        }
    }

    #[test]
    fn joystick_requires_active_static_session_and_expires_without_changing_scope() {
        let mut control = Control::default();
        let now = Instant::now();
        let drive = r#"{"version":1,"op":"drive","speed":10,"bearing":90}"#;
        assert!(!control.handle_at(drive, now).ok);
        let start = json!({"version":1,"op":"start","config":{"position":Position::new(0.0,0.0),"scope":Scope::apps(["example.selected"])}}).to_string();
        assert!(control.handle_at(&start, now).ok);
        assert!(control.handle_at(drive, now).ok);
        let later = control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            now + Duration::from_secs(30),
        );
        assert!(later.state.requested_active);
        let config = later.state.config.unwrap();
        assert_eq!(config.scope, Scope::apps(["example.selected"]));
        assert_eq!(config.position.speed, 0.0);
        assert!((config.position.longitude - 0.00017986).abs() < 0.0000001);
        control.handle_at(
            r#"{"version":1,"op":"stop"}"#,
            now + Duration::from_secs(30),
        );
        assert!(!control.handle_at(drive, now + Duration::from_secs(31)).ok);
        assert!(
            control
                .handle_at(
                    &route_start(&[(0.0, 0.0), (0.0, 1.0)], 10.0),
                    now + Duration::from_secs(31)
                )
                .ok
        );
        assert!(!control.handle_at(drive, now + Duration::from_secs(32)).ok);
    }

    #[test]
    fn joystick_release_holds_position_and_manual_update_cancels_movement() {
        let mut control = Control::default();
        let now = Instant::now();
        control.handle_at(&json!({"version":1,"op":"start","config":{"position":Position::new(0.0,0.0),"scope":Scope::apps(["example.selected"])}}).to_string(), now);
        let drive = r#"{"version":1,"op":"drive","speed":10,"bearing":90}"#;
        control.handle_at(drive, now);
        let released = control.handle_at(
            r#"{"version":1,"op":"drive","speed":0,"bearing":90}"#,
            now + Duration::from_secs(1),
        );
        let later = control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            now + Duration::from_secs(30),
        );
        assert_eq!(released.state.config, later.state.config);
        control.handle_at(drive, now + Duration::from_secs(30));
        let update =
            json!({"version":1,"op":"update","position":Position::new(1.0,2.0)}).to_string();
        assert!(control.handle_at(&update, now + Duration::from_secs(31)).ok);
        let later = control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            now + Duration::from_secs(40),
        );
        assert_eq!(
            later.state.config.unwrap().position,
            Position::new(1.0, 2.0)
        );
    }

    #[test]
    fn route_advances_without_panel_requests_and_holds_the_destination() {
        let mut control = Control::default();
        let start = Instant::now();
        let response = control.handle_at(&route_start(&[(0.0, 0.0), (0.0, 0.001)], 10.0), start);
        assert!(response.ok, "{:?}", response.error);
        let middle = control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            start + Duration::from_secs(5),
        );
        let point = middle.state.config.unwrap().position;
        assert!((point.longitude - 0.00044966).abs() < 0.000001);
        assert!((point.bearing - 90.0).abs() < 0.001);
        assert_eq!(point.speed, 10.0);
        let end = control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            start + Duration::from_secs(20),
        );
        assert!(end.state.requested_active);
        assert_eq!(end.state.config.unwrap().position.longitude, 0.001);
        let end_json = serde_json::to_value(control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            start + Duration::from_secs(30),
        ))
        .unwrap();
        assert_eq!(end_json["state"]["route"]["completed"], true);
        assert_eq!(end_json["state"]["config"]["position"]["speed"], 0.0);
    }

    #[test]
    fn route_pause_resume_and_stop_keep_the_same_application_scope() {
        let mut control = Control::default();
        let start = Instant::now();
        assert!(
            control
                .handle_at(&route_start(&[(0.0, 0.0), (0.0, 0.01)], 10.0), start)
                .ok
        );
        let paused = control.handle_at(
            r#"{"version":1,"op":"pause_route"}"#,
            start + Duration::from_secs(3),
        );
        assert!(paused.ok);
        let position = paused.state.config.unwrap().position;
        assert_eq!(position.speed, 0.0);
        let later = control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            start + Duration::from_secs(30),
        );
        assert_eq!(later.state.config.unwrap().position, position);
        assert!(
            control
                .handle_at(
                    r#"{"version":1,"op":"resume_route"}"#,
                    start + Duration::from_secs(30)
                )
                .ok
        );
        let moving = control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            start + Duration::from_secs(32),
        );
        let config = moving.state.config.unwrap();
        assert!((config.position.longitude - 0.00044966).abs() < 0.000001);
        assert_eq!(config.scope, crate::Scope::apps(["example.selected"]));
        let stopped = control.handle_at(
            r#"{"version":1,"op":"stop"}"#,
            start + Duration::from_secs(33),
        );
        assert!(!stopped.state.requested_active);
        let stopped = serde_json::to_value(stopped).unwrap();
        assert!(stopped["state"]["route"].is_null());
    }

    #[test]
    fn invalid_routes_do_not_replace_saved_position() {
        let mut control = Control::default();
        let before = control.handle(&json!({"version":1,"op":"start","config":{
            "position":Position::new(31.2,121.5),"scope":{"mode":"apps","packages":["example.selected"]}}}).to_string()).state.config;
        control.handle(r#"{"version":1,"op":"stop"}"#);
        for request in [
            route_start(&[(0.0, 0.0)], 1.0),
            route_start(&[(0.0, 0.0), (0.0, 0.0)], 1.0),
            route_start(&[(0.0, 0.0), (0.0, 1.0)], 0.0),
            route_start(&[(91.0, 0.0), (0.0, 1.0)], 1.0),
        ] {
            let reply = control.handle(&request);
            assert!(!reply.ok);
            assert!(!reply.state.requested_active);
            assert_eq!(reply.state.config, before);
        }
    }

    #[test]
    fn expired_hook_heartbeat_is_not_reported_ready() {
        let mut control = Control::default();
        control.handle(r#"{"version":1,"op":"hook_status","installed":true}"#);
        control.hook_seen_at = Some(Instant::now() - Duration::from_secs(4));
        let response = control.handle(r#"{"version":1,"op":"status"}"#);
        assert!(!response.state.hook_connected);
    }

    /// Wi-Fi 的就绪位与回调计数走同一条心跳，但含义完全不同：两位说明"方法挂上了"，
    /// 计数说明"回调真的被走到了"。真机验收要能分别看到这两件事。
    #[test]
    fn wifi_hook_status_reports_readiness_and_call_count() {
        let mut control = Control::default();
        control.handle(r#"{"version":1,"op":"hook_status","installed":true,"wifi_scan":true}"#);
        let state = serde_json::to_value(control.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
        assert_eq!(state["state"]["wifi_scan_hook_ready"], true);
        assert_eq!(state["state"]["wifi_connection_hook_ready"], false);
        assert_eq!(state["state"]["wifi_hook_calls"], 0);

        control.handle(
            r#"{"version":1,"op":"hook_status","installed":true,"wifi_scan":true,"wifi_connection":true,"wifi_calls":7}"#,
        );
        let state = serde_json::to_value(control.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
        assert_eq!(state["state"]["wifi_connection_hook_ready"], true);
        assert_eq!(state["state"]["wifi_hook_calls"], 7);
    }

    #[test]
    fn gnss_raw_counters_pass_through_untouched() {
        let mut control = Control::default();
        // 老版本桥接不带诊断串：字段缺省即可，不能因为少一个字段就整条回包失败。
        control.handle(r#"{"version":1,"op":"hook_status","installed":true,"gnss_raw":true}"#);
        let state = serde_json::to_value(control.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
        assert_eq!(state["state"]["gnss_raw_hook_ready"], true);
        assert_eq!(state["state"]["gnss_raw_detail"], serde_json::Value::Null);

        // 新版本桥接报上来什么就原样给出去，后台不解析它的内部格式。
        control.handle(
            r#"{"version":1,"op":"hook_status","installed":true,"gnss_raw":true,"gnss_raw_detail":"2:1,9,9,9,8,0;3:1,9,9,0,0,0"}"#,
        );
        let state = serde_json::to_value(control.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
        assert_eq!(state["state"]["gnss_raw_detail"], "2:1,9,9,9,8,0;3:1,9,9,0,0,0");
    }

    #[test]
    fn satellite_switches_round_trip_through_the_protocol() {
        let mut control = Control::default();
        let initial = serde_json::to_value(control.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
        // 默认两个通道都是关的。
        assert_eq!(initial["state"]["gnss"]["gnss_enabled"], false);
        assert_eq!(initial["state"]["gnss"]["nmea_enabled"], false);
        let enabled = control.handle(
            r#"{"version":1,"op":"set_gnss","config":{"gnss_enabled":true,"nmea_enabled":true}}"#,
        );
        assert!(enabled.ok, "{:?}", enabled.error);
        let state = serde_json::to_value(enabled).unwrap();
        assert_eq!(state["state"]["gnss"]["gnss_enabled"], true);
        assert_eq!(state["state"]["gnss"]["nmea_enabled"], true);
        // 缺字段的请求按默认值补齐，未知字段要报错而不是被忽略。
        assert!(control
            .handle(r#"{"version":1,"op":"set_gnss","config":{"gnss_enabled":false}}"#)
            .ok);
        assert!(!control
            .handle(r#"{"version":1,"op":"set_gnss","config":{"gnss":true}}"#)
            .ok);
    }

    #[test]
    fn satellite_switches_persist_and_older_files_still_load() {
        let directory = std::env::temp_dir().join(format!("justlocation-gnss-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("state.json");
        let _ = std::fs::remove_file(&path);
        {
            let mut control = Control::open(&path).unwrap();
            assert!(control
                .handle(r#"{"version":1,"op":"set_gnss","config":{"gnss_enabled":true,"nmea_enabled":false}}"#)
                .ok);
        }
        let mut reopened = Control::open(&path).unwrap();
        let state = serde_json::to_value(reopened.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
        assert_eq!(state["state"]["gnss"]["gnss_enabled"], true);
        assert_eq!(state["state"]["gnss"]["nmea_enabled"], false);
        // 没有 gnss 段的旧配置文件仍要能打开。
        std::fs::write(&path, r#"{"version":3,"config":null,"cell_region":null,"telephony":{"cells_enabled":false,"sim_enabled":false,"radius_m":500.0,"subscriptions":[]}}"#).unwrap();
        let mut legacy = Control::open(&path).unwrap();
        let state = serde_json::to_value(legacy.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
        assert_eq!(state["state"]["gnss"]["gnss_enabled"], false);
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// 运营商属性必须在响应构建**之前**应用：否则客户端拿到的 `operator_hook_ready`
    /// 永远是上一次的状态，看起来像"没接管"（真机验收踩过）。顺带钉住"同一状态不重复写"。
    #[test]
    fn operator_properties_follow_the_session_state() {
        use crate::operators::{Properties, tests::Recording};
        let directory = std::env::temp_dir().join(format!("justlocation-operators-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let probe = Arc::new(Recording::default());        probe.set(crate::operators::NETWORK_ALPHA, "中国电信").unwrap();
        let mut control = Control::open_with(
            directory.join("state.json"),
            Some(Box::new(SharedProbe(probe.clone()))),
        )
        .unwrap();

        let state = control.handle(r#"{"version":1,"op":"set_telephony","config":{"cells_enabled":false,"sim_enabled":true,"radius_m":500,"subscriptions":[{"id":1,"slot":0,"mcc":"460","mnc":"11","country":"cn","carrier":"中国联通","enabled":true}]}}"#);
        assert!(state.ok);
        // 只是配置了、还没开始模拟：不该接管。
        assert_eq!(state.state.operator_hook_ready, false);
        assert_eq!(probe.value(crate::operators::NETWORK_ALPHA).unwrap(), "中国电信");

        let started = control.handle(
            r#"{"version":1,"op":"start","config":{"position":{"latitude":31.23,"longitude":121.47,"altitude":0,"accuracy":5,"speed":0,"bearing":0},"scope":{"mode":"all"}}}"#,
        );
        assert!(started.ok);
        assert_eq!(started.state.operator_hook_ready, true, "the properties must be taken over on the first tick");
        assert_eq!(probe.value(crate::operators::NETWORK_ALPHA).unwrap(), "中国联通");

        let writes = probe.write_count();
        let status = control.handle(r#"{"version":1,"op":"status"}"#);
        assert_eq!(status.state.operator_hook_ready, true);
        assert_eq!(probe.write_count(), writes, "unchanged state must not write the properties again");

        let stopped = control.handle(r#"{"version":1,"op":"stop"}"#);
        assert_eq!(stopped.state.operator_hook_ready, false);
        assert_eq!(probe.value(crate::operators::NETWORK_ALPHA).unwrap(), "中国电信", "the real values must be restored after stopping");
        let _ = std::fs::remove_dir_all(&directory);
    }

    struct SharedProbe(Arc<crate::operators::tests::Recording>);

    impl crate::operators::Properties for SharedProbe {
        fn get(&self, name: &str) -> Option<String> { self.0.get(name) }
        fn set(&self, name: &str, value: &str) -> io::Result<()> { self.0.set(name, value) }
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Status,
    Start {
        config: Config,
    },
    StartRoute {
        route: Route,
        scope: Scope,
    },
    PauseRoute,
    ResumeRoute,
    Update {
        position: Position,
    },
    Drive {
        speed: f64,
        bearing: f64,
    },
    Stop,
    Shutdown,
    HookStatus {
        installed: bool,
        #[serde(default)]
        gnss: bool,
        #[serde(default)]
        nmea: bool,
        #[serde(default)]
        cell_callbacks: bool,
        /// Wi-Fi 扫描结果适配。老版本原生模块不带这两个字段，按未安装处理。
        #[serde(default)]
        wifi_scan: bool,
        #[serde(default)]
        wifi_connection: bool,
        /// 桥接自报的 Wi-Fi 回调计数，仅用于诊断。
        #[serde(default)]
        wifi_calls: u32,
        /// GNSS 原始数据两条出口（原始测量 + 导航电文）是否装好。老版本原生模块不带这个字段。
        #[serde(default)]
        gnss_raw: bool,
        /// GNSS 原始通道的计数串，**原样透传**给客户端的 `gnss_raw_detail`。
        ///
        /// <p>为什么不在这里解析成结构体：它是桥接侧的诊断内容，格式由桥接决定，
        /// 后台不该跟着一起改。老版本原生模块不带这个字段。
        #[serde(default)]
        gnss_raw_detail: Option<String>,
    },
    SetCellRegion {
        region: Option<CellRegion>,
    },
    SetTelephony {
        config: TelephonyConfig,
    },
    SetGnss {
        config: GnssConfig,
    },
    SetWifi {
        config: WifiConfig,
    },
    /// 路线录制：不产生输出，只把真实位置累积成一条轨迹。
    RecordStart,
    RecordPoint {
        position: Position,
        /// 本次录制的单调时间戳（秒），由录制端给出。
        seconds: f64,
    },
    /// 结束录制并交回轨迹；没有录到点时返回错误。
    RecordStop,
    /// 取走上一次录制的结果。取走后 `recorded` 回到 null，保证同一份成品只交付一次。
    RecordTake,
    /// 直接丢弃当前录制。
    RecordDiscard,
    TelephonyHookStatus {
        cells: bool,
        sim: bool,
        #[serde(default)]
        subscriptions: Option<Vec<DetectedSubscription>>,
        /// 手机进程读到的**真实**运营商名与 PLMN（`TelephonyManager` 的四个 getter）。
        ///
        /// <p>为什么由手机进程报：这四个值在应用进程里读的是系统属性，而属性正是我们改写的地方；
        /// 运营商服务里的真值没被动过，只有手机进程读得到。它是"还原"最可信的来源。
        /// 老版本桥接不带这四个字段，缺省即视为没有实时真值。
        #[serde(default)]
        network_alpha: Option<String>,
        #[serde(default)]
        sim_alpha: Option<String>,
        #[serde(default)]
        network_numeric: Option<String>,
        #[serde(default)]
        sim_numeric: Option<String>,
    },
    QueryCells {
        target: Coordinate,
        radius_m: f64,
        limit: usize,
    },
}

#[derive(Clone, Serialize)]
pub struct CellQuery {
    pub target: Coordinate,
    pub radius_m: f64,
    pub source: String,
    pub fetched_at_ms: u64,
    pub items: Vec<NearbyCell>,
}

/// 录制过程中的进度：面板据此显示"已录 N 个点 / M 秒"以及是否到达上限。
#[derive(Serialize)]
pub struct RecordProgress {
    pub points: usize,
    pub seconds: f64,
    pub full: bool,
    /// 因与上一点重复或过近而丢弃的采样数。
    pub skipped: u64,
}

/// 录制结束后的成品。点数不足以回放时仍然返回，由界面提示用户再录一段。
#[derive(Serialize)]
pub struct RecordedTrack {
    pub points: Vec<Position>,
    pub seconds: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u32,
    config: Option<Config>,
    cell_region: Option<CellRegion>,
    #[serde(default)]
    telephony: TelephonyConfig,
    /// 旧配置文件没有这一段，缺失时按两个开关都关闭处理。
    #[serde(default)]
    gnss: GnssConfig,
    /// 同上，Wi-Fi 目标列表缺失时按未启用处理。
    #[serde(default)]
    wifi: WifiConfig,
}

#[derive(Serialize)]
pub struct State {
    pub requested_active: bool,
    pub config: Option<Config>,
    pub hook_connected: bool,
    pub location_hook_ready: bool,
    pub gnss_hook_ready: bool,
    pub nmea_hook_ready: bool,
    pub route: Option<RouteState>,
    pub cell_region: Option<CellRegion>,
    pub telephony: TelephonyConfig,
    pub telephony_output: Option<TelephonyFrame>,
    /// **当前输出的基站是不是伪造的兜底数据。**
    ///
    /// <p>打开基站输出、但那一带一条真实小区数据都没有时，装置会为虚拟位置造几个小区，
    /// 免得应用因为"周围一个基站都没有"而退回到自己的定位。造出来的编号只在本机成立、
    /// 云端查不到、也不对应任何真实基站——所以这里如实置 true，让命令行与面板都能提示使用者。
    pub cells_synthesized: bool,
    pub gnss: GnssConfig,
    pub wifi: WifiConfig,
    /// 正在录制时的实时状态；停止或丢弃后回到 null。
    pub recording: Option<RecordProgress>,
    /// 最近一次录制的结果，供面板在结束录制后取走；取走后回到 null。
    pub recorded: Option<RecordedTrack>,
    pub cell_hook_ready: bool,
    pub cell_query_hook_ready: bool,
    pub cell_callback_hook_ready: bool,
    pub sim_hook_ready: bool,
    /// 运营商名称与 PLMN 的属性出口 Hook（`TelephonyManager` 的四个 getter 都读它们）。
    pub operator_hook_ready: bool,
    /// Wi-Fi 服务端两项适配。**分开报告**：扫描结果与连接信息是两次独立的安装，
    /// 装上一项不等于另一项也在（原版报告特别强调过这一点）。
    pub wifi_scan_hook_ready: bool,
    pub wifi_connection_hook_ready: bool,
    /// Wi-Fi 两处回调被走到的次数，**由桥接侧自己报上来**。
    ///
    /// <p>为什么要它：「hook 就绪」只说明方法挂上了，不说明回调真的被走到。真机验收里
    /// 出现过"两项报就绪、应用侧仍是真实数据"，而所有基于日志/文件的诊断都栽在同一个坑上——
    /// `system_server` 既写不进 `/data/adb/justlocation`（`drwx------ root root`），
    /// 也看不到被环形缓冲冲掉的启动期日志。计数走状态回包本身，没有写入失败这回事。
    pub wifi_hook_calls: u32,
    /// GNSS 原始数据两条出口（`GnssMeasurementsProvider` 与 `GnssNavigationMessageProvider`）
    /// 是否都已接管。**是一个合并位**：两条出口在同一次安装里挂，任一条失败就是 false。
    pub gnss_raw_hook_ready: bool,
    /// GNSS 原始通道的计数串（桥接侧原样上报，仅供诊断）。
    ///
    /// <p>和 `wifi_hook_calls` 同一个理由：就绪位只说明"挂上了"，说明不了注册有没有被
    /// 投递到。格式是 `下标:注册数,dispatch 次数,该投次数,真投次数,送达数,失败数`，
    /// 多条用 `;` 分隔；`installGnssRaw` 里两条通道依次是原始测量、导航电文。
    pub gnss_raw_detail: Option<String>,
    pub phone_connected: bool,
    pub detected_subscriptions: Option<Vec<DetectedSubscription>>,
}

#[derive(Serialize)]
pub struct Response {
    pub version: u32,
    pub ok: bool,
    pub error: Option<String>,
    pub state: State,
    pub cells: Option<CellQuery>,
}

#[derive(Default)]
pub struct Control {
    engine: Engine,
    storage: Option<PathBuf>,
    shutdown: bool,
    hook_seen_at: Option<Instant>,
    hook_installed: bool,
    gnss_installed: bool,
    nmea_installed: bool,
    route: Option<Playback>,
    motion: Option<Motion>,
    cell_region: Option<CellRegion>,
    cell_query: Option<CellQuery>,
    telephony: TelephonyConfig,
    gnss: GnssConfig,
    wifi: WifiConfig,
    recording: Option<Recording>,
    /// 录制期间被抽稀掉的采样数，只用于向用户解释点数。
    recording_skipped: u64,
    /// 最近一次录制的成品；面板取走后清空。
    recorded: Option<RecordedTrack>,
    phone_seen_at: Option<Instant>,
    cells_installed: bool,
    cell_callbacks_installed: bool,
    sim_installed: bool,
    wifi_scan_installed: bool,
    wifi_connection_installed: bool,
    /// 桥接上报的 Wi-Fi 回调计数；老版本桥接不带这个字段时保持 0。
    wifi_hook_calls: u32,
    /// GNSS 原始数据两条出口（原始测量 + 导航电文）是否装好；老版本桥接不带这个字段。
    gnss_raw_installed: bool,
    /// 桥接上报的 GNSS 原始通道计数串；老版本桥接不带这个字段时保持 None。
    gnss_raw_detail: Option<String>,
    /// 运营商名称与 PLMN 的系统属性出口（应用进程直接读这些属性）。
    operators: Operators,
    /// 手机进程报上来的**真实**运营商值。属性被我们盖住了，而运营商服务里的没被动过，
    /// 所以它是还原时最可信的真值来源（见 `crate::operators::Live`）。
    live_operators: Option<crate::operators::Live>,
    live_operators_seen_at: Option<Instant>,
    detected_subscriptions: Option<Vec<DetectedSubscription>>,
}

impl Control {
    fn handle_at(&mut self, line: &str, now: Instant) -> Response {
        self.cell_query = None;
        if let Some(motion) = &mut self.motion {
            self.engine
                .update_position(motion.advance(now))
                .expect("validated movement position");
        }
        if let Some(route) = &mut self.route {
            self.engine
                .update_position(route.advance(now))
                .expect("validated route position");
        }
        let result = self.apply(line, now);
        // 运营商属性跟着状态走：只有"该不该接管"变化时才动系统属性。
        // 必须在构建响应**之前**做，否则响应里的 operator_hook_ready 会滞后一次请求：
        // 客户端拿到的永远是上一次的状态，验收时看起来像"没接管"。
        // 只有**新鲜**的手机进程真值才可用：它和 `phone_connected` 用同一个新鲜度尺度。
        let live = self
            .live_operators_seen_at
            .filter(|seen| seen.elapsed() < Duration::from_secs(3))
            .and(self.live_operators.clone());
        if let Err(error) = self
            .operators
            .apply(&self.telephony, self.engine.is_running(), live.as_ref())
        {
            eprintln!("operator properties: {error}");
        }
        self.response(result.err())
    }
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_with(path, None)
    }

    /// `properties` 只在测试里给出：协议层要验证"运营商属性跟着状态走"，
    /// 但测试绝不该真去动设备的系统属性。
    fn open_with(path: impl AsRef<Path>, properties: Option<Box<dyn crate::operators::Properties + Send>>) -> io::Result<Self> {
        let path = path.as_ref();
        let mut engine = Engine::default();
        let mut cell_region = None;
        let mut telephony = TelephonyConfig::default();
        let mut gnss = GnssConfig::default();
        let mut wifi = WifiConfig::default();
        match std::fs::read(path) {
            Ok(bytes) => {
                let value: serde_json::Value = serde_json::from_slice(&bytes)?;
                let config = if value.get("version").is_some() {
                    let stored: Stored = serde_json::from_value(value)?;
                    if stored.version != 2 && stored.version != 3 {
                        return Err(io::Error::other("unsupported configuration version"));
                    }
                    if let Some(region) = &stored.cell_region {
                        region.validate().map_err(io::Error::other)?;
                    }
                    cell_region = stored.cell_region;
                    stored.telephony.validate().map_err(io::Error::other)?;
                    telephony = stored.telephony;
                    stored.gnss.validate().map_err(io::Error::other)?;
                    gnss = stored.gnss;
                    stored.wifi.validate().map_err(io::Error::other)?;
                    wifi = stored.wifi;
                    stored.config
                } else {
                    Some(serde_json::from_value(value)?)
                };
                if let Some(config) = config {
                    engine.start(config).map_err(io::Error::other)?;
                    engine.stop();
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        Ok(Self {
            engine,
            storage: Some(path.to_owned()),
            shutdown: false,
            hook_seen_at: None,
            hook_installed: false,
            gnss_installed: false,
            nmea_installed: false,
            route: None,
            motion: None,
            cell_region,
            cell_query: None,
            telephony,
            gnss,
            wifi,
            // 录制状态不持久化：重启后既没有在录，也没有上一次的成品。
            recording: None,
            recording_skipped: 0,
            recorded: None,
            phone_seen_at: None,
            cells_installed: false,
            cell_callbacks_installed: false,
            sim_installed: false,
            wifi_scan_installed: false,
            wifi_connection_installed: false,
            wifi_hook_calls: 0,
            gnss_raw_installed: false,
            gnss_raw_detail: None,
            operators: {
                let data = path.parent().unwrap_or_else(|| Path::new("."));
                let mut operators = match properties {
                    Some(properties) => Operators::with_properties(properties, data),
                    None => Operators::new(data),
                };
                // 上一次脱管退出可能把模拟值留在系统里：启动时先还原，再进入正常流程。
                // 此刻手机进程通常还没连上（没有实时真值），所以这里只能靠备份文件；
                // 一旦手机进程开始报真值，`handle_at` 那条路径就会用实时值纠正。
                if let Err(error) = operators.recover(None) {
                    eprintln!("operator properties: {error}");
                }
                operators
            },
            live_operators: None,
            live_operators_seen_at: None,
            detected_subscriptions: None,
        })
    }

    pub fn handle(&mut self, line: &str) -> Response {
        self.handle_at(line, Instant::now())
    }

    fn apply(&mut self, line: &str, now: Instant) -> Result<(), String> {
        let request: Request = serde_json::from_str(line).map_err(|error| error.to_string())?;
        if request.version != VERSION {
            return Err("unsupported protocol version".into());
        }
        let previous = self.engine.clone();
        let previous_route = self.route.clone();
        let previous_motion = self.motion.clone();
        let previous_region = self.cell_region.clone();
        let previous_telephony = self.telephony.clone();
        let previous_gnss = self.gnss;
        let previous_wifi = self.wifi.clone();
        let mutates_config = matches!(
            request.command,
            Command::Start { .. }
                | Command::StartRoute { .. }
                | Command::Update { .. }
                | Command::SetCellRegion { .. }
                | Command::SetTelephony { .. }
                | Command::SetGnss { .. }
                | Command::SetWifi { .. }
        );
        let result = match request.command {
            Command::Status => Ok(()),
            Command::SetWifi { config } => {
                config.validate().map_err(str::to_owned)?;
                self.wifi = config;
                Ok(())
            }
            Command::SetGnss { config } => {
                config.validate().map_err(str::to_owned)?;
                self.gnss = config;
                Ok(())
            }
            Command::SetTelephony { config } => {
                config.validate().map_err(str::to_owned)?;
                self.telephony = config;
                Ok(())
            }
            Command::TelephonyHookStatus { cells, sim, subscriptions, network_alpha, sim_alpha, network_numeric, sim_numeric } => {
                if let Some(cards) = &subscriptions { validate_detected(cards).map_err(str::to_owned)?; }
                self.phone_seen_at = Some(Instant::now());
                self.cells_installed = cells;
                self.sim_installed = sim;
                // 四个值一起给才算一份可用的真值快照；只给一部分时不采信（免得半真半假）。
                let live = crate::operators::Live {
                    network_alpha: network_alpha.unwrap_or_default(),
                    sim_alpha: sim_alpha.unwrap_or_default(),
                    network_numeric: network_numeric.unwrap_or_default(),
                    sim_numeric: sim_numeric.unwrap_or_default(),
                };
                if live.is_usable() {
                    self.live_operators = Some(live);
                    self.live_operators_seen_at = Some(Instant::now());
                }
                self.detected_subscriptions = subscriptions;
                Ok(())
            }
            Command::SetCellRegion { region } => {
                if let Some(region) = &region {
                    region.validate().map_err(str::to_owned)?;
                }
                self.cell_region = region;
                Ok(())
            }
            Command::RecordStart => {
                // 录制期间系统回调给出的是我们自己的合成位置，录下来就是绕回自身的轨迹，
                // 所以先要求停止模拟；这与会话本身不冲突（录制不产生输出）。
                if self.engine.is_running() {
                    return Err("stop the simulation before recording a route".into());
                }
                self.recording = Some(Recording::new());
                self.recording_skipped = 0;
                self.recorded = None;
                Ok(())
            }
            Command::RecordPoint { position, seconds } => {
                let recording = self
                    .recording
                    .as_mut()
                    .ok_or("no route recording is in progress")?;
                if !recording.add(position, seconds).map_err(str::to_owned)? {
                    self.recording_skipped += 1;
                }
                Ok(())
            }
            Command::RecordStop => {
                let recording = self.recording.take().ok_or("no route recording is in progress")?;
                if recording.points().is_empty() {
                    // 一个点都没录到就把状态留在"没在录"，并明确报错，避免界面显示一条空轨迹。
                    self.recorded = None;
                    return Err("nothing was recorded".into());
                }
                self.recorded = Some(RecordedTrack {
                    points: recording.points().to_vec(),
                    seconds: recording.seconds(),
                });
                Ok(())
            }
            Command::RecordTake => {
                // 交付一次就清空：否则面板每刷新一次就"收到"一次成品，无法判断该不该提示。
                if self.recorded.take().is_none() {
                    return Err("no recorded route is waiting".into());
                }
                Ok(())
            }
            Command::RecordDiscard => {
                self.recording = None;
                self.recording_skipped = 0;
                self.recorded = None;
                Ok(())
            }
            Command::QueryCells {
                target,
                radius_m,
                limit,
            } => {
                let region = self
                    .cell_region
                    .as_ref()
                    .ok_or("no cell region has been acquired")?;
                let items = region
                    .nearby(target, radius_m, limit)
                    .map_err(str::to_owned)?;
                self.cell_query = Some(CellQuery {
                    target,
                    radius_m,
                    source: region.source.clone(),
                    fetched_at_ms: region.fetched_at_ms,
                    items,
                });
                Ok(())
            }
            Command::HookStatus {
                installed,
                gnss,
                nmea,
                cell_callbacks,
                wifi_scan,
                wifi_connection,
                wifi_calls,
                gnss_raw,
                gnss_raw_detail,
            } => {
                self.hook_seen_at = Some(Instant::now());
                self.hook_installed = installed;
                self.gnss_installed = gnss;
                self.nmea_installed = nmea;
                self.cell_callbacks_installed = cell_callbacks;
                self.wifi_scan_installed = wifi_scan;
                self.wifi_connection_installed = wifi_connection;
                self.wifi_hook_calls = wifi_calls;
                self.gnss_raw_installed = gnss_raw;
                self.gnss_raw_detail = gnss_raw_detail;
                Ok(())
            }
            Command::Start { config } => {
                // 录制与模拟互斥：录制中开始模拟会立刻让录制采到合成位置。
                if self.recording.is_some() {
                    return Err("stop recording before starting the simulation".into());
                }
                self.engine.start(config).map_err(str::to_owned)
            }
            Command::StartRoute { route, scope } => {
                let route = Playback::new(route, now).map_err(str::to_owned)?;
                self.engine
                    .start(Config {
                        position: route.position(),
                        scope,
                    })
                    .map_err(str::to_owned)?;
                self.route = Some(route);
                Ok(())
            }
            Command::PauseRoute | Command::ResumeRoute => {
                let route = self.route.as_mut().ok_or("no route is running")?;
                route
                    .pause(matches!(request.command, Command::PauseRoute))
                    .map_err(str::to_owned)?;
                self.engine
                    .update_position(route.position())
                    .map_err(str::to_owned)
            }
            Command::Update { position } => {
                if self.route.is_some() {
                    return Err("stop the route before selecting another position".into());
                }
                self.engine
                    .update_position(position)
                    .map_err(str::to_owned)?;
                self.motion = None;
                Ok(())
            }
            Command::Drive { speed, bearing } => {
                if !self.engine.is_running() {
                    return Err("start location simulation first".into());
                }
                if self.route.is_some() {
                    return Err("stop the route before using the joystick".into());
                }
                let mut motion = Motion::new(
                    self.engine.config().unwrap().position.clone(),
                    speed,
                    bearing,
                    now,
                )
                .map_err(str::to_owned)?;
                self.engine
                    .update_position(motion.advance(now))
                    .map_err(str::to_owned)?;
                self.motion = if speed == 0.0 { None } else { Some(motion) };
                Ok(())
            }
            Command::Shutdown => {
                self.shutdown = true;
                self.engine.stop();
                self.route = None;
                self.motion = None;
                self.recording = None;
                Ok(())
            }
            Command::Stop => {
                self.engine.stop();
                self.route = None;
                self.motion = None;
                Ok(())
            }
        };
        result?;
        if mutates_config {
            if let Some(path) = &self.storage {
                if let Err(error) = self.save(path) {
                    self.engine = previous;
                    self.route = previous_route;
                    self.motion = previous_motion;
                    self.cell_region = previous_region;
                    self.telephony = previous_telephony;
                    self.gnss = previous_gnss;
                    self.wifi = previous_wifi;
                    return Err(format!("cannot save configuration: {error}"));
                }
            }
        }
        Ok(())
    }

    fn save(&self, path: &Path) -> io::Result<()> {
        let temp = path.with_extension("tmp");
        let mut file = std::fs::File::create(&temp)?;
        serde_json::to_writer(
            &mut file,
            &Stored {
                version: 3,
                config: self.engine.config().cloned(),
                cell_region: self.cell_region.clone(),
                telephony: self.telephony.clone(),
                gnss: self.gnss,
                wifi: self.wifi.clone(),
            },
        )?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(temp, path)
    }

    fn response(&self, error: Option<String>) -> Response {
        let hook_connected = self
            .hook_seen_at
            .is_some_and(|time| time.elapsed() < Duration::from_secs(3));
        let phone_connected = self
            .phone_seen_at
            .is_some_and(|time| time.elapsed() < Duration::from_secs(3));
        let telephony_output = if self.engine.is_running()
            && (self.telephony.cells_enabled || self.telephony.sim_enabled)
        {
            self.engine.config().map(|config| {
                self.telephony.frame(
                    self.cell_region.as_ref(),
                    Coordinate {
                        latitude: config.position.latitude,
                        longitude: config.position.longitude,
                    },
                )
            })
        } else {
            None
        };
        // 单独再报一位：帧里的 `synthesized` 埋在结构里，而这一位是"**现在输出的基站是伪造的**"
        // 这句要紧的话，命令行、面板与动作脚本都该一眼看到，不必去挖嵌套结构。
        let cells_synthesized = telephony_output
            .as_ref()
            .is_some_and(|frame| frame.synthesized);
        Response {
            version: VERSION,
            ok: error.is_none(),
            cells: if error.is_none() {
                self.cell_query.clone()
            } else {
                None
            },
            error,
            state: State {
                requested_active: self.engine.is_running(),
                config: self.engine.config().cloned(),
                hook_connected,
                location_hook_ready: hook_connected && self.hook_installed,
                gnss_hook_ready: hook_connected && self.gnss_installed,
                nmea_hook_ready: hook_connected && self.nmea_installed,
                route: self.route.as_ref().map(Playback::state),
                cell_region: self.cell_region.clone(),
                telephony: self.telephony.clone(),
                telephony_output,
                cells_synthesized,
                gnss: self.gnss,
                wifi: self.wifi.clone(),
                recording: self.recording.as_ref().map(|recording| RecordProgress {
                    points: recording.points().len(),
                    seconds: recording.seconds(),
                    full: recording.is_full(),
                    skipped: self.recording_skipped,
                }),
                recorded: self.recorded.as_ref().map(|track| RecordedTrack {
                    points: track.points.clone(),
                    seconds: track.seconds,
                }),
                cell_hook_ready: phone_connected && self.cells_installed && hook_connected && self.cell_callbacks_installed,
                cell_query_hook_ready: phone_connected && self.cells_installed,
                cell_callback_hook_ready: hook_connected && self.cell_callbacks_installed,
                sim_hook_ready: phone_connected && self.sim_installed,
                operator_hook_ready: self.operators.is_ready(),
                wifi_scan_hook_ready: hook_connected && self.wifi_scan_installed,
                wifi_connection_hook_ready: hook_connected && self.wifi_connection_installed,
                wifi_hook_calls: self.wifi_hook_calls,
            // 原始测量与导航电文**一起**装好才算就绪：两条出口是同一次安装里的两个钩子。
            gnss_raw_hook_ready: hook_connected && self.gnss_raw_installed,
            // 计数串只在钩子还连着的时候有意义；断线时留着旧数字反而误导。
            gnss_raw_detail: if hook_connected { self.gnss_raw_detail.clone() } else { None },
                phone_connected,
                detected_subscriptions: if phone_connected { self.detected_subscriptions.clone() } else { None },
            },
        }
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    pub fn serve(&mut self, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
        loop {
            let mut line = String::new();
            let count = std::io::Read::take(&mut input, MAX_FRAME + 1).read_line(&mut line)?;
            if count == 0 {
                return Ok(());
            }
            if count as u64 > MAX_FRAME {
                let response = self.response(Some("request exceeds 64 KiB".into()));
                serde_json::to_writer(&mut output, &response)?;
                output.write_all(b"\n")?;
                output.flush()?;
                return Ok(());
            }
            serde_json::to_writer(&mut output, &self.handle(&line))?;
            output.write_all(b"\n")?;
            output.flush()?;
            if self.shutdown {
                return Ok(());
            }
        }
    }
}

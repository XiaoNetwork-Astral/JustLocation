use crate::{
    Config, Engine, Position, Scope,
    cells::{CellRegion, Coordinate, NearbyCell},
    gnss::GnssConfig,
    motion::Motion,
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
        let track = response.state.recorded.expect("录制结果要交回给面板");
        assert_eq!(track.points.len(), 2);
        assert!(response.state.recording.is_none());
        // 交付一次就取走：重复取要被拒绝，避免界面反复弹同一条成品。
        let taken = control.handle(r#"{"version":1,"op":"record_take"}"#);
        assert!(taken.ok);
        assert!(taken.state.recorded.is_none(), "取走后同一份成品不应再出现");
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
        self.response(result.err())
    }
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
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
            Command::TelephonyHookStatus { cells, sim, subscriptions } => {
                if let Some(cards) = &subscriptions { validate_detected(cards).map_err(str::to_owned)?; }
                self.phone_seen_at = Some(Instant::now());
                self.cells_installed = cells;
                self.sim_installed = sim;
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
            } => {
                self.hook_seen_at = Some(Instant::now());
                self.hook_installed = installed;
                self.gnss_installed = gnss;
                self.nmea_installed = nmea;
                self.cell_callbacks_installed = cell_callbacks;
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

use super::*;
use crate::{Position, Scope, route::Route};
use serde_json::json;
use std::sync::Arc;

fn route_start(points: &[(f64, f64)], speed: f64) -> String {
    json!({"version":1,"op":"start_route","scope":{"mode":"apps","packages":["example.selected"]},
        "route":{"points":points.iter().map(|(lat, lon)| Position::new(*lat, *lon)).collect::<Vec<_>>(), "speed":speed}}).to_string()
}

fn static_start() -> String {
    json!({"version":1,"op":"start","config":{"position":Position::new(31.0, 121.0),
        "scope":{"mode":"apps","packages":["example.selected"]}}})
    .to_string()
}

fn record_point(latitude: f64, longitude: f64, seconds: f64) -> String {
    json!({"version":1,"op":"record_point","position":Position::new(latitude, longitude),"seconds":seconds}).to_string()
}

fn send(control: &mut Control, line: String) -> Response {
    control.handle(&line)
}

#[test]
fn recording_collects_points_and_hands_them_over_for_replay() {
    let mut control = Control::default();
    assert!(control.handle(r#"{"version":1,"op":"record_start"}"#).ok);
    assert!(send(&mut control, record_point(31.0, 121.0, 0.0)).ok);
    assert!(send(&mut control, record_point(31.0, 121.0, 0.5)).ok);
    assert!(send(&mut control, record_point(31.0, 121.0, 1.0)).ok);
    assert!(send(&mut control, record_point(31.0005, 121.0, 1.5)).ok);

    let progress = control.handle(r#"{"version":1,"op":"status"}"#).state.recording.unwrap();
    assert_eq!(progress.points, 2);
    assert_eq!(progress.skipped, 2);
    assert!(!progress.full);
    assert!((progress.seconds - 1.5).abs() < 1e-9);
    assert!(!control.handle(r#"{"version":1,"op":"status"}"#).state.requested_active);

    let response = control.handle(r#"{"version":1,"op":"record_stop"}"#);
    assert!(response.ok);
    let track = response.state.recorded.expect("the recording must be handed back to the panel");
    assert_eq!(track.points.len(), 2);
    assert!(response.state.recording.is_none());
    let taken = control.handle(r#"{"version":1,"op":"record_take"}"#);
    assert!(taken.ok);
    assert!(
        taken.state.recorded.is_none(),
        "the same take must not appear again after it is taken"
    );
    let again = control.handle(r#"{"version":1,"op":"record_take"}"#);
    assert!(!again.ok);
    assert!(again.error.unwrap().contains("no recorded route"));
    let route = Route { points: track.points, speed: 5.0, repeat_count: 1, repeat_delay: 0.0 };
    assert!(Playback::new(route, Instant::now()).is_ok());
}

#[test]
fn recording_and_simulation_refuse_to_run_together() {
    let mut control = Control::default();
    assert!(send(&mut control, static_start()).ok);
    let refused = control.handle(r#"{"version":1,"op":"record_start"}"#);
    assert!(!refused.ok);
    assert!(refused.error.unwrap().contains("stop the simulation"));
    assert!(refused.state.recording.is_none());

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
    assert!(response.state.recording.is_none());
    assert!(response.state.recorded.is_none());
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
    let paused =
        control.handle_at(r#"{"version":1,"op":"pause_route"}"#, now + Duration::from_secs(12));
    assert!(paused.ok);
    let paused = serde_json::to_value(paused).unwrap();
    assert_eq!(paused["state"]["route"]["completed"], false);
    assert_eq!(paused["state"]["route"]["lap"], 1);
    assert_eq!(paused["state"]["config"]["position"]["speed"], 0.0);
    let later = serde_json::to_value(
        control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(100)),
    )
    .unwrap();
    assert_eq!(paused["state"]["route"], later["state"]["route"]);
    assert!(
        control
            .handle_at(r#"{"version":1,"op":"resume_route"}"#, now + Duration::from_secs(100))
            .ok
    );
    let moving = serde_json::to_value(
        control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(110)),
    )
    .unwrap();
    assert_eq!(moving["state"]["route"]["lap"], 2);
    assert_eq!(moving["state"]["config"]["position"]["speed"], 10.0);
    let waiting = serde_json::to_value(
        control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(121)),
    )
    .unwrap();
    assert!(waiting["state"]["route"]["waiting_seconds"].as_f64().unwrap() > 0.0);
    let stopped = control.handle_at(r#"{"version":1,"op":"stop"}"#, now + Duration::from_secs(121));
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
    let second = serde_json::to_value(
        control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(24)),
    )
    .unwrap();
    assert_eq!(second["state"]["route"]["lap"], 3);
    assert_eq!(second["state"]["route"]["completed"], false);
    let end = serde_json::to_value(
        control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(34)),
    )
    .unwrap();
    assert_eq!(end["state"]["route"]["completed"], true);
    assert_eq!(end["state"]["route"]["lap"], 3);
    assert_eq!(end["state"]["route"]["waiting_seconds"], 0.0);
    assert_eq!(end["state"]["config"]["position"]["longitude"], 0.001);
    assert_eq!(end["state"]["config"]["position"]["speed"], 0.0);
    let later = serde_json::to_value(
        control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(86400)),
    )
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
    let later = control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(30));
    assert!(later.state.requested_active);
    let config = later.state.config.unwrap();
    assert_eq!(config.scope, Scope::apps(["example.selected"]));
    assert_eq!(config.position.speed, 0.0);
    assert!((config.position.longitude - 0.00017986).abs() < 0.0000001);
    control.handle_at(r#"{"version":1,"op":"stop"}"#, now + Duration::from_secs(30));
    assert!(!control.handle_at(drive, now + Duration::from_secs(31)).ok);
    assert!(
        control
            .handle_at(&route_start(&[(0.0, 0.0), (0.0, 1.0)], 10.0), now + Duration::from_secs(31))
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
    let later = control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(30));
    assert_eq!(released.state.config, later.state.config);
    control.handle_at(drive, now + Duration::from_secs(30));
    let update = json!({"version":1,"op":"update","position":Position::new(1.0,2.0)}).to_string();
    assert!(control.handle_at(&update, now + Duration::from_secs(31)).ok);
    let later = control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(40));
    assert_eq!(later.state.config.unwrap().position, Position::new(1.0, 2.0));
}

#[test]
fn route_advances_without_panel_requests_and_holds_the_destination() {
    let mut control = Control::default();
    let start = Instant::now();
    let response = control.handle_at(&route_start(&[(0.0, 0.0), (0.0, 0.001)], 10.0), start);
    assert!(response.ok, "{:?}", response.error);
    let middle =
        control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(5));
    let point = middle.state.config.unwrap().position;
    assert!((point.longitude - 0.00044966).abs() < 0.000001);
    assert!((point.bearing - 90.0).abs() < 0.001);
    assert_eq!(point.speed, 10.0);
    let end = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(20));
    assert!(end.state.requested_active);
    assert_eq!(end.state.config.unwrap().position.longitude, 0.001);
    let end_json = serde_json::to_value(
        control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(30)),
    )
    .unwrap();
    assert_eq!(end_json["state"]["route"]["completed"], true);
    assert_eq!(end_json["state"]["config"]["position"]["speed"], 0.0);
}

#[test]
fn route_pause_resume_and_stop_keep_the_same_application_scope() {
    let mut control = Control::default();
    let start = Instant::now();
    assert!(control.handle_at(&route_start(&[(0.0, 0.0), (0.0, 0.01)], 10.0), start).ok);
    let paused =
        control.handle_at(r#"{"version":1,"op":"pause_route"}"#, start + Duration::from_secs(3));
    assert!(paused.ok);
    let position = paused.state.config.unwrap().position;
    assert_eq!(position.speed, 0.0);
    let later =
        control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(30));
    assert_eq!(later.state.config.unwrap().position, position);
    assert!(
        control
            .handle_at(r#"{"version":1,"op":"resume_route"}"#, start + Duration::from_secs(30))
            .ok
    );
    let moving =
        control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(32));
    let config = moving.state.config.unwrap();
    assert!((config.position.longitude - 0.00044966).abs() < 0.000001);
    assert_eq!(config.scope, crate::Scope::apps(["example.selected"]));
    let stopped =
        control.handle_at(r#"{"version":1,"op":"stop"}"#, start + Duration::from_secs(33));
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

/// Heartbeat readiness reports installation; call counts report actual callback activity.
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
    // Older bridges omit the diagnostic string.
    control.handle(r#"{"version":1,"op":"hook_status","installed":true,"gnss_raw":true}"#);
    let state = serde_json::to_value(control.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
    assert_eq!(state["state"]["gnss_raw_hook_ready"], true);
    assert_eq!(state["state"]["gnss_raw_detail"], serde_json::Value::Null);

    // Forward bridge diagnostics without interpreting their format.
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
    assert_eq!(initial["state"]["gnss"]["gnss_enabled"], false);
    assert_eq!(initial["state"]["gnss"]["nmea_enabled"], false);
    let enabled = control.handle(
        r#"{"version":1,"op":"set_gnss","config":{"gnss_enabled":true,"nmea_enabled":true}}"#,
    );
    assert!(enabled.ok, "{:?}", enabled.error);
    let state = serde_json::to_value(enabled).unwrap();
    assert_eq!(state["state"]["gnss"]["gnss_enabled"], true);
    assert_eq!(state["state"]["gnss"]["nmea_enabled"], true);
    assert!(control.handle(r#"{"version":1,"op":"set_gnss","config":{"gnss_enabled":false}}"#).ok);
    assert!(!control.handle(r#"{"version":1,"op":"set_gnss","config":{"gnss":true}}"#).ok);
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
    // Older configuration files omit the GNSS section.
    std::fs::write(&path, r#"{"version":3,"config":null,"cell_region":null,"telephony":{"cells_enabled":false,"sim_enabled":false,"radius_m":500.0,"subscriptions":[]}}"#).unwrap();
    let mut legacy = Control::open(&path).unwrap();
    let state = serde_json::to_value(legacy.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
    assert_eq!(state["state"]["gnss"]["gnss_enabled"], false);
    let _ = std::fs::remove_dir_all(&directory);
}

/// Apply properties before constructing the response, without rewriting unchanged state.
#[test]
fn operator_properties_follow_the_session_state() {
    use crate::operators::{Properties, tests::Recording};
    let directory =
        std::env::temp_dir().join(format!("justlocation-operators-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let probe = Arc::new(Recording::default());
    probe.set(crate::operators::NETWORK_ALPHA, "中国电信").unwrap();
    let mut control = Control::open_with(
        directory.join("state.json"),
        Some(Box::new(SharedProbe(probe.clone()))),
    )
    .unwrap();

    let state = control.handle(r#"{"version":1,"op":"set_telephony","config":{"cells_enabled":false,"sim_enabled":true,"radius_m":500,"subscriptions":[{"id":1,"slot":0,"mcc":"460","mnc":"11","country":"cn","carrier":"中国联通","enabled":true}]}}"#);
    assert!(state.ok);
    assert_eq!(state.state.operator_hook_ready, false);
    assert_eq!(probe.value(crate::operators::NETWORK_ALPHA).unwrap(), "中国电信");

    let started = control.handle(
        r#"{"version":1,"op":"start","config":{"position":{"latitude":31.23,"longitude":121.47,"altitude":0,"accuracy":5,"speed":0,"bearing":0},"scope":{"mode":"all"}}}"#,
    );
    assert!(started.ok);
    assert_eq!(
        started.state.operator_hook_ready, true,
        "the properties must be taken over on the first tick"
    );
    assert_eq!(probe.value(crate::operators::NETWORK_ALPHA).unwrap(), "中国联通");

    let writes = probe.write_count();
    let status = control.handle(r#"{"version":1,"op":"status"}"#);
    assert_eq!(status.state.operator_hook_ready, true);
    assert_eq!(probe.write_count(), writes, "unchanged state must not write the properties again");

    let stopped = control.handle(r#"{"version":1,"op":"stop"}"#);
    assert_eq!(stopped.state.operator_hook_ready, false);
    assert_eq!(
        probe.value(crate::operators::NETWORK_ALPHA).unwrap(),
        "中国电信",
        "the real values must be restored after stopping"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

struct SharedProbe(Arc<crate::operators::tests::Recording>);

impl crate::operators::Properties for SharedProbe {
    fn get(&self, name: &str) -> Option<String> {
        self.0.get(name)
    }
    fn set(&self, name: &str, value: &str) -> io::Result<()> {
        self.0.set(name, value)
    }
}

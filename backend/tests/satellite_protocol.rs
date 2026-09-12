//! Protocol coverage for satellite installation flags, saved switches and persistence.

use justlocation_backend::protocol::Control;
use serde_json::{Value, json};

fn call(control: &mut Control, request: Value) -> Value {
    serde_json::to_value(control.handle(&request.to_string())).unwrap()
}
fn switches(gnss: bool, nmea: bool) -> Value {
    json!({"version":1,"op":"set_gnss","config":{"gnss_enabled":gnss,"nmea_enabled":nmea}})
}
fn status() -> Value {
    json!({"version":1,"op":"status"})
}
/// Location, GNSS and NMEA installation flags are independent.
fn heartbeat(installed: bool, gnss: bool, nmea: bool) -> Value {
    json!({"version":1,"op":"hook_status","installed":installed,"gnss":gnss,"nmea":nmea})
}

#[test]
fn satellite_readiness_follows_the_bridge_heartbeat() {
    let mut control = Control::default();
    let initial = call(&mut control, status());
    assert_eq!(initial["state"]["gnss_hook_ready"], false);
    assert_eq!(initial["state"]["nmea_hook_ready"], false);
    let reported = call(&mut control, heartbeat(true, true, false));
    assert_eq!(reported["state"]["gnss_hook_ready"], true);
    assert_eq!(reported["state"]["nmea_hook_ready"], false);
    let both = call(&mut control, heartbeat(true, false, true));
    assert_eq!(both["state"]["gnss_hook_ready"], false);
    assert_eq!(both["state"]["nmea_hook_ready"], true);
}

#[test]
fn satellite_readiness_follows_the_bridge_not_the_location_hook() {
    let mut control = Control::default();
    call(&mut control, heartbeat(true, true, true));
    let gone = call(&mut control, heartbeat(false, true, true));
    // Each channel uses its own installation flag plus bridge connectivity.
    assert_eq!(gone["state"]["location_hook_ready"], false);
    assert_eq!(gone["state"]["gnss_hook_ready"], true);
    assert_eq!(gone["state"]["nmea_hook_ready"], true);
    assert_eq!(call(&mut control, status())["state"]["gnss_hook_ready"], true);
    let cleared = call(&mut control, heartbeat(false, false, false));
    assert_eq!(cleared["state"]["gnss_hook_ready"], false);
    assert_eq!(cleared["state"]["nmea_hook_ready"], false);
}

#[test]
fn the_switches_are_independent_of_whether_the_hook_is_ready() {
    let mut control = Control::default();
    let saved = call(&mut control, switches(true, true));
    assert_eq!(saved["ok"], true);
    assert_eq!(saved["state"]["gnss"]["gnss_enabled"], true);
    assert_eq!(saved["state"]["gnss"]["nmea_enabled"], true);
    assert_eq!(saved["state"]["gnss_hook_ready"], false);
    let partial = call(&mut control, switches(false, true));
    assert_eq!(partial["state"]["gnss"]["gnss_enabled"], false);
    assert_eq!(partial["state"]["gnss"]["nmea_enabled"], true);
}

#[test]
fn a_bad_satellite_payload_is_rejected_without_changing_the_saved_switches() {
    let mut control = Control::default();
    call(&mut control, switches(true, false));
    for bad in [
        json!({"version":1,"op":"set_gnss","config":{"gnss":true}}),
        json!({"version":1,"op":"set_gnss","config":{"gnss_enabled":"yes"}}),
        json!({"version":1,"op":"set_gnss"}),
    ] {
        let rejected = call(&mut control, bad.clone());
        assert_eq!(rejected["ok"], false, "{bad} 应该被拒绝");
        assert_eq!(rejected["state"]["gnss"]["gnss_enabled"], true);
        assert_eq!(rejected["state"]["gnss"]["nmea_enabled"], false);
    }
}

#[test]
fn the_switches_survive_a_restart_and_a_failed_write_rolls_them_back() {
    let path =
        std::env::temp_dir().join(format!("justlocation-satellite-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut control = Control::open(&path).unwrap();
    assert_eq!(call(&mut control, switches(true, true))["ok"], true);
    let mut restored = Control::open(&path).unwrap();
    let state = call(&mut restored, status());
    assert_eq!(state["state"]["gnss"]["gnss_enabled"], true);
    assert_eq!(state["state"]["gnss"]["nmea_enabled"], true);
    assert_eq!(state["state"]["requested_active"], false);
    std::fs::remove_file(&path).unwrap();
    // A failed save must roll back the complete configuration change.
    let mut missing = Control::open(path.join("missing").join("state.json")).unwrap();
    let failed = call(&mut missing, switches(true, true));
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["state"]["gnss"]["gnss_enabled"], false);
    assert_eq!(failed["state"]["gnss"]["nmea_enabled"], false);
}

#[test]
fn stopping_the_simulation_keeps_the_satellite_switches() {
    let mut control = Control::default();
    call(&mut control, switches(true, true));
    call(
        &mut control,
        json!({"version":1,"op":"start","config":{"position":justlocation_backend::Position::new(0.,0.),"scope":{"mode":"all"}}}),
    );
    let stopped = call(&mut control, json!({"version":1,"op":"stop"}));
    // Stopping a session preserves saved channel switches.
    assert_eq!(stopped["state"]["requested_active"], false);
    assert_eq!(stopped["state"]["gnss"]["gnss_enabled"], true);
    assert_eq!(stopped["state"]["gnss"]["nmea_enabled"], true);
}

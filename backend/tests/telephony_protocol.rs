use justlocation_backend::protocol::Control;
use serde_json::{Value, json};

fn call(control: &mut Control, request: Value) -> Value {
    serde_json::to_value(control.handle(&request.to_string())).unwrap()
}
fn settings() -> Value {
    json!({"cells_enabled":true,"sim_enabled":true,"radius_m":500.0,
    "subscriptions":[{"id":7,"slot":0,"mcc":"460","mnc":"01","country":"cn","carrier":"Test","enabled":true}]})
}
fn configure() -> Value {
    json!({"version":1,"op":"set_telephony","config":settings()})
}
fn start() -> Value {
    json!({"version":1,"op":"start","config":{"position":justlocation_backend::Position::new(0.,0.),"scope":{"mode":"apps","packages":["example.selected"]}}})
}

#[test]
fn configuration_is_inactive_until_start_and_stop_removes_the_entire_output() {
    let mut control = Control::default();
    let saved = call(&mut control, configure());
    assert_eq!(saved["ok"], true);
    assert!(saved["state"]["telephony_output"].is_null());
    let started = call(&mut control, start());
    assert_eq!(
        started["state"]["telephony_output"]["availability"],
        "missing_region"
    );
    assert_eq!(
        started["state"]["telephony_output"]["subscriptions"][0]["id"],
        7
    );
    let mut bad = configure();
    bad["config"]["subscriptions"][0]["mnc"] = json!("1");
    let rejected = call(&mut control, bad);
    assert_eq!(rejected["ok"], false);
    assert_eq!(rejected["state"]["telephony"], settings());
    let stop = call(&mut control, json!({"version":1,"op":"stop"}));
    assert!(stop["state"]["telephony_output"].is_null());
    assert_eq!(stop["state"]["telephony"], settings());
}

#[test]
fn separate_phone_heartbeat_does_not_erase_location_readiness() {
    let mut control = Control::default();
    call(
        &mut control,
        json!({"version":1,"op":"hook_status","installed":true,"gnss":true,"nmea":true}),
    );
    let phone = call(
        &mut control,
        json!({"version":1,"op":"telephony_hook_status","cells":true,"sim":false}),
    );
    assert_eq!(phone["ok"], true);
    assert_eq!(phone["state"]["location_hook_ready"], true);
    assert_eq!(phone["state"]["cell_query_hook_ready"], true);
    assert_eq!(phone["state"]["cell_callback_hook_ready"], false);
    assert_eq!(phone["state"]["cell_hook_ready"], false);
    assert_eq!(phone["state"]["sim_hook_ready"], false);
    let callbacks = call(&mut control, json!({"version":1,"op":"hook_status","installed":true,"cell_callbacks":true}));
    assert_eq!(callbacks["state"]["cell_hook_ready"], true);
    let location = call(
        &mut control,
        json!({"version":1,"op":"hook_status","installed":false}),
    );
    assert_eq!(location["state"]["cell_hook_ready"], false);
    assert_eq!(location["state"]["cell_query_hook_ready"], true);
    assert_eq!(location["state"]["location_hook_ready"], false);
}

#[test]
fn saved_telephony_reopens_stopped_and_failed_writes_roll_back() {
    let path = std::env::temp_dir().join(format!(
        "justlocation-telephony-{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut control = Control::open(&path).unwrap();
    assert_eq!(call(&mut control, configure())["ok"], true);
    assert_eq!(call(&mut control, start())["ok"], true);
    let mut restored = Control::open(&path).unwrap();
    let state = call(&mut restored, json!({"version":1,"op":"status"}));
    assert_eq!(state["state"]["telephony"], settings());
    assert!(state["state"]["telephony_output"].is_null());
    std::fs::remove_file(&path).unwrap();
    let mut missing = Control::open(path.join("missing.json")).unwrap();
    let response = call(&mut missing, configure());
    assert_eq!(response["ok"], false);
    assert_eq!(response["state"]["telephony"]["cells_enabled"], false);
}

#[test]
fn phone_reports_current_cards_without_persisting_or_accepting_private_identifiers() {
    let mut control = Control::default();
    let cards = json!([{"id":7,"slot":0,"mcc":"460","mnc":"001","country":"cn","carrier":"Test"}]);
    let report = json!({"version":1,"op":"telephony_hook_status","cells":true,"sim":true,"subscriptions":cards});
    let state = call(&mut control, report.clone());
    assert_eq!(state["state"]["phone_connected"], true);
    assert_eq!(state["state"]["detected_subscriptions"], cards);
    assert_eq!(state["state"]["telephony"]["subscriptions"], json!([]));
    let mut invalid = report.clone();
    invalid["subscriptions"][0]["iccid"] = json!("not collected");
    assert_eq!(call(&mut control, invalid)["ok"], false);
    let mut invalid = report.clone();
    invalid["subscriptions"][0]["slot"] = json!(2);
    assert_eq!(call(&mut control, invalid)["ok"], false);
    let mut duplicate = report.clone();
    duplicate["subscriptions"] = json!([cards[0],cards[0]]);
    assert_eq!(call(&mut control, duplicate)["ok"], false);
    // A transient read failure is different from a phone with no inserted cards.
    let no_read = call(&mut control, json!({"version":1,"op":"telephony_hook_status","cells":true,"sim":true,"subscriptions":null}));
    assert!(no_read["state"]["detected_subscriptions"].is_null());
    let empty = call(&mut control, json!({"version":1,"op":"telephony_hook_status","cells":true,"sim":true,"subscriptions":[]}));
    assert_eq!(empty["state"]["detected_subscriptions"], json!([]));
}

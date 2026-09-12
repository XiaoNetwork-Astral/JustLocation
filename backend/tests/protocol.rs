use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn start() -> Value {
    json!({"version": 1, "op": "start", "config": {
        "position": {"latitude": 31.2, "longitude": 121.5, "altitude": 0.0,
            "accuracy": 5.0, "speed": 0.0, "bearing": 0.0},
        "scope": {"mode": "apps", "packages": ["example.selected"]}
    }})
}

fn exchange(lines: &[String]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_justlocationd"))
        .arg("stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for line in lines {
        writeln!(input, "{line}").unwrap();
    }
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn process_shares_state_between_requests_and_stop_disables_it() {
    let replies = exchange(&[
        start().to_string(),
        json!({"version": 1, "op": "status"}).to_string(),
        json!({"version": 1, "op": "stop"}).to_string(),
    ]);
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["ok"], true);
    assert_eq!(replies[1]["state"]["requested_active"], true);
    assert_eq!(replies[1]["state"]["config"]["scope"]["packages"][0], "example.selected");
    assert_eq!(replies[2]["state"]["requested_active"], false);
}

#[test]
fn unsupported_protocol_and_malformed_json_do_not_change_state() {
    let replies = exchange(&[
        start().to_string(),
        "not json".into(),
        json!({"version": 99, "op": "stop"}).to_string(),
        json!({"version": 1, "op": "status"}).to_string(),
    ]);
    assert_eq!(replies[1]["ok"], false);
    assert_eq!(replies[2]["ok"], false);
    assert_eq!(replies[3]["state"]["requested_active"], true);
}

#[test]
fn invalid_update_returns_error_with_unchanged_position() {
    let replies = exchange(&[
        start().to_string(),
        json!({"version": 1, "op": "update",
        "position": {"latitude": 100, "longitude": 0, "altitude": 0,
            "accuracy": 5, "speed": 0, "bearing": 0}})
        .to_string(),
    ]);
    assert_eq!(replies[1]["ok"], false);
    assert_eq!(replies[1]["state"]["config"]["position"]["latitude"], 31.2);
}

#[test]
fn empty_or_unknown_scope_is_not_treated_as_all_applications() {
    let mut empty = start();
    empty["config"]["scope"]["packages"] = json!([]);
    let mut unknown = start();
    unknown["config"]["scope"]["mode"] = json!("unknown");
    let replies = exchange(&[empty.to_string(), unknown.to_string()]);
    assert!(replies.iter().all(|reply| reply["ok"] == false));
    assert!(replies.iter().all(|reply| reply["state"]["requested_active"] == false));
}

#[test]
fn shutdown_stops_output_and_closes_the_control_process() {
    let replies =
        exchange(&[start().to_string(), json!({"version":1,"op":"shutdown"}).to_string()]);
    assert_eq!(replies[1]["ok"], true);
    assert_eq!(replies[1]["state"]["requested_active"], false);
}

#[test]
fn system_hook_heartbeat_reports_readiness_without_starting_output() {
    let replies = exchange(&[
        json!({"version":1,"op":"status"}).to_string(),
        json!({"version":1,"op":"hook_status","installed":true}).to_string(),
        json!({"version":1,"op":"hook_status","installed":false}).to_string(),
    ]);
    assert_eq!(replies[0]["state"]["hook_connected"], false);
    assert_eq!(replies[1]["state"]["location_hook_ready"], true);
    assert_eq!(replies[1]["state"]["requested_active"], false);
    assert_eq!(replies[2]["state"]["location_hook_ready"], false);
}

#[test]
fn gnss_and_nmea_readiness_are_independent_and_older_bridges_report_them_unavailable() {
    let replies = exchange(&[
        json!({"version":1,"op":"hook_status","installed":true,"gnss":true,"nmea":false})
            .to_string(),
        json!({"version":1,"op":"hook_status","installed":true,"gnss":false,"nmea":true})
            .to_string(),
        json!({"version":1,"op":"hook_status","installed":true}).to_string(),
    ]);
    assert_eq!(replies[0]["ok"], true);
    assert_eq!(replies[0]["state"]["gnss_hook_ready"], true);
    assert_eq!(replies[0]["state"]["nmea_hook_ready"], false);
    assert_eq!(replies[1]["state"]["gnss_hook_ready"], false);
    assert_eq!(replies[1]["state"]["nmea_hook_ready"], true);
    assert_eq!(replies[2]["state"]["gnss_hook_ready"], false);
    assert_eq!(replies[2]["state"]["nmea_hook_ready"], false);
}

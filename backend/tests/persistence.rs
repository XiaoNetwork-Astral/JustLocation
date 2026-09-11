use justlocation_backend::protocol::Control;
use serde_json::json;
use std::path::PathBuf;

fn file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("justlocation-{}-{name}.json", std::process::id()))
}

fn start() -> String {
    json!({"version":1,"op":"start","config":{
        "position":{"latitude":0,"longitude":0,"altitude":0,"accuracy":5,"speed":0,"bearing":0},
        "scope":{"mode":"apps","packages":["example.selected"]}
    }})
    .to_string()
}

#[test]
fn restart_restores_configuration_but_does_not_resume_output() {
    let path = file("restart");
    let _ = std::fs::remove_file(&path);
    let mut first = Control::open(&path).unwrap();
    assert!(first.handle(&start()).ok);
    drop(first);
    let mut next = Control::open(&path).unwrap();
    let state = next.handle(r#"{"version":1,"op":"status"}"#).state;
    assert!(!state.requested_active);
    assert_eq!(state.config.unwrap().position.latitude, 0.0);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn failed_save_leaves_session_unchanged() {
    let path = file("missing-parent").join("config.json");
    let mut control = Control::open(&path).unwrap();
    let response = control.handle(&start());
    assert!(!response.ok);
    assert!(!response.state.requested_active);
    assert!(response.state.config.is_none());
}

#[test]
fn corrupt_configuration_is_reported_without_overwriting_it() {
    let path = file("corrupt");
    std::fs::write(&path, "broken").unwrap();
    assert!(Control::open(&path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn route_save_failure_rolls_back_and_restart_does_not_resume_movement() {
    let request = json!({"version":1,"op":"start_route","scope":{"mode":"apps","packages":["example.selected"]},
        "route":{"speed":1.5,"points":[justlocation_backend::Position::new(0.0,0.0),justlocation_backend::Position::new(0.0,0.001)]}}).to_string();
    let mut missing = Control::open(file("route-parent").join("config.json")).unwrap();
    let reply = missing.handle(&request);
    assert!(!reply.ok);
    assert!(!reply.state.requested_active);
    assert!(reply.state.route.is_none());
    assert!(reply.state.config.is_none());
    let path = file("route-restart");
    let _ = std::fs::remove_file(&path);
    let mut control = Control::open(&path).unwrap();
    assert!(control.handle(&request).ok);
    drop(control);
    let mut restored = Control::open(&path).unwrap();
    let reply = restored.handle(r#"{"version":1,"op":"status"}"#);
    assert!(!reply.state.requested_active);
    assert!(reply.state.route.is_none());
    assert_eq!(reply.state.config.unwrap().position.longitude, 0.0);
    std::fs::remove_file(path).unwrap();
}

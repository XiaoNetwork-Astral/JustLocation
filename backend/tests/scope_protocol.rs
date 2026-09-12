use justlocation_backend::{Position, Scope, protocol::Control};
use serde_json::json;
use std::fs;

#[test]
fn scope_changes_preserve_playback_persist_and_roll_back_failed_saves() {
    let directory = std::env::temp_dir().join(format!("justlocation-scope-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.json");
    let mut control = Control::open(&path).unwrap();
    let set = |packages: Vec<&str>| {
        json!({"version":1,"op":"set_scope","scope":{"mode":"apps","packages":packages}})
            .to_string()
    };
    let state = control.handle(&set(vec!["example.first"]));
    assert!(state.ok);
    assert!(!state.state.requested_active);
    let start = json!({"version":1,"op":"start_route","scope":{"mode":"apps","packages":["example.first"]},"route":{"points":[Position::new(31.,121.),Position::new(31.01,121.)],"speed":1}});
    assert!(control.handle(&start.to_string()).ok);
    assert!(control.handle(r#"{"version":1,"op":"pause_route"}"#).ok);
    let state = control.handle(&set(vec!["example.second"]));
    assert!(state.ok);
    assert!(state.state.requested_active);
    assert!(state.state.route.unwrap().paused);
    assert_eq!(state.state.config.unwrap().scope, Scope::apps(["example.second"]));
    assert!(!control.handle(&set(vec![])).ok);
    let mut reopened = Control::open(&path).unwrap();
    let state = reopened.handle(r#"{"version":1,"op":"status"}"#);
    assert_eq!(state.state.config.unwrap().scope, Scope::apps(["example.second"]));
    assert!(!state.state.requested_active);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let failure = control.handle(&set(vec!["example.third"]));
    assert!(!failure.ok);
    assert!(failure.state.route.unwrap().paused);
    assert_eq!(failure.state.config.unwrap().scope, Scope::apps(["example.second"]));
    fs::remove_dir(path).unwrap();
    for entry in fs::read_dir(&directory).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(directory).unwrap();
}

use justlocation_backend::protocol::Control;
use serde_json::json;

#[test]
fn old_files_default_off_new_settings_persist_and_failed_writes_roll_back() {
    let directory = std::env::temp_dir().join(format!("jl-realism-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.json");
    std::fs::write(&path, r#"{"version":3,"config":null,"cell_region":null}"#).unwrap();
    let mut control = Control::open(&path).unwrap();
    assert!(!control.handle(r#"{"version":1,"op":"status"}"#).state.realism.enabled);
    let request =
        json!({"version":1,"op":"set_realism","config":{"enabled":true,"seed":17}}).to_string();
    assert!(control.handle(&request).ok);
    let reopened = Control::open(&path).unwrap().handle(r#"{"version":1,"op":"status"}"#);
    assert!(reopened.state.realism.enabled);
    assert_eq!(reopened.state.realism.seed, Some(17));
    assert!(!reopened.state.requested_active);
    assert!(
        !control.handle(r#"{"version":1,"op":"set_realism","config":{"speed_variation":9}}"#).ok
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let failure = control.handle(r#"{"version":1,"op":"set_realism","config":{"enabled":false}}"#);
    assert!(!failure.ok);
    assert!(failure.state.realism.enabled);
    std::fs::remove_dir_all(directory).unwrap();
}

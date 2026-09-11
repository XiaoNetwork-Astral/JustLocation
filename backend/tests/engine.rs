use justlocation_backend::{Config, Engine, Position, Scope};

fn config(latitude: f64, longitude: f64, scope: Scope) -> Config {
    Config {
        position: Position::new(latitude, longitude),
        scope,
    }
}

#[test]
fn zero_coordinates_are_valid_and_are_not_missing_data() {
    let mut engine = Engine::default();
    engine.start(config(0.0, 0.0, Scope::All)).unwrap();
    assert_eq!(engine.output_for("example.app").unwrap().latitude, 0.0);
}

#[test]
fn selected_apps_receive_output_and_other_apps_do_not() {
    let mut engine = Engine::default();
    engine
        .start(config(31.2, 121.5, Scope::apps(["example.selected"])))
        .unwrap();
    assert!(engine.output_for("example.selected").is_some());
    assert!(engine.output_for("example.other").is_none());
}

#[test]
fn empty_app_selection_never_becomes_global_output() {
    let mut engine = Engine::default();
    assert!(engine.start(config(31.2, 121.5, Scope::apps([]))).is_err());
    assert!(engine.output_for("example.app").is_none());
}

#[test]
fn invalid_coordinates_are_rejected_before_starting() {
    for (latitude, longitude) in [
        (90.1, 0.0),
        (-90.1, 0.0),
        (0.0, 180.1),
        (0.0, -180.1),
        (f64::NAN, 0.0),
        (0.0, f64::INFINITY),
    ] {
        let mut engine = Engine::default();
        assert!(
            engine
                .start(config(latitude, longitude, Scope::All))
                .is_err()
        );
        assert!(!engine.is_running());
    }
}

#[test]
fn poles_and_date_line_are_valid_coordinates() {
    for (latitude, longitude) in [(90.0, 180.0), (-90.0, -180.0)] {
        let mut engine = Engine::default();
        engine
            .start(config(latitude, longitude, Scope::All))
            .unwrap();
        assert_eq!(
            engine.output_for("example.app").unwrap().longitude,
            longitude
        );
    }
}

#[test]
fn rejected_update_preserves_the_previous_output() {
    let mut engine = Engine::default();
    engine.start(config(31.2, 121.5, Scope::All)).unwrap();
    let before = engine.output_for("example.app").cloned();
    assert!(
        engine
            .update_position(Position::new(f64::NAN, 42.0))
            .is_err()
    );
    assert_eq!(engine.output_for("example.app").cloned(), before);
}

#[test]
fn update_changes_position_without_expanding_scope() {
    let mut engine = Engine::default();
    engine
        .start(config(31.2, 121.5, Scope::apps(["example.selected"])))
        .unwrap();
    engine.update_position(Position::new(30.0, 120.0)).unwrap();
    assert_eq!(
        engine.output_for("example.selected").unwrap().latitude,
        30.0
    );
    assert!(engine.output_for("example.other").is_none());
}

#[test]
fn stop_is_idempotent_and_editing_does_not_restart_output() {
    let mut engine = Engine::default();
    engine.start(config(31.2, 121.5, Scope::All)).unwrap();
    engine.stop();
    engine.stop();
    engine.update_position(Position::new(30.0, 120.0)).unwrap();
    assert!(!engine.is_running());
    assert!(engine.output_for("example.app").is_none());
}

#[test]
fn starting_an_active_session_does_not_replace_it() {
    let mut engine = Engine::default();
    engine
        .start(config(31.2, 121.5, Scope::apps(["example.selected"])))
        .unwrap();
    assert!(engine.start(config(0.0, 0.0, Scope::All)).is_err());
    assert_eq!(
        engine.output_for("example.selected").unwrap().latitude,
        31.2
    );
    assert!(engine.output_for("example.other").is_none());
}

#[test]
fn negative_accuracy_and_speed_are_rejected() {
    for field in ["accuracy", "speed"] {
        let mut engine = Engine::default();
        let mut bad = config(31.2, 121.5, Scope::All);
        if field == "accuracy" {
            bad.position.accuracy = -1.0;
        } else {
            bad.position.speed = -1.0;
        }
        assert!(engine.start(bad).is_err());
        assert!(!engine.is_running());
    }
}

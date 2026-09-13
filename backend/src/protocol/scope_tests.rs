use super::*;
use crate::{
    Position, Scope,
    scope::{Feature, Scopes},
};
use serde_json::json;

fn set(control: &mut Control, feature: Option<Feature>, scope: Scope) -> Response {
    control
        .handle(&json!({"version":1,"op":"set_scope","feature":feature,"scope":scope}).to_string())
}

#[test]
fn feature_edits_and_source_switches_do_not_change_other_selections() {
    let mut control = Control::default();
    assert!(set(&mut control, Some(Feature::Wifi), Scope::apps(["example.wifi"])).ok);
    assert!(control.session.engine.config().is_none());
    assert!(control.handle(r#"{"version":1,"op":"stop"}"#).ok);
    assert!(control.session.engine.config().is_none());
    for (feature, package) in [
        (Feature::Position, "example.position"),
        (Feature::Route, "example.route"),
        (Feature::Wifi, "example.wifi"),
        (Feature::Sim, "example.sim"),
    ] {
        assert!(set(&mut control, Some(feature), Scope::apps([package])).ok);
    }
    let position = Scope::apps(["example.position"]);
    let route = Scope::apps(["example.route"]);
    let before = control.session.scopes.clone();
    assert!(
        control
            .handle(
                &json!({"version":1,"op":"start","config":{
        "position":Position::new(0.,0.),"scope":position}})
                .to_string()
            )
            .ok
    );
    assert!(control.handle(r#"{"version":1,"op":"drive","speed":1,"bearing":90}"#).ok);
    assert_eq!(control.session.scopes, before);
    assert!(control.handle(r#"{"version":1,"op":"stop"}"#).ok);
    assert!(
        control
            .handle(
                &json!({"version":1,"op":"start_route","scope":route,
        "route":{"points":[Position::new(0.,0.),Position::new(0.,1.)],"speed":1}})
                .to_string()
            )
            .ok
    );
    let state =
        set(&mut control, Some(Feature::Position), Scope::apps(["example.new-position"])).state;
    assert_eq!(state.config.unwrap().scope, route);
    let new_route = Scope::apps(["example.new-route"]);
    let state = set(&mut control, Some(Feature::Route), new_route.clone()).state;
    assert_eq!(state.config.unwrap().scope, new_route);
    for op in ["pause_route", "resume_route"] {
        let reply = control.handle(&json!({"version":1,"op":op}).to_string());
        assert!(reply.ok);
        assert_eq!(reply.state.config.unwrap().scope, new_route);
    }
    let stopped = control.handle(r#"{"version":1,"op":"stop"}"#).state;
    assert!(!stopped.requested_active);
    assert_eq!(stopped.config.unwrap().scope, stopped.scopes.position);
    assert_eq!(stopped.scopes.route, new_route);
    assert_eq!(stopped.scopes.wifi, before.wifi);
    assert_eq!(stopped.scopes.sim, before.sim);
    assert!(set(&mut control, None, position.clone()).ok);
    assert_eq!(control.session.scopes, Scopes::shared(position));
}

#[test]
fn scope_migration_persistence_and_failed_save_are_atomic() {
    let dir = std::env::temp_dir().join(format!("justlocation-scopes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.json");
    let config = json!({"position":Position::new(1.,2.),"scope":Scope::apps(["example.legacy"])});
    for version in [None, Some(2), Some(3)] {
        let stored = match version {
            None => config.clone(),
            Some(version) => json!({"version":version,"config":config,"cell_region":null}),
        };
        std::fs::write(&path, stored.to_string()).unwrap();
        let control = Control::open(&path).unwrap();
        assert_eq!(control.session.scopes, Scopes::shared(Scope::apps(["example.legacy"])));
        assert!(!control.session.engine.is_running());
    }
    let mut control = Control::open(&path).unwrap();
    assert!(set(&mut control, Some(Feature::Wifi), Scope::apps(["example.wifi"])).ok);
    assert!(set(&mut control, Some(Feature::Route), Scope::apps(["example.route"])).ok);
    assert!(
        control
            .handle(
                &json!({"version":1,"op":"start_route","scope":control.session.scopes.route,
        "route":{"points":[Position::new(0.,0.),Position::new(0.,1.)],"speed":1}})
                .to_string()
            )
            .ok
    );
    let reopened = Control::open(&path).unwrap();
    assert_eq!(reopened.session.scopes, control.session.scopes);
    assert_eq!(reopened.session.engine.config().unwrap().scope, reopened.session.scopes.position);
    assert!(!reopened.session.engine.is_running());
    let before = control.session.scopes.clone();
    assert!(!set(&mut control, Some(Feature::Wifi), Scope::apps([""])).ok);
    assert_eq!(control.session.scopes, before);
    // A directory as the save destination forces atomic_save to fail on every host.
    control.storage = Some(dir.clone());
    assert!(!set(&mut control, Some(Feature::Route), Scope::All).ok);
    assert_eq!(control.session.scopes, before);
    assert_eq!(control.session.engine.config().unwrap().scope, before.route);
    let mut stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(stored["version"], 4);
    stored.as_object_mut().unwrap().remove("scopes");
    std::fs::write(&path, stored.to_string()).unwrap();
    assert!(Control::open(&path).is_err());
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

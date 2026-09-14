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

fn set_steps(linked: bool) -> String {
    json!({"version":1,"op":"set_steps","config":{"enabled":true,
        "cadence":2.0,"movement_linked":linked,"stride_m":0.75,"daily_reset":true}})
    .to_string()
}

#[test]
fn virtual_subscription_publications_follow_model_scope_and_phone_acknowledgements() {
    let mut control = Control::default();
    assert!(control.handle(r#"{"version":1,"op":"set_scope","feature":"sim","scope":{"mode":"apps","packages":["example.selected"]}}"#).ok);
    let config = json!({"cells_enabled":false,"sim_enabled":true,"subscriptions":[],
        "virtual_sim":{"subscriptions":[{"id":1900000001,"slot":1,"mcc":"460","mnc":"01","country":"cn","carrier":"Virtual","enabled":true}],"default_slot":1}});
    assert!(
        send(&mut control, json!({"version":1,"op":"set_telephony","config":config}).to_string())
            .ok
    );
    assert!(control.handle(&static_start()).ok);
    let heartbeat = json!({"version":1,"op":"telephony_hook_status","cells":true,"sim":true,
        "virtual_sim_queries":true,"active_modem_count":2,"subscriptions":[]});
    let state = send(&mut control, heartbeat.to_string()).state;
    let token = state.virtual_sim_version.unwrap();
    assert!(token.starts_with("jl-"));
    assert_eq!(state.telephony_output.unwrap().virtual_ids, vec![1900000001]);
    assert!(state.virtual_sim_query_hook_ready);
    assert!(state.virtual_sim_applied.is_none());
    let mut applied = heartbeat.clone();
    applied["virtual_sim_applied"] = json!(token);
    let state = send(&mut control, applied.to_string()).state;
    assert_eq!(state.virtual_sim_version.as_deref(), Some(token.as_str()));
    assert_eq!(state.virtual_sim_applied.as_deref(), Some(token.as_str()));
    let changed = control.handle(r#"{"version":1,"op":"set_scope","feature":"sim","scope":{"mode":"apps","packages":["example.changed"]}}"#).state;
    assert_ne!(changed.virtual_sim_version.as_deref(), Some(token.as_str()));
    assert_eq!(changed.virtual_sim_applied.as_deref(), Some(token.as_str()));
    let mut physical = heartbeat.clone();
    physical["subscriptions"] =
        json!([{"id":7,"slot":1,"mcc":"460","mnc":"11","country":"cn","carrier":"Real"}]);
    let state = send(&mut control, physical.to_string()).state;
    assert_eq!(state.virtual_sim_version.as_deref(), Some("off"));
    assert!(state.telephony_output.unwrap().virtual_ids.is_empty());
    let resumed = send(&mut control, heartbeat.to_string()).state.virtual_sim_version;
    assert_ne!(resumed.as_deref(), Some(token.as_str()));
    let stopped = control.handle(r#"{"version":1,"op":"stop"}"#).state;
    assert_eq!(stopped.virtual_sim_version.as_deref(), Some("off"));
    assert!(stopped.telephony_output.is_none());
}

#[test]
fn realism_drift_keeps_the_static_anchor_and_never_adds_linked_steps() {
    let now = Instant::now();
    let mut control = Control::default();
    let settings = r#"{"version":1,"op":"set_realism","config":{"enabled":true,"seed":7}}"#;
    assert!(control.handle_at(settings, now).ok);
    assert!(control.handle_at(&static_start(), now).ok);
    control.handle_at(&set_steps(true), now);
    let anchor = control.session.engine.config().unwrap().position.clone();
    let first = control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(5));
    let later =
        control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(1000));
    assert_ne!(first.state.config.unwrap().position, later.state.config.unwrap().position);
    assert_eq!(control.session.engine.config().unwrap().position, anchor);
    assert_eq!(later.state.step_count.total, 0);
    assert!(!control.handle_at(settings, now + Duration::from_secs(1000)).ok);
}

#[test]
fn planned_routes_keep_corners_and_never_drift_off_the_provider_geometry() {
    let now = Instant::now();
    let mut control = Control::default();
    assert!(control.handle_at(r#"{"version":1,"op":"set_realism","config":{"enabled":true,"seed":7,"drift_radius_m":100,"corner_radius_m":100}}"#, now).ok);
    let mut input: serde_json::Value =
        serde_json::from_str(&route_start(&[(0., 0.), (0., 0.001), (0.001, 0.001)], 1.4)).unwrap();
    input["route"]["geometry"] = json!({"provider":"amap","mode":"walking","distance_m":222,"duration_s":160,"segments":[{"first":0,"last":2,"distance_m":222,"duration_s":160,"road_name":"","instruction":"","attributes":{}}]});
    assert!(control.handle_at(&input.to_string(), now).ok);
    assert!(
        (control.session.route.as_ref().unwrap().state().total_distance - 222.390160467).abs()
            < 0.00001
    );
    for seconds in 0..240 {
        let output = control
            .handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(seconds))
            .state
            .config
            .unwrap()
            .position;
        let anchor = &control.session.engine.config().unwrap().position;
        assert_eq!(output.latitude, anchor.latitude);
        assert_eq!(output.longitude, anchor.longitude);
        assert!(output.latitude.abs() < 1e-12 || (output.longitude - 0.001).abs() < 1e-12);
    }
}

#[test]
fn speed_variation_integrates_joystick_distance_only_until_lease_expiry() {
    let now = Instant::now();
    let mut control = Control::default();
    control.handle_at(r#"{"version":1,"op":"set_realism","config":{"enabled":true,"seed":5,"drift_radius_m":0,"altitude_m":0,"bearing_degrees":0}}"#,now);
    control.handle_at(&static_start(), now);
    control.handle_at(&set_steps(true), now);
    let anchor = control.session.engine.config().unwrap().position.clone();
    control.handle_at(r#"{"version":1,"op":"drive","speed":0.75,"bearing":90}"#, now);
    let expected = 1.5 * control.session.realism.average_factor(now, now + Duration::from_secs(2));
    let reply = control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(60));
    let output = reply.state.config.unwrap().position;
    assert!((crate::route::distance(&anchor, &output) - expected).abs() < 1e-6);
    assert_eq!(output.speed, 0.0);
    assert!(
        (reply.state.step_count.total as f64 + reply.state.step_count.fraction - expected / 0.75)
            .abs()
            < 1e-9
    );
}

#[test]
fn realism_route_progress_is_independent_of_polling_across_repeat_waits() {
    let now = Instant::now();
    let settings = r#"{"version":1,"op":"set_realism","config":{"enabled":true,"seed":9,"drift_radius_m":0,"altitude_m":0,"bearing_degrees":0}}"#;
    let route = json!({"version":1,"op":"start_route","scope":{"mode":"all"},"route":{
        "points":[Position::new(0.0,0.0),Position::new(0.0,0.0001)],"speed":1.0,"repeat_count":10,"repeat_delay":3.0
    }}).to_string();
    let (mut coarse, mut fine) = (Control::default(), Control::default());
    for control in [&mut coarse, &mut fine] {
        assert!(control.handle_at(settings, now).ok);
        assert!(control.handle_at(&route, now).ok);
    }
    let status = r#"{"version":1,"op":"status"}"#;
    for i in 1..=400 {
        fine.handle_at(status, now + Duration::from_millis(i * 100));
    }
    coarse.handle_at(status, now + Duration::from_secs(40));
    let a = coarse.session.route.as_ref().unwrap();
    let b = fine.session.route.as_ref().unwrap();
    assert!((a.travelled() - b.travelled()).abs() < 1e-6);
    assert_eq!(a.state().lap, b.state().lap);
    assert!(crate::route::distance(&a.position(), &b.position()) < 1e-6);
    let paused =
        fine.handle_at(r#"{"version":1,"op":"pause_route"}"#, now + Duration::from_secs(40));
    let held = fine.handle_at(status, now + Duration::from_secs(80));
    assert_eq!(paused.state.route.unwrap().distance, held.state.route.unwrap().distance);
}

#[test]
fn steps_start_stop_and_rate_changes_charge_only_the_previous_interval() {
    let now = Instant::now();
    let mut control = Control::default();
    assert!(control.handle_at(&static_start(), now).ok);
    assert!(control.handle_at(&set_steps(false), now).ok);
    let status = r#"{"version":1,"op":"status"}"#;
    assert_eq!(control.handle_at(status, now + Duration::from_secs(5)).state.step_count.total, 10);
    let stopped = control.handle_at(r#"{"version":1,"op":"stop"}"#, now + Duration::from_secs(6));
    assert_eq!(stopped.state.step_count.total, 12);
    assert_eq!(stopped.state.step_rate, 0.0);
    assert_eq!(control.handle_at(status, now + Duration::from_secs(30)).state.step_count.total, 12);
}

#[test]
fn linked_steps_respect_joystick_lease_and_do_not_count_static_speed_metadata() {
    let now = Instant::now();
    let mut control = Control::default();
    control.handle_at(&static_start(), now);
    control.handle_at(&set_steps(true), now);
    let drive = r#"{"version":1,"op":"drive","speed":0.75,"bearing":90}"#;
    control.handle_at(drive, now);
    let status = r#"{"version":1,"op":"status"}"#;
    let expired = control.handle_at(status, now + Duration::from_secs(60));
    assert_eq!(expired.state.step_count.total, 2);
    assert_eq!(expired.state.step_rate, 0.0);
    assert_eq!(control.handle_at(status, now + Duration::from_secs(90)).state.step_count.total, 2);
}

#[test]
fn linked_route_steps_exclude_repeat_waits_pauses_and_time_after_arrival() {
    let now = Instant::now();
    let mut control = Control::default();
    control.handle_at(&set_steps(true), now);
    let mut request: serde_json::Value =
        serde_json::from_str(&route_start(&[(0.0, 0.0), (0.0, 0.00001)], 0.75)).unwrap();
    request["route"]["repeat_count"] = json!(2);
    request["route"]["repeat_delay"] = json!(10);
    assert!(control.handle_at(&request.to_string(), now).ok);
    let paused =
        control.handle_at(r#"{"version":1,"op":"pause_route"}"#, now + Duration::from_secs(1));
    assert_eq!(paused.state.step_count.total, 1);
    assert_eq!(paused.state.step_rate, 0.0);
    control.handle_at(r#"{"version":1,"op":"resume_route"}"#, now + Duration::from_secs(100));
    let final_state =
        control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(200)).state;
    assert_eq!(final_state.step_count.total, 2);
    assert_eq!(final_state.step_rate, 0.0);
}

#[test]
fn manual_step_baseline_cannot_decrease_and_does_not_change_daily_statistics() {
    let mut control = Control::default();
    let reply = control.handle(r#"{"version":1,"op":"set_step_count","total":1000}"#);
    assert!(reply.ok);
    assert_eq!(reply.state.step_count.total, 1000);
    assert_eq!(reply.state.step_count.today, 0);
    assert_eq!(reply.state.step_count.epoch, 1);
    assert!(!control.handle(r#"{"version":1,"op":"set_step_count","total":999}"#).ok);
}

#[test]
fn step_counters_survive_restart_and_disk_failure_cannot_prevent_stop() {
    let directory = std::env::temp_dir().join(format!("justlocation-steps-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("config.json");
    let counter_path = path.with_extension("steps.json");
    let now = Instant::now();
    let mut control = Control::open(&path).unwrap();
    assert!(control.handle_at(&static_start(), now).ok);
    assert!(control.handle_at(&set_steps(false), now).ok);
    assert!(control.handle_at(r#"{"version":1,"op":"status"}"#, now + Duration::from_secs(5)).ok);
    let mut restored = Control::open(&path).unwrap();
    let state = restored.handle(r#"{"version":1,"op":"status"}"#).state;
    assert_eq!(state.step_count.total, 10);
    assert!(!state.requested_active);
    assert_eq!(state.step_rate, 0.0);
    std::fs::remove_file(&counter_path).unwrap();
    std::fs::create_dir(&counter_path).unwrap();
    let reply = control.handle_at(r#"{"version":1,"op":"stop"}"#, now + Duration::from_secs(6));
    assert!(!reply.ok);
    assert!(!reply.state.requested_active);
    assert_eq!(reply.state.step_count.total, 10);
    assert_eq!(reply.state.step_rate, 0.0);
    std::fs::remove_dir(counter_path).unwrap();
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn the_raw_motion_channel_mirrors_the_delivered_motion_state() {
    let mut control = Control::default();
    let now = Instant::now();
    assert!(control.handle_at(&static_start(), now).ok);
    // The channel is only offered while the bridge that would deliver it is alive.
    assert!(
        control
            .handle(
                r#"{"version":1,"op":"hook_status","installed":true,"gnss":false,"nmea":false,"cell_callbacks":false,"virtual_sim_callbacks":false,"wifi_scan":false,"wifi_connection":false,"wifi_calls":0,"gnss_raw":false}"#
            )
            .ok
    );
    // Step events alone do not offer a raw sensor channel.
    assert!(control.handle(r#"{"version":1,"op":"set_steps","config":{"enabled":true,"cadence":2.0,"movement_linked":true,"stride_m":0.75,"daily_reset":false}}"#).ok);
    assert!(control.handle(r#"{"version":1,"op":"status"}"#).state.motion_output.is_none());
    // Turning the raw channel on offers gravity while standing still: no gait, no rotation.
    assert!(control.handle(r#"{"version":1,"op":"set_steps","config":{"enabled":true,"cadence":2.0,"movement_linked":true,"stride_m":0.75,"daily_reset":false,"motion_sensors":true}}"#).ok);
    let idle = control.handle(r#"{"version":1,"op":"status"}"#).state.motion_output.unwrap();
    assert_eq!(idle.cadence, 0.0);
    assert_eq!(idle.speed, 0.0);
    assert!((idle.accelerometer[1] - crate::steps::GRAVITY).abs() < 1e-6);
    assert_eq!(idle.gyroscope, [0.0, 0.0, 0.0]);
    // Movement reaches the raw channel: cadence follows the speed and stride, and the sample is a
    // gait rather than a standstill.
    assert!(control.handle(r#"{"version":1,"op":"drive","speed":1.5,"bearing":90}"#).ok);
    let moving = control.handle(r#"{"version":1,"op":"status"}"#).state.motion_output.unwrap();
    assert_eq!(moving.speed, 1.5);
    assert_eq!(moving.bearing, 90.0);
    assert!((moving.cadence - 2.0).abs() < 1e-9, "cadence {}", moving.cadence);
    assert!(
        (moving.accelerometer[1] - crate::steps::GRAVITY).abs() > 1e-6
            || moving.accelerometer[0].abs() > 1e-6,
        "a moving device must not report a pure standstill"
    );
    // The channel disappears when the step simulation is switched off.
    assert!(control.handle(r#"{"version":1,"op":"set_steps","config":{"enabled":false,"cadence":2.0,"movement_linked":true,"stride_m":0.75,"daily_reset":false,"motion_sensors":true}}"#).ok);
    assert!(control.handle(r#"{"version":1,"op":"status"}"#).state.motion_output.is_none());
    // Invalid motion values from the bridge are refused instead of producing impossible samples.
    let invalid = control.handle(
        r#"{"version":1,"op":"step_hook_status","installed":true,"events":1,"motion_cadence":-1.0}"#,
    );
    assert!(!invalid.ok);
}

#[test]
fn a_segment_break_moves_without_counting_the_gap_as_travel() {
    let mut control = Control::default();
    let start = Instant::now();
    assert!(control.handle_at(&set_steps(true), start).ok);
    // Two segments a kilometre apart: sixty metres in the first, a break, then sixty metres in the
    // second. The distance between the segments is a gap in the recording, not travel.
    let route = json!({"version":1,"op":"start_route",
        "scope":{"mode":"apps","packages":["example.selected"]},
        "route":{
            "points":[
                Position::new(31.0, 121.0),
                Position::new(31.00054, 121.0),
                Position::new(31.00900, 121.0),
                Position::new(31.00954, 121.0)
            ],
            "speed":6.0,
            "breaks":[2]
        }})
    .to_string();
    assert!(control.handle_at(&route, start).ok);
    let planned = control
        .handle_at(r#"{"version":1,"op":"status"}"#, start)
        .state
        .route
        .as_ref()
        .unwrap()
        .total_distance;
    // Sixty metres per segment, so about a hundred and twenty in total, not a kilometre more.
    assert!(
        (planned - 120.0).abs() < 5.0,
        "a break must not add the gap to the route distance: {planned}m"
    );

    let mut previous_steps = 0;
    let mut previous_distance = 0.0;
    for tenth in 1..=200 {
        let at = start + Duration::from_millis(100 * tenth);
        let state = control.handle_at(r#"{"version":1,"op":"status"}"#, at);
        let route = state.state.route.as_ref().unwrap();
        let steps = state.state.step_count.total;
        assert!(steps >= previous_steps, "the count dropped inside a segment");
        assert!(steps - previous_steps <= 1, "the cadence allowed at most one step per 100ms");
        // The route distance advances inside a segment and never jumps by the gap between them.
        assert!(
            route.distance >= previous_distance - 1e-6,
            "the route distance went backwards at {}ms",
            tenth * 100
        );
        assert!(
            route.distance - previous_distance <= 5.0,
            "a single 100ms sample advanced the route by {}m",
            route.distance - previous_distance
        );
        previous_steps = steps;
        previous_distance = route.distance;
    }
    let finished = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(200));
    let total_steps = finished.state.step_count.total;
    assert!(total_steps > 0, "the route must count steps");
    // Two hundred and forty metres at a 0.75 m stride would be three hundred and twenty steps,
    // capped at two per second over the forty seconds the route is actually moving.
    assert!(
        total_steps <= 80,
        "the count followed the clock rather than the travel: {total_steps} steps"
    );
}

#[test]
fn laps_roll_over_without_stalling_the_position_or_the_count() {
    let mut control = Control::default();
    let start = Instant::now();
    assert!(control.handle_at(&set_steps(true), start).ok);
    // A hundred-metre route at ten metres per second: ten seconds of travel, then a five-second
    // gap, then the next lap from the start.
    let route = json!({"version":1,"op":"start_route",
        "scope":{"mode":"apps","packages":["example.selected"]},
        "route":{"points":[
            Position::new(31.0, 121.0),
            Position::new(31.0009, 121.0)
        ],"speed":10.0,"repeat_count":3,"repeat_delay":5.0}})
    .to_string();
    assert!(control.handle_at(&route, start).ok);

    let mut previous_steps = 0;
    let mut previous_lap = 0;
    let mut longest_pause = 0.0_f64;
    let mut since_count = 0.0_f64;
    let mut saw_gap = false;
    // Thirty-two seconds covers two full laps plus the gap between them.
    for second in 1..=32 {
        let state = control.handle_at(
            r#"{"version":1,"op":"status"}"#,
            start + Duration::from_secs_f64(second as f64),
        );
        let route = state.state.route.as_ref().unwrap();
        let steps = state.state.step_count.total;
        // The count never goes backwards and never jumps by more than the cadence allows.
        assert!(steps >= previous_steps, "the step count dropped at {second}s");
        assert!(steps - previous_steps <= 2, "the count jumped by {} at {second}s", steps - previous_steps);
        if steps == previous_steps {
            since_count += 1.0;
            longest_pause = longest_pause.max(since_count);
        } else {
            since_count = 0.0;
        }
        if route.waiting_seconds > 0.0 {
            saw_gap = true;
        } else if since_count == 0.0 {
            // A moving second advances the lap distance; the whole point is that a lap boundary does
            // not freeze it while the route is still playing.
            previous_lap = route.lap;
        }
        previous_steps = steps;
    }
    assert!(previous_steps > 0, "three laps must count steps");
    assert!(saw_gap, "the repeat gap between laps was never observed");
    // Counting may only pause for the configured gap, not for a whole lap or beyond.
    assert!(
        longest_pause <= 6.0,
        "the count stalled for {longest_pause}s, longer than the repeat gap"
    );
    assert!(previous_lap >= 2, "only {previous_lap} lap boundaries were reached in 32 seconds");

    // Finish the route: after arrival nothing moves and nothing counts.
    let arrived = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(400));
    assert!(arrived.state.route.as_ref().unwrap().completed);
    let settled = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(900));
    assert_eq!(settled.state.step_count.total, arrived.state.step_count.total);
    assert_eq!(
        settled.state.config.clone().unwrap().position.latitude,
        arrived.state.config.clone().unwrap().position.latitude
    );
}

#[test]
fn a_paused_route_keeps_its_distance_and_does_not_count_the_pause_as_movement() {
    let mut control = Control::default();
    let start = Instant::now();
    assert!(control.handle_at(&set_steps(true), start).ok);
    // Two hundred metres northwards at two metres per second: a hundred seconds of travel.
    assert!(control.handle_at(&route_start(&[(31.0, 121.0), (31.0018, 121.0)], 2.0), start).ok);
    let played = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(10));
    assert!(
        (played.state.route.as_ref().unwrap().distance - 20.0).abs() < 1.0,
        "ten seconds at 2 m/s travelled {}m",
        played.state.route.as_ref().unwrap().distance
    );
    assert!(played.state.step_count.total > 0, "a playing route must count steps");

    // A pause holds the position, the distance and the count, however long it lasts. The pause and
    // the first paused read happen at the same instant, so the interval between them is zero.
    let paused = control.handle_at(r#"{"version":1,"op":"pause_route"}"#, start + Duration::from_secs(11));
    assert!(paused.ok);
    let travelled = paused.state.route.as_ref().unwrap().distance;
    let steps = paused.state.step_count.total;
    let paused = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(11));
    let held = paused.state.route.as_ref().unwrap();
    assert!(held.paused);
    assert!(
        (held.distance - travelled).abs() < 0.01,
        "a paused route moved from {travelled}m to {}m",
        held.distance
    );
    let later = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(60));
    assert!(
        (later.state.route.as_ref().unwrap().distance - travelled).abs() < 0.01,
        "a paused route kept moving while the clock advanced"
    );
    assert_eq!(later.state.step_count.total, steps, "a paused route must not count steps");
    assert_eq!(later.state.config.clone().unwrap().position.speed, 0.0);

    // Resuming continues from where it stopped instead of jumping over the pause: fifty seconds
    // later it has travelled the extra fifty seconds, not the ninety-nine that the clock advanced.
    assert!(control.handle_at(r#"{"version":1,"op":"resume_route"}"#, start + Duration::from_secs(60)).ok);
    let resumed = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(110));
    let after = resumed.state.route.as_ref().unwrap().distance;
    let expected = travelled + 50.0 * 2.0;
    assert!(
        (after - expected).abs() < 2.0,
        "resumed distance {after}m should be about {expected}m, so the pause was skipped"
    );
    assert!(resumed.state.step_count.total > steps, "a resumed route must count again");

    // Arrival holds the position and stops both counting and movement.
    let arrived = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(400));
    let arrival = arrived.state.route.as_ref().unwrap();
    assert!(arrival.completed, "the route should have finished");
    let settled = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(900));
    assert_eq!(
        settled.state.step_count.total, arrived.state.step_count.total,
        "a finished route must not keep counting"
    );
    assert_eq!(
        settled.state.config.clone().unwrap().position.longitude,
        arrived.state.config.clone().unwrap().position.longitude
    );
}

#[test]
fn steps_follow_the_same_motion_as_the_position_and_stop_when_it_does() {
    let mut control = Control::default();
    let start = Instant::now();
    assert!(control.handle_at(&static_start(), start).ok);
    assert!(
        control
            .handle_at(
                r#"{"version":1,"op":"set_steps","config":{"enabled":true,"cadence":2.0,"movement_linked":true,"stride_m":0.75,"daily_reset":false}}"#,
                start
            )
            .ok
    );
    // Standing still counts nothing, however long the session runs.
    let idle = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(30));
    assert_eq!(idle.state.step_count.total, 0, "a standstill must not count steps");

    // Movement counts steps and moves the position: two metres per second at a 0.75 m stride would
    // be 2.67 steps per second, capped by the configured cadence of two.
    assert!(control.handle_at(r#"{"version":1,"op":"drive","speed":2,"bearing":0}"#, start + Duration::from_secs(30)).ok);
    let after = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(31));
    let counted = after.state.step_count.total;
    assert_eq!(counted, 2, "one second of movement at the cadence cap must count two steps");
    let travelled = crate::cells::Coordinate { latitude: 31.0, longitude: 121.0 }.distance_to(
        crate::cells::Coordinate {
            latitude: after.state.config.clone().unwrap().position.latitude,
            longitude: after.state.config.clone().unwrap().position.longitude,
        },
    );
    assert!(travelled > 1.5, "the position must move as the steps are counted: {travelled}m");

    // The lease lasts two seconds from the command, so the whole movement is two metres per second
    // for two seconds. Counting it in one late tick must give the same result as counting it tick by
    // tick: the travelled distance cannot depend on how often the state is read.
    let idle = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(60));
    assert_eq!(idle.state.step_count.total, 4, "the expired lease is counted once, not per sample");
    assert_eq!(idle.state.config.clone().unwrap().position.speed, 0.0);
    // A further read long after the lease adds neither distance nor steps.
    let later = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(120));
    assert_eq!(later.state.step_count.total, 4, "a finished lease must stop the count");
    assert_eq!(
        later.state.config.clone().unwrap().position.longitude,
        idle.state.config.clone().unwrap().position.longitude,
        "a finished lease must stop the movement"
    );

    // A full route counts as it plays, and a late tick that crosses a break cannot invent steps for
    // the interval it skipped.
    let mut route = Control::default();
    let route_start_at = Instant::now();
    assert!(route.handle_at(&set_steps(true), route_start_at).ok);
    assert!(route.handle_at(&route_start(&[(31.0, 121.0), (31.002, 121.0)], 1.5), route_start_at).ok);
    let playing = route.handle_at(r#"{"version":1,"op":"status"}"#, route_start_at + Duration::from_secs(2));
    assert!(playing.state.step_count.total > 0, "a playing route must count steps");
    assert!(playing.state.step_count.total <= 4, "two seconds at a 1.5 m/s stride is about four steps");
    // A tick far beyond the route's end adds nothing further once the route has finished.
    let finished = route.handle_at(r#"{"version":1,"op":"status"}"#, route_start_at + Duration::from_secs(600));
    let stable = route.handle_at(r#"{"version":1,"op":"status"}"#, route_start_at + Duration::from_secs(900));
    assert_eq!(
        finished.state.step_count.total, stable.state.step_count.total,
        "a finished route must not keep counting"
    );
}

#[test]
fn a_delivered_moving_fix_reports_the_speed_and_heading_its_coordinates_show() {
    let mut control = Control::default();
    let start = Instant::now();
    assert!(control.handle_at(&static_start(), start).ok);
    // A joystick command moves the target eastward at 2 m/s.
    assert!(control.handle_at(r#"{"version":1,"op":"drive","speed":2,"bearing":90}"#, start).ok);
    let first = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(1));
    let a = first.state.config.clone().unwrap().position;
    assert!((a.speed - 2.0).abs() < 1e-9, "delivered speed {}", a.speed);
    assert!((a.bearing - 90.0).abs() < 1e-9, "delivered bearing {}", a.bearing);
    // One more second of movement: the displacement has to match the reported speed.
    let second = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(2));
    let b = second.state.config.clone().unwrap().position;
    let metres = crate::cells::Coordinate { latitude: a.latitude, longitude: a.longitude }
        .distance_to(crate::cells::Coordinate { latitude: b.latitude, longitude: b.longitude });
    assert!((metres - a.speed).abs() < 0.5, "travelled {metres}m in one second at {}m/s", a.speed);
    // The lease expires after two seconds, so a later fix reports a standstill instead of a speed
    // that no longer matches the coordinates.
    let idle = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(40));
    let c = idle.state.config.clone().unwrap().position;
    assert_eq!(c.speed, 0.0, "an expired lease must not keep declaring movement");
    assert!((c.bearing - 90.0).abs() < 1e-9, "the heading of the last movement is retained");
}

#[test]
fn a_route_fix_reports_geometry_speed_and_heading() {
    let mut control = Control::default();
    let start = Instant::now();
    // Northward: bearing must follow the segment direction, not a default.
    assert!(control.handle_at(&route_start(&[(31.0, 121.0), (31.01, 121.0)], 3.0), start).ok);
    let first = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(1));
    let a = first.state.config.clone().unwrap().position;
    assert!(a.speed > 0.0, "a playing route must report its speed");
    assert!(a.bearing < 1.0 || a.bearing > 359.0, "northward bearing was {}", a.bearing);
    assert!(a.latitude > 31.0, "the route must have advanced northwards");
    // Pausing stops the reported movement as well.
    assert!(control.handle(r#"{"version":1,"op":"pause_route"}"#).ok);
    let paused = control.handle(r#"{"version":1,"op":"status"}"#);
    assert_eq!(paused.state.config.clone().unwrap().position.speed, 0.0);
}

#[test]
fn realism_variation_reaches_the_reported_speed_as_well_as_the_coordinates() {
    let mut control = Control::default();
    let start = Instant::now();
    // Realism only applies to a stopped session, so it is configured before the start.
    assert!(
        control
            .handle_at(
                r#"{"version":1,"op":"set_realism","config":{"enabled":true,"seed":7}}"#,
                start
            )
            .ok
    );
    assert!(control.handle_at(&static_start(), start).ok);
    assert!(control.handle_at(r#"{"version":1,"op":"drive","speed":10,"bearing":0}"#, start).ok);
    let mut varied = 0;
    let mut previous: Option<f64> = None;
    for step in 0..12 {
        let at = start + Duration::from_millis(150 * step);
        let state = control.handle_at(r#"{"version":1,"op":"status"}"#, at);
        let position = state.state.config.clone().unwrap().position;
        // The allowed speed variation is ±10%, and it must be applied exactly once.
        assert!(
            (9.0..=11.0).contains(&position.speed),
            "speed outside the configured variation: {}",
            position.speed
        );
        if let Some(last) = previous {
            if (position.speed - last).abs() > 1e-9 {
                varied += 1;
            }
        }
        previous = Some(position.speed);
    }
    assert!(varied > 0, "the variation never reached the delivered speed");
    // Reported speed and travelled distance have to agree: one second of movement at the reported
    // speed covers the reported distance.
    let first = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(1));
    let a = first.state.config.clone().unwrap().position;
    let second = control.handle_at(r#"{"version":1,"op":"status"}"#, start + Duration::from_secs(2));
    let b = second.state.config.clone().unwrap().position;
    let metres = crate::cells::Coordinate { latitude: a.latitude, longitude: a.longitude }
        .distance_to(crate::cells::Coordinate { latitude: b.latitude, longitude: b.longitude });
    assert!(metres > 0.0, "a moving session must cover ground");
    // Drift is bounded by the configured radius, so the displacement stays near the speed.
    assert!(
        metres <= 11.0 + 2.0 * 2.0_f64.sqrt() + 1.0,
        "displacement {metres}m does not match the reported speed {}m/s",
        a.speed
    );
    // With realism off the delivered speed is exactly the commanded one.
    let mut plain = Control::default();
    assert!(plain.handle_at(&static_start(), Instant::now()).ok);
    assert!(plain.handle(r#"{"version":1,"op":"drive","speed":10,"bearing":0}"#).ok);
    assert_eq!(
        plain.handle(r#"{"version":1,"op":"status"}"#).state.config.unwrap().position.speed,
        10.0
    );
}

#[test]
fn a_stationary_session_shows_drift_without_accumulating_distance_or_speed() {
    let mut control = Control::default();
    let start = Instant::now();
    assert!(
        control
            .handle_at(
                r#"{"version":1,"op":"set_realism","config":{"enabled":true,"seed":11,"drift_radius_m":20,"speed_variation":0.5}}"#,
                start
            )
            .ok
    );
    assert!(control.handle_at(&static_start(), start).ok);
    let mut points = Vec::new();
    for step in 0..40 {
        let at = start + Duration::from_millis(250 * step);
        let state = control.handle_at(r#"{"version":1,"op":"status"}"#, at);
        let position = state.state.config.clone().unwrap().position;
        // A static session never declares movement, whatever the drift does to the coordinates.
        assert_eq!(position.speed, 0.0, "a static session reported movement");
        points.push(crate::cells::Coordinate {
            latitude: position.latitude,
            longitude: position.longitude,
        });
    }
    // Drift is bounded by its radius and does not walk away from the anchor.
    let anchor = crate::cells::Coordinate { latitude: 31.0, longitude: 121.0 };
    let furthest = points.iter().map(|point| anchor.distance_to(*point)).fold(0.0, f64::max);
    assert!(furthest <= 20.0 + 1e-6, "drift reached {furthest}m");
    // The point-to-point wander is observation error, not travel: it stays within a small multiple
    // of the drift radius instead of growing with the number of samples.
    let wander: f64 = points.windows(2).map(|pair| pair[0].distance_to(pair[1])).sum();
    assert!(wander <= 5.0 * 20.0, "stationary drift accumulated {wander}m of apparent travel");
    // Halving the sampling interval does not double the apparent travel, so the error is bounded
    // rather than accumulating per sample.
    let mut coarse = Control::default();
    assert!(coarse.handle_at(&static_start(), start).ok);
    let mut coarse_points = Vec::new();
    for step in 0..10 {
        let at = start + Duration::from_millis(1000 * step);
        let state = coarse.handle_at(r#"{"version":1,"op":"status"}"#, at);
        let position = state.state.config.unwrap().position;
        coarse_points.push(crate::cells::Coordinate {
            latitude: position.latitude,
            longitude: position.longitude,
        });
    }
    let coarse_wander: f64 =
        coarse_points.windows(2).map(|pair| pair[0].distance_to(pair[1])).sum();
    assert!(
        coarse_wander <= 5.0 * 20.0,
        "stationary drift accumulated {coarse_wander}m of apparent travel"
    );
}

#[test]
fn a_delivered_position_states_when_it_was_sampled_and_reading_it_again_does_not_refresh_it() {
    let mut control = Control::default();
    let start = control.handle_at(&static_start(), Instant::now());
    assert!(start.ok);
    let sampled = start.state.position_sampled_ms.expect("a started session reports a sample time");
    assert!(sampled > 0);
    // The same position read again keeps its sample time: a cache read is not a new fix.
    let later = control.handle_at(r#"{"version":1,"op":"status"}"#, Instant::now());
    assert_eq!(later.state.position_sampled_ms, Some(sampled));
    // Leased movement produces a new position, so the sample time advances.
    assert!(control.handle(r#"{"version":1,"op":"drive","speed":1.5,"bearing":90}"#).ok);
    let moved = control.handle(r#"{"version":1,"op":"status"}"#);
    let moved_at = moved.state.position_sampled_ms.expect("a moving session reports a sample time");
    assert!(moved_at >= sampled);
    // An explicit position change is a new sample as well.
    assert!(
        control
            .handle(r#"{"version":1,"op":"update","position":{"latitude":31.5,"longitude":121.5,"altitude":0,"accuracy":5,"speed":0,"bearing":0}}"#)
            .ok
    );
    let updated = control.handle(r#"{"version":1,"op":"status"}"#);
    assert!(updated.state.position_sampled_ms.is_some());
    // Nothing is being delivered after a stop, so there is no sample time to report.
    assert!(control.handle(r#"{"version":1,"op":"stop"}"#).ok);
    assert_eq!(control.handle(r#"{"version":1,"op":"status"}"#).state.position_sampled_ms, None);
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
    let route = Route {
        geometry: None,
        points: track.points,
        speed: 5.0,
        repeat_count: 1,
        repeat_delay: 0.0,
        breaks: vec![],
    };
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
        assert_eq!(response.state.route.unwrap().plan.as_ref().unwrap().repeat_count, 1);
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

    // Android's property getters are global: an app-only SIM scope must restore them immediately.
    let selective = control.handle(r#"{"version":1,"op":"set_scope","feature":"sim","scope":{"mode":"apps","packages":["example.selected"]}}"#);
    assert!(selective.ok);
    assert!(!selective.state.operator_hook_ready);
    assert_eq!(probe.value(crate::operators::NETWORK_ALPHA).unwrap(), "中国电信");
    assert!(
        control
            .handle(r#"{"version":1,"op":"set_scope","feature":"sim","scope":{"mode":"all"}}"#)
            .ok
    );
    assert_eq!(probe.value(crate::operators::NETWORK_ALPHA).unwrap(), "中国联通");

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

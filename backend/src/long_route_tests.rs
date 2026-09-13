use crate::{
    Position,
    protocol::Control,
    route::{Playback, Route},
    route_store,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

fn route(count: usize) -> Route {
    Route {
        geometry: None,
        points: (0..count).map(|i| Position::new(31.0 + i as f64 * 0.00001, 121.0)).collect(),
        breaks: vec![],
        speed: 1000.0,
        repeat_count: 2,
        repeat_delay: 1.0,
    }
}
fn dir() -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("justlocation-long-{}", crate::scode::new_id().unwrap()));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn call(control: &mut Control, mut value: Value) -> Value {
    value["version"] = json!(1);
    let response = control.handle(&value.to_string());
    assert!(response.ok, "{:?}", response.error);
    let bytes = serde_json::to_vec(&response).unwrap();
    assert!(bytes.len() < 128 * 1024, "response exceeded the control transport limit");
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn long_routes_round_trip_chunks_and_play_to_the_end_of_each_lap() {
    let dir = dir();
    for count in [1000, 10000, 100000] {
        let plan = route(count);
        let now = Instant::now();
        let id = route_store::save(&dir, &plan).unwrap();
        let loaded = route_store::load(&dir, &id).unwrap();
        assert_eq!(plan.points, loaded.points);
        let mut playback = Playback::new(loaded, now).unwrap();
        assert!(playback.state().plan.is_none());
        let travel = playback.state().total_distance / plan.speed;
        playback.advance(now + Duration::from_secs_f64(travel + 0.2));
        assert_eq!(playback.position(), {
            let mut p = plan.points.last().unwrap().clone();
            p.speed = 0.;
            p
        });
        assert_eq!(playback.state().lap, 1);
        playback.pause(true).unwrap();
        playback.advance(now + Duration::from_secs_f64(travel + 10.2));
        playback.pause(false).unwrap();
        playback.advance(now + Duration::from_secs_f64(2. * travel + 12.));
        assert!(playback.state().completed);
        assert_eq!(playback.state().lap, 2);
        let mut control = Control::open(dir.join("config.json")).unwrap();
        let reply =
            call(&mut control, json!({"op":"start_route_ref","id":id,"scope":{"mode":"all"}}));
        assert_eq!(reply["state"]["route"]["point_count"], count);
        assert!(reply["state"]["route"]["plan"].is_null());
        let mut points = vec![];
        for offset in (0..count).step_by(128) {
            let reply = call(&mut control, json!({"op":"route_page","offset":offset,"limit":128}));
            let page: route_store::Page = serde_json::from_value(reply["page"].clone()).unwrap();
            points.extend(page.points);
        }
        assert_eq!(points, plan.points);
        call(&mut control, json!({"op":"stop"}));
        route_store::remove(&dir, &id).unwrap();
        eprintln!(
            "long-route {count}: storage, pages and playback {:.3}s",
            now.elapsed().as_secs_f64()
        );
    }
    fs::remove_file(dir.join("config.json")).unwrap();
    fs::remove_dir(dir.join("routes")).unwrap();
    fs::remove_dir(dir).unwrap();
}

#[test]
fn missing_and_retried_upload_chunks_do_not_publish_partial_routes() {
    let dir = dir();
    let plan = route(10000);
    let id = route_store::begin(
        &dir,
        route_store::Upload {
            point_count: 10000,
            speed: plan.speed,
            repeat_count: 2,
            repeat_delay: 1.,
        },
    )
    .unwrap();
    assert!(route_store::finish(&dir, &id).is_err());
    for offset in (0..10000).step_by(128).rev() {
        let page = route_store::page(&plan.points, &[], offset, 128).unwrap();
        route_store::append(&dir, &id, page.clone()).unwrap();
        route_store::append(&dir, &id, page).unwrap();
    }
    assert_eq!(route_store::finish(&dir, &id).unwrap().points, plan.points);
    assert!(route_store::load(&dir, "../config").is_err());
    route_store::abort(&dir, &id).unwrap();
    fs::remove_dir(dir.join("route-uploads")).unwrap();
    fs::remove_dir(dir).unwrap();
}

#[test]
fn ten_thousand_recorded_points_survive_restart_and_pause_gaps_without_large_status_frames() {
    let dir = dir();
    let config = dir.join("config.json");
    let journal = dir.join("config.recording.jsonl");
    let mut control = Control::open(&config).unwrap();
    let plan = route(10000);
    let start = Instant::now();
    let id = call(&mut control, json!({"op":"record_start"}))["state"]["recording"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (i, point) in plan.points.iter().enumerate() {
        if i == 5000 {
            drop(control);
            // A killed process can leave a partial, unacknowledged final line.
            fs::OpenOptions::new()
                .append(true)
                .open(&journal)
                .unwrap()
                .write_all(b"{\"event\":")
                .unwrap();
            control = Control::open(&config).unwrap();
            let state = call(&mut control, json!({"op":"status"}));
            assert_eq!(state["state"]["recording"]["points"], 5000);
            assert_eq!(state["state"]["recording"]["paused"], true);
            assert!(
                !control
                    .handle(
                        &json!({"version":1,"op":"record_point","position":point,"seconds":0})
                            .to_string()
                    )
                    .ok
            );
            call(&mut control, json!({"op":"record_resume"}));
        }
        let seconds = if i < 5000 { i } else { i - 5000 };
        call(&mut control, json!({"op":"record_point","position":point,"seconds":seconds}));
    }
    call(&mut control, json!({"op":"record_stop"}));
    drop(control);
    control = Control::open(&config).unwrap();
    let state = call(&mut control, json!({"op":"status"}));
    assert_eq!(state["state"]["recorded"]["point_count"], 10000);
    assert_eq!(state["state"]["recorded"]["seconds"], 9998.0);
    assert_eq!(state["state"]["recorded"]["points"], json!([]));
    assert!(!control.handle(r#"{"version":1,"op":"record_start"}"#).ok);
    let mut points = vec![];
    let mut breaks = vec![];
    for offset in (0..10000).step_by(128) {
        let reply =
            call(&mut control, json!({"op":"record_page","id":id,"offset":offset,"limit":128}));
        let page: route_store::Page = serde_json::from_value(reply["page"].clone()).unwrap();
        points.extend(page.points);
        breaks.extend(page.breaks);
    }
    assert_eq!(points, plan.points);
    assert_eq!(breaks, vec![5000]);
    call(&mut control, json!({"op":"record_take","id":id}));
    eprintln!(
        "recorded 10000 durable samples and recovered/pages in {:.3}s",
        start.elapsed().as_secs_f64()
    );
    fs::remove_file(journal).unwrap();
    fs::remove_dir(dir).unwrap();
}

#[test]
fn segment_breaks_never_interpolate_the_gap_or_add_it_to_distance() {
    let plan = Route {
        geometry: None,
        points: vec![
            Position::new(0., 0.),
            Position::new(0., 0.001),
            Position::new(10., 10.),
            Position::new(10., 10.001),
        ],
        breaks: vec![2],
        speed: 10.,
        repeat_count: 1,
        repeat_delay: 0.,
    };
    let now = Instant::now();
    for radius in [0., 5.] {
        let mut playback = Playback::smoothed(plan.clone(), now, radius).unwrap();
        assert!(playback.state().total_distance < 225.);
        let point = playback.advance(
            now + Duration::from_secs_f64(
                crate::route::distance(&plan.points[0], &plan.points[1]) / 10. + 0.01,
            ),
        );
        assert!((point.latitude - 10.).abs() < 0.000001);
        assert!((point.longitude - 10.).abs() < 0.00001);
    }
}

#[test]
fn failed_recording_writes_do_not_accept_samples_or_erase_an_unsaved_track() {
    let dir = dir();
    let config = dir.join("config.json");
    let journal = dir.join("config.recording.jsonl");
    let mut control = Control::open(config).unwrap();
    call(&mut control, json!({"op":"record_start"}));
    fs::rename(&journal, dir.join("saved.jsonl")).unwrap();
    fs::create_dir(&journal).unwrap();
    let point = Position::new(31., 121.);
    let reply = control
        .handle(&json!({"version":1,"op":"record_point","position":point,"seconds":0}).to_string());
    assert!(!reply.ok);
    assert_eq!(reply.state.recording.unwrap().points, 0);
    assert!(!control.handle(r#"{"version":1,"op":"record_discard"}"#).ok);
    assert!(call(&mut control, json!({"op":"status"}))["state"]["recording"].is_object());
    fs::remove_dir(journal).unwrap();
    fs::remove_file(dir.join("saved.jsonl")).unwrap();
    fs::remove_dir(dir).unwrap();
}

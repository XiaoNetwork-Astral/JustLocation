use justlocation_backend::cells::{CellRegion, Coordinate};
use justlocation_backend::protocol::Control;
use serde_json::{Value, json};

fn region() -> Value {
    json!({"center":{"latitude":0.0,"longitude":179.999},"radius_m":5000.0,
        "source":"test fixture (synthetic)","fetched_at_ms":1000,"cells":[
        {"identity":{"radio":"lte","mcc":"460","mnc":"01","tac":1,"ci":9},
         "position":{"latitude":0.0,"longitude":-179.999},"range_m":1000.0},
        {"identity":{"radio":"nr","mcc":"460","mnc":"001","tac":16777215,"nci":68719476735_u64},
         "position":{"latitude":0.0,"longitude":179.9995},"range_m":500.0}
    ]})
}

#[test]
fn nearby_query_uses_target_and_handles_the_date_line_without_a_device_position() {
    let data: CellRegion = serde_json::from_value(region()).unwrap();
    data.validate().unwrap();
    let found = data.nearby(Coordinate { latitude: 0.0, longitude: 179.999 }, 500.0, 8).unwrap();
    assert_eq!(found.len(), 2);
    assert!(found[0].distance_m < 60.0);
    assert!(found[1].distance_m > 220.0 && found[1].distance_m < 225.0);
    let encoded = serde_json::to_value(&found[0]).unwrap();
    assert_eq!(encoded["cell"]["identity"]["nci"], 68719476735_u64);
    assert_eq!(encoded["cell"]["identity"]["mnc"], "001");
    assert_eq!(data.nearby(data.center, 500.0, 1).unwrap().len(), 1);
}

#[test]
fn stale_region_is_distinct_from_a_successful_empty_query() {
    let mut value = region();
    value["cells"] = json!([]);
    let data: CellRegion = serde_json::from_value(value).unwrap();
    assert!(data.nearby(data.center, 100.0, 8).unwrap().is_empty());
    assert!(data.nearby(Coordinate { latitude: 10.0, longitude: 20.0 }, 100.0, 8).is_err());
    // A query that extends beyond the acquired region must not claim full coverage.
    assert!(data.nearby(data.center, 5001.0, 8).is_err());
}

#[test]
fn invalid_identities_and_duplicate_cells_are_rejected_before_replacing_a_region() {
    for (path, value) in [
        ("/cells/0/identity/ci", json!(268435456_u64)),
        ("/cells/1/identity/nci", json!(68719476736_u64)),
        ("/cells/0/identity/mnc", json!("1")),
        ("/cells/0/identity/mcc", json!("46a")),
        ("/cells/0/range_m", json!(-1)),
        ("/center/latitude", json!(91)),
        ("/radius_m", json!(0)),
    ] {
        let mut input = region();
        *input.pointer_mut(path).unwrap() = value;
        let data: CellRegion = serde_json::from_value(input).unwrap();
        assert!(data.validate().is_err(), "{path}");
    }
    let mut input = region();
    let duplicate = input["cells"][0].clone();
    input["cells"].as_array_mut().unwrap().push(duplicate);
    assert!(serde_json::from_value::<CellRegion>(input).unwrap().validate().is_err());
    let mut input = region();
    input["cells"][0]["identity"]["radio"] = json!("future");
    assert!(serde_json::from_value::<CellRegion>(input).is_err());
}

#[test]
fn all_supported_radio_types_keep_their_own_identifiers_and_unknown_fields_stay_unknown() {
    let identities = [
        json!({"radio":"gsm","mcc":"001","mnc":"01","lac":65535,"cid":65535}),
        json!({"radio":"wcdma","mcc":"001","mnc":"01","lac":65535,"cid":268435455,"psc":511}),
        json!({"radio":"lte","mcc":"001","mnc":"01","tac":65535,"ci":268435455,"pci":503}),
        json!({"radio":"nr","mcc":"001","mnc":"01","tac":16777215,"nci":68719476735_u64,"pci":1007}),
        json!({"radio":"cdma","sid":32767,"nid":65535,"bid":65535}),
    ];
    for identity in identities {
        let mut input = region();
        input["cells"] = json!([{"identity":identity,"position":{"latitude":0,"longitude":179.999},"range_m":0}]);
        let data: CellRegion = serde_json::from_value(input).unwrap();
        data.validate().unwrap();
        let output = serde_json::to_value(data).unwrap();
        assert_eq!(output["cells"][0]["identity"], identity);
    }
}

#[test]
fn control_queries_acquired_data_without_starting_simulation_and_keeps_it_on_invalid_update() {
    let mut control = Control::default();
    let set = json!({"version":1,"op":"set_cell_region","region":region()});
    assert!(control.handle(&set.to_string()).ok);
    let query = json!({"version":1,"op":"query_cells","target":{"latitude":0,"longitude":179.999},"radius_m":500,"limit":8});
    let response = serde_json::to_value(control.handle(&query.to_string())).unwrap();
    assert_eq!(response["ok"], true);
    assert_eq!(response["state"]["requested_active"], false);
    assert_eq!(response["cells"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(response["cells"]["source"], "test fixture (synthetic)");
    let mut invalid = set.clone();
    invalid["region"]["radius_m"] = json!(-1);
    assert!(!control.handle(&invalid.to_string()).ok);
    assert!(control.handle(&query.to_string()).ok);
    let status = serde_json::to_value(control.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
    assert!(status["cells"].is_null(), "a prior query must not leak into another response");
    assert!(control.handle(r#"{"version":1,"op":"set_cell_region","region":null}"#).ok);
    assert!(!control.handle(&query.to_string()).ok);
}

#[test]
fn region_save_is_atomic_and_legacy_position_config_still_opens() {
    let path =
        std::env::temp_dir().join(format!("justlocation-{}-cell-region.json", std::process::id()));
    let legacy = json!({"position":justlocation_backend::Position::new(0.0,179.999),"scope":{"mode":"apps","packages":["example.selected"]}});
    std::fs::write(&path, legacy.to_string()).unwrap();
    let set = json!({"version":1,"op":"set_cell_region","region":region()}).to_string();
    let mut control = Control::open(&path).unwrap();
    assert!(control.handle(&set).ok);
    drop(control);
    let mut restored = Control::open(&path).unwrap();
    let response = serde_json::to_value(restored.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
    assert_eq!(response["state"]["requested_active"], false);
    assert_eq!(response["state"]["config"]["position"]["longitude"], 179.999);
    assert_eq!(response["state"]["cell_region"]["cells"].as_array().unwrap().len(), 2);
    std::fs::remove_file(&path).unwrap();
    let mut missing = Control::open(path.join("missing/config.json")).unwrap();
    assert!(!missing.handle(&set).ok);
    let response = serde_json::to_value(missing.handle(r#"{"version":1,"op":"status"}"#)).unwrap();
    assert!(response["state"]["cell_region"].is_null());
}

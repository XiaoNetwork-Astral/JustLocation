use justlocation_backend::{cells::CellRegion, telephony::TelephonyConfig};
use serde_json::json;

fn config() -> TelephonyConfig {
    serde_json::from_value(json!({"cells_enabled":true,"sim_enabled":true,"radius_m":500,
        "subscriptions":[
            {"id":7,"slot":0,"mcc":"460","mnc":"01","country":"cn","carrier":"Test A","enabled":true},
            {"id":9,"slot":1,"mcc":"460","mnc":"001","country":"cn","carrier":"Test B","enabled":true}
        ]})).unwrap()
}

#[test]
fn unselected_placeholder_card_does_not_require_operator_fields() {
    let mut config = config();
    let unused = &mut config.subscriptions[1];
    unused.enabled = false;
    unused.mcc.clear(); unused.mnc.clear(); unused.country.clear(); unused.carrier.clear();
    assert!(config.validate().is_ok());
    config.subscriptions[1].enabled = true;
    assert!(config.validate().is_err());
}
fn region() -> CellRegion {
    serde_json::from_value(json!({"center":{"latitude":0,"longitude":0},"radius_m":1000,
        "source":"test","fetched_at_ms":1,"cells":[
            {"identity":{"radio":"lte","mcc":"460","mnc":"01","tac":1,"ci":12},"position":{"latitude":0,"longitude":0.002},"range_m":0},
            {"identity":{"radio":"lte","mcc":"460","mnc":"01","tac":1,"ci":11},"position":{"latitude":0,"longitude":0.001},"range_m":0},
            {"identity":{"radio":"nr","mcc":"460","mnc":"001","tac":2,"nci":68719476735u64},"position":{"latitude":0,"longitude":0},"range_m":0}
        ]})).unwrap()
}

#[test]
fn per_subscription_cells_keep_plmn_width_and_only_one_registered_cell() {
    let config = config();
    config.validate().unwrap();
    let frame = config.frame(
        Some(&region()),
        justlocation_backend::cells::Coordinate {
            latitude: 0.,
            longitude: 0.,
        },
    );
    let value = serde_json::to_value(frame).unwrap();
    assert_eq!(value["availability"], "ready");
    assert_eq!(value["groups"][0]["subscription_id"], 7);
    assert_eq!(value["groups"][0]["cells"][0]["identity"]["ci"], 11);
    assert_eq!(value["groups"][0]["cells"][0]["registered"], true);
    assert_eq!(value["groups"][0]["cells"][1]["registered"], false);
    assert_eq!(
        value["groups"][1]["cells"][0]["identity"]["nci"],
        68719476735u64
    );
    assert!(
        value["groups"][0]["cells"][0]["identity"]
            .get("pci")
            .is_none()
    );
    assert_eq!(value["subscriptions"][1]["mnc"], "001");
}

#[test]
fn moving_reselects_serving_cell_and_missing_coverage_is_explicit_empty_output() {
    let config = config();
    let region = region();
    let frame = serde_json::to_value(config.frame(
        Some(&region),
        justlocation_backend::cells::Coordinate {
            latitude: 0.,
            longitude: 0.002,
        },
    ))
    .unwrap();
    assert_eq!(frame["groups"][0]["cells"][0]["identity"]["ci"], 12);
    for (region, target, expected) in [
        (Some(&region), 1., "outside_region"),
        (None, 0., "missing_region"),
    ] {
        let value = serde_json::to_value(config.frame(
            region,
            justlocation_backend::cells::Coordinate {
                latitude: 0.,
                longitude: target,
            },
        ))
        .unwrap();
        assert_eq!(value["availability"], expected);
        assert_eq!(value["groups"][0]["cells"], json!([]));
        assert_eq!(value["subscriptions"].as_array().unwrap().len(), 2);
    }
}

#[test]
fn disabled_subscriptions_and_channels_do_not_generate_output() {
    let mut config = config();
    config.subscriptions[0].enabled = false;
    let frame = serde_json::to_value(config.frame(
        Some(&region()),
        justlocation_backend::cells::Coordinate {
            latitude: 0.,
            longitude: 0.,
        },
    ))
    .unwrap();
    assert_eq!(frame["groups"].as_array().unwrap().len(), 1);
    assert_eq!(frame["subscriptions"].as_array().unwrap().len(), 1);
    config.cells_enabled = false;
    config.sim_enabled = false;
    let frame = serde_json::to_value(config.frame(
        None,
        justlocation_backend::cells::Coordinate {
            latitude: 0.,
            longitude: 0.,
        },
    ))
    .unwrap();
    assert_eq!(frame["groups"], json!([]));
    assert_eq!(frame["subscriptions"], json!([]));
}

#[test]
fn invalid_or_ambiguous_subscriptions_are_rejected() {
    let base = serde_json::to_value(config()).unwrap();
    for (index, field, value) in [
        (1, "id", json!(7)),
        (1, "slot", json!(0)),
        (0, "mnc", json!("1")),
        (0, "country", json!("CN")),
        (0, "id", json!(2147483647)),
        (0, "slot", json!(8)),
    ] {
        let mut bad = base.clone();
        bad["subscriptions"][index][field] = value;
        let parsed = serde_json::from_value::<TelephonyConfig>(bad);
        assert!(
            parsed.is_err() || parsed.unwrap().validate().is_err(),
            "{field}"
        );
    }
    let mut config = config();
    config.subscriptions.clear();
    assert!(config.validate().is_err());
    config.cells_enabled = false;
    config.sim_enabled = false;
    assert!(config.validate().is_ok());
}

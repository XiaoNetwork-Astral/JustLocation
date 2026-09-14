//! Convert selected saved attachments into one validated location/environment change.
use crate::{
    Position,
    cells::{Cell, CellIdentity, CellRegion, Coordinate},
    scode::Address,
    wifi::{WifiConfig, WifiTarget},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const MAX_SNAPSHOT: usize = 2 * 1024 * 1024;
pub const MAX_SNAPSHOT_CELLS: usize = 32;
pub const MAX_SNAPSHOT_WIFI: usize = 128;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change<T> {
    #[default]
    Keep,
    Apply(T),
    Clear,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Environment {
    pub cells: Change<CellRegion>,
    pub wifi: Change<WifiConfig>,
}
impl Environment {
    pub fn validate(&self) -> Result<(), String> {
        if let Change::Apply(region) = &self.cells {
            region.validate()?;
        }
        if let Change::Apply(wifi) = &self.wifi {
            wifi.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum AttachmentAction {
    #[default]
    Keep,
    Apply,
    Clear,
}

/// One measured device snapshot written by the probe app: a real fix, the real radio environment
/// and the platform metadata needed to tell them apart from simulated output.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: u32,
    pub captured_ms: u64,
    /// Simulation state when the snapshot was taken; a simulated fix is never a real capture.
    pub simulated: bool,
    pub location: SnapshotLocation,
    #[serde(default)]
    pub cells: Vec<Value>,
    #[serde(default)]
    pub wifi: Vec<Value>,
    /// Coordinate system of the location fields, always WGS84 for platform output.
    pub coordinate_system: String,
    #[serde(default)]
    pub capture: SnapshotCapture,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotLocation {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub altitude: Option<f64>,
    #[serde(default)]
    pub accuracy: Option<f64>,
    #[serde(default)]
    pub speed: Option<f64>,
    #[serde(default)]
    pub bearing: Option<f64>,
    #[serde(default)]
    pub provider: Option<String>,
    /// True when the platform returned a previously known fix instead of a fresh one.
    #[serde(default)]
    pub from_last_known: bool,
    /// Platform fix time and monotonic timestamps, kept so age stays verifiable later.
    #[serde(default)]
    pub fix_time_ms: Option<u64>,
    #[serde(default)]
    pub fix_elapsed_ms: Option<u64>,
    #[serde(default)]
    pub received_elapsed_ms: Option<u64>,
    /// Whether the delivered fix reported itself as mock.
    #[serde(default)]
    pub mock: Option<bool>,
    /// Platform `Location.extras` satellite count; absent means the fix carried no such extra.
    #[serde(default)]
    pub extras_satellites: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SnapshotCapture {
    pub package: Option<String>,
    pub location_permission: Option<String>,
    pub wifi_permission: Option<String>,
    pub phone_permission: Option<String>,
    pub location_enabled: Option<bool>,
    /// Platform API level of the capturing app.
    pub sdk: Option<u32>,
    /// Whether the probe saw a mock fix while collecting, including a rejected one.
    pub mock_active: Option<bool>,
}

/// The exact field contract of the probe app's snapshot.
///
/// The probe builds this document field by field and the backend refuses unknown fields on
/// purpose, so every field the probe writes must also be listed here; a test compares the two.
pub const SNAPSHOT_FIELDS: &[(&str, &[&str])] = &[
    (
        "",
        &["version", "captured_ms", "simulated", "coordinate_system", "location", "cells", "wifi", "capture"],
    ),
    (
        "location",
        &[
            "latitude",
            "longitude",
            "altitude",
            "accuracy",
            "speed",
            "bearing",
            "provider",
            "from_last_known",
            "fix_time_ms",
            "fix_elapsed_ms",
            "received_elapsed_ms",
            "mock",
            "extras_satellites",
        ],
    ),
    (
        "capture",
        &[
            "package",
            "location_permission",
            "wifi_permission",
            "phone_permission",
            "location_enabled",
            "sdk",
            "mock_active",
        ],
    ),
    ("cell", &["radio", "area", "id", "mcc", "mnc", "registered", "dbm", "connection_status"]),
    ("wifi", &["SSID", "BSSID", "rssi", "frequency", "scan_ms"]),
];

impl Snapshot {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_SNAPSHOT {
            return Err("environment snapshot exceeds 2 MiB".into());
        }
        let snapshot: Self =
            serde_json::from_str(text).map_err(|e| format!("invalid environment snapshot: {e}"))?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    /// Every rejection is about provenance or the platform metadata needed to trust the values.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported environment snapshot version".into());
        }
        if self.simulated {
            return Err(
                "the environment snapshot was taken while the simulation was active; stop the simulation and collect again"
                    .into(),
            );
        }
        if self.coordinate_system != "wgs84" {
            return Err("environment snapshot must use WGS84 coordinates".into());
        }
        if self.captured_ms == 0 {
            return Err("environment snapshot is missing the capture time".into());
        }
        if self.location.from_last_known {
            return Err(
                "the environment snapshot has no fresh location fix; wait for a real fix and collect again"
                    .into(),
            );
        }
        if self.cells.len() > MAX_SNAPSHOT_CELLS {
            return Err(format!("environment snapshot has more than {MAX_SNAPSHOT_CELLS} cells"));
        }
        if self.wifi.len() > MAX_SNAPSHOT_WIFI {
            return Err(format!(
                "environment snapshot has more than {MAX_SNAPSHOT_WIFI} Wi-Fi records"
            ));
        }
        self.position()?;
        Ok(())
    }

    fn position(&self) -> Result<Position, String> {
        let mut position = Position::new(self.location.latitude, self.location.longitude);
        for (name, value) in [
            ("altitude", self.location.altitude),
            ("accuracy", self.location.accuracy),
            ("speed", self.location.speed),
            ("bearing", self.location.bearing),
        ] {
            if let Some(value) = value {
                match name {
                    "altitude" => position.altitude = value,
                    "accuracy" => position.accuracy = value,
                    "speed" => position.speed = value,
                    _ => position.bearing = value,
                }
            }
        }
        position.validate().map_err(str::to_owned)?;
        Ok(position)
    }
}

/// Build a saved place from a measured snapshot. Attachments are stored exactly as read, so an
/// unreadable cell tower coordinate stays recorded-only instead of becoming an invented position.
pub fn collected(
    snapshot: &Snapshot,
    name: &str,
    id: &str,
    cells: bool,
    wifi: bool,
) -> Result<Address, String> {
    snapshot.validate()?;
    let position = snapshot.position()?;
    let mut address = Map::new();
    let mut field = |key: &str, value: Value| {
        address.insert(key.to_owned(), value);
    };
    field("id", json!(id));
    field("name", json!(name));
    field("from", json!(1));
    field("pinned", json!(false));
    field("latitude", json!(position.latitude));
    field("longitude", json!(position.longitude));
    field("altitude", json!(position.altitude));
    field("accuracy", json!(position.accuracy));
    field("speed", json!(position.speed));
    field("bearing", json!(position.bearing));
    if let Some(provider) = &snapshot.location.provider {
        field("provider", json!(provider));
    }
    field(
        "collected",
        json!({
            "captured_ms": snapshot.captured_ms,
            "coordinate_system": snapshot.coordinate_system,
            "source": "device-probe",
            "cells": snapshot.cells.len(),
            "wifi": snapshot.wifi.len(),
            // A fix that carries no satellite count is recorded as such, not filled in with a
            // plausible number: consumers do read this extra.
            "extras_satellites": snapshot.location.extras_satellites,
            "capture": snapshot.capture,
        }),
    );
    // Keep an empty array explicit: an explicit empty attachment means "measured, found none".
    if cells {
        field("nearbyCells", Value::Array(snapshot.cells.clone()));
    }
    if wifi {
        field("nearbyWifis", Value::Array(snapshot.wifi.clone()));
    }
    let address = Address(address);
    address.validate()?;
    Ok(address)
}

pub fn position(address: &Address) -> Result<Position, String> {
    let mut position = address.position()?;
    for (name, field) in [
        ("accuracy", &mut position.accuracy),
        ("speed", &mut position.speed),
        ("bearing", &mut position.bearing),
    ] {
        if let Some(value) = address.0.get(name) {
            *field = value.as_f64().ok_or_else(|| format!("address.{name} must be a number"))?;
        }
    }
    position.validate()?;
    Ok(position)
}

pub fn prepare(
    address: &Address,
    cells: AttachmentAction,
    wifi: AttachmentAction,
) -> Result<Environment, String> {
    let position = position(address)?;
    let center = Coordinate { latitude: position.latitude, longitude: position.longitude };
    let entries = |key: &str| -> Result<&[Value], String> {
        match address.0.get(key) {
            None | Some(Value::Null) => Ok(&[]),
            Some(Value::Array(values)) => Ok(values),
            _ => Err(format!("address.{key} must be an array")),
        }
    };
    let cells = match cells {
        AttachmentAction::Keep => Change::Keep,
        AttachmentAction::Clear => Change::Clear,
        AttachmentAction::Apply => {
            let values = entries("nearbyCells")?;
            if values.is_empty() {
                Change::Keep
            } else {
                let cells = values.iter().map(cell).collect::<Result<Vec<_>, _>>()?;
                let radius_m =
                    cells.iter().map(|c| center.distance_to(c.position)).fold(0.0, f64::max)
                        + 1000.0;
                let region = CellRegion {
                    center,
                    radius_m,
                    source: "saved-place-attachments".into(),
                    fetched_at_ms: 0,
                    cells,
                };
                region.validate()?;
                Change::Apply(region)
            }
        }
    };
    let wifi = match wifi {
        AttachmentAction::Keep => Change::Keep,
        AttachmentAction::Clear => Change::Clear,
        AttachmentAction::Apply => {
            let values = entries("nearbyWifis")?;
            if values.is_empty() {
                Change::Keep
            } else {
                let config = WifiConfig {
                    enabled: true,
                    targets: values.iter().map(wifi_target).collect::<Result<_, _>>()?,
                };
                config.validate()?;
                Change::Apply(config)
            }
        }
    };
    let result = Environment { cells, wifi };
    result.validate()?;
    Ok(result)
}

fn cell(value: &Value) -> Result<Cell, String> {
    if value.get("identity").is_some() {
        return serde_json::from_value(value.clone())
            .map_err(|e| format!("invalid cell attachment: {e}"));
    }
    // A measured cell keeps the identity the platform reported; only a tower coordinate the
    // source actually supplied can make it applicable to the simulation.
    if value.get("radio").is_some() && value.get("area").is_some() {
        return measured_cell(value);
    }
    // The original S-code cell model supplies tower coordinates explicitly.
    let number =
        |key: &str| value[key].as_u64().ok_or_else(|| format!("cell attachment is missing {key}"));
    let text = |key: &str| -> Result<String, String> {
        if let Some(text) = value[key].as_str() {
            Ok(text.to_owned())
        } else if let Some(number) = value[key].as_u64() {
            Ok(if key == "mnc" { format!("{number:02}") } else { number.to_string() })
        } else {
            Err(format!("cell attachment is missing {key}"))
        }
    };
    let mcc = text("mcc")?;
    let mnc = text("mnc")?;
    let area = number("lac")?;
    let id = number("cellid")?;
    let radio = value["radio_type"].as_str().unwrap_or("").to_ascii_uppercase();
    let identity =
        match radio.as_str() {
            "GSM" => json!({"radio":"gsm","mcc":mcc,"mnc":mnc,"lac":area,"cid":id}),
            "UMTS" | "WCDM" | "WCDMA" => {
                json!({"radio":"wcdma","mcc":mcc,"mnc":mnc,"lac":area,"cid":id})
            }
            "LTE" => json!({"radio":"lte","mcc":mcc,"mnc":mnc,"tac":area,"ci":id}),
            "NR" => json!({"radio":"nr","mcc":mcc,"mnc":mnc,"tac":area,"nci":id}),
            _ => return Err(
                "unsupported legacy cell radio; keep or edit this attachment before applying it"
                    .into(),
            ),
        };
    let identity: CellIdentity = serde_json::from_value(identity).map_err(|e| e.to_string())?;
    identity.validate()?;
    let coordinate = |name| {
        value[name]
            .as_f64()
            .ok_or_else(|| format!("cell attachment is missing tower coordinate {name}"))
    };
    Ok(Cell {
        identity,
        position: Coordinate { latitude: coordinate("lat")?, longitude: coordinate("lon")? },
        range_m: value.get("range").and_then(Value::as_f64).unwrap_or(0.0),
    })
}

/// Convert a canonical measured cell attachment. Missing tower coordinates are reported, not
/// replaced by the simulated device position: the cell was heard, but its location is unknown.
fn measured_cell(value: &Value) -> Result<Cell, String> {
    let radio = value["radio"].as_str().unwrap_or("").to_ascii_lowercase();
    let plmn = || -> Result<(String, String), String> {
        let mcc = value["mcc"].as_str().ok_or("measured cell is missing mcc")?.to_owned();
        let mnc = value["mnc"].as_str().ok_or("measured cell is missing mnc")?.to_owned();
        Ok((mcc, mnc))
    };
    let area = value["area"].as_u64().ok_or("measured cell is missing area")?;
    let id = value["id"].as_u64().ok_or("measured cell is missing id")?;
    let narrow =
        |value: u64| -> Result<u32, String> { value.try_into().map_err(|_| "measured cell ID is out of range".to_owned()) };
    let (mcc, mnc) = plmn()?;
    let identity = match radio.as_str() {
        "gsm" => {
            CellIdentity::Gsm { mcc, mnc, lac: narrow(area)?, cid: narrow(id)?, arfcn: None, bsic: None }
        }
        "wcdma" => CellIdentity::Wcdma {
            mcc,
            mnc,
            lac: narrow(area)?,
            cid: narrow(id)?,
            psc: None,
            uarfcn: None,
        },
        "lte" => {
            CellIdentity::Lte { mcc, mnc, tac: narrow(area)?, ci: narrow(id)?, pci: None, earfcn: None }
        }
        "nr" => CellIdentity::Nr { mcc, mnc, tac: narrow(area)?, nci: id, pci: None, nrarfcn: None },
        "cdma" => CellIdentity::Cdma { sid: narrow(area)?, nid: 0, bid: narrow(id)? },
        _ => return Err("measured cell has an unsupported radio type".into()),
    };
    identity.validate()?;
    let coordinate = |name| {
        value[name]
            .as_f64()
            .ok_or_else(|| format!("cell attachment is missing tower coordinate {name}"))
    };
    Ok(Cell {
        identity,
        position: Coordinate { latitude: coordinate("lat")?, longitude: coordinate("lon")? },
        range_m: value.get("range").and_then(Value::as_f64).unwrap_or(0.0),
    })
}

fn wifi_target(value: &Value) -> Result<WifiTarget, String> {
    let mut target = json!({
        "id": match value["id"].as_str() { Some(id) => id.to_owned(), None => crate::scode::new_id()? },
        "ssid":value.get("ssid").or_else(|| value.get("SSID")).ok_or("Wi-Fi attachment is missing SSID")?,
        "bssid":value.get("bssid").or_else(|| value.get("BSSID")).ok_or("Wi-Fi attachment is missing BSSID")?,
    });
    for (key, alias) in [("rssi", "level"), ("link_speed", "linkSpeed"), ("frequency", "frequency")]
    {
        if let Some(value) = value.get(key).or_else(|| value.get(alias)) {
            target[key] = value.clone();
        }
    }
    let target: WifiTarget =
        serde_json::from_value(target).map_err(|e| format!("invalid Wi-Fi attachment: {e}"))?;
    target.validate()?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Value {
        // Mirrors the probe app's document exactly, including fields this version does not read.
        json!({
            "version": 1,
            "captured_ms": 1_700_000_000_000u64,
            "simulated": false,
            "coordinate_system": "wgs84",
            "location": {
                "latitude": 31.230416,
                "longitude": 121.473701,
                "altitude": 12.5,
                "accuracy": 8.0,
                "speed": 1.2,
                "bearing": 87.0,
                "provider": "gps",
                "from_last_known": false,
                "fix_time_ms": 1_700_000_000_000u64,
                "fix_elapsed_ms": 123_456u64,
                "received_elapsed_ms": 123_470u64,
                "mock": false,
                "extras_satellites": 9
            },
            "cells": [
                {"radio": "lte", "mcc": "460", "mnc": "00", "area": 12345, "id": 67890,
                 "registered": true, "dbm": -95, "connection_status": 1},
                {"radio": "nr", "mcc": "460", "mnc": "01", "area": 12345, "id": 68719476735u64,
                 "registered": false, "dbm": -110}
            ],
            "wifi": [
                {"SSID": "测试网络", "BSSID": "02:00:00:00:00:01", "rssi": -45,
                 "frequency": 2437, "scan_ms": 1_700_000_000_000u64}
            ],
            "capture": {
                "package": "me.idk.justlocation.probe",
                "location_permission": "granted",
                "wifi_permission": "granted",
                "phone_permission": "granted",
                "location_enabled": true,
                "sdk": 35,
                "mock_active": false
            }
        })
    }

    #[test]
    fn the_probe_contract_lists_every_field_this_version_accepts() {
        // Every documented field is accepted, so a probe that writes one of them cannot be
        // rejected by a stale contract.
        let value = snapshot();
        for (section, fields) in SNAPSHOT_FIELDS {
            let target = if section.is_empty() {
                &value
            } else if *section == "cell" {
                &value["cells"][0]
            } else if *section == "wifi" {
                &value["wifi"][0]
            } else {
                &value[*section]
            };
            for field in *fields {
                assert!(
                    target.get(field).is_some(),
                    "the snapshot fixture must carry {section}.{field}"
                );
            }
        }
        Snapshot::parse(&value.to_string()).expect("the recorded probe contract must be accepted");
        // An unlisted field is a contract change and must be made explicit instead of ignored.
        let mut unknown = value.clone();
        unknown["location"]["new_platform_field"] = json!(1);
        assert!(Snapshot::parse(&unknown.to_string()).is_err());
        let mut unknown = value;
        unknown["capture"]["new_capture_field"] = json!(1);
        assert!(Snapshot::parse(&unknown.to_string()).is_err());
    }

    fn parsed(value: Value) -> Snapshot {
        Snapshot::parse(&value.to_string()).unwrap()
    }

    #[test]
    fn a_measured_snapshot_becomes_a_place_with_explicit_provenance_and_attachments() {
        let snapshot = parsed(snapshot());
        let address = collected(&snapshot, "实测地点", "jl-test", true, true).unwrap();
        assert_eq!(address.0["name"], "实测地点");
        assert_eq!(address.0["from"], 1);
        assert_eq!(address.0["latitude"], 31.230416);
        assert_eq!(address.0["accuracy"], 8.0);
        assert_eq!(address.0["provider"], "gps");
        assert_eq!(address.0["collected"]["source"], "device-probe");
        assert_eq!(address.0["collected"]["cells"], 2);
        assert_eq!(address.0["collected"]["wifi"], 1);
        assert_eq!(address.0["collected"]["capture"]["location_enabled"], true);
        assert_eq!(address.0["collected"]["extras_satellites"], 9);
        assert_eq!(address.0["speed"], 1.2);
        assert_eq!(address.0["bearing"], 87.0);
        assert_eq!(address.0["nearbyCells"].as_array().unwrap().len(), 2);
        assert_eq!(address.0["nearbyWifis"][0]["BSSID"], "02:00:00:00:00:01");
        // The saved place is a normal library entry.
        address.validate().unwrap();
        let encoded = crate::scode::encode(&address).unwrap();
        let decoded = crate::scode::decode(&encoded).unwrap();
        assert_eq!(decoded, address);
    }

    #[test]
    fn an_explicit_empty_attachment_stays_distinguishable_from_a_missing_one() {
        let mut value = snapshot();
        value["cells"] = json!([]);
        value["wifi"] = json!([]);
        let snapshot = parsed(value);
        let kept = collected(&snapshot, "空环境", "jl-empty", true, true).unwrap();
        assert_eq!(kept.0["nearbyCells"], json!([]));
        assert_eq!(kept.0["nearbyWifis"], json!([]));
        let dropped = collected(&snapshot, "无附件", "jl-none", false, false).unwrap();
        assert!(!dropped.0.contains_key("nearbyCells"));
        assert!(!dropped.0.contains_key("nearbyWifis"));
    }

    #[test]
    fn simulated_stale_or_foreign_snapshots_are_rejected() {
        for (field, value, expected) in [
            ("simulated", json!(true), "simulation was active"),
            ("captured_ms", json!(0), "missing the capture time"),
            ("coordinate_system", json!("gcj02"), "must use WGS84"),
            ("version", json!(2), "unsupported environment snapshot version"),
        ] {
            let mut candidate = snapshot();
            candidate[field] = value;
            let error = Snapshot::parse(&candidate.to_string()).unwrap_err();
            assert!(error.contains(expected), "{field}: {error}");
        }
        let mut cached = snapshot();
        cached["location"]["from_last_known"] = json!(true);
        assert!(
            Snapshot::parse(&cached.to_string())
                .unwrap_err()
                .contains("no fresh location fix")
        );
        let mut invalid = snapshot();
        invalid["location"]["latitude"] = json!(95.0);
        assert!(Snapshot::parse(&invalid.to_string()).is_err());
        let mut too_many = snapshot();
        too_many["cells"] = json!(vec![json!({"radio": "lte"}); MAX_SNAPSHOT_CELLS + 1]);
        assert!(
            Snapshot::parse(&too_many.to_string()).unwrap_err().contains("more than 32 cells")
        );
        assert!(Snapshot::parse("{}").is_err());
        assert!(Snapshot::parse(&" ".repeat(MAX_SNAPSHOT + 1)).is_err());
    }

    #[test]
    fn switching_to_a_measured_place_requires_a_tower_coordinate_instead_of_inventing_one() {
        let snapshot = parsed(snapshot());
        let address = collected(&snapshot, "实测地点", "jl-test", true, true).unwrap();
        // The probe records the identity the platform reported; it has no tower coordinate.
        let error = prepare(&address, AttachmentAction::Apply, AttachmentAction::Apply).unwrap_err();
        assert!(error.contains("tower coordinate lat"), "{error}");
        // Wi-Fi records carry everything the Wi-Fi channel needs.
        let wifi = prepare(&address, AttachmentAction::Keep, AttachmentAction::Apply).unwrap();
        assert!(matches!(wifi.cells, Change::Keep));
        match wifi.wifi {
            Change::Apply(config) => {
                assert!(config.enabled);
                assert_eq!(config.targets.len(), 1);
                assert_eq!(config.targets[0].bssid, "02:00:00:00:00:01");
                assert_eq!(config.targets[0].rssi, -45);
            }
            other => panic!("expected an applied Wi-Fi config, got {other:?}"),
        }
        // A tower coordinate supplied later by a dataset or an edit makes the same place usable.
        let mut located = address.clone();
        located.0["nearbyCells"][0]["lat"] = json!(31.2);
        located.0["nearbyCells"][0]["lon"] = json!(121.5);
        located.0["nearbyCells"][1]["lat"] = json!(31.21);
        located.0["nearbyCells"][1]["lon"] = json!(121.51);
        let applied = prepare(&located, AttachmentAction::Apply, AttachmentAction::Keep).unwrap();
        match applied.cells {
            Change::Apply(region) => {
                assert_eq!(region.cells.len(), 2);
                assert_eq!(region.source, "saved-place-attachments");
                assert!(region.radius_m > 1000.0);
                assert!(matches!(region.cells[0].identity, CellIdentity::Lte { .. }));
                assert!(matches!(region.cells[1].identity, CellIdentity::Nr { .. }));
            }
            other => panic!("expected an applied cell region, got {other:?}"),
        }
        assert!(matches!(applied.wifi, Change::Keep));
    }

    #[test]
    fn attachment_modes_are_independent_and_legacy_entries_still_apply() {
        // Legacy S-code attachments keep their explicit tower coordinates.
        let address: Address = serde_json::from_value(json!({
            "id": "old", "latitude": 31.0, "longitude": 121.0,
            "nearbyCells": [{
                "mcc": 460, "mnc": 1, "lac": 100, "cellid": 200, "radio_type": "LTE",
                "lat": 31.01, "lon": 121.01, "range": 500.0
            }],
            "nearbyWifis": [{"SSID": "旧网络", "BSSID": "02:00:00:00:00:02", "level": -60}]
        }))
        .unwrap();
        let applied = prepare(&address, AttachmentAction::Apply, AttachmentAction::Apply).unwrap();
        match applied.cells {
            Change::Apply(region) => {
                assert_eq!(region.cells.len(), 1);
                assert!(matches!(
                    &region.cells[0].identity,
                    CellIdentity::Lte { mcc, mnc, tac, ci, .. }
                        if mcc == "460" && mnc == "01" && *tac == 100 && *ci == 200
                ));
                assert_eq!(region.cells[0].range_m, 500.0);
            }
            other => panic!("expected an applied cell region, got {other:?}"),
        }
        let cleared = prepare(&address, AttachmentAction::Clear, AttachmentAction::Clear).unwrap();
        assert!(matches!(cleared.cells, Change::Clear));
        assert!(matches!(cleared.wifi, Change::Clear));
        // Keep is the default for both attachments.
        let kept = prepare(&address, AttachmentAction::Keep, AttachmentAction::Keep).unwrap();
        assert_eq!(kept, Environment::default());
    }

    #[test]
    fn malformed_attachments_fail_before_anything_is_applied() {
        let mut address: Address = serde_json::from_value(json!({
            "id": "old", "latitude": 31.0, "longitude": 121.0,
            "nearbyCells": [{"mcc": "460", "mnc": "00", "radio": "lte", "area": 1, "id": 2,
                             "lat": 31.0, "lon": 121.0}],
            "nearbyWifis": []
        }))
        .unwrap();
        assert!(prepare(&address, AttachmentAction::Apply, AttachmentAction::Apply).is_ok());
        address.0["nearbyCells"] = json!({"not": "an array"});
        assert!(
            prepare(&address, AttachmentAction::Apply, AttachmentAction::Keep)
                .unwrap_err()
                .contains("must be an array")
        );
        address.0["nearbyCells"] = json!([{"radio": "lte", "mcc": "460", "mnc": "00"}]);
        assert!(prepare(&address, AttachmentAction::Apply, AttachmentAction::Keep).is_err());
        address.0["nearbyCells"] = json!([]);
        address.0["nearbyWifis"] = json!([{"BSSID": "02:00:00:00:00:01"}]);
        assert!(
            prepare(&address, AttachmentAction::Keep, AttachmentAction::Apply)
                .unwrap_err()
                .contains("missing SSID")
        );
        // An attachment that is present but empty keeps the current environment instead of
        // silently clearing it.
        address.0["nearbyWifis"] = json!([]);
        let kept = prepare(&address, AttachmentAction::Apply, AttachmentAction::Apply).unwrap();
        assert!(matches!(kept.cells, Change::Keep));
        assert!(matches!(kept.wifi, Change::Keep));
    }

    #[test]
    fn position_fields_only_overwrite_values_the_address_actually_carries() {
        let address: Address =
            serde_json::from_value(json!({"id": "x", "latitude": 1.0, "longitude": 2.0})).unwrap();
        let plain = position(&address).unwrap();
        assert_eq!(plain.altitude, 0.0);
        assert_eq!(plain.accuracy, 5.0);
        assert_eq!(plain.speed, 0.0);
        assert_eq!(plain.bearing, 0.0);
        let mut typed = address.clone();
        typed.0.insert("accuracy".into(), json!(12.5));
        typed.0.insert("speed".into(), json!(3.0));
        typed.0.insert("bearing".into(), json!(90.0));
        let specified = position(&typed).unwrap();
        assert_eq!(specified.accuracy, 12.5);
        assert_eq!(specified.speed, 3.0);
        assert_eq!(specified.bearing, 90.0);
        let mut invalid = address.clone();
        invalid.0.insert("accuracy".into(), json!("high"));
        assert!(position(&invalid).unwrap_err().contains("must be a number"));
    }
}

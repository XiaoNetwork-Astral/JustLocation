//! Compatible location sharing without coupling address metadata to live simulation state.
use crate::Position;
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_SIZE: usize = 2 * 1024 * 1024;

/// Preserve all address fields and attachments, including fields unknown to this version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Address(pub Map<String, Value>);

impl Address {
    pub fn position(&self) -> Result<Position, String> {
        let number = |name: &str| {
            self.0
                .get(name)
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("address.{name} must be a number"))
        };
        let mut position = Position::new(number("latitude")?, number("longitude")?);
        position.altitude = if self.0.contains_key("altitude") { number("altitude")? } else { 0.0 };
        position.validate().map_err(str::to_owned)?;
        Ok(position)
    }

    pub fn validate(&self) -> Result<(), String> {
        self.position()?;
        for field in ["nearbyCells", "nearbyWifis"] {
            if let Some(value) = self.0.get(field) {
                if !value.is_null() && !value.is_array() {
                    return Err(format!("address.{field} must be an array or null"));
                }
            }
        }
        Ok(())
    }
}

pub fn encode(address: &Address) -> Result<String, String> {
    address.validate()?;
    let document = json!({
        "header": {
            "Version": "1.1",
            "AppPackageName": "me.idk.justlocation.joystick",
            "AppVersionName": env!("CARGO_PKG_VERSION"),
            "AppVersionCode": "1",
            "Language": "zh"
        },
        "data": { "address": address }
    });
    let bytes = serde_json::to_vec(&document).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SIZE {
        return Err("S code JSON exceeds 2 MiB".into());
    }
    let mut compressor = GzEncoder::new(Vec::new(), Compression::default());
    compressor.write_all(&bytes).map_err(|e| e.to_string())?;
    let compressed = compressor.finish().map_err(|e| e.to_string())?;
    let base64 = STANDARD.encode(compressed);
    // Android Base64.DEFAULT emits LF after every 76 characters, including the final line.
    let mut result = String::new();
    for line in base64.as_bytes().chunks(76) {
        result.push_str(std::str::from_utf8(line).unwrap());
        result.push('\n');
    }
    if result.len() > MAX_SIZE {
        return Err("S code exceeds 2 MiB".into());
    }
    Ok(result)
}

pub fn decode(code: &str) -> Result<Address, String> {
    if code.len() > MAX_SIZE {
        return Err("S code exceeds 2 MiB".into());
    }
    let mut text = code.trim();
    for prefix in ["S Code:", "S Code：", "S-Code:", "S-Code：", "S码：", "S码:"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest;
            break;
        }
    }
    let compact: String = text.chars().filter(|ch| !ch.is_ascii_whitespace()).collect();
    let compressed = STANDARD
        .decode(&compact)
        .or_else(|_| STANDARD_NO_PAD.decode(&compact))
        .map_err(|_| "invalid S code Base64".to_owned())?;
    let mut bytes = Vec::new();
    GzDecoder::new(compressed.as_slice())
        .take(MAX_SIZE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "invalid S code GZIP data".to_owned())?;
    if bytes.len() > MAX_SIZE {
        return Err("S code JSON exceeds 2 MiB".into());
    }
    let document: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid S code JSON")?;
    let address = document
        .get("data")
        .and_then(|data| data.get("address"))
        .and_then(Value::as_object)
        .ok_or("S code is missing data.address")?;
    let address = Address(address.clone());
    address.validate()?;
    Ok(address)
}

/// Import creates a new local identity; attachment choices are independent and explicit.
/// It never changes live simulation or installs the imported attachment data globally.
pub fn import(code: &str, keep_cells: bool, keep_wifi: bool) -> Result<Address, String> {
    let mut address = decode(code)?;
    address.0.insert("id".into(), json!(new_id()?));
    address.0.insert("from".into(), json!(2));
    if !keep_cells {
        address.0.remove("nearbyCells");
    }
    if !keep_wifi {
        address.0.remove("nearbyWifis");
    }
    Ok(address)
}

pub(crate) fn new_id() -> Result<String, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let time = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(format!("jl-{time:x}-{:x}-{sequence:x}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_java_gzip_fixture_with_android_default_base64_framing() {
        // Independently generated with JDK GZIPOutputStream and a 76-column LF Base64 encoder.
        // This is synthetic compatibility data, not an export collected from an installed app.
        let decoded = decode(include_str!("../tests/fixtures/address.scode")).unwrap();
        assert_eq!(decoded.0["id"], "fixture-original");
        assert_eq!(decoded.0["name"], "兼容测试");
        assert_eq!(decoded.0["latitude"], 31.1234567890123);
        assert_eq!(decoded.0["longitude"], 121.987654321098);
        assert_eq!(decoded.0["nearbyCells"][0]["nci"], 68719476735u64);
        assert_eq!(decoded.0["nearbyCells"][0]["mnc"], "01");
        assert_eq!(decoded.0["nearbyWifis"][0]["SSID"], "测试网络");
        assert_eq!(decoded.0["extras"]["note"], "保留");
    }

    fn address() -> Address {
        serde_json::from_value(json!({
            "id":"old", "from":0, "name":"测试地点", "country":"中国", "city":"测试城市",
            "address":"测试地址", "latitude":31.1234567890123, "longitude":121.987654321098,
            "altitude":12.34567890123, "stickTime":1234567890123u64,
            "nearbyCells":[{"nci":68719476735u64,"mcc":"460","mnc":"01"}],
            "nearbyWifis":[{"BSSID":"02:00:00:00:00:01","SSID":"测试网络"}],
            "extras":{"note":"保留附件"}, "futureField":{"enabled":true}
        }))
        .unwrap()
    }

    #[test]
    fn round_trip_preserves_double_precision_metadata_and_attachments() {
        let original = address();
        let code = encode(&original).unwrap();
        assert!(code.ends_with('\n'));
        let lines: Vec<_> = code.lines().collect();
        assert!(lines.len() > 1);
        assert!(lines[..lines.len() - 1].iter().all(|line| line.len() == 76));
        assert_eq!(decode(&code).unwrap(), original);
        assert_eq!(decode(&code.replace('\n', "")).unwrap(), original);
    }

    #[test]
    fn accepts_all_prefixes_wrapping_and_unpadded_base64() {
        let original = address();
        let code = encode(&original).unwrap();
        for prefix in ["S Code:", "S Code：", "S-Code:", "S-Code：", "S码：", "S码:"] {
            assert_eq!(
                decode(&format!(" {prefix} \r\n{}", code.replace('\n', " \r\n"))).unwrap(),
                original
            );
        }
        assert_eq!(decode(code.replace('\n', "").trim_end_matches('=')).unwrap(), original);
    }

    #[test]
    fn imports_use_new_ids_and_independent_attachment_choices() {
        let code = encode(&address()).unwrap();
        for keep_cells in [false, true] {
            for keep_wifi in [false, true] {
                let first = import(&code, keep_cells, keep_wifi).unwrap();
                let second = import(&code, keep_cells, keep_wifi).unwrap();
                assert_ne!(first.0["id"], second.0["id"]);
                assert_ne!(first.0["id"], "old");
                assert_eq!(first.0["from"], 2);
                assert_eq!(first.0.contains_key("nearbyCells"), keep_cells);
                assert_eq!(first.0.contains_key("nearbyWifis"), keep_wifi);
                assert_eq!(first.0["extras"], address().0["extras"]);
                assert_eq!(first.0["futureField"], address().0["futureField"]);
            }
        }
    }

    fn compressed_json(value: Value) -> String {
        let mut compressor = GzEncoder::new(Vec::new(), Compression::default());
        compressor.write_all(value.to_string().as_bytes()).unwrap();
        STANDARD.encode(compressor.finish().unwrap())
    }

    #[test]
    fn header_is_informational_and_backup_maps_are_not_locations() {
        assert_eq!(
            decode(&compressed_json(json!({"data":{"address":address()}}))).unwrap(),
            address()
        );
        assert!(
            decode(&compressed_json(
                json!({"header":{"Version":"1.1"},"data":{"address":{"key":address()}}})
            ))
            .is_err()
        );
    }

    #[test]
    fn rejects_corruption_out_of_range_and_expansion_bombs() {
        for invalid in ["", "not-base64", "e30=", "S码：!!!!"] {
            assert!(decode(invalid).is_err());
        }
        let mut invalid = address();
        invalid.0.insert("latitude".into(), json!(91));
        assert!(encode(&invalid).is_err());
        assert!(decode(&compressed_json(json!({"data":{"address":invalid}}))).is_err());
        let bomb = compressed_json(
            json!({"data":{"address":{"latitude":0,"longitude":0,"name":"a".repeat(MAX_SIZE)}}}),
        );
        assert_eq!(decode(&bomb).unwrap_err(), "S code JSON exceeds 2 MiB");
        let code = encode(&address()).unwrap().replace('\n', "");
        let mut bytes = STANDARD.decode(code).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert!(decode(&STANDARD.encode(bytes)).is_err());
    }
}

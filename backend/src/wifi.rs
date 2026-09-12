//! Saved Wi-Fi simulation targets.
//! BSSID validation accepts a matching substring for compatibility with existing input.
//! Default signal and link values are model parameters, not radio measurements.

use serde::{Deserialize, Serialize};

/// Default signal and link parameters.
pub const DEFAULT_RSSI: i32 = 200;
pub const DEFAULT_LINK_SPEED: i32 = 866;
pub const DEFAULT_FREQUENCY: i32 = 5745;
/// Maximum saved targets.
pub const MAX_TARGETS: usize = 32;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WifiConfig {
    /// Wi-Fi simulation is disabled by default.
    pub enabled: bool,
    /// Targets saved by the control panel.
    pub targets: Vec<WifiTarget>,
}

impl WifiConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.targets.len() > MAX_TARGETS {
            return Err("too many wifi targets");
        }
        for target in &self.targets {
            target.validate()?;
        }
        let mut ids = std::collections::HashSet::new();
        for target in &self.targets {
            if !ids.insert(&target.id) {
                return Err("duplicate wifi id");
            }
        }
        Ok(())
    }
}

fn default_rssi() -> i32 {
    DEFAULT_RSSI
}
fn default_link_speed() -> i32 {
    DEFAULT_LINK_SPEED
}
fn default_frequency() -> i32 {
    DEFAULT_FREQUENCY
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WifiTarget {
    pub id: String,
    pub ssid: String,
    pub bssid: String,
    #[serde(default = "default_rssi")]
    pub rssi: i32,
    #[serde(default = "default_link_speed")]
    pub link_speed: i32,
    #[serde(default = "default_frequency")]
    pub frequency: i32,
}

/// Accept a MAC address substring with case-insensitive hex and colon or hyphen separators.
fn find_mac(text: &str) -> bool {
    let bytes = text.as_bytes();
    let hex = |b: u8| b.is_ascii_hexdigit();
    let separator = |b: u8| b == b':' || b == b'-';
    if bytes.len() < 17 {
        return false;
    }
    for start in 0..=bytes.len() - 17 {
        let candidate = &bytes[start..start + 17];
        let shaped = (0..17).all(|index| {
            if index % 3 == 2 { separator(candidate[index]) } else { hex(candidate[index]) }
        });
        if shaped {
            return true;
        }
    }
    false
}

impl WifiTarget {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.trim().is_empty() || self.id.len() > 64 {
            return Err("invalid wifi id");
        }
        // Allow spaces in SSIDs, but reject empty names and control characters.
        if self.ssid.trim().is_empty()
            || self.ssid.len() > 64
            || self.ssid.chars().any(char::is_control)
        {
            return Err("invalid wifi ssid");
        }
        if self.bssid.len() > 64 || self.bssid.chars().any(char::is_control) {
            return Err("invalid wifi bssid");
        }
        Ok(())
    }

    pub fn looks_like_bssid(text: &str) -> bool {
        find_mac(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> WifiTarget {
        WifiTarget {
            id: "w1".into(),
            ssid: "Home".into(),
            bssid: "aa:bb:cc:dd:ee:ff".into(),
            rssi: DEFAULT_RSSI,
            link_speed: DEFAULT_LINK_SPEED,
            frequency: DEFAULT_FREQUENCY,
        }
    }

    #[test]
    fn missing_signal_fields_fall_back_to_the_original_defaults() {
        let parsed: WifiTarget =
            serde_json::from_str(r#"{"id":"w1","ssid":"Home","bssid":"aa:bb:cc:dd:ee:ff"}"#)
                .unwrap();
        assert_eq!(parsed.rssi, 200);
        assert_eq!(parsed.link_speed, 866);
        assert_eq!(parsed.frequency, 5745);
        assert_eq!(parsed, target());
    }

    #[test]
    fn validation_rejects_empty_names_and_control_characters() {
        let mut value = target();
        value.ssid = "   ".into();
        assert!(value.validate().is_err());
        value = target();
        value.ssid = "Home\u{1}".into();
        assert!(value.validate().is_err());
        value = target();
        value.id = String::new();
        assert!(value.validate().is_err());
        assert!(target().validate().is_ok());
    }

    #[test]
    fn bssid_check_matches_the_lenient_behaviour_of_the_original() {
        for text in [
            "aa:bb:cc:dd:ee:ff",
            "AA-BB-CC-DD-EE-FF",
            "网关 aa:bb:cc:dd:ee:ff 的地址",
            "前置aa:bb:cc:dd:ee:ff",
            // Compatibility requires accepting valid address substrings within longer text.
            "aa:bb:cc:dd:ee:ff:00",
        ] {
            assert!(WifiTarget::looks_like_bssid(text), "{text} should be accepted");
        }
        for text in ["", "aa:bb:cc:dd:ee", "zz:bb:cc:dd:ee:ff", "aa bb cc dd ee ff"] {
            assert!(!WifiTarget::looks_like_bssid(text), "{text} should be rejected");
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(
            serde_json::from_str::<WifiTarget>(
                r#"{"id":"w1","ssid":"Home","bssid":"aa:bb:cc:dd:ee:ff","channel":6}"#
            )
            .is_err()
        );
    }

    #[test]
    fn a_config_defaults_to_off_with_no_targets() {
        let config = WifiConfig::default();
        assert!(!config.enabled);
        assert!(config.targets.is_empty());
        config.validate().unwrap();
        let partial: WifiConfig = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert!(partial.enabled);
        assert!(partial.targets.is_empty());
        assert!(serde_json::from_str::<WifiConfig>(r#"{"wifi_enabled":true}"#).is_err());
    }

    #[test]
    fn a_config_round_trips_with_its_targets() {
        let text =
            r#"{"enabled":true,"targets":[{"id":"w1","ssid":"Home","bssid":"aa:bb:cc:dd:ee:ff"}]}"#;
        let config: WifiConfig = serde_json::from_str(text).unwrap();
        config.validate().unwrap();
        assert!(config.enabled);
        assert_eq!(config.targets.len(), 1);
        assert_eq!(config.targets[0].rssi, DEFAULT_RSSI);
        let again: WifiConfig =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(again, config);
    }

    #[test]
    fn validation_covers_the_list_not_just_one_entry() {
        let mut config = WifiConfig { enabled: true, targets: Vec::new() };
        let mut broken = WifiTarget {
            id: "w2".into(),
            ssid: "  ".into(),
            bssid: String::new(),
            rssi: DEFAULT_RSSI,
            link_speed: DEFAULT_LINK_SPEED,
            frequency: DEFAULT_FREQUENCY,
        };
        config.targets.push(WifiTarget {
            id: "w1".into(),
            ssid: "Home".into(),
            bssid: "aa:bb:cc:dd:ee:ff".into(),
            ..broken.clone()
        });
        config.targets.push(broken.clone());
        assert!(config.validate().is_err());
        config.targets.pop();
        config.targets.push(WifiTarget { ssid: "Other".into(), ..broken.clone() });
        assert!(config.validate().is_ok());
        broken.id = "w1".into();
        broken.ssid = "Other".into();
        config.targets.push(broken);
        assert!(config.validate().is_err());
        let too_many = WifiConfig {
            enabled: true,
            targets: (0..MAX_TARGETS + 1)
                .map(|index| WifiTarget {
                    id: format!("w{index}"),
                    ssid: format!("net{index}"),
                    bssid: String::new(),
                    rssi: DEFAULT_RSSI,
                    link_speed: DEFAULT_LINK_SPEED,
                    frequency: DEFAULT_FREQUENCY,
                })
                .collect(),
        };
        assert!(too_many.validate().is_err());
    }
}

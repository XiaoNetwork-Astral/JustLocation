//! GNSS status and NMEA configuration. Status uses preset satellite data;
//! NMEA suppression drops matching callbacks without synthesizing sentences.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GnssConfig {
    /// Deliver preset GNSS/GPS status callbacks.
    pub gnss_enabled: bool,
    /// Suppress NMEA callbacks within the configured scope.
    pub nmea_enabled: bool,
}

impl GnssConfig {
    /// Boolean switches need no range checks; keep the common channel validation interface.
    pub fn validate(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_both_channels_off() {
        let config = GnssConfig::default();
        assert!(!config.gnss_enabled);
        assert!(!config.nmea_enabled);
        config.validate().unwrap();
    }

    #[test]
    fn reads_a_partial_object_and_rejects_unknown_fields() {
        let partial: GnssConfig = serde_json::from_str(r#"{"gnss_enabled":true}"#).unwrap();
        assert!(partial.gnss_enabled);
        assert!(!partial.nmea_enabled);
        assert!(serde_json::from_str::<GnssConfig>(r#"{"gnss":true}"#).is_err());
    }
}

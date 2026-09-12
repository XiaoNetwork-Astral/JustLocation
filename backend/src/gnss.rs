//! GNSS and NMEA configuration. The bridge synthesizes status, LNAV and raw GPS
//! measurements from one orbit model, and NMEA from the simulated position.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GnssConfig {
    /// Deliver synthetic GNSS status, measurements and navigation messages.
    pub gnss_enabled: bool,
    /// Deliver synthetic NMEA within the configured scope.
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

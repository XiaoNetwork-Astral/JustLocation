use crate::{
    Config, cells::CellRegion, gnss::GnssConfig, telephony::TelephonyConfig, wifi::WifiConfig,
};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Stored {
    pub(super) version: u32,
    pub(super) config: Option<Config>,
    #[serde(default)]
    pub(super) scopes: Option<crate::scope::Scopes>,
    pub(super) cell_region: Option<CellRegion>,
    #[serde(default)]
    pub(super) telephony: TelephonyConfig,
    /// Older files default to disabled satellite channels.
    #[serde(default)]
    pub(super) gnss: GnssConfig,
    /// Older files default to disabled Wi-Fi output.
    #[serde(default)]
    pub(super) wifi: WifiConfig,
    #[serde(default)]
    pub(super) steps: crate::steps::StepConfig,
    #[serde(default)]
    pub(super) realism: crate::realism::RealismConfig,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            version: 4,
            config: None,
            scopes: Some(crate::scope::Scopes::default()),
            cell_region: None,
            telephony: TelephonyConfig::default(),
            gnss: GnssConfig::default(),
            wifi: WifiConfig::default(),
            steps: crate::steps::StepConfig::default(),
            realism: crate::realism::RealismConfig::default(),
        }
    }
}

impl Stored {
    pub(super) fn load(path: &Path) -> io::Result<Self> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        if value.get("version").is_none() {
            let config: Config = serde_json::from_value(value)?;
            return Ok(Self {
                scopes: Some(crate::scope::Scopes::shared(config.scope.clone())),
                config: Some(config),
                ..Self::default()
            });
        }
        let mut stored: Self = serde_json::from_value(value)?;
        if !matches!(stored.version, 2..=4) {
            return Err(io::Error::other("unsupported configuration version"));
        }
        if stored.scopes.is_none() {
            if stored.version == 4 {
                return Err(io::Error::other("configuration is missing feature scopes"));
            }
            stored.scopes = Some(crate::scope::Scopes::shared(
                stored.config.as_ref().map_or(crate::Scope::All, |c| c.scope.clone()),
            ));
        }
        stored.scopes.as_ref().unwrap().validate().map_err(io::Error::other)?;
        if let Some(region) = &stored.cell_region {
            region.validate().map_err(io::Error::other)?;
        }
        stored.telephony.validate().map_err(io::Error::other)?;
        stored.gnss.validate().map_err(io::Error::other)?;
        stored.wifi.validate().map_err(io::Error::other)?;
        stored.steps.validate().map_err(io::Error::other)?;
        stored.realism.validate().map_err(io::Error::other)?;
        Ok(stored)
    }

    pub(super) fn save(&self, path: &Path) -> io::Result<()> {
        crate::storage::atomic_save(path, &serde_json::to_vec(self)?)
    }
}

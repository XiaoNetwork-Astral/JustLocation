use crate::{
    cell_providers::{Provider, ProviderKind},
    storage::atomic_save,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::Path};

#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(super) struct Settings {
    #[serde(deserialize_with = "crate::cell_providers::lenient_kind")]
    pub(super) primary: ProviderKind,
    #[serde(default, deserialize_with = "crate::cell_providers::lenient_optional_kind")]
    pub(super) fallback: Option<ProviderKind>,
    pub(super) opencellid_key: String,
    pub(super) custom_endpoint: String,
    pub(super) custom_token: Option<String>,
    /// Opt-in daily updates. Older settings default to disabled.
    #[serde(default)]
    pub(super) dataset_auto_update: bool,
    /// Country MCC for automatic updates; zero means unset.
    #[serde(default)]
    pub(super) dataset_mcc: u16,
    /// UTC date of the last successful check, in YYYY-MM-DD format.
    #[serde(default)]
    pub(super) dataset_last_check_day: Option<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            primary: ProviderKind::OpenCellId,
            fallback: Some(ProviderKind::Custom),
            opencellid_key: String::new(),
            custom_endpoint: String::new(),
            custom_token: None,
            dataset_auto_update: false,
            dataset_mcc: 0,
            dataset_last_check_day: None,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SettingsUpdate {
    primary: ProviderKind,
    fallback: Option<ProviderKind>,
    opencellid_key: Option<String>,
    custom_endpoint: String,
    custom_token: Option<String>,
    /// Omitted fields preserve existing values for older clients.
    #[serde(default)]
    dataset_auto_update: Option<bool>,
    #[serde(default)]
    dataset_mcc: Option<u16>,
}

impl Settings {
    pub(super) fn provider(&self, kind: ProviderKind) -> Provider {
        match kind {
            ProviderKind::OpenCellId => Provider::OpenCellId { key: self.opencellid_key.clone() },
            ProviderKind::Custom => Provider::Custom {
                endpoint: self.custom_endpoint.clone(),
                token: self.custom_token.clone(),
            },
        }
    }
    pub(super) fn public(&self) -> Value {
        json!({
            "primary": self.primary,
            "fallback": self.fallback,
            "opencellid_configured": !self.opencellid_key.is_empty(),
            "custom_endpoint": self.custom_endpoint,
            "custom_token_configured": self.custom_token.is_some(),
            "dataset_auto_update": self.dataset_auto_update,
            "dataset_mcc": self.dataset_mcc,
            "dataset_last_check_day": self.dataset_last_check_day,
        })
    }

    pub(super) fn load(directory: &Path) -> Result<Settings, String> {
        match fs::read(directory.join("cell-providers.json")) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| "cannot read provider settings".into())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(_) => Err("cannot read provider settings".into()),
        }
    }

    pub(super) fn save(&self, directory: &Path) -> Result<(), String> {
        atomic_save(
            &directory.join("cell-providers.json"),
            &serde_json::to_vec(self).map_err(|_| "cannot save provider settings".to_string())?,
        )
        .map_err(|_| "cannot save provider settings".to_string())
    }

    pub(super) fn update(&mut self, update: SettingsUpdate) -> Result<(), String> {
        if !update.custom_endpoint.is_empty() {
            crate::cell_http::validate_endpoint(&update.custom_endpoint)
                .map_err(|_| "enter an HTTP(S) address; keep credentials in their own field")?;
        }
        if (update.primary == ProviderKind::Custom || update.fallback == Some(ProviderKind::Custom))
            && update.custom_endpoint.is_empty()
        {
            return Err("enter the custom provider address".into());
        }
        if update.fallback == Some(update.primary) {
            return Err("the primary and fallback providers must differ".into());
        }
        self.primary = update.primary;
        self.fallback = update.fallback;
        // Changing servers must not silently send the old server's credential elsewhere.
        if self.custom_endpoint != update.custom_endpoint {
            self.custom_token = None;
        }
        self.custom_endpoint = update.custom_endpoint;
        if let Some(key) = update.opencellid_key {
            self.opencellid_key = credential(key)?;
        }
        if let Some(token) = update.custom_token {
            let token = credential(token)?;
            self.custom_token = (!token.is_empty()).then_some(token);
        }
        if let Some(enabled) = update.dataset_auto_update {
            self.dataset_auto_update = enabled;
        }
        if let Some(mcc) = update.dataset_mcc {
            self.dataset_mcc = mcc;
        }
        Ok(())
    }
}

fn credential(value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err("invalid credential format".into());
    }
    Ok(value)
}

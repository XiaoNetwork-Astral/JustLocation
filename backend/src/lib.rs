#[cfg(test)]
mod long_route_tests;
mod storage;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub mod cell_http;
pub mod cell_providers;
pub mod cell_service;
pub mod cells;
pub mod cli;
pub mod coordinates;
pub mod dataset;
pub mod geocoding;
pub mod gnss;
pub mod jitter;
pub mod maps;
pub mod motion;
pub mod operators;
pub mod protocol;
pub mod realism;
pub mod record;
mod record_journal;
pub mod route;
pub mod route_store;
pub mod routing;
pub mod scode;
pub mod smoothing;
pub mod steps;
pub mod telephony;
pub mod transport;
pub mod wifi;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
    pub accuracy: f64,
    pub speed: f64,
    pub bearing: f64,
}

impl Position {
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self { latitude, longitude, altitude: 0.0, accuracy: 5.0, speed: 0.0, bearing: 0.0 }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.latitude.is_finite() || !(-90.0..=90.0).contains(&self.latitude) {
            return Err("latitude must be finite and between -90 and 90");
        }
        if !self.longitude.is_finite() || !(-180.0..=180.0).contains(&self.longitude) {
            return Err("longitude must be finite and between -180 and 180");
        }
        if !self.altitude.is_finite()
            || !self.accuracy.is_finite()
            || self.accuracy < 0.0
            || !self.speed.is_finite()
            || self.speed < 0.0
            || !self.bearing.is_finite()
            || !(0.0..360.0).contains(&self.bearing)
        {
            return Err("invalid altitude, accuracy, speed or bearing");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "packages", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    All,
    Apps(BTreeSet<String>),
}

impl Scope {
    pub fn apps<const N: usize>(packages: [&str; N]) -> Self {
        Self::Apps(packages.into_iter().map(str::to_owned).collect())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub position: Position,
    pub scope: Scope,
}

#[derive(Clone, Default)]
pub struct Engine {
    config: Option<Config>,
    running: bool,
}

impl Engine {
    pub fn set_scope(&mut self, scope: Scope) -> Result<(), &'static str> {
        if let Scope::Apps(packages) = &scope {
            if packages.is_empty() || packages.iter().any(|name| name.trim().is_empty()) {
                return Err("select at least one application");
            }
        }
        self.config
            .get_or_insert_with(|| Config { position: Position::new(0., 0.), scope: Scope::All })
            .scope = scope;
        Ok(())
    }
    pub fn start(&mut self, config: Config) -> Result<(), &'static str> {
        if self.running {
            return Err("already running; stop before starting another session");
        }
        config.position.validate()?;
        if let Scope::Apps(packages) = &config.scope {
            if packages.is_empty() || packages.iter().any(|name| name.trim().is_empty()) {
                return Err("select at least one application");
            }
        }
        self.config = Some(config);
        self.running = true;
        Ok(())
    }

    pub fn update_position(&mut self, position: Position) -> Result<(), &'static str> {
        position.validate()?;
        let config = self.config.as_mut().ok_or("no configured session")?;
        config.position = position;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
    }
    pub fn is_running(&self) -> bool {
        self.running
    }
    pub fn config(&self) -> Option<&Config> {
        self.config.as_ref()
    }

    pub fn output_for(&self, package: &str) -> Option<&Position> {
        if !self.running {
            return None;
        }
        let config = self.config.as_ref()?;
        match &config.scope {
            Scope::All => Some(&config.position),
            Scope::Apps(packages) if packages.contains(package) => Some(&config.position),
            _ => None,
        }
    }
}

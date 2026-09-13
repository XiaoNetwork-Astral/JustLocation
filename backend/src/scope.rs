use crate::Scope;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    Position,
    Route,
    Wifi,
    Sim,
}

impl Feature {
    pub fn key(self) -> &'static str {
        match self {
            Self::Position => "position",
            Self::Route => "route",
            Self::Wifi => "wifi",
            Self::Sim => "sim",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scopes {
    pub position: Scope,
    pub route: Scope,
    pub wifi: Scope,
    pub sim: Scope,
}

impl Default for Scopes {
    fn default() -> Self {
        Self::shared(Scope::All)
    }
}

impl Scopes {
    pub fn shared(scope: Scope) -> Self {
        Self { position: scope.clone(), route: scope.clone(), wifi: scope.clone(), sim: scope }
    }

    pub fn get(&self, feature: Feature) -> &Scope {
        match feature {
            Feature::Position => &self.position,
            Feature::Route => &self.route,
            Feature::Wifi => &self.wifi,
            Feature::Sim => &self.sim,
        }
    }

    pub fn set(&mut self, feature: Option<Feature>, scope: Scope) -> Result<(), &'static str> {
        scope.validate()?;
        match feature {
            Some(Feature::Position) => self.position = scope,
            Some(Feature::Route) => self.route = scope,
            Some(Feature::Wifi) => self.wifi = scope,
            Some(Feature::Sim) => self.sim = scope,
            None => *self = Self::shared(scope),
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        for feature in [Feature::Position, Feature::Route, Feature::Wifi, Feature::Sim] {
            self.get(feature).validate()?;
        }
        Ok(())
    }
}

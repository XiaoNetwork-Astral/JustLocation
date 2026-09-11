//! One snapshot for telephony queries and callbacks. Signal levels are simulation settings;
//! supplier records do not contain measurements from the simulated receiver.
use crate::cells::{CellIdentity, CellRegion, Coordinate};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Current physical/eSIM slot mapping. No subscriber or hardware identifiers are collected.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetectedSubscription {
    pub id: i32,
    pub slot: u8,
    pub mcc: String,
    pub mnc: String,
    pub country: String,
    pub carrier: String,
}

pub fn validate_detected(subscriptions: &[DetectedSubscription]) -> Result<(), &'static str> {
    let mut ids = HashSet::new();
    let mut slots = HashSet::new();
    if subscriptions.len() > 2 { return Err("at most two subscriptions are supported"); }
    for sub in subscriptions {
        if sub.id < 0 || sub.id == i32::MAX || sub.slot > 1 || !ids.insert(sub.id) || !slots.insert(sub.slot)
            || (!sub.mcc.is_empty() && (sub.mcc.len() != 3 || !sub.mcc.bytes().all(|b| b.is_ascii_digit())))
            || (!sub.mnc.is_empty() && (!(2..=3).contains(&sub.mnc.len()) || !sub.mnc.bytes().all(|b| b.is_ascii_digit())))
            || (!sub.country.is_empty() && (sub.country.len() != 2 || !sub.country.bytes().all(|b| b.is_ascii_lowercase())))
            || sub.carrier.len() > 128 || sub.carrier.chars().any(char::is_control)
        { return Err("invalid detected subscription"); }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subscription {
    pub id: i32,
    pub slot: u8,
    pub mcc: String,
    pub mnc: String,
    pub country: String,
    pub carrier: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdma_sid: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TelephonyConfig {
    pub cells_enabled: bool,
    pub sim_enabled: bool,
    pub radius_m: f64,
    pub subscriptions: Vec<Subscription>,
}
impl Default for TelephonyConfig {
    fn default() -> Self {
        Self {
            cells_enabled: false,
            sim_enabled: false,
            radius_m: 500.,
            subscriptions: Vec::new(),
        }
    }
}
impl TelephonyConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.radius_m.is_finite() || !(1. ..=200_000.).contains(&self.radius_m) {
            return Err("invalid telephony radius");
        }
        if self.subscriptions.len() > 2 {
            return Err("at most two subscriptions are supported");
        }
        let mut ids = HashSet::new();
        let mut slots = HashSet::new();
        for sub in &self.subscriptions {
            if sub.id < 0
                || sub.id == i32::MAX
                || sub.slot > 1
                || !ids.insert(sub.id)
                || !slots.insert(sub.slot)
            {
                return Err("invalid or duplicate subscription id/slot");
            }
            if !sub.enabled { continue; }
            if sub.mcc.len() != 3
                || !(2..=3).contains(&sub.mnc.len())
                || !sub
                    .mcc
                    .bytes()
                    .chain(sub.mnc.bytes())
                    .all(|b| b.is_ascii_digit())
                || sub.country.len() != 2
                || !sub.country.bytes().all(|b| b.is_ascii_lowercase())
                || sub.carrier.trim().is_empty()
                || sub.carrier.len() > 128
                || sub.carrier.chars().any(char::is_control)
                || sub.cdma_sid.is_some_and(|sid| sid > 32767)
            {
                return Err("invalid subscription operator");
            }
        }
        if (self.cells_enabled || self.sim_enabled) && !self.subscriptions.iter().any(|s| s.enabled)
        {
            return Err("select an active subscription first");
        }
        Ok(())
    }

    pub fn frame(&self, region: Option<&CellRegion>, target: Coordinate) -> TelephonyFrame {
        // A moving target must remain inside the acquired area with the entire search circle.
        // An uncovered area stays empty, rather than reusing cells from the previous target.
        let availability = if !self.cells_enabled {
            Availability::Disabled
        } else {
            match region {
                None => Availability::MissingRegion,
                Some(region)
                    if target.validate().is_err()
                        || region.center.distance_to(target) + self.radius_m
                            > region.radius_m + 0.01 =>
                {
                    Availability::OutsideRegion
                }
                Some(_) => Availability::Ready,
            }
        };
        let mut groups = Vec::new();
        let mut subscriptions = Vec::new();
        for sub in self.subscriptions.iter().filter(|s| s.enabled) {
            if self.sim_enabled {
                subscriptions.push(sub.clone());
            }
            if !self.cells_enabled {
                continue;
            }
            let mut nearby = Vec::new();
            if availability == Availability::Ready {
                for cell in &region.unwrap().cells {
                    let matches = match &cell.identity {
                        CellIdentity::Gsm { mcc, mnc, .. }
                        | CellIdentity::Wcdma { mcc, mnc, .. }
                        | CellIdentity::Lte { mcc, mnc, .. }
                        | CellIdentity::Nr { mcc, mnc, .. } => mcc == &sub.mcc && mnc == &sub.mnc,
                        CellIdentity::Cdma { sid, .. } => sub.cdma_sid == Some(*sid),
                    };
                    let distance = target.distance_to(cell.position);
                    if matches && distance <= self.radius_m {
                        nearby.push((cell, distance));
                    }
                }
            }
            nearby.sort_by(|(a, da), (b, db)| {
                da.total_cmp(db)
                    .then_with(|| a.identity.key().cmp(&b.identity.key()))
            });
            nearby.truncate(16);
            let cells = nearby
                .into_iter()
                .enumerate()
                .map(|(index, (cell, distance))| {
                    // Smooth, deterministic test signal, bounded to each Android radio's dBm range.
                    let loss = (20. * (1. + distance / 100.).log10()).round() as i32;
                    let dbm = match cell.identity {
                        CellIdentity::Gsm { .. } => (-65 - loss).clamp(-113, -51),
                        CellIdentity::Wcdma { .. } => (-75 - loss).clamp(-120, -24),
                        CellIdentity::Lte { .. } => (-80 - loss).clamp(-140, -44),
                        CellIdentity::Nr { .. } => (-80 - loss).clamp(-140, -44),
                        CellIdentity::Cdma { .. } => (-75 - loss).clamp(-120, -1),
                    };
                    OutputCell {
                        identity: cell.identity.clone(),
                        position: cell.position,
                        registered: index == 0,
                        dbm,
                    }
                })
                .collect();
            groups.push(CellGroup {
                subscription_id: sub.id,
                slot: sub.slot,
                cells,
            });
        }
        TelephonyFrame {
            availability,
            subscriptions,
            groups,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Disabled,
    Ready,
    MissingRegion,
    OutsideRegion,
}
#[derive(Clone, Debug, Serialize)]
pub struct OutputCell {
    pub identity: CellIdentity,
    pub position: Coordinate,
    pub registered: bool,
    pub dbm: i32,
}
#[derive(Clone, Debug, Serialize)]
pub struct CellGroup {
    pub subscription_id: i32,
    pub slot: u8,
    pub cells: Vec<OutputCell>,
}
#[derive(Clone, Debug, Serialize)]
pub struct TelephonyFrame {
    pub availability: Availability,
    pub subscriptions: Vec<Subscription>,
    pub groups: Vec<CellGroup>,
}

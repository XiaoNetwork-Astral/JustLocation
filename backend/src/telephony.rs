//! One snapshot for telephony queries and callbacks. Signal levels are simulation settings;
//! supplier records do not contain measurements from the simulated receiver.
use crate::cells::{Cell, CellIdentity, CellRegion, Coordinate};
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
    if subscriptions.len() > 2 {
        return Err("at most two subscriptions are supported");
    }
    for sub in subscriptions {
        if sub.id < 0
            || sub.id == i32::MAX
            || sub.slot > 1
            || !ids.insert(sub.id)
            || !slots.insert(sub.slot)
            || (!sub.mcc.is_empty()
                && (sub.mcc.len() != 3 || !sub.mcc.bytes().all(|b| b.is_ascii_digit())))
            || (!sub.mnc.is_empty()
                && (!(2..=3).contains(&sub.mnc.len())
                    || !sub.mnc.bytes().all(|b| b.is_ascii_digit())))
            || (!sub.country.is_empty()
                && (sub.country.len() != 2 || !sub.country.bytes().all(|b| b.is_ascii_lowercase())))
            || sub.carrier.len() > 128
            || sub.carrier.chars().any(char::is_control)
        {
            return Err("invalid detected subscription");
        }
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
        Self { cells_enabled: false, sim_enabled: false, radius_m: 500., subscriptions: Vec::new() }
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
            if !sub.enabled {
                continue;
            }
            if sub.mcc.len() != 3
                || !(2..=3).contains(&sub.mnc.len())
                || !sub.mcc.bytes().chain(sub.mnc.bytes()).all(|b| b.is_ascii_digit())
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
        let mut synthesized = false;
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
            // Synthesize fallback identities only when no acquired cells are available.
            // The response marks them as synthetic; they do not identify real towers.
            let synthesized_here = nearby.is_empty() && availability == Availability::Ready;
            let placeholder = if synthesized_here {
                synthesize_cells(region.unwrap(), &sub, target, self.radius_m)
            } else {
                Vec::new()
            };
            for cell in &placeholder {
                nearby.push((cell, target.distance_to(cell.position)));
            }
            if !placeholder.is_empty() {
                synthesized = true;
            }
            nearby.sort_by(|(a, da), (b, db)| {
                da.total_cmp(db).then_with(|| a.identity.key().cmp(&b.identity.key()))
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
            groups.push(CellGroup { subscription_id: sub.id, slot: sub.slot, cells });
        }
        TelephonyFrame { availability, subscriptions, groups, synthesized }
    }
}

/// Generate deterministic cell identities and offsets for a position using the template PLMN.
/// Keep repeated queries stable and include LTE and NR identities.
fn synthesize_cells(
    region: &CellRegion,
    subscription: &Subscription,
    target: Coordinate,
    radius_m: f64,
) -> Vec<Cell> {
    // Use a matching template's radio type, or LTE if none matches.
    let radio_hint = region
        .cells
        .iter()
        .find(|cell| {
            matches!(
                &cell.identity,
                CellIdentity::Lte { mcc, mnc, .. } | CellIdentity::Nr { mcc, mnc, .. }
                    if mcc == &subscription.mcc && mnc == &subscription.mnc
            )
        })
        .map(|cell| cell.identity.clone());
    // Quantize coordinates to derive stable identities and offsets.
    let grid_latitude = (target.latitude * 1e4).round() as i64;
    let grid_longitude = (target.longitude * 1e4).round() as i64;
    let seed = (grid_latitude.wrapping_mul(31) ^ grid_longitude.wrapping_mul(17)) as u64;
    // Place cells 100 to 350 meters away, within the default 500-meter query radius.
    let step = (radius_m * 0.25).clamp(60.0, 400.0);
    let offsets = [(0.35, -0.55), (0.8, 0.3), (-0.5, 0.85)];
    let mut cells = Vec::with_capacity(offsets.len());
    for (index, (north, east)) in offsets.iter().enumerate() {
        let latitude_delta = (step * north) / 111_320.0;
        let longitude_delta =
            (step * east) / (111_320.0 * target.latitude.to_radians().cos().abs().max(0.05));
        let position = Coordinate {
            latitude: (target.latitude + latitude_delta).clamp(-90.0, 90.0),
            longitude: target.longitude + longitude_delta,
        };
        let mix = seed.wrapping_add((index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        // Alternate the template's radio type with the other LTE/NR type.
        let want_nr = match radio_hint {
            Some(CellIdentity::Nr { .. }) => index != 1,
            _ => index == 1,
        };
        let identity = if want_nr {
            CellIdentity::Nr {
                mcc: subscription.mcc.clone(),
                mnc: subscription.mnc.clone(),
                tac: 60_001,
                nci: 2_000_000_000 + (mix % 9_000_000_000),
                pci: Some((mix % 1008) as u32),
                nrarfcn: Some(636_666),
            }
        } else {
            CellIdentity::Lte {
                mcc: subscription.mcc.clone(),
                mnc: subscription.mnc.clone(),
                tac: 60_001,
                ci: 1_000_000 + (mix % 9_000_000) as u32,
                pci: Some((mix % 504) as u32),
                earfcn: Some(1_650),
            }
        };
        // Keep identities within platform ranges.
        if identity.validate().is_err() {
            continue;
        }
        cells.push(Cell { identity, position, range_m: 1_000.0 });
    }
    cells
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
    /// True when cells are synthetic fallbacks rather than acquired tower records.
    pub synthesized: bool,
}

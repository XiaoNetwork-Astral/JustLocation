//! Geographic cell data acquired from an explicit source, independent of the device's radio.
//! No identity or frequency is inferred from a coordinate. Unknown optional values stay absent.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coordinate {
    pub latitude: f64,
    pub longitude: f64,
}

impl Coordinate {
    pub fn validate(self) -> Result<(), &'static str> {
        if !self.latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.latitude)
            || !self.longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude)
        {
            return Err("invalid cell coordinate");
        }
        Ok(())
    }

    pub fn distance_to(self, other: Self) -> f64 {
        let dlat = (other.latitude - self.latitude).to_radians();
        let dlon = (other.longitude - self.longitude).to_radians();
        let h = (dlat / 2.0).sin().powi(2)
            + self.latitude.to_radians().cos()
                * other.latitude.to_radians().cos()
                * (dlon / 2.0).sin().powi(2);
        2.0 * 6_371_008.8 * h.clamp(0.0, 1.0).sqrt().asin()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "radio", rename_all = "snake_case", deny_unknown_fields)]
pub enum CellIdentity {
    Gsm {
        mcc: String,
        mnc: String,
        lac: u32,
        cid: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        arfcn: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bsic: Option<u32>,
    },
    Wcdma {
        mcc: String,
        mnc: String,
        lac: u32,
        cid: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        psc: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uarfcn: Option<u32>,
    },
    Lte {
        mcc: String,
        mnc: String,
        tac: u32,
        ci: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pci: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        earfcn: Option<u32>,
    },
    Nr {
        mcc: String,
        mnc: String,
        tac: u32,
        nci: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pci: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nrarfcn: Option<u32>,
    },
    Cdma {
        sid: u32,
        nid: u32,
        bid: u32,
    },
}

fn plmn(mcc: &str, mnc: &str) -> bool {
    mcc.len() == 3
        && (2..=3).contains(&mnc.len())
        && mcc.bytes().chain(mnc.bytes()).all(|byte| byte.is_ascii_digit())
}
fn optional(value: Option<u32>, maximum: u32) -> bool {
    value.is_none_or(|value| value <= maximum)
}

impl CellIdentity {
    pub fn validate(&self) -> Result<(), &'static str> {
        // Android 15 CellIdentity constructors' ranges. A missing field maps to UNAVAILABLE later.
        let valid = match self {
            Self::Gsm { mcc, mnc, lac, cid, arfcn, bsic } => {
                plmn(mcc, mnc)
                    && *lac <= 65535
                    && *cid <= 65535
                    && optional(*arfcn, 65535)
                    && optional(*bsic, 63)
            }
            Self::Wcdma { mcc, mnc, lac, cid, psc, uarfcn } => {
                plmn(mcc, mnc)
                    && *lac <= 65535
                    && *cid <= 268435455
                    && optional(*psc, 511)
                    && optional(*uarfcn, 16383)
            }
            Self::Lte { mcc, mnc, tac, ci, pci, earfcn } => {
                plmn(mcc, mnc)
                    && *tac <= 65535
                    && *ci <= 268435455
                    && optional(*pci, 503)
                    && optional(*earfcn, 262143)
            }
            Self::Nr { mcc, mnc, tac, nci, pci, nrarfcn } => {
                plmn(mcc, mnc)
                    && *tac <= 16777215
                    && *nci <= 68719476735
                    && optional(*pci, 1007)
                    && optional(*nrarfcn, 3279165)
            }
            Self::Cdma { sid, nid, bid } => *sid <= 32767 && *nid <= 65535 && *bid <= 65535,
        };
        if valid { Ok(()) } else { Err("invalid cell identity") }
    }

    pub(crate) fn key(&self) -> String {
        // Identity excludes optional channel/physical codes, so two observations cannot create
        // duplicate logical cells just because one of them has more metadata.
        match self {
            Self::Gsm { mcc, mnc, lac, cid, .. } => format!("gsm:{mcc}:{mnc}:{lac}:{cid}"),
            Self::Wcdma { mcc, mnc, lac, cid, .. } => format!("wcdma:{mcc}:{mnc}:{lac}:{cid}"),
            Self::Lte { mcc, mnc, tac, ci, .. } => format!("lte:{mcc}:{mnc}:{tac}:{ci}"),
            Self::Nr { mcc, mnc, tac, nci, .. } => format!("nr:{mcc}:{mnc}:{tac}:{nci}"),
            Self::Cdma { sid, nid, bid } => format!("cdma:{sid}:{nid}:{bid}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub identity: CellIdentity,
    pub position: Coordinate,
    /// Metres; zero means the source has not supplied a coverage estimate.
    pub range_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellRegion {
    pub center: Coordinate,
    /// Area acquired from the source, not an assertion of complete real-world coverage.
    pub radius_m: f64,
    pub source: String,
    pub fetched_at_ms: u64,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NearbyCell {
    pub cell: Cell,
    pub distance_m: f64,
}

impl CellRegion {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.center.validate()?;
        if !self.radius_m.is_finite()
            || !(1.0..=200_000.0).contains(&self.radius_m)
            || self.source.trim().is_empty()
            || self.source.len() > 256
            || self.cells.len() > 128
        {
            return Err("invalid cell region (up to 128 cells)");
        }
        let mut identities = HashSet::new();
        for cell in &self.cells {
            cell.identity.validate()?;
            cell.position.validate()?;
            if !cell.range_m.is_finite() || !(0.0..=200_000.0).contains(&cell.range_m) {
                return Err("invalid cell range");
            }
            if !identities.insert(cell.identity.key()) {
                return Err("duplicate cell identity");
            }
            if self.center.distance_to(cell.position) > self.radius_m + 0.01 {
                return Err("cell is outside the acquired region");
            }
        }
        Ok(())
    }

    pub fn nearby(
        &self,
        target: Coordinate,
        radius_m: f64,
        limit: usize,
    ) -> Result<Vec<NearbyCell>, &'static str> {
        self.validate()?;
        target.validate()?;
        if !radius_m.is_finite() || radius_m <= 0.0 || !(1..=32).contains(&limit) {
            return Err("invalid nearby cell query");
        }
        if self.center.distance_to(target) + radius_m > self.radius_m + 0.01 {
            return Err("target query is outside the acquired cell region");
        }
        let mut found: Vec<_> = self
            .cells
            .iter()
            .filter_map(|cell| {
                let distance_m = target.distance_to(cell.position);
                (distance_m <= radius_m).then(|| NearbyCell { cell: cell.clone(), distance_m })
            })
            .collect();
        found.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
        found.truncate(limit);
        Ok(found)
    }
}

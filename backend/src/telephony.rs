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
            // 兜底：这一片没有任何真实小区数据可用时，就**为虚拟位置造几个**。
            //
            // <p>为什么需要：应用会拿小区去问云端"我在哪"，而"没有小区"本身就是一种强信号——
            // 地图软件会退回到它自己算出来的真实位置。造几个出来，等于把这条退路堵上。
            //
            // <p>**这是伪造数据**：造出来的编号只在本机成立，云端查不到、也不对应任何真实基站。
            // 所以它一旦生效，帧里会带上 `synthesized`，状态与日志里都会写明，
            // 不让使用者把"看起来有基站"误当成"读到了真实基站"。
            let synthesized_here = nearby.is_empty() && availability == Availability::Ready;
            // 造出来的小区要先在一个活到本轮结束的容器里，`nearby` 借的是它的元素；
            // 直接把临时值塞进去会借用失败（`cell` 每轮就析构了）。
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
            synthesized,
        }
    }
}

/// 为虚拟位置造几个小区。
///
/// <p>身份沿用 `template` 的 PLMN（MCC/MNC）——那是使用者在配置里指定的运营商，
/// 换掉就等于宣称"你连的是另一家网"，比"位置是假的"更容易露馅。编号由目标坐标确定性地派生：
/// 同一个位置每次造出来的编号一致（应用反复查询不会看到编号在跳），换了位置才变。
///
/// <p>造 LTE 与 NR 各一到两个：现代 ROM 上"只有 4G 没有 5G"本身就是一种可疑特征，
/// 而这台设备平时是 NSA/SA 双模。
fn synthesize_cells(
    region: &CellRegion,
    subscription: &Subscription,
    target: Coordinate,
    radius_m: f64,
) -> Vec<Cell> {
    // 从模板里找一个同 PLMN 的小区，取它的制式；没有就用 LTE。
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
    // 目标坐标的定点网格：用于派生确定性的编号与偏移。
    let grid_latitude = (target.latitude * 1e4).round() as i64;
    let grid_longitude = (target.longitude * 1e4).round() as i64;
    let seed = (grid_latitude.wrapping_mul(31) ^ grid_longitude.wrapping_mul(17)) as u64;
    // 小区放在 100~350 米之内：超出 `telephony.radius_m`（默认 500 米）就会被自己的筛选丢掉。
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
        let mix = seed.wrapping_add(index as u64 * 0x9e37_79b9_7f4a_7c15);
        // 先按模板决定制式：第 0、2 个跟模板走，第 1 个特意用另一种，凑出双制式。
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
        // 身份必须过平台侧的范围校验，否则这条带出去只会被拒。
        if identity.validate().is_err() {
            continue;
        }
        cells.push(Cell {
            identity,
            position,
            range_m: 1_000.0,
        });
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
    /// 帧里的小区是不是**伪造的兜底数据**（真实数据一条都没找到时才会为真）。
    /// 交给状态与日志如实报出去：伪造可以接受，"假装是真实基站"不行。
    pub synthesized: bool,
}

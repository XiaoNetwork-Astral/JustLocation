//! Virtual subscriptions occupy only confirmed empty slots. Persisted IDs never alias real SIMs.
use crate::telephony::{DetectedSubscription, Subscription, TelephonyConfig};
use serde::{Deserialize, Serialize};

pub const FIRST_ID: i32 = 1_900_000_000;
pub const LAST_ID: i32 = 2_000_000_001;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VirtualSimConfig {
    pub subscriptions: Vec<Subscription>,
    pub default_slot: Option<u8>,
}

impl VirtualSimConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        crate::telephony::validate_subscriptions(&self.subscriptions)?;
        if self.subscriptions.iter().any(|s| !(FIRST_ID..=LAST_ID).contains(&s.id)) {
            return Err("virtual subscription IDs must be in 1900000000..=2000000001");
        }
        if self
            .default_slot
            .is_some_and(|slot| !self.subscriptions.iter().any(|s| s.slot == slot && s.enabled))
        {
            return Err("the virtual default slot must name an enabled virtual subscription");
        }
        Ok(())
    }

    pub fn allocate_id(
        &self,
        slot: u8,
        physical: &[Subscription],
        detected: Option<&[DetectedSubscription]>,
    ) -> Result<i32, &'static str> {
        if let Some(existing) = self.subscriptions.iter().find(|s| s.slot == slot) {
            return Ok(existing.id);
        }
        (FIRST_ID..=LAST_ID)
            .find(|id| {
                !self.subscriptions.iter().chain(physical).any(|s| s.id == *id)
                    && !detected.is_some_and(|items| items.iter().any(|s| s.id == *id))
            })
            .ok_or("no virtual subscription ID is available")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    ModemsUnknown,
    SlotUnavailable,
    SubscriptionsUnknown,
    SlotOccupied,
    IdConflict,
    SimDisabled,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Blocked {
    pub slot: u8,
    pub reason: BlockReason,
}

pub struct Resolved {
    pub config: TelephonyConfig,
    pub ids: Vec<i32>,
    pub default_id: Option<i32>,
    pub blocked: Vec<Blocked>,
    pub has_real_subscriptions: bool,
}

/// A new publication token is needed only when the visible virtual model or its scope changes.
/// The phone acknowledges it after replacing its snapshot and invalidating application caches.
#[derive(Default)]
pub struct Publication {
    key: Option<(crate::Scope, Vec<Subscription>, Option<i32>, bool)>,
    token: Option<String>,
}

impl Publication {
    pub fn update(
        &mut self,
        frame: Option<&crate::telephony::TelephonyFrame>,
        scope: &crate::Scope,
    ) -> Option<String> {
        let key = frame.filter(|frame| !frame.virtual_ids.is_empty()).map(|frame| {
            (
                scope.clone(),
                frame
                    .subscriptions
                    .iter()
                    .filter(|s| frame.virtual_ids.contains(&s.id))
                    .cloned()
                    .collect(),
                frame.virtual_default_id,
                frame.has_real_subscriptions,
            )
        });
        if key.is_none() {
            self.key = None;
            self.token = Some("off".into());
        } else if self.key != key {
            self.key = key;
            self.token = crate::scode::new_id().ok();
        }
        self.token.clone()
    }
}

impl Resolved {
    pub fn frame(
        &self,
        region: Option<&crate::cells::CellRegion>,
        target: crate::cells::Coordinate,
    ) -> crate::telephony::TelephonyFrame {
        let mut frame = self.config.frame(region, target);
        frame.virtual_ids = self.ids.clone();
        frame.virtual_default_id = self.default_id;
        frame.virtual_blocked = self.blocked.clone();
        frame.has_real_subscriptions = self.has_real_subscriptions;
        frame
    }
}

impl TelephonyConfig {
    pub fn resolve(
        &self,
        detected: Option<&[DetectedSubscription]>,
        modem_count: Option<u8>,
    ) -> Resolved {
        let mut config = self.clone();
        config.virtual_sim = VirtualSimConfig::default();
        // Once virtual slots are configured, physical templates must match a real subscription.
        if !self.virtual_sim.subscriptions.is_empty() {
            config.subscriptions.retain(|s| {
                detected.is_some_and(|items| {
                    items.iter().any(|real| real.id == s.id && real.slot == s.slot)
                })
            });
        }
        let mut virtuals = Vec::new();
        let mut blocked = Vec::new();
        for sub in self.virtual_sim.subscriptions.iter().filter(|s| s.enabled) {
            let reason = if !self.sim_enabled {
                Some(BlockReason::SimDisabled)
            } else if modem_count.is_none() {
                Some(BlockReason::ModemsUnknown)
            } else if sub.slot >= modem_count.unwrap() {
                Some(BlockReason::SlotUnavailable)
            } else if detected.is_none() {
                Some(BlockReason::SubscriptionsUnknown)
            } else if detected.unwrap().iter().any(|real| real.slot == sub.slot) {
                Some(BlockReason::SlotOccupied)
            } else if detected.unwrap().iter().any(|real| real.id == sub.id)
                || self.subscriptions.iter().any(|physical| physical.id == sub.id)
            {
                Some(BlockReason::IdConflict)
            } else {
                None
            };
            if let Some(reason) = reason {
                blocked.push(Blocked { slot: sub.slot, reason });
            } else {
                virtuals.push(sub.clone());
            }
        }
        virtuals.sort_by_key(|s| s.slot);
        let default_id = virtuals
            .iter()
            .find(|s| Some(s.slot) == self.virtual_sim.default_slot)
            .or_else(|| virtuals.first())
            .map(|s| s.id);
        let ids = virtuals.iter().map(|s| s.id).collect();
        config.subscriptions.extend(virtuals);
        Resolved {
            config,
            ids,
            default_id,
            blocked,
            has_real_subscriptions: detected.is_some_and(|s| !s.is_empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn subscription(id: i32, slot: u8) -> Subscription {
        Subscription {
            id,
            slot,
            mcc: "460".into(),
            mnc: "01".into(),
            country: "cn".into(),
            carrier: format!("Test {slot}"),
            enabled: true,
            cdma_sid: None,
        }
    }
    fn detected(id: i32, slot: u8) -> DetectedSubscription {
        DetectedSubscription {
            id,
            slot,
            mcc: "460".into(),
            mnc: "11".into(),
            country: "cn".into(),
            carrier: "Real".into(),
        }
    }
    fn config() -> TelephonyConfig {
        TelephonyConfig {
            sim_enabled: true,
            virtual_sim: VirtualSimConfig {
                subscriptions: vec![subscription(FIRST_ID, 0), subscription(FIRST_ID + 1, 1)],
                default_slot: Some(1),
            },
            ..Default::default()
        }
    }
    #[test]
    fn unknown_modems_or_subscriptions_never_mean_empty_slots() {
        let config = config();
        for (real, count, reason) in [
            (None, None, BlockReason::ModemsUnknown),
            (None, Some(2), BlockReason::SubscriptionsUnknown),
            (Some(&[][..]), Some(0), BlockReason::SlotUnavailable),
        ] {
            let resolved = config.resolve(real, count);
            assert!(resolved.ids.is_empty());
            assert!(resolved.blocked.iter().all(|b| b.reason == reason));
        }
        let one_modem = config.resolve(Some(&[]), Some(1));
        assert_eq!(one_modem.ids, vec![FIRST_ID]);
        assert_eq!(one_modem.default_id, Some(FIRST_ID));
        assert_eq!(one_modem.blocked[0].reason, BlockReason::SlotUnavailable);
    }
    #[test]
    fn zero_one_and_two_real_subscriptions_preserve_identity_and_slot_ownership() {
        let mut config = config();
        config.subscriptions = vec![subscription(7, 0), subscription(9, 1)];
        let empty = config.resolve(Some(&[]), Some(2));
        assert_eq!(empty.ids, vec![FIRST_ID, FIRST_ID + 1]);
        assert_eq!(empty.default_id, Some(FIRST_ID + 1));
        assert_eq!(empty.config.subscriptions.len(), 2);
        assert!(!empty.has_real_subscriptions);
        let one = config.resolve(Some(&[detected(7, 0)]), Some(2));
        assert_eq!(one.ids, vec![FIRST_ID + 1]);
        assert_eq!(one.config.subscriptions[0].id, 7);
        assert_eq!(one.blocked[0].reason, BlockReason::SlotOccupied);
        assert!(one.has_real_subscriptions);
        let two = config.resolve(Some(&[detected(7, 0), detected(9, 1)]), Some(2));
        assert!(two.ids.is_empty());
        assert_eq!(two.config.subscriptions.iter().map(|s| s.id).collect::<Vec<_>>(), vec![7, 9]);
        // An ID collision on the other slot cannot silently remap a previously published ID.
        let collision = config.resolve(Some(&[detected(FIRST_ID, 1)]), Some(2));
        assert!(collision.ids.is_empty());
        assert_eq!(collision.blocked[0].reason, BlockReason::IdConflict);
        config.sim_enabled = false;
        assert!(config.resolve(Some(&[]), Some(2)).ids.is_empty());
    }
    #[test]
    fn allocated_ids_are_stable_across_edits_and_saved_configuration_round_trips() {
        let mut config = config();
        config.validate().unwrap();
        let saved = serde_json::to_string(&config).unwrap();
        let restored: TelephonyConfig = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            restored.virtual_sim.allocate_id(0, &[], Some(&[detected(FIRST_ID, 1)])).unwrap(),
            FIRST_ID
        );
        config.virtual_sim.subscriptions.clear();
        config.virtual_sim.default_slot = None;
        assert_eq!(
            config.virtual_sim.allocate_id(0, &[], Some(&[detected(FIRST_ID, 1)])).unwrap(),
            FIRST_ID + 1
        );
        config.virtual_sim.default_slot = Some(1);
        assert!(config.validate().is_err());
    }
}

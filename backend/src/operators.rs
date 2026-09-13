//! Operator name and PLMN system properties read directly by Android applications.
//! The root daemon saves original values before replacement and restores them on stop
//! or startup recovery. Values are comma-separated by SIM slot.
use crate::telephony::TelephonyConfig;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const NETWORK_ALPHA: &str = "gsm.operator.alpha";
pub const NETWORK_NUMERIC: &str = "gsm.operator.numeric";
pub const SIM_ALPHA: &str = "gsm.sim.operator.alpha";
pub const SIM_NUMERIC: &str = "gsm.sim.operator.numeric";

/// Android property buffer size, including the trailing NUL byte.
pub const PROPERTY_VALUE_MAX: usize = 92;

/// Property access boundary, replaceable in host tests.
pub trait Properties {
    fn get(&self, name: &str) -> Option<String>;
    fn set(&self, name: &str, value: &str) -> io::Result<()>;
}

/// Device property access through getprop and setprop.
pub struct SystemProperties;

impl Properties for SystemProperties {
    fn get(&self, name: &str) -> Option<String> {
        let output = Command::new("getprop").arg(name).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!value.is_empty()).then_some(value)
    }

    fn set(&self, name: &str, value: &str) -> io::Result<()> {
        let status = Command::new("setprop").arg(name).arg(value).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!("setprop {name} failed: {status}")))
        }
    }
}

const ALL: [&str; 4] = [NETWORK_ALPHA, NETWORK_NUMERIC, SIM_ALPHA, SIM_NUMERIC];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spoof {
    /// Comma-separated carrier names by SIM slot; empty means inactive.
    pub alpha: String,
    /// Comma-separated PLMNs by SIM slot.
    pub numeric: String,
}

impl Spoof {
    /// Build replacement values for enabled subscriptions with a carrier name and PLMN.
    pub fn values(config: &TelephonyConfig, running: bool) -> Self {
        if !running || !config.sim_enabled {
            return Self::default();
        }
        let mut entries: Vec<(u8, &str, String)> = config
            .subscriptions
            .iter()
            .filter(|sub| {
                sub.enabled
                    && !sub.carrier.trim().is_empty()
                    && sub.mcc.len() == 3
                    && (2..=3).contains(&sub.mnc.len())
            })
            .map(|sub| (sub.slot, sub.carrier.as_str(), format!("{}{}", sub.mcc, sub.mnc)))
            .collect();
        if entries.is_empty() {
            return Self::default();
        }
        entries.sort_by_key(|(slot, _, _)| *slot);
        let slots = entries.iter().map(|(slot, _, _)| *slot).max().unwrap_or(0) as usize + 1;
        let mut alpha = vec![String::new(); slots];
        let mut numeric = vec![String::new(); slots];
        for (slot, carrier, plmn) in &entries {
            alpha[*slot as usize] = (*carrier).to_string();
            numeric[*slot as usize] = plmn.clone();
        }
        Self { alpha: join(&alpha, true), numeric: join(&numeric, false) }
    }

    pub fn is_active(&self) -> bool {
        !self.alpha.is_empty()
    }
}

/// Join slot names within the property byte limit, truncating only at UTF-8 boundaries.
fn join(values: &[String], truncate: bool) -> String {
    let mut result = String::new();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            result.push(',');
        }
        if !truncate {
            result.push_str(value);
            continue;
        }
        for character in value.chars() {
            if result.len() + character.len_utf8() >= PROPERTY_VALUE_MAX {
                break;
            }
            result.push(character);
        }
    }
    if result.is_empty() { String::new() } else { result }
}

struct Real {
    values: Vec<(String, String)>,
}

/// Original operator values reported by the phone process.
/// These take precedence over properties that may already contain simulated values.
/// The caller checks heartbeat freshness before using them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Live {
    pub network_alpha: String,
    pub sim_alpha: String,
    pub network_numeric: String,
    pub sim_numeric: String,
}

impl Live {
    /// Accept any nonempty value; unregistered networks can have empty operator fields.
    pub fn is_usable(&self) -> bool {
        !(self.network_alpha.is_empty()
            && self.sim_alpha.is_empty()
            && self.network_numeric.is_empty()
            && self.sim_numeric.is_empty())
    }

    /// Values in the same order as ALL.
    fn values(&self) -> Vec<(String, String)> {
        ALL.iter()
            .map(|name| {
                let value = match *name {
                    NETWORK_ALPHA => &self.network_alpha,
                    SIM_ALPHA => &self.sim_alpha,
                    NETWORK_NUMERIC => &self.network_numeric,
                    _ => &self.sim_numeric,
                };
                ((*name).to_string(), value.clone())
            })
            .collect()
    }
}

impl Default for Operators {
    /// Default control instances access properties only when entering simulation.
    fn default() -> Self {
        Self::new(Path::new(crate::transport::DATA_DIR))
    }
}

/// Apply and restore property replacements as simulation state changes.
pub struct Operators {
    properties: Box<dyn Properties + Send>,
    /// Recovery fallback when fresh phone values and in-memory originals are unavailable.
    backup: PathBuf,
    real: Option<Real>,
    ready: bool,
    applied: Option<Vec<(String, String)>>,
    // A phone heartbeat may echo a property value written by this daemon.
    written: Vec<(String, String)>,
}

impl Operators {
    pub fn new(directory: &Path) -> Self {
        Self::with_properties(Box::new(SystemProperties), directory)
    }

    pub fn with_properties(properties: Box<dyn Properties + Send>, directory: &Path) -> Self {
        Self {
            properties,
            backup: directory.join("operator.bak"),
            real: None,
            ready: false,
            applied: None,
            written: Vec::new(),
        }
    }

    /// Whether replacement properties have been applied successfully.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Apply the desired session state using caller-validated fresh operator values.
    pub fn apply(
        &mut self,
        config: &TelephonyConfig,
        running: bool,
        live: Option<&Live>,
    ) -> io::Result<()> {
        let desired = Spoof::values(config, running);
        if desired.is_active() {
            let backup = if self.real.is_none() {
                let values = self.real_values(live);
                // Keep originals even if publication fails; stop can still restore from memory.
                let result = write_backup(&self.backup, &values);
                self.real = Some(Real { values });
                result
            } else {
                Ok(())
            };
            let replacements = self.replacements(&desired, live);
            if !self.ready || self.applied.as_ref() != Some(&replacements) {
                for replacement in &replacements {
                    if !self.written.contains(replacement) {
                        self.written.push(replacement.clone());
                    }
                }
                self.ready = false;
                for (name, value) in &replacements {
                    self.properties.set(name, value)?;
                }
                self.applied = Some(replacements);
            }
            self.ready = true;
            backup
        } else {
            self.restore(live)
        }
    }

    /// Prefer fresh phone values, then in-memory originals, then current properties.
    fn real_values(&self, live: Option<&Live>) -> Vec<(String, String)> {
        let mut values = self.real.as_ref().map(|real| real.values.clone()).unwrap_or_else(|| {
            ALL.iter()
                .map(|name| ((*name).to_string(), self.properties.get(name).unwrap_or_default()))
                .collect()
        });
        self.merge_live(&mut values, live);
        values
    }

    fn merge_live(&self, originals: &mut [(String, String)], live: Option<&Live>) {
        if let Some(live) = live.filter(|live| live.is_usable()) {
            for (name, value) in live.values() {
                let echo =
                    self.written.iter().any(|(key, written)| *key == name && *written == value);
                if !value.is_empty() && !echo {
                    if let Some((_, saved)) = originals.iter_mut().find(|(key, _)| *key == name) {
                        *saved = value;
                    }
                }
            }
        }
    }

    /// Restore fresh phone values, in-memory originals or the backup, in that order.
    /// Remove the backup after successful restoration.
    pub fn restore(&mut self, live: Option<&Live>) -> io::Result<()> {
        let mut values = match &self.real {
            Some(real) => real.values.clone(),
            None => {
                if !self.backup.exists() {
                    self.ready = false;
                    return Ok(());
                }
                read_backup(&self.backup)?
            }
        };
        self.merge_live(&mut values, live);
        for (name, value) in &values {
            // Empty strings are the real operator values on a device without a SIM.
            self.properties.set(name, value)?;
        }
        let _ = fs::remove_file(&self.backup);
        self.real = None;
        self.ready = false;
        self.applied = None;
        Ok(())
    }

    /// Restore a backup left by a previous daemon, preferring fresh phone values if available.
    pub fn recover(&mut self, live: Option<&Live>) -> io::Result<bool> {
        if !self.backup.exists() {
            return Ok(false);
        }
        self.restore(live)?;
        Ok(true)
    }

    fn replacements(&self, spoof: &Spoof, live: Option<&Live>) -> Vec<(String, String)> {
        self.real_values(live)
            .into_iter()
            .map(|(name, original)| {
                let alpha = name == NETWORK_ALPHA || name == SIM_ALPHA;
                let mut slots: Vec<String> = original.split(',').map(str::to_string).collect();
                let replacement = if alpha { &spoof.alpha } else { &spoof.numeric };
                for (index, value) in replacement.split(',').enumerate() {
                    if value.is_empty() {
                        continue;
                    }
                    if slots.len() <= index {
                        slots.resize(index + 1, String::new());
                    }
                    slots[index] = value.to_string();
                }
                (name, join(&slots, alpha))
            })
            .collect()
    }
}

fn write_backup(path: &Path, values: &[(String, String)]) -> io::Result<()> {
    let mut text = String::new();
    for (name, value) in values {
        // Newlines cannot be represented in the backup format.
        if name.contains('\n') || value.contains('\n') {
            return Err(io::Error::other("property value contains a newline"));
        }
        text.push_str(name);
        text.push('=');
        text.push_str(value);
        text.push('\n');
    }
    crate::storage::atomic_save(path, text.as_bytes())
}

fn read_backup(path: &Path) -> io::Result<Vec<(String, String)>> {
    let text = fs::read_to_string(path)?;
    let mut values = Vec::new();
    for line in text.lines() {
        let Some((name, value)) = line.split_once('=') else {
            return Err(io::Error::other("malformed operator backup"));
        };
        values.push((name.to_string(), value.to_string()));
    }
    Ok(values)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::telephony::Subscription;
    use std::sync::{Arc, Mutex};

    fn subscription(slot: u8, carrier: &str, mcc: &str, mnc: &str, enabled: bool) -> Subscription {
        Subscription {
            id: i32::from(slot) + 1,
            slot,
            mcc: mcc.into(),
            mnc: mnc.into(),
            country: "cn".into(),
            carrier: carrier.into(),
            enabled,
            cdma_sid: None,
        }
    }

    fn config(subscriptions: Vec<Subscription>, sim_enabled: bool) -> TelephonyConfig {
        TelephonyConfig {
            cells_enabled: false,
            sim_enabled,
            radius_m: 500.,
            subscriptions,
            virtual_sim: Default::default(),
        }
    }

    #[test]
    fn builds_one_entry_per_slot() {
        let spoof = Spoof::values(
            &config(
                vec![
                    subscription(0, "中国联通", "460", "11", true),
                    subscription(1, "中国移动", "460", "00", true),
                ],
                true,
            ),
            true,
        );
        assert_eq!(spoof.alpha, "中国联通,中国移动");
        assert_eq!(spoof.numeric, "46011,46000");
    }

    /// Preserve empty slots so framework phoneId indexing remains correct.
    #[test]
    fn keeps_empty_slots_in_place() {
        let spoof = Spoof::values(
            &config(vec![subscription(1, "中国移动", "460", "00", true)], true),
            true,
        );
        assert_eq!(spoof.alpha, ",中国移动");
        assert_eq!(spoof.numeric, ",46000");
    }

    #[test]
    fn stays_out_when_not_running_or_disabled_or_incomplete() {
        let full = vec![subscription(0, "中国联通", "460", "11", true)];
        assert_eq!(Spoof::values(&config(full.clone(), true), false), Spoof::default());
        assert_eq!(Spoof::values(&config(full.clone(), false), true), Spoof::default());
        assert_eq!(
            Spoof::values(
                &config(vec![subscription(0, "中国联通", "460", "11", false)], true),
                true
            ),
            Spoof::default()
        );
        assert_eq!(
            Spoof::values(&config(vec![subscription(0, "  ", "460", "11", true)], true), true),
            Spoof::default()
        );
        assert_eq!(
            Spoof::values(&config(vec![subscription(0, "中国联通", "460", "", true)], true), true),
            Spoof::default()
        );
    }

    #[test]
    fn truncates_long_names_to_fit_the_property_limit() {
        let long = "中".repeat(60);
        let spoof =
            Spoof::values(&config(vec![subscription(0, &long, "460", "11", true)], true), true);
        assert!(spoof.alpha.len() < PROPERTY_VALUE_MAX, "length {}", spoof.alpha.len());
        assert!(spoof.alpha.chars().all(|c| c == '中'), "must not cut a character in half");
    }

    /// Shared fake property state with write counts for control and operator tests.
    #[derive(Default)]
    pub struct Recording {
        values: Mutex<Vec<(String, String)>>,
        writes: Mutex<Vec<(String, String)>>,
    }

    impl Recording {
        pub fn value(&self, name: &str) -> Option<String> {
            self.values
                .lock()
                .unwrap()
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
        pub fn write_count(&self) -> usize {
            self.writes.lock().unwrap().len()
        }
    }

    impl Properties for Recording {
        fn get(&self, name: &str) -> Option<String> {
            self.value(name)
        }
        fn set(&self, name: &str, value: &str) -> io::Result<()> {
            self.writes.lock().unwrap().push((name.to_string(), value.to_string()));
            let mut values = self.values.lock().unwrap();
            match values.iter_mut().find(|(key, _)| key == name) {
                Some(entry) => entry.1 = value.to_string(),
                None => values.push((name.to_string(), value.to_string())),
            }
            Ok(())
        }
    }

    #[derive(Clone)]
    struct Shared(Arc<Recording>);

    impl Properties for Shared {
        fn get(&self, name: &str) -> Option<String> {
            self.0.get(name)
        }
        fn set(&self, name: &str, value: &str) -> io::Result<()> {
            self.0.set(name, value)
        }
    }

    fn directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir()
            .join(format!("justlocation-operators-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn fake(values: &[(&str, &str)]) -> Shared {
        Shared(Arc::new(Recording {
            values: Mutex::new(
                values.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            ),
            writes: Mutex::new(Vec::new()),
        }))
    }

    #[test]
    fn writes_simulated_values_then_restores_the_real_ones() {
        let dir = directory("roundtrip");
        let shared = fake(&[
            (NETWORK_ALPHA, "中国电信,中国电信"),
            (SIM_ALPHA, "中国电信,中国电信"),
            (NETWORK_NUMERIC, "46011,46011"),
            (SIM_NUMERIC, "46011,46011"),
        ]);
        let mut operators = Operators::with_properties(Box::new(shared.clone()), &dir);
        let configuration = config(vec![subscription(0, "中国联通", "460", "11", true)], true);

        operators.apply(&configuration, true, None).unwrap();
        assert!(operators.is_ready());
        // Only the configured slot changes; the other real slot keeps its original fields.
        assert_eq!(shared.get(NETWORK_ALPHA).unwrap(), "中国联通,中国电信");
        assert_eq!(shared.get(NETWORK_NUMERIC).unwrap(), "46011,46011");
        assert!(dir.join("operator.bak").exists());

        let before = shared.0.writes.lock().unwrap().len();
        operators.apply(&configuration, true, None).unwrap();
        assert_eq!(shared.0.writes.lock().unwrap().len(), before);

        operators.apply(&configuration, false, None).unwrap();
        assert!(!operators.is_ready());
        assert_eq!(shared.get(NETWORK_ALPHA).unwrap(), "中国电信,中国电信");
        assert_eq!(shared.get(NETWORK_NUMERIC).unwrap(), "46011,46011");
        assert!(!dir.join("operator.bak").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn live_values_win_over_the_properties_and_the_backup_is_removed() {
        let dir = directory("live");
        // Properties still contain simulated values from a previous daemon.
        let shared = fake(&[
            (NETWORK_ALPHA, "中国联通,中国联通"),
            (SIM_ALPHA, "中国联通,中国联通"),
            (NETWORK_NUMERIC, "46011,46011"),
            (SIM_NUMERIC, "46011,46011"),
        ]);
        let mut operators = Operators::with_properties(Box::new(shared.clone()), &dir);
        let live = Live {
            network_alpha: "中国电信,中国电信".into(),
            sim_alpha: "中国电信,中国电信".into(),
            network_numeric: "46011,46011".into(),
            sim_numeric: "46011,46011".into(),
        };
        // A stale backup must not override fresh phone values.
        fs::write(dir.join("operator.bak"), format!("{NETWORK_ALPHA}=错的值\n")).unwrap();

        operators
            .apply(
                &config(vec![subscription(0, "中国电信", "460", "11", true)], true),
                true,
                Some(&live),
            )
            .unwrap();
        operators
            .apply(
                &config(vec![subscription(0, "中国电信", "460", "11", true)], true),
                false,
                Some(&live),
            )
            .unwrap();

        assert_eq!(shared.get(NETWORK_ALPHA).unwrap(), "中国电信,中国电信", "应当还原成实时真值");
        assert!(!dir.join("operator.bak").exists(), "还原成功后备份要删掉");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn live_values_are_used_instead_of_the_dirty_properties() {
        let dir = directory("dirty");
        let shared = fake(&[
            (NETWORK_ALPHA, "中国电信,中国电信"),
            (SIM_ALPHA, "中国电信,中国电信"),
            (NETWORK_NUMERIC, "46011,46011"),
            (SIM_NUMERIC, "46011,46011"),
        ]);
        let configuration = config(vec![subscription(0, "中国联通", "460", "11", true)], true);
        let live = Live {
            network_alpha: "中国电信,中国电信".into(),
            sim_alpha: "中国电信,中国电信".into(),
            network_numeric: "46011,46011".into(),
            sim_numeric: "46011,46011".into(),
        };

        let mut first = Operators::with_properties(Box::new(shared.clone()), &dir);
        first.apply(&configuration, true, Some(&live)).unwrap();
        first.apply(&configuration, false, Some(&live)).unwrap();
        assert!(!dir.join("operator.bak").exists());

        // Simulate a restart with replaced properties and no in-memory originals.
        shared.set(NETWORK_ALPHA, "中国联通").unwrap();
        shared.set(SIM_ALPHA, "中国联通").unwrap();
        shared.set(NETWORK_NUMERIC, "46011").unwrap();
        shared.set(SIM_NUMERIC, "46011").unwrap();
        let mut restarted = Operators::with_properties(Box::new(shared.clone()), &dir);
        restarted.apply(&configuration, true, Some(&live)).unwrap();

        let backup = fs::read_to_string(dir.join("operator.bak")).unwrap_or_default();
        assert!(!backup.contains("中国联通"), "模拟值被当成了真实值写进备份：{backup}");
        assert!(backup.contains("中国电信"), "备份里应当是手机进程报来的真值：{backup}");

        restarted.apply(&configuration, false, Some(&live)).unwrap();
        assert_eq!(shared.get(NETWORK_ALPHA).unwrap(), "中国电信,中国电信");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recovers_a_leftover_backup_on_start() {
        let dir = directory("recover");
        let shared = fake(&[(NETWORK_ALPHA, "上一个会话留下的模拟值")]);
        fs::write(dir.join("operator.bak"), format!("{NETWORK_ALPHA}=中国电信\n")).unwrap();

        let mut operators = Operators::with_properties(Box::new(shared.clone()), &dir);
        assert!(operators.recover(None).unwrap());
        assert_eq!(shared.get(NETWORK_ALPHA).unwrap(), "中国电信");
        assert!(!dir.join("operator.bak").exists());
        assert!(!operators.recover(None).unwrap());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn restores_empty_operators_and_ignores_own_heartbeat_echo() {
        let dir = directory("no-sim-echo");
        let shared = fake(&[]);
        let mut operators = Operators::with_properties(Box::new(shared.clone()), &dir);
        let first = config(vec![subscription(0, "Virtual", "460", "01", true)], true);
        operators.apply(&first, true, None).unwrap();
        let echo = Live {
            network_alpha: "Virtual".into(),
            sim_alpha: "Virtual".into(),
            network_numeric: "46001".into(),
            sim_numeric: "46001".into(),
        };
        let second = config(vec![subscription(0, "Updated", "460", "02", true)], true);
        operators.apply(&second, true, Some(&echo)).unwrap();
        assert_eq!(shared.get(NETWORK_ALPHA).as_deref(), Some("Updated"));
        assert_eq!(shared.get(SIM_NUMERIC).as_deref(), Some("46002"));
        operators.apply(&second, false, Some(&echo)).unwrap();
        for name in ALL {
            assert_eq!(shared.get(name).as_deref(), Some(""));
        }
        // Stale echoes must neither change idle properties nor become next-session originals.
        let writes = shared.0.write_count();
        operators.apply(&second, false, Some(&echo)).unwrap();
        assert_eq!(shared.0.write_count(), writes);
        operators.apply(&first, true, Some(&echo)).unwrap();
        operators.apply(&first, false, Some(&echo)).unwrap();
        assert_eq!(shared.get(NETWORK_ALPHA).as_deref(), Some(""));
        assert!(!dir.join("operator.bak").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn startup_recovery_writes_empty_original_properties() {
        let dir = directory("recover-empty");
        let shared = fake(&[(NETWORK_ALPHA, "Virtual"), (SIM_NUMERIC, "46001")]);
        fs::write(dir.join("operator.bak"), format!("{NETWORK_ALPHA}=\n{SIM_NUMERIC}=\n")).unwrap();
        let mut operators = Operators::with_properties(Box::new(shared.clone()), &dir);
        assert!(operators.recover(None).unwrap());
        assert_eq!(shared.get(NETWORK_ALPHA).as_deref(), Some(""));
        assert_eq!(shared.get(SIM_NUMERIC).as_deref(), Some(""));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn virtual_second_slot_preserves_the_first_real_card() {
        let dir = directory("mixed-slots");
        let shared = fake(&[
            (NETWORK_ALPHA, "Network"),
            (SIM_ALPHA, "Card"),
            (NETWORK_NUMERIC, "46011"),
            (SIM_NUMERIC, "46003"),
        ]);
        let mut operators = Operators::with_properties(Box::new(shared.clone()), &dir);
        let config = config(vec![subscription(1, "Virtual", "460", "01", true)], true);
        operators.apply(&config, true, None).unwrap();
        assert_eq!(shared.get(NETWORK_ALPHA).as_deref(), Some("Network,Virtual"));
        assert_eq!(shared.get(SIM_ALPHA).as_deref(), Some("Card,Virtual"));
        assert_eq!(shared.get(NETWORK_NUMERIC).as_deref(), Some("46011,46001"));
        assert_eq!(shared.get(SIM_NUMERIC).as_deref(), Some("46003,46001"));
        let echo = Live {
            network_alpha: "Network,Virtual".into(),
            sim_alpha: "Card,Virtual".into(),
            network_numeric: "46011,46001".into(),
            sim_numeric: "46003,46001".into(),
        };
        operators.apply(&config, false, Some(&echo)).unwrap();
        assert_eq!(shared.get(NETWORK_ALPHA).as_deref(), Some("Network"));
        assert_eq!(shared.get(SIM_ALPHA).as_deref(), Some("Card"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restores_from_memory_when_the_backup_cannot_be_written() {
        let dir = directory("unwritable");
        let shared = fake(&[(NETWORK_ALPHA, "中国电信")]);
        // Use a file as the parent directory to force backup creation to fail.
        let blocker = dir.join("blocker");
        fs::write(&blocker, b"").unwrap();
        let mut operators = Operators::with_properties(Box::new(shared.clone()), &blocker);
        let configuration = config(vec![subscription(0, "中国联通", "460", "11", true)], true);

        assert!(
            operators.apply(&configuration, true, None).is_err(),
            "a failed backup must be reported"
        );
        assert_eq!(
            shared.get(NETWORK_ALPHA).unwrap(),
            "中国联通",
            "the properties must still be taken over"
        );
        operators.apply(&configuration, false, None).unwrap();
        assert_eq!(
            shared.get(NETWORK_ALPHA).unwrap(),
            "中国电信",
            "the real values must be restored"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}

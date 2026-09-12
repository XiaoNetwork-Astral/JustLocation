//! 运营商名称与 PLMN 的系统属性出口。
//!
//! `TelephonyManager` 的 `getNetworkOperatorName` / `getNetworkOperator` /
//! `getSimOperatorName` / `getSimOperator` 在 Android 15 上读的是系统属性
//! （`gsm.operator.alpha` 等），而且**完全在应用进程内完成，不经过 system_server**，
//! 所以服务端的 hook 拦不住它们（2026-09-12 真机验收确认）。属性本身对任何读取者都是
//! 共享内存，因此这里由守护进程（root）直接改写属性：不需要往应用进程注入任何东西。
//!
//! **改动前先把真实值备份到 `operator.bak`**，停止模拟、状态过期或开关关闭时原样写回；
//! 守护进程若在脱管状态下重启，下次启动会读这份备份先还原，避免模拟值留在系统里。
//! 写入格式按卡槽位用逗号分隔，与 framework 读属性的方式一致（`values[phoneId]`）。
use crate::telephony::TelephonyConfig;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const NETWORK_ALPHA: &str = "gsm.operator.alpha";
pub const NETWORK_NUMERIC: &str = "gsm.operator.numeric";
pub const SIM_ALPHA: &str = "gsm.sim.operator.alpha";
pub const SIM_NUMERIC: &str = "gsm.sim.operator.numeric";

/// `__system_property_get` 的缓冲区长度上限（`PROP_VALUE_MAX`）。超过就写不进去，
/// 所以拼串时必须留出余量；`setprop` 与读取两侧受同一限制。
pub const PROPERTY_VALUE_MAX: usize = 92;

/// 属性的读写出口。做成 trait 是为了在主机上测试状态机，不必真的去动设备属性。
pub trait Properties {
    fn get(&self, name: &str) -> Option<String>;
    fn set(&self, name: &str, value: &str) -> io::Result<()>;
}

/// 设备上的实现：借用系统自带的 `getprop` / `setprop`。守护进程本身就是 root。
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
    /// 按卡槽号拼好的运营商名称列表，逗号分隔；空串表示不接管。
    pub alpha: String,
    /// 按卡槽号拼好的 PLMN 列表（MCC+MNC），逗号分隔。
    pub numeric: String,
}

impl Spoof {
    /// 该不该接管：模拟运行中、SIM 通道开启、且至少有一张启用的卡。
    /// 名称或 PLMN 缺失（含 MNC 为空）的卡不参与——与 ServiceState 的替换规则一致，
    /// 不替用户编一个运营商名字出来。
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
        Self {
            alpha: join(&alpha, true),
            numeric: join(&numeric, false),
        }
    }

    pub fn is_active(&self) -> bool {
        !self.alpha.is_empty()
    }
}

/// 逗号拼接；名称是用户可填的多字节字符串，超长时按字符边界截断，
/// 保证整串留在 `PROPERTY_VALUE_MAX` 之内（否则 `setprop` 直接失败）。
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

/// 手机进程报上来的**真实**运营商值。
///
/// <p>为什么需要它：我们在属性上写的模拟值会盖掉真值，而**属性本身是唯一被盖住的地方**——
/// 运营商服务里的真值没被动过，手机进程（`com.android.phone`，我们的模块也加载在那里）
/// 读到的就是它。有了这个来源，还原就不必只依赖本地备份文件。
///
/// <p>`telephony_hook_status` 心跳里带过来，所以它和 `phone_connected` 一样有新鲜度要求。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Live {
    pub network_alpha: String,
    pub sim_alpha: String,
    pub network_numeric: String,
    pub sim_numeric: String,
}

impl Live {
    /// 四个值里有任何一个有效就值得采信：`sim.*` 来自卡、`gsm.operator.*` 来自当前注册，
    /// 未注册时后者本来就是空的。
    pub fn is_usable(&self) -> bool {
        !(self.network_alpha.is_empty()
            && self.sim_alpha.is_empty()
            && self.network_numeric.is_empty()
            && self.sim_numeric.is_empty())
    }

    /// 按 `ALL` 的顺序取出来，方便与属性值逐项对照。
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
    /// 给 `Control::default()` 用（含主机的协议测试）。默认实例指向正式数据目录，
    /// 但只有真正进入模拟时才会去读写属性。
    fn default() -> Self {
        Self::new(Path::new(crate::transport::DATA_DIR))
    }
}

/// 属性替换的状态机：只在"该不该接管"发生变化时动属性。
pub struct Operators {
    properties: Box<dyn Properties + Send>,
    /// 真值的**保底**来源：接管前把属性值抄一份。
    ///
    /// <p>它天生不可靠——属性一旦被我们改写，抄下来的就可能是模拟值。所以它只排在
    /// 最后：**优先用手机进程报来的实时真值**（那才是运营商服务里的原值），
    /// 拿不到时才退到内存，再退到这份文件。
    backup: PathBuf,
    real: Option<Real>,
    ready: bool,
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
        }
    }

    /// 属性是否正被我们接管；供状态里的"运营商"就绪位使用。
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// 每次状态轮询调用一次。状态没变时不做任何系统调用。
    ///
    /// <p>`live` 是手机进程报上来的**真实**运营商值（可能过期，由调用方判新鲜度）。
    /// 它只在两处起作用：接管前**核对**当前属性值是不是真值，以及还原时**优先**用它。
    pub fn apply(
        &mut self,
        config: &TelephonyConfig,
        running: bool,
        live: Option<&Live>,
    ) -> io::Result<()> {
        let desired = Spoof::values(config, running);
        if desired.is_active() {
            if self.real.is_some() {
                return Ok(()); // 已在接管中
            }
            let values = self.real_values(live);
            // 备份写不下去也必须继续：真实值先留在内存里，
            // 否则一旦写属性成功而备份失败，停止时就再也找不到真实值了。
            let backup = write_backup(&self.backup, &values);
            self.real = Some(Real { values });
            self.write(&desired)?;
            self.ready = true;
            backup
        } else {
            self.restore(live)
        }
    }

    /// 取"当前的真实值"，按可信度排序（用户 2026-09-12 定的思路）：
    /// 1. **手机进程报上来的实时值**——属性被我们盖住了，但运营商服务里的没被动过，这是真值；
    /// 2. 内存里记着的（接管中又调用一次的情况）；
    /// 3. 属性里读到的值——**明知不可靠**：如果守护进程上次是带着模拟值异常退出的，
    ///    读到的就是模拟值。它只作为"实在没有别的来源"时的保底。
    fn real_values(&self, live: Option<&Live>) -> Vec<(String, String)> {
        if let Some(live) = live.filter(|live| live.is_usable()) {
            return live.values();
        }
        if let Some(real) = &self.real {
            return real.values.clone();
        }
        ALL.iter()
            .map(|name| ((*name).to_string(), self.properties.get(name).unwrap_or_default()))
            .collect()
    }

    /// 原样写回真实值。
    ///
    /// <p>优先用 `live`（手机进程报上来的实时值），其次用内存里记着的、最后读备份文件。
    /// **真值写到属性上之后就把备份删掉**：属性已经正确了，留着一份只会让下次启动
    /// 又多一个可能过期的真值来源。
    pub fn restore(&mut self, live: Option<&Live>) -> io::Result<()> {
        let values = if let Some(live) = live.filter(|live| live.is_usable()) {
            live.values()
        } else {
            match &self.real {
                Some(real) => real.values.clone(),
                None => {
                    if !self.backup.exists() {
                        self.ready = false;
                        return Ok(());
                    }
                    read_backup(&self.backup)?
                }
            }
        };
        for (name, value) in &values {
            // `setprop` 不接受空串（`setprop X ""` 会被拒），而"未注册"本来就是空值：
            // 这种情况下不要动属性，宁可留着一个可能不准确的值，也不要让整次还原失败。
            if value.is_empty() {
                continue;
            }
            self.properties.set(name, value)?;
        }
        // **真值已经写回属性，这份保底备份就没用了**：留着它只会让下次启动多一个
        // 可能过期的真值来源（用户 2026-09-12 明确：还原成功就删掉）。
        let _ = fs::remove_file(&self.backup);
        self.real = None;
        self.ready = false;
        Ok(())
    }

    /// 守护进程启动时调用：处理上次留下的备份（脱管退出后再启动）。
    ///
    /// <p>如果手机进程已经在报真实值，就**以它为准**（备份可能是上次脱管时被写坏的）。
    pub fn recover(&mut self, live: Option<&Live>) -> io::Result<bool> {
        if !self.backup.exists() {
            return Ok(false);
        }
        self.restore(live)?;
        Ok(true)
    }

    fn write(&mut self, spoof: &Spoof) -> io::Result<()> {
        for (name, value) in [
            (NETWORK_ALPHA, &spoof.alpha),
            (SIM_ALPHA, &spoof.alpha),
            (NETWORK_NUMERIC, &spoof.numeric),
            (SIM_NUMERIC, &spoof.numeric),
        ] {
            self.properties.set(name, value)?;
        }
        Ok(())
    }
}

fn write_backup(path: &Path, values: &[(String, String)]) -> io::Result<()> {
    let mut text = String::new();
    for (name, value) in values {
        // 属性值里不会出现换行；出现就说明拿到的不是真实属性，宁可不备份。
        if name.contains('\n') || value.contains('\n') {
            return Err(io::Error::other("property value contains a newline"));
        }
        text.push_str(name);
        text.push('=');
        text.push_str(value);
        text.push('\n');
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, text)?;
    fs::rename(temporary, path)
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

    /// 卡槽 1 有卡、卡槽 0 没配时，空位要留出来，否则 framework 会按错位取值。
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
        // 卡被停用
        assert_eq!(
            Spoof::values(&config(vec![subscription(0, "中国联通", "460", "11", false)], true), true),
            Spoof::default()
        );
        // 名称为空、MNC 缺失
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
        let spoof = Spoof::values(
            &config(vec![subscription(0, &long, "460", "11", true)], true),
            true,
        );
        assert!(spoof.alpha.len() < PROPERTY_VALUE_MAX, "length {}", spoof.alpha.len());
        assert!(spoof.alpha.chars().all(|c| c == '中'), "must not cut a character in half");
    }

    /// 记录写入次数、可事后取回同一份状态的假属性，供其它模块（如协议层）的测试注入。
    #[derive(Default)]
    pub struct Recording {
        values: Mutex<Vec<(String, String)>>,
        writes: Mutex<Vec<(String, String)>>,
    }

    impl Recording {
        pub fn value(&self, name: &str) -> Option<String> {
            self.values.lock().unwrap().iter().find(|(key, _)| key == name).map(|(_, value)| value.clone())
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

    /// 让测试既能驱动 `Operators`，又能在事后读同一份属性状态。
    #[derive(Clone)]
    struct Shared(Arc<Recording>);

    impl Properties for Shared {
        fn get(&self, name: &str) -> Option<String> { self.0.get(name) }
        fn set(&self, name: &str, value: &str) -> io::Result<()> { self.0.set(name, value) }
    }

    fn directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("justlocation-operators-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn fake(values: &[(&str, &str)]) -> Shared {
        Shared(Arc::new(Recording {
            values: Mutex::new(values.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()),
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
        assert_eq!(shared.get(NETWORK_ALPHA).unwrap(), "中国联通");
        assert_eq!(shared.get(NETWORK_NUMERIC).unwrap(), "46011");
        assert!(dir.join("operator.bak").exists());

        // 状态没变时不该再动属性。
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

    /// 手机进程报来的实时真值**优先于**属性里读到的值。
    ///
    /// <p>这是用户 2026-09-12 定的思路：属性被我们盖住了，真值只在运营商服务里，
    /// 所以还原要优先走实时值；备份只是它读不到时的退路，而且**用上之后要删掉**。
    #[test]
    fn live_values_win_over_the_properties_and_the_backup_is_removed() {
        let dir = directory("live");
        // 属性上挂着上一轮的模拟值——这正是"脱管退出"留下的现场。
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
        // 冒充"上一次脱管时写坏的备份"：如果实现去看它，就会还原成错的运营商。
        fs::write(dir.join("operator.bak"), format!("{NETWORK_ALPHA}=错的值\n")).unwrap();

        operators.apply(&config(vec![subscription(0, "中国电信", "460", "11", true)], true), true, Some(&live)).unwrap();
        operators.apply(&config(vec![subscription(0, "中国电信", "460", "11", true)], true), false, Some(&live)).unwrap();

        assert_eq!(shared.get(NETWORK_ALPHA).unwrap(), "中国电信,中国电信", "应当还原成实时真值");
        assert!(!dir.join("operator.bak").exists(), "还原成功后备份要删掉");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// **有实时真值时，我们自己写的模拟值绝不会被当成真实值记下来。**
    ///
    /// <p>这是备份机制最危险的一条路径：守护进程带着模拟值异常退出 → 下次启动无备份可用 →
    /// 一接管就把属性上的模拟值抄进备份，从此真值永久丢失（用户 2026-09-12 指出）。
    /// 解药不是让备份变聪明，而是**根本不去读属性**：手机进程报来的实时真值优先，
    /// 备份只在连它都没有时才作数。
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

        // 第一轮：正常接管再停止（备份已按设计删掉）。
        let mut first = Operators::with_properties(Box::new(shared.clone()), &dir);
        first.apply(&configuration, true, Some(&live)).unwrap();
        first.apply(&configuration, false, Some(&live)).unwrap();
        assert!(!dir.join("operator.bak").exists());

        // 模拟"脱管退出"：属性停在模拟值上，新实例的内存里没有真值、也没有指纹。
        shared.set(NETWORK_ALPHA, "中国联通").unwrap();
        shared.set(SIM_ALPHA, "中国联通").unwrap();
        shared.set(NETWORK_NUMERIC, "46011").unwrap();
        shared.set(SIM_NUMERIC, "46011").unwrap();
        let mut restarted = Operators::with_properties(Box::new(shared.clone()), &dir);
        restarted.apply(&configuration, true, Some(&live)).unwrap();

        let backup = fs::read_to_string(dir.join("operator.bak")).unwrap_or_default();
        assert!(
            !backup.contains("中国联通"),
            "模拟值被当成了真实值写进备份：{backup}"
        );
        assert!(backup.contains("中国电信"), "备份里应当是手机进程报来的真值：{backup}");

        // 而且这次停止能把属性真正还原回真值。
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
        // 没有备份时恢复应当是空操作。
        assert!(!operators.recover(None).unwrap());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// 备份目录不可写时也必须能还原：真实值留在内存里，不能只依赖那个文件。
    #[test]
    fn restores_from_memory_when_the_backup_cannot_be_written() {
        let dir = directory("unwritable");
        let shared = fake(&[(NETWORK_ALPHA, "中国电信")]);
        // 让备份路径落在一个"文件"底下，写备份必然失败。
        let blocker = dir.join("blocker");
        fs::write(&blocker, b"").unwrap();
        let mut operators = Operators::with_properties(Box::new(shared.clone()), &blocker);
        let configuration = config(vec![subscription(0, "中国联通", "460", "11", true)], true);

        assert!(operators.apply(&configuration, true, None).is_err(), "a failed backup must be reported");
        assert_eq!(shared.get(NETWORK_ALPHA).unwrap(), "中国联通", "the properties must still be taken over");
        operators.apply(&configuration, false, None).unwrap();
        assert_eq!(shared.get(NETWORK_ALPHA).unwrap(), "中国电信", "the real values must be restored");
        fs::remove_dir_all(&dir).unwrap();
    }
}

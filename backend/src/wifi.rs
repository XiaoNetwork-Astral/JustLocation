//! Wi-Fi 模拟的保存对象。
//!
//! 重实现规格第 7.1 节：用户可以从附近列表选择，或手工填写 SSID（网络名称）
//! 与 BSSID（接入点地址），保存后在历史列表里切换，并支持编辑、删除和撤销删除。
//! 原版手工输入的校验用 MAC 格式正则但取 `find()` 而不是完整匹配，因此允许子串命中；
//! 这里保留这个宽松行为，避免把用户在别处复制的地址判为非法。
//!
//! 缺省值沿用原版模型：`rssi = 200`、`linkspeed = 866`、`frequency = 5745`。
//! 注意 200 不是常规 dBm 测量值，只是原版模型的取值，不要当成真实信号强度。

use serde::{Deserialize, Serialize};

/// 原版模型的缺省信号与链路参数。
pub const DEFAULT_RSSI: i32 = 200;
pub const DEFAULT_LINK_SPEED: i32 = 866;
pub const DEFAULT_FREQUENCY: i32 = 5745;

fn default_rssi() -> i32 {
    DEFAULT_RSSI
}
fn default_link_speed() -> i32 {
    DEFAULT_LINK_SPEED
}
fn default_frequency() -> i32 {
    DEFAULT_FREQUENCY
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WifiTarget {
    pub id: String,
    pub ssid: String,
    pub bssid: String,
    #[serde(default = "default_rssi")]
    pub rssi: i32,
    #[serde(default = "default_link_speed")]
    pub link_speed: i32,
    #[serde(default = "default_frequency")]
    pub frequency: i32,
}

/// 允许 `aa:bb:cc:dd:ee:ff` 这类写法，分隔符每个位置都可为冒号或短横线，大小写不限。
/// 用"在原串里能否找到"（相当于原版的 `find()`）而不是完整匹配，所以带前缀后缀的文本也能通过。
fn find_mac(text: &str) -> bool {
    let bytes = text.as_bytes();
    let hex = |b: u8| b.is_ascii_hexdigit();
    let separator = |b: u8| b == b':' || b == b'-';
    if bytes.len() < 17 {
        return false;
    }
    for start in 0..=bytes.len() - 17 {
        let candidate = &bytes[start..start + 17];
        let shaped = (0..17).all(|index| {
            if index % 3 == 2 {
                separator(candidate[index])
            } else {
                hex(candidate[index])
            }
        });
        if shaped {
            return true;
        }
    }
    false
}

impl WifiTarget {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.trim().is_empty() || self.id.len() > 64 {
            return Err("invalid wifi id");
        }
        // SSID 允许空格，但不允许空串或控制字符：原版把名称直接交给系统对象构造。
        if self.ssid.trim().is_empty() || self.ssid.len() > 64 || self.ssid.chars().any(char::is_control) {
            return Err("invalid wifi ssid");
        }
        if self.bssid.len() > 64 || self.bssid.chars().any(char::is_control) {
            return Err("invalid wifi bssid");
        }
        Ok(())
    }

    /// 手工输入的 BSSID 是否像一个接入点地址。
    pub fn looks_like_bssid(text: &str) -> bool {
        find_mac(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> WifiTarget {
        WifiTarget {
            id: "w1".into(),
            ssid: "Home".into(),
            bssid: "aa:bb:cc:dd:ee:ff".into(),
            rssi: DEFAULT_RSSI,
            link_speed: DEFAULT_LINK_SPEED,
            frequency: DEFAULT_FREQUENCY,
        }
    }

    #[test]
    fn missing_signal_fields_fall_back_to_the_original_defaults() {
        // 面板可以只发名称与地址，其余沿用原版模型缺省值。
        let parsed: WifiTarget =
            serde_json::from_str(r#"{"id":"w1","ssid":"Home","bssid":"aa:bb:cc:dd:ee:ff"}"#).unwrap();
        assert_eq!(parsed.rssi, 200);
        assert_eq!(parsed.link_speed, 866);
        assert_eq!(parsed.frequency, 5745);
        assert_eq!(parsed, target());
    }

    #[test]
    fn validation_rejects_empty_names_and_control_characters() {
        let mut value = target();
        value.ssid = "   ".into();
        assert!(value.validate().is_err());
        value = target();
        value.ssid = "Home\u{1}".into();
        assert!(value.validate().is_err());
        value = target();
        value.id = String::new();
        assert!(value.validate().is_err());
        assert!(target().validate().is_ok());
    }

    #[test]
    fn bssid_check_matches_the_lenient_behaviour_of_the_original() {
        for text in [
            "aa:bb:cc:dd:ee:ff",
            "AA-BB-CC-DD-EE-FF",
            "网关 aa:bb:cc:dd:ee:ff 的地址",
            "前置aa:bb:cc:dd:ee:ff",
            // 原版用 find() 而非完整匹配，所以更长的串只要包含一个合法地址就算通过。
            "aa:bb:cc:dd:ee:ff:00",
        ] {
            assert!(WifiTarget::looks_like_bssid(text), "{text} 应该被接受");
        }
        for text in ["", "aa:bb:cc:dd:ee", "zz:bb:cc:dd:ee:ff", "aa bb cc dd ee ff"] {
            assert!(!WifiTarget::looks_like_bssid(text), "{text} 不应该被接受");
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(serde_json::from_str::<WifiTarget>(
            r#"{"id":"w1","ssid":"Home","bssid":"aa:bb:cc:dd:ee:ff","channel":6}"#
        )
        .is_err());
    }
}

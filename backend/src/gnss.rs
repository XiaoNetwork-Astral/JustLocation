//! 卫星通道（GNSS 状态与 NMEA）的配置。
//!
//! 重实现规格第 7.4 节确认原版做的是：位置模拟运行、卫星开关开启时，
//! 每隔约一秒把**预置的卫星状态数组**投递给匹配范围的监听；数组不按经纬度、
//! 日期或星历计算。NMEA 一侧只在命中范围时丢弃回调，不合成报文。
//! 因此这里只保存"是否启用"两个开关，不假装能生成真实卫星数据。

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GnssConfig {
    /// 是否投递卫星状态（GnssStatus / GPS 状态回调）。
    pub gnss_enabled: bool,
    /// 是否拦截并丢弃匹配范围内的 NMEA 回调。
    pub nmea_enabled: bool,
}

impl GnssConfig {
    /// 两个开关都是布尔值，没有范围可校验；保留这个方法是为了与其它通道统一调用方式。
    pub fn validate(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_both_channels_off() {
        let config = GnssConfig::default();
        assert!(!config.gnss_enabled);
        assert!(!config.nmea_enabled);
        config.validate().unwrap();
    }

    #[test]
    fn reads_a_partial_object_and_rejects_unknown_fields() {
        // 缺字段的请求要能解析（旧面板不会发新字段）。
        let partial: GnssConfig = serde_json::from_str(r#"{"gnss_enabled":true}"#).unwrap();
        assert!(partial.gnss_enabled);
        assert!(!partial.nmea_enabled);
        // 拼错字段名必须报错，而不是被静默忽略。
        assert!(serde_json::from_str::<GnssConfig>(r#"{"gnss":true}"#).is_err());
    }
}

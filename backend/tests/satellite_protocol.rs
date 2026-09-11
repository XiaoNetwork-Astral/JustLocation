//! 卫星通道（GNSS 状态与 NMEA）的协议级测试。
//!
//! 这些测试把三件事串起来验证：桥接上报的安装位、面板保存的开关、以及配置的持久化。
//! 单独测 `GnssConfig` 只能证明两个布尔值能解析，证明不了"开关关掉之后就绪位不再上报"这类联动。

use justlocation_backend::protocol::Control;
use serde_json::{Value, json};

fn call(control: &mut Control, request: Value) -> Value {
    serde_json::to_value(control.handle(&request.to_string())).unwrap()
}
fn switches(gnss: bool, nmea: bool) -> Value {
    json!({"version":1,"op":"set_gnss","config":{"gnss_enabled":gnss,"nmea_enabled":nmea}})
}
fn status() -> Value {
    json!({"version":1,"op":"status"})
}
/// 桥接心跳：`installed` 是定位通道，`gnss`/`nmea` 是两个卫星通道各自的安装位。
fn heartbeat(installed: bool, gnss: bool, nmea: bool) -> Value {
    json!({"version":1,"op":"hook_status","installed":installed,"gnss":gnss,"nmea":nmea})
}

#[test]
fn satellite_readiness_follows_the_bridge_heartbeat() {
    let mut control = Control::default();
    // 还没心跳时两个通道都不算就绪。
    let initial = call(&mut control, status());
    assert_eq!(initial["state"]["gnss_hook_ready"], false);
    assert_eq!(initial["state"]["nmea_hook_ready"], false);
    let reported = call(&mut control, heartbeat(true, true, false));
    assert_eq!(reported["state"]["gnss_hook_ready"], true);
    assert_eq!(reported["state"]["nmea_hook_ready"], false);
    // 两个安装位互不牵连：只关 GNSS，NMEA 自己那一位仍按心跳上报。
    let both = call(&mut control, heartbeat(true, false, true));
    assert_eq!(both["state"]["gnss_hook_ready"], false);
    assert_eq!(both["state"]["nmea_hook_ready"], true);
}

#[test]
fn satellite_readiness_follows_the_bridge_not_the_location_hook() {
    let mut control = Control::default();
    call(&mut control, heartbeat(true, true, true));
    let gone = call(&mut control, heartbeat(false, true, true));
    // 就绪位的规则是"桥接可达 + 该通道有安装标志"，与定位接口是否装好无关
    // （基站查询那一位也是同样规则）。所以这里定位通道已经不可用，
    // 卫星通道仍会按自己的安装标志报告可用——面板因此要分别看这两类状态，
    // 不能因为一个可用就认为整条链路都在工作。
    assert_eq!(gone["state"]["location_hook_ready"], false);
    assert_eq!(gone["state"]["gnss_hook_ready"], true);
    assert_eq!(gone["state"]["nmea_hook_ready"], true);
    // 桥接心跳彻底停下时，卫星那两位才跟着消失。
    assert_eq!(call(&mut control, status())["state"]["gnss_hook_ready"], true);
    let cleared = call(&mut control, heartbeat(false, false, false));
    assert_eq!(cleared["state"]["gnss_hook_ready"], false);
    assert_eq!(cleared["state"]["nmea_hook_ready"], false);
}

#[test]
fn the_switches_are_independent_of_whether_the_hook_is_ready() {
    let mut control = Control::default();
    // 桥接没上报任何安装位，面板仍然可以先把开关存下来。
    let saved = call(&mut control, switches(true, true));
    assert_eq!(saved["ok"], true);
    // 两个开关同属 state.gnss 这一个配置对象，没有各自的 state.nmea。
    assert_eq!(saved["state"]["gnss"]["gnss_enabled"], true);
    assert_eq!(saved["state"]["gnss"]["nmea_enabled"], true);
    assert_eq!(saved["state"]["gnss_hook_ready"], false);
    // 关掉一个不影响另一个。
    let partial = call(&mut control, switches(false, true));
    assert_eq!(partial["state"]["gnss"]["gnss_enabled"], false);
    assert_eq!(partial["state"]["gnss"]["nmea_enabled"], true);
}

#[test]
fn a_bad_satellite_payload_is_rejected_without_changing_the_saved_switches() {
    let mut control = Control::default();
    call(&mut control, switches(true, false));
    for bad in [
        json!({"version":1,"op":"set_gnss","config":{"gnss":true}}),
        json!({"version":1,"op":"set_gnss","config":{"gnss_enabled":"yes"}}),
        json!({"version":1,"op":"set_gnss"}),
    ] {
        let rejected = call(&mut control, bad.clone());
        assert_eq!(rejected["ok"], false, "{bad} 应该被拒绝");
        assert_eq!(rejected["state"]["gnss"]["gnss_enabled"], true);
        assert_eq!(rejected["state"]["gnss"]["nmea_enabled"], false);
    }
}

#[test]
fn the_switches_survive_a_restart_and_a_failed_write_rolls_them_back() {
    let path = std::env::temp_dir().join(format!("justlocation-satellite-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut control = Control::open(&path).unwrap();
    assert_eq!(call(&mut control, switches(true, true))["ok"], true);
    // 重新打开：配置要留下来，而会话本身不自动开始。
    let mut restored = Control::open(&path).unwrap();
    let state = call(&mut restored, status());
    assert_eq!(state["state"]["gnss"]["gnss_enabled"], true);
    assert_eq!(state["state"]["gnss"]["nmea_enabled"], true);
    assert_eq!(state["state"]["requested_active"], false);
    std::fs::remove_file(&path).unwrap();
    // 存储不可写时，改动必须整体回滚，不能只改一半。
    let mut missing = Control::open(path.join("missing").join("state.json")).unwrap();
    let failed = call(&mut missing, switches(true, true));
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["state"]["gnss"]["gnss_enabled"], false);
    assert_eq!(failed["state"]["gnss"]["nmea_enabled"], false);
}

#[test]
fn stopping_the_simulation_keeps_the_satellite_switches() {
    let mut control = Control::default();
    call(&mut control, switches(true, true));
    call(
        &mut control,
        json!({"version":1,"op":"start","config":{"position":justlocation_backend::Position::new(0.,0.),"scope":{"mode":"all"}}}),
    );
    let stopped = call(&mut control, json!({"version":1,"op":"stop"}));
    // 停止是一次会话操作，不该顺手把用户设好的通道开关清掉。
    assert_eq!(stopped["state"]["requested_active"], false);
    assert_eq!(stopped["state"]["gnss"]["gnss_enabled"], true);
    assert_eq!(stopped["state"]["gnss"]["nmea_enabled"], true);
}

# 环境通道实现计划

用户于 2026-09-10 调整优先级：环境通道先做，包含实际基站功能；地图、路线、备份的后续开发暂停。Android 15 ARM64 是首要目标。最新要求：手机用于提供网络，暂时不得使用手机。继续本地开发与 AVD 检查，真机测试暂缓。

## 覆盖清单

| 通道 | 数据与控制 | Android 输出链 | 当前状态 |
| --- | --- | --- | --- |
| GNSS | 卫星编号、星座、信号、仰角、方位、载频、参与定位标记 | 状态监听、首次定位、周期更新、停止恢复 | 合成帧、监听代理和派发代码已接入，Java 本地测试通过；独立设置及真机验证待完成 |
| NMEA | 与当前 WGS84 位置、时间、速度、航向和卫星一致的语句与校验和 | 独立 NMEA 注册、原回调替换及周期更新 | GGA/RMC/GSA/GSV/VTG 与独立监听代码已接入，本地测试通过；独立设置及真机验证待完成 |
| 原始测量与导航电文 | 独立能力状态；不能把固定卫星表宣称为真实射频观测 | 测量和电文注册/注销、会话切换及恢复 | 待研究与实现 |
| Wi-Fi | 接入点集合、连接项、SSID/BSSID、信号、频率与连接属性 | 扫描列表、连接信息、NetworkCapabilities 查询及回调 | 待实现 |
| 基站 | GSM、WCDMA、LTE、NR、CDMA 的身份/信号、注册状态与订阅映射 | CellInfo、CellIdentity/CellLocation、异步更新、ServiceState、TelephonyCallback 和信号回调 | 供应商、缓存、输出帧、电话查询、Registry 监听及面板已接入；主机测试及 AVD 对象/签名检查通过，实际 Hook 与 ROM 验证待完成 |
| SIM | 实际订阅/卡槽识别、MCC/MNC、国家与运营商 | SubscriptionInfo 列表、按订阅/卡槽/ICCID 查询 | 面板及八个授权查询已接入；保留真实卡槽、号码脱敏及列表过滤。TelephonyManager 属性读取、订阅变更通知等仍未覆盖，`sim_hook_ready` 目前仅表示 SubscriptionInfo 查询就绪 |
| 步数 | 步频、累计步数、启停、每日清零和移动联动 | 计步器/步伐检测订阅、事件时间与停止恢复 | 待实现 |

每条通道分开报告配置启用、Hook 安装和实际输出验证。查询及回调使用同一作用范围；保留系统权限与脱敏处理，不修改全局真实缓存。后台停止、状态过期或单通道关闭后恢复原始输出。异常不能拖垮已工作的定位通道。

## 实现约束与参考

- 旧样本仅作为功能参考：未确认 NR、requestCellInfoUpdate、现代 Wi-Fi 能力回调、完整 NMEA 和原始测量链。本项目对这些单独实现和验证。
- 任意经纬度不自动对应一组真实基站。首选 OpenCellID，预留 Fake Location 备用，也支持自定义供应商。Fake Location 鉴权与请求编码尚未验证，目前不会实际调用。区域缓存和标准化 JSON 导入已实现，批量 CSV 与区域索引尚未实现。
- SIM 模拟只改变查询结果，不写入真实 SIM/eSIM，也不建立无线连接。
- 服务端适配优先。电话服务位于系统电话应用时定向加载；普通应用维持轻量入口后卸载，不安装功能 Hook。
- AOSP Android 15 接口参考副本位于 `build/environment-reference`，从 aosp-mirror 的 `android-15.0.0_r1` 分支获取；实际 ROM 签名需要真机补验。

## 2026-09-11 接入验证

- ServiceState 查询在系统权限和定位脱敏之后替换；监听周期派发先按 Registry 的精确/粗略定位权限裁剪真实缓存副本，再替换。已隐藏的 CellIdentity、运营商保持隐藏，停止后恢复最新的授权缓存，不修改真实共享对象。
- 无可用目标基站时，ServiceState 不沿用真实基站身份。仅改写蜂窝注册信息，WLAN 注册项及实际服务状态保留；运营商名称未知时为空。尚不能据此声称所有电话 API、广播和网络属性都已覆盖。
- 专用 AVD 的真实 CellInfo/SubscriptionInfo/ServiceState 对象、脱敏、Parcel 往返和电话服务/Registry 签名通过。AVD 没有 Root/Zygisk，因此此项不验证实际 Hook 安装、DEX 信任设置或目标 ROM 稳定性。
- 完整 ARM64 构建通过，日志 `build/environment-arm64-build.log`。本轮未使用手机，也未安装模块。

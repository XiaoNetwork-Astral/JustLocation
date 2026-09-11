# 开发进度

更新：2026-09-11。继续优先开发环境通道。用户目前需要手机提供网络，明确要求暂时不要使用手机；本轮仅使用本机和专用 AVD，真机测试暂缓。以下按阶段记录；最新结果见文末及 environment-channels.md。

## 已具备的主链路

| 阶段 | 当前结果 | 验证范围 |
| --- | --- | --- |
| 单仓库工作区 | Native、Rust、React、Kotlin，统一构建和打包 | 本机构建 |
| 控制与状态 | 后台命令、范围、配置保存、停止恢复 | Rust 测试及此前真机验证 |
| 系统定位 Hook | system_server 中最近位置、单次定位、持续回调与主动派发 | Android 15 ARM64 真机；百度跳点已由用户观察改善 |
| 基础 WebUI | 位置、收藏、作用范围、主题、三个功能菜单 | 本地浏览器；新菜单未部署到手机 |
| 单次路线 | 点编辑、速度、暂停/继续、终点停留、GPX 导入 | 单次移动已在真机验证；文件选择待管理器验证 |
| 可选摇杆 | 面板启动/关闭、速度、触摸移动、松手停止、贴边隐藏 | 基础操作已在真机验证；新菜单关闭反馈待验收 |

## 本轮完成：路线重复与保存复用

- Rust 新增 `repeat_count`（总次数，默认 1，范围 1–10000）、`repeat_delay`（两轮之间的秒数，默认 0，范围 0–86400）。旧请求保持单次行为。
- 状态新增 `lap`（从 1 开始）、`waiting_seconds`。`distance` / `total_distance` 是当前一轮的进度。
- 每轮完成后停在终点，间隔结束后回到起点重新播放。最后一轮没有尾随等待，位置保持终点、速度为 0。暂停冻结移动及间隔倒计时。
- 用单调时钟和总活动时长直接计算轮次。等待不使用线程睡眠；暂停、停止可以立即处理；长时间未更新不会逐轮循环补算。
- 面板可设置重复次数和间隔，显示当前轮次、等待/暂停状态；运行期间锁定编辑。路线页范围入口改为右上角图标。
- 可命名保存路线，恢复坐标、速度和重复参数；不自动启动，删除支持撤销。保存失败时保留原列表，同名保存提示换名。
- 存储仍使用当前 WebUI 的 localStorage；本轮已补手动备份文件，未实现跨管理器自动同步。

TDD 先记录了重复命令不被识别、面板缺少设置与保存操作的失败，再完成实现。新增覆盖等待中暂停/继续/停止、总次数、轮次边界、长更新时间间隔、无效请求不替换当前路线、草稿迁移、保存后重开复用、撤销及存储失败。浏览器覆盖 320 像素宽度、长名称、两套主题以及运行中的等待状态恢复。

验证结果：`node build.mjs test` 通过（Rust 36 项、前端 28 项及 TypeScript 检查）；`node build.mjs test:e2e` 6 条通过；`node build.mjs build` 完成 Native、ARM64 Rust、WebUI、Android 和 ZIP 构建。日志位于 `build/route-repeat-library-{test,e2e,build}.log`，界面截图位于 `build/webui-route-library-{light,dark}.png`。产物 `dist/justlocation-0.1.0-dev-arm64.zip` 已更新，未安装到手机。

## 本轮继续完成：备份、坐标转换和地图

- 设置页可导出位置、收藏标记和保存路线的 JSON 备份；导入先预览，再合并，重复内容跳过，ID 冲突保留两份。无效文件拒绝整份导入，第二项写入失败时回滚第一项。
- KernelSU 中分块写到 `Download/JustLocation`，完整写入后才发布文件；普通浏览器使用下载。前者已做命令级模拟测试，实际管理器写入待手机验收。
- 常用路线可导出 WGS84 GPX。GPX 保存坐标与海拔，速度和重复参数用 JSON 备份保留。
- 位置编辑支持 WGS84、GCJ-02、BD-09 输入，保存时统一为 WGS84；算法参考 MIT coordtransform，GCJ 适用范围判断是近似矩形，不是行政区域边界。
- 独立全屏地图选点保留名称和海拔，返回不修改输入。路线支持单点地图编辑及整条路线自由规划、编号连线、撤销末点、确认提交；仍按点连接，不是沿道路导航。
- 底图使用 Leaflet 1.9.4 / OpenStreetMap，显示署名；自动测试统一拦截瓦片为本地模拟图片，没有请求公共瓦片。实际网络加载和 KernelSU WebView 行为待验收。

验证：Rust 36 项、前端 43 项、TypeScript 检查及浏览器 11 条通过，完整构建与 ZIP 已更新。日志 `build/map-backup-test.log`、`build/route-map-e2e.log`、`build/map-backup-build.log`；截图 `build/webui-map-{light,dark}.png`、`build/webui-route-map.png`。未连接手机或安装模块。

## 后续阶段

2026-09-10 环境阶段进展：

- GNSS 合成卫星帧、NMEA 语句、监听代理和独立派发已接入 Java / Native 桥接，Java 本地测试通过（`build/gnss-integration-test.log`）。独立通道设置、原始测量/导航电文、ARM64 整体构建和真机验证仍待完成。
- 基站模型覆盖五类无线制式；已实现 OpenCellID 区域查询与分页、自定义 JSON 供应商、来源与许可信息、区域缓存、离线查询及标准化 JSON 导入。独立 CLI 查询进程不占用定位控制后台。Fake Location 备用供应商仅预留，鉴权和请求编码仍未验证；批量 CSV 与区域索引尚未实现。
- WebUI 基站菜单现可进入全屏查询页，包含收起的供应商设置、在线/离线查询、导入导出。基站模拟开关仍禁用，电话服务 Hook 尚未实现，查询成功不代表已输出模拟基站。
- 本次主机回归通过：Rust 51 项、前端 45 项及 TypeScript 检查，日志 `build/environment-regression-test.log`。新环境代码尚未完成 ARM64 整体打包、浏览器回归和真机验收，`dist` 仍是此前版本。本轮未访问手机。

1. **环境通道（用户最新优先级）**：优先完整推进 Wi-Fi、基站、SIM、GNSS/NMEA/原始测量与步数。基站从 UI 占位升级为实际功能，覆盖同步查询、异步查询、监听更新、作用范围和停止恢复。以 Android 15 为目标，逐项记录实际覆盖，不能用旧样本的方法存在代替验收。
2. **环境通道验证**：先做模型及接口测试，在 AVD 验证 Android 对象和检查 App；目标进程 Hook、ROM 签名及真实接收端由手机补充验收。普通应用仍不安装功能 Hook；电话服务所在的系统应用进程需专门接入。
3. **地图、路线和备份后续暂停**：已完成代码保留。路线录制尚未开始实现，等环境通道完成后再继续。
4. **真机验收**：环境通道代码完成后，集中验证环境输出、重复路线实际坐标/速度、等待中停止、GPX 文件选择、新菜单与摇杆关闭反馈。
5. **高德专项**：按用户要求留到主要功能完善后；网络融合定位只是待验证方向。

AVD 为 Android 15 x86_64，没有 KSU/Zygisk，不能证明 ARM64 system_server 或电话服务 Hook 的稳定性。

## 基站输出与电话服务接入（2026-09-10 晚）

- Rust 已加入基站/SIM 配置和输出帧，保存格式升级到 v3 并兼容 v2。按卡槽与订阅匹配 MCC/MNC（保留前导零）或 CDMA SID，选取附近基站，移动时重新选取注册基站，信号强度为距离模型生成。无覆盖区域或超出区域返回空基站，停止后移除输出帧。
- Java 已实现 GSM/WCDMA/LTE/NR/CDMA 的 CellInfo、CellIdentity、信号及 SubscriptionInfo 对象。缺失 PCI/ARFCN 等信息保留 unavailable，NR 的 36 位编号使用 long，跨 Binder 的 Parcel 往返通过 AVD 检查。
- PhoneInterfaceManager 同步查询、缓存查询和一次性异步查询已接入，替换点位于原有权限与订阅有效性检查之后。TelephonyRegistry 监听代理与周期派发已接入；保留 Binder 身份、权限和当前用户检查，停止后按监听事件恢复最新真实缓存，不改动共享真实缓存。
- Native 增加 UID 1001 且进程名为 `com.android.phone` 的定向入口；普通应用不安装功能 Hook。电话模块 DEX 使用 LSPlant 单独标记为可信，并对相关调用者取消方法内联。电话进程实际启动、ROM 行为与 SELinux 仍待真机验证。
- 基站查询、基站监听、SIM 分开报告就绪状态；查询与监听两端心跳均正常时，`cell_hook_ready` 才为 true。SIM 对象与配置已完成，SIM 查询 Hook 和用户配置尚未完成。
- 本轮通过 Rust 58 项、前端 45 项及 TypeScript 检查，Java 新增查询 4 项、监听 4 项测试。日志：`build/telephony-integration-host.log`、`build/telephony-integration-java.log`。AVD 真实对象及电话/Registry 方法签名检查通过：`build/telephony-avd-signatures.log`。
- AVD APK 优化移除了 PhoneInterfaceManager 私有命令常量。使用 Jadx 核对 AVD TeleService DEX 中实际值为 60/62/66，与 Android 15 AOSP 一致；常量字段存在时优先读取，否则用上述值。其他 ROM 须核对，不能仅凭方法签名声称适配。参考位于 `build/environment-reference/avd-PhoneInterfaceManager.java`。
- ARM64 构建暴露 HTTPS 依赖缺少 C 编译器配置，已在 `build.mjs` 补 NDK clang/llvm-ar 与目标 API。首次电话查询版本完整构建通过（`build/telephony-arm64-build-fixed.log`）；其后的 Registry/取消内联改动待最终重新打包。未安装到手机。
- 下一步：SIM 查询与卡槽识别、面板配置；随后推进 Wi-Fi、步数、GNSS 独立控制及原始数据通道。现有基站菜单暂不开放启用，避免在卡槽配置尚未完成时显示可用。

## 基站面板、SIM 查询与 ServiceState（2026-09-11）

- 面板识别实际活动卡槽，只采集订阅 ID、卡槽、MCC/MNC、国家与运营商六项；不采集号码或 ICCID。读取失败和没有 SIM 卡分别呈现，不自动覆盖用户保存的模拟配置。
- 基站与 SIM 运营商开关、卡槽选择、运营商编辑、模拟半径、查询和应用数据已接通。状态刷新保留未保存输入，保存失败保留草稿；未勾选的卡不要求填写运营商信息。主菜单可直接切换基站开关，保留 SIM 设置。
- SubscriptionManagerService 八个列表/单项查询在原有授权、过滤和脱敏之后替换运营商。保留真实卡槽和订阅 ID，不新增虚构卡片，不改变原始数据库。属性类 TelephonyManager API 和订阅变化通知仍待实现。
- ServiceState 查询及 Registry 监听替换网络注册项的蜂窝身份、制式和运营商，并清理真实无线频道等元数据。粗略/精确定位脱敏结果保持隐藏；WLAN 注册及实际连接状态保留。无目标数据清空对应身份，停止恢复授权真实缓存。
- Native 电话心跳加入 UTF-8 卡槽清单，限制传输大小；Java 响应改用标准 UTF-8 解码，支持运营商名称中的非 BMP 字符。
- 前端 50 项、TypeScript 及浏览器 12 条通过（`build/telephony-ui-green.log`、`build/telephony-ui-e2e.log`）；ServiceState 查询 2 项、Registry 5 项及其余 Java 测试通过（`build/service-state-green.log`）。AVD 对象、权限裁剪、Parcel 往返和实际服务签名通过（`build/service-state-avd.log`）。
- 完整 ARM64 构建已通过，当前 ZIP 包含上述改动：`dist/justlocation-0.1.0-dev-arm64.zip`；日志 `build/environment-arm64-build.log`。未安装到手机。AVD 无 KSU/Zygisk，实际进程 Hook、SELinux 和 ROM 适配仍待用户允许使用手机后验证。

## 卫星通道（2026-09-11 续）

GNSS 状态与 NMEA 的**配置与投递都已接好**：

- 配置层（提交 `baaaee8`）：`backend/src/gnss.rs` 定义两个开关，协议新增 `set_gnss`，配置随启动写入 `Stored`（旧配置文件缺少该段仍可加载），保存失败会连同其它配置一起回滚；WebUI 在位置页新增"卫星"卡片。
- 投递层（本轮）：桥接里 `GnssListener` 早已实现按活跃注册每秒投递合成卫星状态（`GnssFrame` 的合成星座，规格第 7.4 节确认原版投递的也是预置数组而非计算结果），但**投递不看通道开关**，等于开关没接线。现在两条通道各受自己的开关控制：只有"定位模拟运行 + 应用在作用范围内 + 开关打开"三者同时成立才接管，任一不满足即原样转发系统回调，也就是"未启用等于系统原样"。另外把模拟期间到达的真实 NMEA 报文改为**停止后补发**（上限 32 条）而不是丢弃，避免应用看到报文流凭空中断。
- 测试：桥接 JVM 单元测试 36 项（新增 4 项覆盖开关门控、关闭时原样转发、停止后补发、补发数量有界），后端 74 项、前端 86 项、e2e 15 条，`node build.mjs build` 通过并已重新打包 ZIP。

**仍未真机验证**：这四条行为都只在 JVM 单元测试里验证过。真机上需要确认的是——开关打开后应用确实收到合成卫星状态与 NMEA、关闭后恢复系统输出、以及停止模拟时的报文补发不会造成异常。原始草稿 `build/pending/satellite_protocol.rs` 与失败日志 `build/satellite-red.log` 保留作参考。之后继续 Wi-Fi 输出、步数、原始测量与导航电文，以及剩余 SIM 接口。

## 暂停记录（2026-09-11）

用户准备休息，要求暂停，明天继续。本轮未使用手机；“手机提供网络，暂时不要使用”的限制仍然有效，不能因恢复开发就自动恢复真机调试。

当前完整安装包已构建，未部署。基站/SIM 面板和 ServiceState 查询、监听、脱敏及恢复已完成本地与 AVD 对象检查，实际 Zygisk Hook 验证尚未进行。

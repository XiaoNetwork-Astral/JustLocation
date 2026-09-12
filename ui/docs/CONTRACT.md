# 面板与后台的协议约定

这份文档是 `ui/` 与模块之间**唯一的耦合面**。只要保持这里的约定不变，面板可以独立开发、独立替换，后台也可以独立演进。

## 通道

面板运行在 KernelSU 的 WebView 里，通过 KernelSU JavaScript SDK 的 `exec(command, options, callback)` 执行 shell 命令。两件事都走这个通道：

```
# 控制与状态（唯一常驻进程）
/data/adb/modules/justlocation/bin/justlocationd request <base64-json>
# 基站数据子命令（独立进程，用于查询供应商与缓存）
/data/adb/modules/justlocation/bin/justlocationd cells <base64-json>
```

`<base64-json>` 是**单个 shell 参数**：把 `{ "version": 1, ...command }` 用 UTF-8 编码后 base64。命令与响应都不带换行；**响应帧上限 64 KiB**（前端在 65536 处预拦）。

响应形状：

```json
{ "version": 1, "ok": true, "error": null, "state": { ... }, "cells": null }
```

- `errno !== 0` 表示命令本身没跑起来（模块没启用、后台没起），此时读 `stderr`。
- `ok === false` 表示后台拒绝了这次命令，原因在 `error` 里（面向用户的中文/英文句子，可直接显示）。
- 校验：`version === 1`、`ok` 是布尔、`state.requested_active` 是布尔、`state` 里有 `config` 键。任一不满足就报"后台响应格式不兼容"。

**命令枚举是严格模式**（Rust 侧 `deny_unknown_fields`）：多塞任何字段整条命令会被拒绝，所以前端发的对象要精确匹配下面的形状。

## 命令

| op | 参数 | 语义 |
|---|---|---|
| `status` | — | 读快照；同时会推进运动/路线补偿计算，所以轮询也是"报时" |
| `start` | `config: { position, scope }` | 开始定位模拟 |
| `update` | `position` | 换目标位置；**有路线在跑时会被拒绝** |
| `stop` | — | 停止模拟，同时清掉路线与运动状态 |
| `start_route` | `route: RoutePlan, scope: Scope` | 用路线首点启动播放 |
| `pause_route` / `resume_route` | — | 无路线时报错 |
| `drive` | `speed, bearing` | 让后台按方向/速度推进位置（摇杆用；**前端尚未接入**） |
| `set_telephony` | `config: TelephonyConfig` | 写基站/SIM 配置 |
| `set_cell_region` | `region: CellRegion \| null` | 写"已应用到模拟"的基站数据 |
| `set_gnss` | `config: GnssConfig` | 写卫星两个开关 |
| `set_wifi` | `config: WifiConfig` | 写 Wi-Fi 目标列表 |
| `record_start` | — | 开始录制路线：只收集真实位置，不产生输出 |
| `record_point` | `position: Position, seconds: f64` | 把一条真实定位交给录制；`seconds` 是本次录制的单调时间戳（从 0 开始，秒） |
| `record_stop` | — | 结束录制并把轨迹交回 `state.recorded`；一个点都没录到时报错 |
| `record_discard` | — | 丢弃当前录制 |
| `query_cells` | `target, radius_m, limit` | 对已应用区域做邻近查询（**前端尚未接入**） |
| `shutdown` | — | 停止一切并让 `serve` 进程退出（**前端尚未接入**） |
| `hook_status` / `telephony_hook_status` | — | **由系统侧 Java 上报**，面板不发 |

## 数据形状

```ts
interface Position { latitude: number; longitude: number; altitude: number; accuracy: number; speed: number; bearing: number }
type Scope = { mode: 'all' } | { mode: 'apps'; packages: string[] }
interface Config { position: Position; scope: Scope }

interface RoutePlan { points: Position[]; speed: number; repeat_count?: number; repeat_delay?: number }  // speed 单位 m/s
interface RouteState { plan: RoutePlan; distance: number; total_distance: number; paused: boolean; completed: boolean; lap?: number; waiting_seconds?: number }

interface DetectedSubscription { id: number; slot: number; mcc: string; mnc: string; country: string; carrier: string }
interface Subscription extends DetectedSubscription { enabled: boolean; cdma_sid?: number }
interface TelephonyConfig { cells_enabled: boolean; sim_enabled: boolean; radius_m: number; subscriptions: Subscription[] }

interface GnssConfig { gnss_enabled: boolean; nmea_enabled: boolean }
interface WifiTarget { id: string; ssid: string; bssid: string; rssi: number; link_speed: number; frequency: number }
interface WifiConfig { enabled: boolean; targets: WifiTarget[] }

/** 录制进度：面板据此显示"已录 N 个点 / M 秒"。 */
interface RecordProgress { points: number; seconds: number; full: boolean; skipped: number }
/** 录制成品；点数不足以回放时也会返回，由界面提示用户再录一段。 */
interface RecordedTrack { points: Position[]; seconds: number }

interface State {
  requested_active: boolean
  config: Config | null
  // 通道就绪位：区分"后台连上了"与"接口真的接上了"
  hook_connected?: boolean; location_hook_ready?: boolean
  phone_connected?: boolean; cell_hook_ready?: boolean; sim_hook_ready?: boolean
  gnss_hook_ready?: boolean; nmea_hook_ready?: boolean
  cell_query_hook_ready?: boolean; cell_callback_hook_ready?: boolean
  // 运营商名称与 PLMN：TelephonyManager 的 getter 读系统属性，与订阅记录是两条独立的出口
  operator_hook_ready?: boolean
  // Wi-Fi 服务端两项适配：扫描结果与连接信息是两次独立的安装，分开报告
  wifi_scan_hook_ready?: boolean; wifi_connection_hook_ready?: boolean
  route?: RouteState | null
  detected_subscriptions?: DetectedSubscription[] | null
  telephony?: TelephonyConfig
  gnss?: GnssConfig
  wifi?: WifiConfig
  /** 正在录制时的实时进度；停止或丢弃后回到 null。 */
  recording?: RecordProgress | null
  /** 最近一次录制的结果，面板取走后清空。 */
  recorded?: RecordedTrack | null
  telephony_output?: { availability: 'disabled' | 'ready' | 'missing_region' | 'outside_region'; groups: { cells: unknown[] }[] } | null
}
```

## 路线录制怎么用

录制与定位模拟**互斥**，两个方向都会被拒绝：

- 模拟运行时发 `record_start` → `stop the simulation before recording a route`。
- 录制中发 `start` / `start_route` → `stop recording before starting the simulation`。

原因是系统回调在模拟运行时给出的是我们自己的合成位置，照单全收会录出一条绕回自身的轨迹。
所以流程是：停止模拟 → `record_start` → 持续发 `record_point` → `record_stop` 取回
`state.recorded` → 把它的 `points` 当作一条路线（`RoutePlan`）交给 `start_route`。

后台侧的抽稀与上限（实现在 `backend/src/record.rs`）：与上一点距离小于 **0.5 米**的采样按重复丢弃
（计入 `skipped`），最多 **128 个点**（与路线模型一致），录满后 `full=true` 并自动停止接收新点，
已录到的部分仍可保存。**点数小于 2 时不能回放**，界面应提示再录一段，而不是发一条会被拒绝的路线。

**还没实现的一半**：谁来提供真实位置。录制端（`record_start` 之后持续发 `record_point` 的那一方）
尚未接入——需要在设备上取真实定位（模拟停止时系统回调就是真实位置，或者用定位之外的通道），
再经 root 侧送回后台。后台不关心位置从哪来，只要点符合上面的约定。

## 心跳与新鲜度

- **面板不发心跳**。后台按 `hook_status` / `telephony_hook_status` 的到达时间判断系统侧是否在线，窗口 **3 秒**；超过 3 秒的快照不用于模拟。
- `location_hook_ready = hook_connected && installed`；`cell_hook_ready` 还要求 `cells_installed` 与 `cell_callbacks_installed`。所以"开关开着"和"真的接上了"是两件事，界面要把这两种状态分开表达。
- 停止操作在下一次轮询生效；面板空闲时也应继续轮询（当前实现：模拟中 2 秒、空闲 5 秒、后台不可达 15 秒）。

## 客户端侧的其它约定

除 `justlocationd` 外，面板还会直接执行这些命令（`joystick.ts` / `fileExport.ts`）：

- 摇杆：`pm path me.idk.justlocation.joystick`（检查模块自带的摇杆 App 是否装了；显示名 JustLoystick）、`am start -n me.idk.justlocation.joystick/.CallActivity --ef speed <m/s>`（透明空壳 Activity，由它在前台状态下启动摇杆服务；界面给的 km/h 需先除以 3.6）、`am stopservice -n me.idk.justlocation.joystick/.JoystickService`。关闭摇杆**不停止位置模拟**。摇杆 App 随模块安装、随模块卸载删除，没有启动器与页面；它要调 `su`，**需要用户在 KernelSU 管理器里授权一次 root**，否则面板点开摇杆会看到"会话未就绪"。
  **不要改成 `am start-foreground-service`**：shell 从后台启动前台服务会被系统拒绝（`Background start not allowed`），这条限制只认调用方，授 appop、改待机桶、用 root 下发都绕不过去（2026-09-12 真机逐条试过）。
- 导出：在 KernelSU 下分块 base64 写入 `Download/JustLocation`，写完才发布文件；普通浏览器走 Blob 下载。
- 应用列表：KernelSU SDK 的 `listPackages` / `getPackagesInfo`，不是 `justlocationd`。

## 持久化

- 后台的配置存在 `/data/adb/justlocation`（单文件、原子替换、保存失败回滚），面板**不直接读**它，一切经 `status`。
- 面板自己的历史位置、路线草稿、主题、作用范围草稿存在 WebView 的 `localStorage`，键名：`justlocation.places`、`justlocation.routes`、`justlocation.route`、`justlocation.scope`、`justlocation.theme`、`justlocation.style`、`justlocation.joystick.speed`。清除站点数据会丢，所以要保留导出备份的入口。

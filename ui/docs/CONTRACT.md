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
| `set_wifi` | `config: WifiConfig` | 写 Wi-Fi 目标列表（**输出通道尚未实现**） |
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

interface State {
  requested_active: boolean
  config: Config | null
  // 通道就绪位：区分"后台连上了"与"接口真的接上了"
  hook_connected?: boolean; location_hook_ready?: boolean
  phone_connected?: boolean; cell_hook_ready?: boolean; sim_hook_ready?: boolean
  gnss_hook_ready?: boolean; nmea_hook_ready?: boolean
  cell_query_hook_ready?: boolean; cell_callback_hook_ready?: boolean
  route?: RouteState | null
  detected_subscriptions?: DetectedSubscription[] | null
  telephony?: TelephonyConfig
  gnss?: GnssConfig
  wifi?: WifiConfig
  telephony_output?: { availability: 'disabled' | 'ready' | 'missing_region' | 'outside_region'; groups: { cells: unknown[] }[] } | null
}
```

## 心跳与新鲜度

- **面板不发心跳**。后台按 `hook_status` / `telephony_hook_status` 的到达时间判断系统侧是否在线，窗口 **3 秒**；超过 3 秒的快照不用于模拟。
- `location_hook_ready = hook_connected && installed`；`cell_hook_ready` 还要求 `cells_installed` 与 `cell_callbacks_installed`。所以"开关开着"和"真的接上了"是两件事，界面要把这两种状态分开表达。
- 停止操作在下一次轮询生效；面板空闲时也应继续轮询（当前实现：模拟中 2 秒、空闲 5 秒、后台不可达 15 秒）。

## 客户端侧的其它约定

除 `justlocationd` 外，面板还会直接执行这些命令（`joystick.ts` / `fileExport.ts`）：

- 摇杆：`pm path me.idk.justlocation.companion`（检查可选 App 是否装了）、`am start -n me.idk.justlocation.companion/.JoystickActivity --es speed <km/h> --ez open true`、`am stopservice -n me.idk.justlocation.companion/.JoystickService`。关闭摇杆**不停止位置模拟**。
- 导出：在 KernelSU 下分块 base64 写入 `Download/JustLocation`，写完才发布文件；普通浏览器走 Blob 下载。
- 应用列表：KernelSU SDK 的 `listPackages` / `getPackagesInfo`，不是 `justlocationd`。

## 持久化

- 后台的配置存在 `/data/adb/justlocation`（单文件、原子替换、保存失败回滚），面板**不直接读**它，一切经 `status`。
- 面板自己的历史位置、路线草稿、主题、作用范围草稿存在 WebView 的 `localStorage`，键名：`justlocation.places`、`justlocation.routes`、`justlocation.route`、`justlocation.scope`、`justlocation.theme`、`justlocation.style`、`justlocation.joystick.speed`。清除站点数据会丢，所以要保留导出备份的入口。

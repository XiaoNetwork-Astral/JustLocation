# JustLocation

个人自用的 Android 虚拟定位项目。采用一个 Git 仓库管理 Zygisk 模块、Rust 后台、React 控制面板和可选 Kotlin App。

首个运行目标为 Android 15 ARM64、KernelSU 3.3.0（32601-2）和 Zygisk Next，面板使用 KernelSU WebUI。

## 工作区布局

正式工程直接位于当前仓库根目录：

```text
JustLocation/
├── native/              # C++：Zygisk 入口、JNI、LSPlant 与 ShadowHook 接入
├── backend/             # Rust：常驻后台、运行状态、路线计算和数据存储
├── webui/               # React + TypeScript：KernelSU WebUI 控制面板
├── android/             # 共用一个 Gradle 工程
│   ├── bridge/          # Java/Kotlin：系统服务适配，编译成模块内的 DEX
│   └── companion/       # Kotlin：可选 App，提供悬浮摇杆等原生操作
├── module/              # 模块元数据、安装和启动脚本等打包输入
├── .codex/              # 本地协作资料，忽略提交
│   ├── project/         # 备忘录与必要的工作记录
│   └── references/      # 下载的参考仓库
├── build/               # 构建时生成：集中组装与临时产物，忽略提交
└── dist/                # 构建时生成：最终模块 ZIP、可选 APK，忽略提交
```

`build/` 和 `dist/` 由构建过程按需创建。各语言工具自己的缓存与中间产物也忽略提交。

## 模块分工

- `backend` 是运行状态的管理者。路线执行独立于面板和可选 App，控制端负责发出操作并展示状态。
- `native` 负责进入目标进程及连接 Hook 库。普通应用只经过轻量入口判断并卸载；目标系统进程才初始化 Hook 和业务代码。
- `android/bridge` 随模块打包，负责 Android 对象、服务接口和回调，不依赖安装 `android/companion`。
- `webui` 构建出的静态资源装入模块的 `webroot/`，不在源码目录中维护第二份副本。
- `module` 保存安装包的源文件；组装出的完整模块目录放在 `build/`。

## 构建组织

各语言保留自己的常规工具：`native` 使用 CMake/Android NDK，`backend` 使用 Cargo，`webui` 使用 npm，`android` 使用 Gradle。初期一个 Rust 包、一个前端包、一个包含两个模块的 Gradle 工程，不额外拆分仓库或引入多项目调度框架。

根目录使用 `build.mjs` 统一组织构建，无需 Python。需要 Node.js、Rust 1.89+、JDK 21 和 Android SDK。NDK、CMake、Build Tools 版本见 `project-config.json`，SDK 位置通过 `ANDROID_HOME` 或 `android/local.properties` 的 `sdk.dir` 配置。

```sh
node build.mjs setup       # npm 锁定依赖及 Rust Android target
node build.mjs test        # Rust 行为/进程测试、TypeScript 和 React 交互测试
node build.mjs test:android # Android bridge 的 JVM 单元测试
node build.mjs build       # 构建全部组件并输出模块 ZIP、可选 debug APK
node build.mjs test:e2e    # 本地 Chromium：应用选择、窄屏编辑、抽屉、主题、宽屏布局
node build.mjs test:device <adb-serial> # 手机临时独立进程：后台、ART Hook 和生产 DEX 引导
node build.mjs test:transport <adb-serial> # ARM64/root：一次性子进程验证 SELinux socket 交接
node build.mjs test:transport-io <adb-serial> # ARM64 或 x86_64：通信及延迟请求，不需 root
```

浏览器测试需先安装 Playwright Chromium headless shell，并将 `PLAYWRIGHT_BROWSERS_PATH` 指向根目录 `build/cache/playwright`；执行 `webui/node_modules/playwright/cli.js install chromium --only-shell`。单独构建可用 `native`、`backend`、`webui`、`android`；`pack` 仅组装已有产物，修改代码后应使用 `build`。

Windows 的 `setup` 将 CMake 4.0.3 下载到 `build/tools`，不修改 SDK。Ninja 使用 SDK CMake 3.31.6 附带版本。其他宿主或自定义安装可通过 `JUSTLOCATION_CMAKE` 指定 CMake 4.0.2+ 可执行文件。

正式构建依赖由工程配置固定版本获取，不能读取 `.codex/references` 中的参考副本。这样将来只克隆本仓库也能恢复构建环境。

当前已有可构建的四部分工程、Rust 坐标与应用范围模型、配置保存、常驻 Unix socket 服务，以及 React 第一版定位/范围/设置界面。面板沿用旧应用的侧边导航、目标位置卡片和历史记录布局，手机采用底部坐标弹层，宽屏展开侧栏，外观参考 RikkaHub 的 Material 风格。

作用范围从位置或路线页进入，不再单列侧栏入口；返回和完成放在顶部，应用列表独立滚动。通过 KernelSU SDK 读取已安装应用，支持搜索、勾选和显示系统应用。草稿自动保存，切到全部应用再切回来、退出重开都保留勾选；模拟运行时不能修改范围。已在 KernelSU 3.3.0 真机验证新版界面、范围切换保留选择和系统返回键。

Zygisk 入口在普通应用内请求卸载，仅 `system_server` 加载独立运行库及 DEX。运行库已连接 LSPlant/ShadowHook，按 Android 15 AOSP 接口接入最近位置、单次定位及已有定位回调替换；系统原有的权限、AppOps、粗略位置处理继续执行。新增每秒主动派发，通过平台的活跃订阅和原有发送逻辑投递，仅匹配目标包名，不改全局位置缓存。派发的 3 项单元测试及真机独立进程接口/DEX 预检通过；更新并解锁后，最近位置、单次定位、换点、排除范围、持续订阅中换点及停止后原订阅恢复真实输出均通过，系统进程 PID 保持不变。

路线支持按顺序编辑坐标点、调整顺序、设置速度、开始、暂停、继续和停止。后台按单调时钟推进，关闭面板后继续；重启后台不会自动恢复运行。可设置总播放次数和每轮间隔：等待时停在终点，间隔结束后回到起点，最后一轮结束后保持终点。等待期间可以暂停、继续或立即停止，不阻塞后台控制。旧版未指定重复设置的请求仍按单次播放处理。

坐标点之间沿最短球面路径插值。位置和单个路线点可在全屏地图选取；地图规划支持添加多个点、预览编号连线和撤销，确认后才替换草稿，尚无沿道路规划。位置编辑支持 WGS84、GCJ-02、BD-09 输入，保存为 WGS84。底图采用 Leaflet / OpenStreetMap，需联网。

GPX 支持导入、分段预览和选择，也可以导出保存的路线。路线可命名保存，重新选用时恢复坐标、速度和重复设置，删除后可撤销；导入或选用不会自动开始。设置页支持位置与路线的 JSON 备份、预览和合并导入，保留收藏及重复参数。KernelSU 下导出到 `Download/JustLocation`，普通浏览器使用下载。数据仍保存在当前管理器的 WebUI 存储中，清除前应导出备份。真机已验证单次路线推进、暂停和继续；重复播放、地图、文件选择、导出写入及常用路线目前只通过本地验证。

主动派发更新后，用户观察百度已无明显跳点；高德仍显示实际位置，按用户要求暂缓专项排查。网络融合定位是待验证方向，尚未确定根因。基础检查 App 通过不能替代地图验收。当前未实现 GNSS 状态/NMEA/原始测量及 Wi-Fi、基站等环境通道的配套处理。

ZIP 属于开发阶段产物，不能将面板的“会话已启动”或“Hook 已就绪”当作真实定位已改变。可选 Kotlin App 提供独立的定位接收检查页面，直接调用 Android 最近位置/单次/持续定位接口，不从 Rust 后台读坐标；将 `me.idk.justlocation.companion` 加入作用范围，可用于检查开始、换点、停止及未选中范围的行为。

悬浮摇杆由面板统一打开、设置最高速度和关闭，App 完成悬浮窗授权后显示小摇杆，拖到边缘可收起，点边缘把手展开。按住摇杆沿球面移动，松手停留；后台在 2 秒未收到输入后停止移动。真机已验证触摸移动、松手保持、Android 定位接口输出及输入超时停止。

位置卡片右侧集中放置独立模拟、基站、摇杆三个图标菜单，不再显示单独的摇杆卡片。独立模拟在指定应用和全部应用间切换，保留已选列表；运行时锁定范围。基站菜单仅呈现禁用的启用按钮和空列表，尚未接入查询或模拟。启停按钮在后台确认后变色，支持浅色和深色主题。前端通过 28 项测试及 6 条浏览器用例，包含 320 像素窄屏菜单、路线保存复用、等待状态恢复、异步启停确认及摇杆命令；尚未更新到手机。

已在目标手机（Android 15、HyperOS OS3.0.5.0.VMRCNXM、Zygisk Next 1.5.0）完成独立进程预检：ARM64 后台的内存会话、LSPlant/ShadowHook 初始化、测试方法 Hook/原方法/解除 Hook、ROM 定位方法签名，以及生产运行库加载 DEX、安装 provider Hook 和上报心跳均通过。`test:device` 只以 adb shell 身份在临时目录运行，退出后清理，不安装模块/APK、不使用 root、不调用真实定位服务。它不会验证 Zygisk 加载时机、system_server 的 SELinux 环境、与现有 LSPosed 的共存或实际定位输出。探针单独构建，不进入模块 ZIP。

2026-09-09 已经用户同意，在唯一目标手机安装开发模块及检查 APK，并重启一次。手机正常完成启动，观察期间 `system_server` PID 保持不变。后台首次因当前 Rust 工具链的 Android `File::try_lock` 返回 Unsupported 而退出；已改用 Android libc `flock`，真机验证锁竞争及释放、后台 Unix socket 状态查询和重复启动拒绝，并更新设备上的后台程序，无需再次重启。

2026-09-10 的修复版已在目标手机两次正常启动，真实后台报告 `hook_connected=true`、`location_hook_ready=true`。检查 App 的 GPS/network 最近位置、单次定位、换点、停止及排除应用范围均通过；持续 GPS 订阅能在运行中换点，停止模拟后同一订阅恢复原始输出。首次的 ShadowHook 错误 12 已通过无 `LD_LIBRARY_PATH` 的 fd 加载测试复现：辅助库无法按名称找到。加入 `$ORIGIN` RUNPATH、在系统进程降低权限前初始化 ShadowHook 后，独立探针和实际开机均确认初始化成功。

但 2026-09-09 23:18 的更新验证随后卡在系统启动阶段。23:21 已通过 ADB/root 创建模块 `disable` 标记并重启撤回，23:22 确认 `sys.boot_completed=1`，系统进程恢复运行。日志显示 LSPlant memfd 执行映射和继承的 companion socket 被 SELinux 拒绝；前者有备用分配路径，不能直接认定为卡住的根因。模拟会话一直停止，未进行真实位置替换。

后续修复已移除全局 ClassLoader Hook，改用 Android 15 已缓存的 system_server ClassLoader，直接安装 provider Hook。companion 改为在 root 域创建 socketpair，并通过 SCM_RIGHTS 传递客户端 fd；一次性 native 子进程验证了 UID 1000 / system_server 下旧 zygote 标签 socket 被拒绝、新 socket 往返成功。服务端等待请求保留连接，客户端响应超时和两端发送超时仍为 4 秒；AVD 已复现并修复原先闲置 5 秒永久断连的问题。

最后一处开机循环来自已有 LSPosed Hook 的 `LDR X17 / BR X17` 跳转入口：ShadowHook 原方法回跳覆盖 X17，导致系统主线程原地循环。`tests/device/veneer.cpp` 在独立进程复现故障，再验证保留寄存器的修复，已纳入 `test:device`。真实重装后系统主线程正常运行、桥接心跳正常，手机上原有 LSPosed 保持启用。

安装模块和检查 APK 后，解锁手机并保持定位开启，可用以下命令验证真实 Android 输出：

```powershell
.\tests\device\location.ps1 -Serial <adb-serial>                  # 最近位置、单次、换点、停止、排除范围
.\tests\device\location.ps1 -Serial <adb-serial> -Mode continuous # 同一订阅中换点
.\tests\device\location.ps1 -Serial <adb-serial> -Mode restore    # 停止模拟后同一订阅恢复原始输出
.\tests\device\location.ps1 -Serial <adb-serial> -Mode route -PreserveConfig # 路线暂停/继续及实际输出，结束恢复原配置
```

前三项已在目标手机的基础桥接版本通过，全程 `system_server` PID 保持不变；新主动派发版本需重跑。测试默认要求空配置或上次测试留下的配置；加 `-PreserveConfig` 可临时使用测试配置，结束后恢复原有配置并停止模拟。测试仅对检查 App 开启模拟，排除范围用不存在的测试包名，结束时关闭检查 App。KernelSU 3.3.0 管理器内也已验证面板读取后台、开始模拟、检查 App 收到目标坐标，以及从面板停止后恢复原始输出。

## 本地 AVD

当前本机已准备 `JustLocation_API35`：Android 15 AOSP x86_64，串号固定为 `emulator-5580`，WHPX，2 GB 内存。SDK/镜像与数据位于 `build/avd-sdk`、`build/avd-home`，不提交。以下命令使用这套已准备的本地环境：

```powershell
.\tools\avd.ps1 start          # 默认无窗口；加 -Visible 显示窗口
.\tools\avd.ps1 status
node tools/avd-console.mjs avd name
node build.mjs test:transport-io emulator-5580
.\tests\device\avd-location.ps1 # 已安装检查 APK 后验证 Android 定位接收
.\tools\avd.ps1 stop
```

控制台工具固定端口并核对 AVD 名称，按模拟器提示的路径读取本机认证 token，不输出 token。定位测试通过模拟器的 [geo fix 命令](https://developer.android.com/studio/run/emulator-console) 输入 GPS 坐标。2026-09-10 已通过检查 App 的持续回调、换点、停止接收、最近位置和单次定位测试，以及闲置后继续通信的探针；系统进程保持不变。测试等待当前订阅收到定位，并容许模拟器坐标的微小舍入误差，不验证模块坐标替换。

AVD 可使用 `adb root`，但尚未安装 KernelSU/Zygisk；由于 ShadowHook 只支持 ARM，当前 x86_64 AVD 不能运行完整 ARM64 模块。完整模块的虚拟机验证仍需匹配架构及内核环境。本机本轮启动曾在受限进程的 WHPX 初始化处停住，退出该 AVD 后以获批的非受限进程启动成功，无需修改虚拟设备数据或手机。

若需要撤回且 adb/root 可用，在手机上创建 `/data/adb/modules/justlocation/disable` 并重启；本次启动故障已通过这条路径恢复。无法进入正常系统时，按 [KernelSU 官方恢复说明](https://kernelsu.org/guide/rescue-from-bootloop.html) 使用安全模式禁用模块；硬件按键安全模式尚未在此设备演练。

后台以 `justlocationd serve` 启动，数据位于 `/data/adb/justlocation`。WebUI 使用 KernelSU `exec` 调用 `justlocationd request <base64-json>`，命令经 Unix socket 交给同一个常驻进程；`stdio` 用于主机进程测试。协议版本为 1，支持 `status/start/update/stop/shutdown`；系统桥接通过 root companion 上报 `hook_status` 心跳并读取快照。心跳每秒更新，超过 3 秒的快照不用于模拟，停止操作在下一次轮询生效。配置重启后保留但默认停止；保存失败不改变原会话。历史位置暂存在 WebUI 本地存储中。

开发按阶段推进：工作区 → 后台模型 → 控制链路/WebUI → 系统定位 Hook → 路线与环境通道 → 可选 App → Android 15 真机验收。功能采用先失败测试、再实现、通过后整理的 TDD 流程；主机测试和编译不能替代系统接口的真机验证。

当前阶段和待办见 [开发进度](docs/development-plan.md)。

## 上游参考

Zygisk 工程布局基于 [5ec1cff/zygisk-module-template](https://github.com/5ec1cff/zygisk-module-template)，使用其提供的 `zygisk.hpp`，保留头文件内 MIT 许可及版权声明。构建入口改写为 Node.js，安装流程调整为 KernelSU 的默认解压与权限机制，打包 Rust、DEX 和 WebUI，只安装实际构建的 ARM64 文件。

原生构建固定 [LSPlant](https://github.com/LSPosed/LSPlant) 的 `b2f9279014091c83faac094c5fc7c5498261ace8` 和 [ShadowHook 2.0.1](https://github.com/bytedance/android-inline-hook/tree/v2.0.1)，由 CMake 获取实际源码及 DexBuilder 子依赖。ShadowHook 的警告组选用 `-Wall -Wextra -Werror`，其同时列出 ARM32/ARM64 符号的导出表允许当前架构缺少另一架构符号，保留正常未定义引用检查。`native/cmake/shadowhook-arm64.cmake` 生成带 X17 修复的编译副本，使用上游已有的寄存器保留回跳路径，入口仍只改写四字节。下载的上游源码保持原样；补丁不匹配时配置失败，升级依赖需重新核对。

WebUI 使用官方 [KernelSU JavaScript SDK](https://github.com/tiann/KernelSU/tree/main/js)，视觉参考 [RikkaHub](https://github.com/rikkahub/rikkahub)，不复制其业务界面。构建包 `licenses/` 保留相关依赖许可和 SDK 的 Apache-2.0 声明。

# JustLocation

个人自用的 Android 虚拟定位工具：Zygisk 接入系统服务，Rust 管理模拟状态与数据，Kotlin 提供摇杆和自检 App，React 提供控制面板源码。目标环境为 Android 15 / ARM64、KernelSU 与 Zygisk Next。

模块 id 为 `justlocation`，管理器显示名为 **Zygisk - JustLocation**。模块当前不打包 Web UI；控制命令通过 `justlocationd request` 和 `justlocationd cells` 调用，管理器操作按钮 `action.sh` 只报告状态。

## 工程结构

```text
JustLocation/
├── zygisk/              # 系统接入：C++、JNI、Java 桥接与本模块测试
│   ├── src/
│   ├── include/
│   ├── cmake/
│   ├── bridge/          # 独立 Gradle library，编译成模块中的 DEX
│   ├── tests/           # ART、加载器和传输探针
│   └── build.mjs
├── backend/             # Rust 后台；src/、tests/ 与 Cargo 配置
│   └── build.mjs
├── app/                 # 两个独立 Kotlin App
│   ├── joystick/        # 摇杆与路线录制，随模块安装和卸载
│   ├── probe/           # 逐通道自检，单独 APK
│   └── build.mjs
├── ui/                  # React + TypeScript；源码、单测、浏览器测试与参考资料
│   └── build.mjs
├── integration/         # 跨模块协议和设备验收
│   ├── CONTRACT.md
│   ├── device/
│   └── build.mjs
├── packaging/           # 组合各模块的产物
│   ├── module/          # 安装、启动、卸载脚本和许可
│   └── build.mjs
├── tool/                # 共用构建工具与本地模拟器工具
│   ├── build.mjs
│   └── gradle/          # 共用 Gradle wrapper；映射 bridge、joystick、probe
├── build.mjs            # 统一命令入口，只负责调度
├── project-config.json  # NDK、CMake、目标 ABI 等构建版本
├── build/               # 可生成的产物、工具缓存与临时文件，不提交
├── dist/                # 模块 ZIP 与 APK，不提交
└── .deepseek/           # 私人输入、协作记录与参考副本，不参与构建和提交
```

`zygisk/` 负责进入目标系统进程、安装 Hook、转换 Android 对象和投递回调；`backend/` 负责运行状态、路线计算、配置与基站数据。控制端通过[协议约定](integration/CONTRACT.md)发送命令，彼此不引用实现源码。桥接 DEX 不依赖两个 App，三个 Gradle 项目只共享工具链。跨模块流程由 `integration/` 验证，最终分发由 `packaging/` 组装。

每个模块自己的测试随模块保存。根入口调用模块的 `build.mjs`，各语言仍使用 CMake/NDK、Cargo、Gradle、npm，不引入额外的多项目调度框架。

## 构建与测试

需要 Node.js、Rust 1.89+、JDK 21 和 Android SDK。固定工具版本见 `project-config.json`。SDK 位置由 `ANDROID_HOME`、`ANDROID_SDK_ROOT` 或 `tool/gradle/local.properties` 的 `sdk.dir` 指定。

```sh
node build.mjs setup        # 安装锁定的 npm 依赖、Rust Android target 和构建工具
node build.mjs test         # Rust 测试、TypeScript 检查和 React 测试
node build.mjs test:android # bridge 与摇杆的 JVM 测试
node build.mjs test:e2e     # 前端 Playwright 浏览器测试
node build.mjs build       # 构建四个模块并打包
node build.mjs zygisk      # C++ 运行库和桥接 DEX
node build.mjs backend     # ARM64 Rust 后台
node build.mjs app         # 摇杆与自检 APK
node build.mjs ui          # 前端检查与构建
node build.mjs pack        # 只组装已有产物
```

原有 `native` 命令单独构建 C++，`android` 命令构建 DEX 和两个 APK。原生产物继续集中在 `build/native/`，Rust 产物在 `build/cargo/`，DEX 在 `build/bridge/`。

Gradle 可直接从 `tool/gradle/` 使用 wrapper；Cargo 可在 `backend/` 运行；前端的独立开发与测试见 [ui/README.md](ui/README.md)。浏览器测试需要预先安装 Chromium：

```sh
# 在 ui/ 下运行；路径替换为当前仓库绝对路径
PLAYWRIGHT_BROWSERS_PATH=<仓库根>/build/cache/playwright node node_modules/playwright/cli.js install chromium --only-shell
```

Windows 的统一入口通过 Node 直接调用 npm CLI，缓存写入 `build/cache/`，临时文件写入 `build/tmp/`。CMake 可用 `JUSTLOCATION_CMAKE` 指定；正式构建固定获取上游依赖，不读取 `.deepseek/references/`。

## 设备验收

```sh
node build.mjs test:device <serial>       # 独立进程：后台、ART、DEX 引导
node build.mjs test:checks <serial>       # 安装自检 APK，配置模拟并验收 Android 输出
node build.mjs test:transport <serial>    # ARM64/root：独立子进程验证 socket 交接
node build.mjs test:transport-io <serial> # ARM64/x86_64：传输与延迟请求
```

设备测试需要明确串号。整体验收会安装自检 APK 并更改模拟配置；`test:device` 的 bridge 模式有已知加载器问题，不能把它当作已通过的回归。编译和主机测试不能替代目标 ROM 上的 Hook 与实际输出验收。

本地模拟器工具为 `tool/avd.ps1 start|status|stop` 和 `tool/avd-console.mjs`。已有 x86_64 AVD 可测试 App 和部分 Android 接口，未安装 KernelSU/Zygisk，不能运行完整 ARM64 模块。

## App 与分发

- 摇杆包名 `me.idk.justlocation.joystick`，显示名 **JustLoystick**。APK 放入模块的 `bin/joystick.apk`，安装和卸载由模块脚本负责。它没有启动器图标，使用 `am start -n me.idk.justlocation.joystick/.CallActivity --ef speed <m/s>` 唤起透明 Activity，再启动前台服务。root 权限需在 KernelSU 管理器中授予。
- 自检包名 `me.idk.justlocation.probe`，产物为 `dist/justlocation-probe-debug.apk`，不进入模块 ZIP。`integration/device/run-checks.mjs` 通过公开 Android 接口验收各通道。
- `packaging/module/` 仅保存分发输入。打包脚本在 `build/module/` 组装，生成安装期 `checksums` 清单以及运行期 `.sha256` 摘要文件，再输出 ZIP。构建脚本、测试和自检 APK 不进入模块包。

## 代码风格

使用 UTF-8、LF、文件末尾换行与空格缩进；批处理文件使用 CRLF。JavaScript/TypeScript、JSON 和样式文件使用 2 空格，C++、Java/Kotlin 与 Rust 使用 4 空格，具体由 `.editorconfig` 约定。

各语言保留常用命名习惯，模块间统一职责表达和状态字段。注释使用中文解释原因与约束；日志、错误消息、命令输出使用英文；界面标签保持中文，真实数据值不翻译。模块内部的格式化与命名细节随各模块整理落实。

## 上游与许可

Zygisk 入口基于 [zygisk-module-template](https://github.com/5ec1cff/zygisk-module-template)，`zygisk/include/zygisk.hpp` 保留原许可与版权声明。原生构建固定 [LSPlant](https://github.com/LSPosed/LSPlant) 的 `b2f9279014091c83faac094c5fc7c5498261ace8` 与 [ShadowHook 2.0.1](https://github.com/bytedance/android-inline-hook/tree/v2.0.1)。

`zygisk/cmake/shadowhook-arm64.cmake` 从上游生成保留 X17 的编译副本，上游源码保持原样；升级依赖时需核对补丁。分发包保留所带依赖的许可。前端采用官方 [KernelSU JavaScript SDK](https://github.com/tiann/KernelSU/tree/main/js)，其源码与资源保留在 `ui/`。

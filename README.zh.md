# JustLocation

[English](README.md) | 中文

用于 Android 定位模拟的 Zygisk 模块，支持路线回放和悬浮摇杆。

## CLI

安装模块并重启后，在手机终端或 `adb shell` 中使用。CLI 是模块目录内的 `justlocationd`，未加入 `PATH`。先进入 Root shell，再设置快捷别名：

```sh
su
alias justlocation='/data/adb/modules/justlocation/bin/justlocationd'
justlocation --help
```

别名仅对当前终端会话有效。常用命令：

```sh
justlocation start --lat 39.907333 --lon 116.391083 --all  # 为全部应用开启模拟
justlocation status                                    # 查看当前状态
justlocation joystick open                             # 打开悬浮摇杆
justlocation joystick close                            # 关闭摇杆
justlocation stop                                      # 停止模拟
justlocation route --help                              # 查看路线命令
```

坐标默认为 WGS84。将 `--all` 换成 `--app 包名` 可指定生效应用。摇杆需要在 KernelSU → 超级用户中给 JustLoystick 开启 Root 授权。其他参数通过 `命令 --help` 查看。

地图 Key、自动路线规划与候选选择：[地图服务用法](docs/maps.md)。

悬浮控件、方向锁定和朝向跟随：[摇杆用法](docs/joystick.md)。

## 构建

- Node.js 22.12+
- Rust 1.89+（通过 rustup 安装）
- JDK 21
- Android SDK（Platform 36）— 设置 `ANDROID_HOME`，或在 `tool/gradle/local.properties` 中配置 `sdk.dir`

工具链版本：[project-config.json](project-config.json)。

```sh
git clone https://github.com/XiaoNetwork-Astral/JustLocation.git
cd JustLocation
node build.mjs setup
node build.mjs build
```

模块 ZIP 和 APK 输出到 `dist/`。摇杆随模块打包，自检 App 单独提供。

## 开发

```sh
node build.mjs test         # Rust、TypeScript 和 React 测试
node build.mjs test:android # Android JVM 测试
node build.mjs help         # 全部构建与测试命令
```

## 致谢

基于以下开源项目：

- [zygisk-module-template](https://github.com/5ec1cff/zygisk-module-template)
- [LSPlant](https://github.com/LSPosed/LSPlant)
- [ShadowHook](https://github.com/bytedance/android-inline-hook)
- [KernelSU JavaScript SDK](https://github.com/tiann/KernelSU/tree/main/js)
- [coordtransform](https://github.com/wandergis/coordtransform)
- [leaflet.ChineseCRS](https://github.com/gumblex/leaflet.ChineseCRS)

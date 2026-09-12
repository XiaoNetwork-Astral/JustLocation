# JustLocation

[English](README.md) | 中文

用于 Android 定位模拟的 Zygisk 模块，支持路线回放和悬浮摇杆。

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

## 地点 S 码

后端可以独立完成 S 码互通，不需要 UI 或正在运行的服务。在已将 `justlocationd` 加入 PATH 的终端中运行以下命令；Android 上可执行文件位于 `/data/adb/modules/justlocation/bin/`：

```sh
justlocationd scode encode < address.json > location.scode
justlocationd scode decode < location.scode > original-address.json
justlocationd scode import --without-wifi < location.scode > imported-address.json
```

地址 JSON 包含 `latitude`、`longitude`、可选的 `altitude`，以及名称、地址等元数据和 `nearbyCells` / `nearbyWifis` 附件。解码保留原地址；导入生成新 ID 并设为 `from=2`。导入默认保留两种附件，可分别通过 `--without-cells`、`--without-wifi` 去掉。操作只输出文件内容，不改变模拟状态。输入和解压后 JSON 均限 2 MiB；详情见 `scode --help`。

## 运动现实感

停止模拟后，可通过 `set_realism` 设置参数，省略的字段使用默认值。总开关默认关闭；启用后，静态、路线与摇杆输出共用连续变化的随机源，保存的坐标保持不变。默认漂移半径 2 米、高度 ±1 米、方向 ±3°、速度 ±10%、每 5 秒过渡、拐角最多提前 5 米圆滑转向。可选 `seed` 用于复现同一段模拟。

字段为 `enabled`、`drift_radius_m`（0–100）、`altitude_m`（0–100）、`bearing_degrees`（0–45）、`speed_variation`（0–0.5）、`period_seconds`（1–60）、`corner_radius_m`（0–100）和 `seed`。速度浮动会影响实际路程；路线暂停和重复等待按真实时间处理，静态漂移不计步。原始路线点和端点与用于播放的平滑路径分开保留。

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

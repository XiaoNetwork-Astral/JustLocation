# JustLocation

[English](README.md) | 中文

用于 Android 定位模拟的 Zygisk 模块，支持路线回放和悬浮摇杆。

## 构建

需要 Node.js、Rust 1.89+（通过 rustup 安装）、JDK 21 和 Android SDK。设置 `ANDROID_HOME`，或在 `tool/gradle/local.properties` 中配置 `sdk.dir`。工具链版本见 [project-config.json](project-config.json)。

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

基于 [zygisk-module-template](https://github.com/5ec1cff/zygisk-module-template)，使用 [LSPlant](https://github.com/LSPosed/LSPlant)、[ShadowHook](https://github.com/bytedance/android-inline-hook) 和 [KernelSU JavaScript SDK](https://github.com/tiann/KernelSU/tree/main/js)。依赖的许可文件随模块分发。

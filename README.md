# JustLocation

English | [中文](README.zh.md)

A Zygisk module for Android location simulation, with route playback and a floating joystick.

## Build

- Node.js
- Rust 1.89+ with rustup
- JDK 21
- Android SDK — set `ANDROID_HOME` or `sdk.dir` in `tool/gradle/local.properties`

Toolchain versions: [project-config.json](project-config.json).

```sh
git clone https://github.com/XiaoNetwork-Astral/JustLocation.git
cd JustLocation
node build.mjs setup
node build.mjs build
```

The module ZIP and APKs are written to `dist/`. The joystick is bundled with the module; the diagnostic app is packaged separately.

## Development

```sh
node build.mjs test         # Rust, TypeScript and React tests
node build.mjs test:android # Android JVM tests
node build.mjs help         # All build and test commands
```

## Credits

Built with the following projects:

- [zygisk-module-template](https://github.com/5ec1cff/zygisk-module-template)
- [LSPlant](https://github.com/LSPosed/LSPlant)
- [ShadowHook](https://github.com/bytedance/android-inline-hook)
- [KernelSU JavaScript SDK](https://github.com/tiann/KernelSU/tree/main/js)

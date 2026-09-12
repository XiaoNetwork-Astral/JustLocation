# JustLocation

English | [中文](README.zh.md)

A Zygisk module for Android location simulation, with route playback and a floating joystick.

## Build

- Node.js 22.12+
- Rust 1.89+ with rustup
- JDK 21
- Android SDK (Platform 36) — set `ANDROID_HOME` or `sdk.dir` in `tool/gradle/local.properties`

Toolchain versions: [project-config.json](project-config.json).

```sh
git clone https://github.com/XiaoNetwork-Astral/JustLocation.git
cd JustLocation
node build.mjs setup
node build.mjs build
```

The module ZIP and APKs are written to `dist/`. The joystick is bundled with the module; the diagnostic app is packaged separately.

## Location S codes

The backend can exchange location S codes without the UI or a running service. Run these in a shell with the built `justlocationd` on PATH (on Android it is in `/data/adb/modules/justlocation/bin/`):

```sh
justlocationd scode encode < address.json > location.scode
justlocationd scode decode < location.scode > original-address.json
justlocationd scode import --without-wifi < location.scode > imported-address.json
```

Address JSON contains `latitude`, `longitude`, optional `altitude`, and any address metadata or `nearbyCells` / `nearbyWifis` attachments. Decode preserves the address; import creates a new ID and sets `from=2`. Import keeps both attachment types unless `--without-cells` or `--without-wifi` is supplied. Neither operation changes simulation state. Input and expanded JSON are limited to 2 MiB. Use `scode --help` for details.

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
- [coordtransform](https://github.com/wandergis/coordtransform)
- [leaflet.ChineseCRS](https://github.com/gumblex/leaflet.ChineseCRS)

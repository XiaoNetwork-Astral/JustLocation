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

## Motion realism

`set_realism` accepts an optional configuration (omitted fields use defaults). Stop simulation before changing it. It defaults to disabled; enabling it applies the same continuous noise to static, route and joystick output. Saved coordinates remain unchanged. Defaults: 2 m drift radius, ±1 m altitude, ±3° bearing, ±10% speed, 5 s transitions, and up to 5 m corner cuts. `seed` optionally makes a session reproducible.

The fields are `enabled`, `drift_radius_m` (0–100), `altitude_m` (0–100), `bearing_degrees` (0–45), `speed_variation` (0–0.5), `period_seconds` (1–60), `corner_radius_m` (0–100), and `seed`. Speed variation changes travelled distance; route pauses and repeat waits use real time. Static drift does not count as steps. Original route points and endpoints are retained separately from the smoothed playback path.

## Map provider keys

Amap, Tencent and Baidu place search use separate WebService keys. The `maps` endpoint accepts version 1 requests with `settings`, `configure_key` (`provider`, `key`; an empty key removes it) and `search` (`provider`, `query`, `region`). Provider IDs are `amap`, `tencent` and `baidu`. Keys are kept in a private `map-keys.json` file and are never returned in settings or included in S codes. Missing keys return the provider's configuration link. Search results use WGS84 coordinates; these keys are for search services, not the current public tile URLs.

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

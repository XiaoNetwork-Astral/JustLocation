# JustLocation

English | [中文](README.zh.md)

A Zygisk module for Android location simulation, with route playback and a floating joystick.

## CLI

After installing the module and rebooting, open a terminal on the phone (or `adb shell`). The CLI is `justlocationd` inside the module directory; it is not on `PATH`. Enter a root shell and create a shortcut:

```sh
su
alias justlocation='/data/adb/modules/justlocation/bin/justlocationd'
justlocation --help
```

The alias lasts for the current shell session. Common commands:

```sh
justlocation start --lat 39.907333 --lon 116.391083 --all  # Start for all apps
justlocation status                                    # Show current state
justlocation joystick open                             # Open the floating joystick
justlocation joystick close                            # Close the joystick
justlocation stop                                      # Stop simulation
justlocation route --help                              # Show route commands
```

Coordinates default to WGS84. Replace `--all` with `--app PACKAGE` to select an app. The joystick needs Root access in KernelSU → Superuser → JustLoystick. Use `COMMAND --help` for more options.

Map keys, route planning and candidate selection: [map service guide](docs/maps.md).

Floating controls, direction lock and compass following: [joystick guide](docs/joystick.md).

Separate position, route, Wi-Fi and SIM app lists: [application scopes](docs/scopes.md).

Subscription metadata for empty SIM slots: [virtual subscriptions](docs/virtual-sim.md).

Portable data, category selection and restore previews: [backups](docs/backups.md).

Measured places with their cells and Wi-Fi, and the satellite field on simulated fixes: [places](docs/places.md).

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

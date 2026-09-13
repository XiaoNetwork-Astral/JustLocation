# Application scopes

Position, route playback, Wi-Fi and SIM operator simulation each retain their own application list.

```sh
justlocationd scope set --feature position --app com.example.map
justlocationd scope set --feature route --app com.example.runner
justlocationd scope set --feature wifi --app com.example.wifi
justlocationd scope set --feature sim --app com.example.phone
justlocationd scope get --feature route
```

Repeat `--app` to select multiple packages. `--all` explicitly selects every eligible caller. Package lists apply to that package in any Android user/profile where the system permits access; they do not grant permissions or cross-profile access.

| Output                                                      | Scope                           |
| ----------------------------------------------------------- | ------------------------------- |
| Static position, saved place, joystick                      | `position`                      |
| Route playback, including pause and the held destination    | `route`                         |
| GNSS, NMEA, cell identity and radio signal                  | Current position or route scope |
| Step sensors                                                | Current position or route scope |
| Wi-Fi connection and scans                                  | `wifi`                          |
| Subscription operator fields and ServiceState carrier names | `sim`                           |

Each channel also requires its own enabled setting and an active simulation session. Stopping ends every simulated channel. Wi-Fi and SIM lists remain independent when the position source changes. A ServiceState result can replace its cell/radio fields and its carrier names separately; fields outside the caller's scopes retain the authorized system values.

`start` and `place start` use the saved position scope; `route start` uses the saved route scope. An explicit `--app` or `--all` on those commands changes only that source's saved scope. The first start still requires an explicit application choice if no position configuration has been saved. Stopping a route restores the position scope for the next static session.

Changes apply to existing listeners on the next heartbeat, without requiring re-registration. Getter calls and callbacks use the same selected package and 20-second heartbeat expiry. Android validates the package against the calling UID during queries and registration; listener delivery uses the framework's recorded identity and existing permission/AppOps filtering. Missing or blank callers receive system output, even with `--all`. Explicit stop is processed on the next heartbeat; loss of the daemon restores system output once the last valid snapshot expires. This does not clear caches inside other applications.

Wi-Fi replacement also reuses the service's package/UID, location, scan AppOps and user/profile checks after the original call. Soft denial, null output and protected MAC/network fields retain their system restrictions.

## Compatibility and operator property limits

Stored configuration version 4 contains `scopes.position`, `scopes.route`, `scopes.wifi` and `scopes.sim`. Unversioned and version 2/3 configurations copy the old shared list into all four. An invalid or incomplete version 4 selection is rejected. The control protocol remains version 1: `state.config.scope` is the effective position/route scope, and `state.scopes` contains the four saved selections.

The legacy `scope set --app ...` command, or `set_scope` without `feature`, changes all four lists together. Bare `scope get` reads the effective position scope. New clients should include `feature` when editing one list. The CLI refuses feature edits against a daemon that does not report separate scopes; update the daemon and bridge together. Legacy bridges read the effective shared scope and cannot enforce the separate Wi-Fi/SIM selections.

Android's `TelephonyManager.getNetworkOperatorName()` and `getSimOperatorName()` read shared operator properties. In the current system/phone bridge architecture those property values cannot differ by caller. With `sim` set to an application list, the daemon restores the real global properties; scoped subscription and ServiceState results still receive configured operator fields. Only `scope set --feature sim --all` permits global operator-property replacement. Consequently, a property-based operator-name query can remain real inside a selected application. See the [Android 15 TelephonyManager implementation](https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/android-15.0.0_r1/telephony/java/android/telephony/TelephonyManager.java).

## Validation

Rust tests cover migration, independent edits, source switching, restart, failed-save rollback and global property restoration. JVM tests cover shared parsing, invalid selections, existing listener changes, expiry and Wi-Fi permission/redaction decisions. The connected API 35 phone has passed the focused ServiceState/scope object check; this does not install hooks or prove cross-application output.

`integration/device/run-scopes.mjs ADB SERIAL ARM64_BINARY` runs a temporary daemon in its own private directory to check CLI behavior and persistence. It does not feed the installed module's hooks. Full position/route/Wi-Fi cross-application validation requires installing the matching module and remains a separate device check.

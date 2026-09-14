# Places and their environment

A place stores one position together with the cells and Wi-Fi networks measured at that position. Starting a place can then switch position and environment together, so location, cell and Wi-Fi simulation describe the same real spot.

Environment data is measured by the probe app, not produced by the module. Nothing in this flow invents a tower identity, a tower coordinate or a Wi-Fi network: what the platform reported is what gets stored.

## Collect a real place

```sh
justlocation place collect 'Home'
```

The example uses the `justlocation` alias from the [README](../README.md). `place collect` asks the probe app `me.idk.justlocation.probe` for one measurement, stores it as a new place and does **not** start the simulation. Stop the simulation before collecting; a snapshot taken while the simulation is active is rejected instead of being stored as a real place.

The probe collects one fresh fix plus the radio environment read at the same moment:

- location: coordinates, altitude, accuracy, speed, bearing, provider, platform and monotonic timestamps, whether the fix came from a cache, and `Location.extras` satellite count when the platform supplies it;
- cells: radio type, MCC/MNC, area and identity as the platform reported them, registration state and signal level;
- Wi-Fi: SSID, BSSID, RSSI, frequency and the platform's scan timestamp;
- capture metadata: package, permission results and whether location was enabled.

A mock fix is never stored: the probe drops a fix that reports itself as mock and records the situation as `simulated`, which the backend refuses.

To store a snapshot that already exists, pass a file and skip the on-device capture:

```sh
justlocation place collect 'Imported measurement' --input snapshot.json --without-wifi
```

`--without-cells` and `--without-wifi` omit an attachment. Collected places store their attachment arrays even when they are empty, because "measured and found none" differs from "not measured". The saved place carries a `collected` object with the capture time, coordinate system, source, attachment counts and the fix's satellite extra, so a later reader can tell a measured value from an assumed one.

## Start a place

```sh
justlocation place start PLACE_ID --cells apply --wifi apply --app com.example.target
justlocation place start PLACE_ID --cells clear
```

Default is `keep` for both attachments, which starts the position and leaves the current environment untouched. `apply` uses the saved attachment, `clear` explicitly empties that channel. An attachment that is present but empty is treated as `keep`, so a place collected without cells never silently turns cell simulation off; clearing requires the explicit flag.

The switch validates the whole request before it changes anything: attachments are prepared and validated first, then the engine starts and the session (position, scope, cells, Wi-Fi) is replaced as one step, then the result is persisted. A rejected attachment or a failed save leaves the previous session in place.

Applying cells needs tower coordinates. A `CellRegion` is geographic: the module answers cell queries from the cells inside it, and `--cells apply` therefore reports `cell attachment is missing tower coordinate lat` for a measurement that carries only an identity. Coordinate-bearing attachments still apply normally:

- S-code attachments from the original app carry explicit tower coordinates;
- legacy cell objects with `lat`/`lon` keep working, including `mcc`/`mnc`/`lac`/`cellid` with `radio_type`;
- a measured cell becomes applicable after a dataset lookup or a manual edit adds `lat`/`lon`.

Until such a coordinate exists, the cell is stored as a record of what was heard and cannot configure cell simulation. This is reported as an error rather than filled in with the simulated device position.

Wi-Fi attachments carry everything the Wi-Fi channel needs, so a measured Wi-Fi list applies without extra data.

## Boundaries

- Collection reads real device state. It requires the probe app, an unlocked device, location enabled and the location permission; each of these is reported in the snapshot when it is missing.
- The measurement happens once per command. Repeated collection creates a new place; it never edits an earlier one.
- Reads are the platform's; no root-only identity, subscriber identifier or hardware identifier is collected.
- A cell region describes the area that was acquired, not complete real-world coverage, and a recorded cell identity is not proof that a tower was measured from the simulated device.
- Applying an environment changes the simulation switches (`cells_enabled`, Wi-Fi `enabled`). It does not touch the application scope of those channels; a place start sets the position scope only.

## What a measured location carries

The probe records the fields a consumer actually reads from a fix, so a stored place can be compared with what an application saw:

| Field | Recorded as |
| ----- | ----------- |
| position and quality | coordinates, altitude, accuracy, speed, bearing, provider |
| time | `fix_time_ms` (wall clock), `fix_elapsed_ms` (monotonic) and the receiving time |
| freshness | `from_last_known` distinguishes a requested fix from a cached one |
| mock flag | `mock`, plus the snapshot-wide `simulated` flag |
| satellites | `extras_satellites`, the platform's `Location.extras` count, or `-1` when the fix carried none |
| permissions | location, Wi-Fi and phone permission results, and whether location was enabled |

A count of `-1` is an absent field, not a zero: it means the fix did not carry `extras.satellites` at all. That distinction matters because a consumer can reject a fix whose satellite extra is missing or zero.

## Satellites in simulated fixes

A location simulated for a GPS request states how many satellites solved it, through the same
`Location.extras` field a real fix uses. The count and the satellite list are the same epoch, so
these always agree with each other:

- `Location.extras.getInt("satellites")` on a simulated GPS fix;
- `GnssStatus.getSatelliteCount()` for a listening app;
- the used-satellite count in the spoken NMEA sentences (`GPGGA` and `GPGSV`).

A network, fused or passive fix is not a satellite solution and carries no satellite extra, and
neither does any fix while the satellite output is switched off. The field is absent rather than
zero in those cases, matching how the platform reports no satellites: a consumer that rejects a
missing or zero count is not being told something the position does not support.

## Validation

Host tests cover snapshot acceptance and rejection (simulated, cached, non-WGS84, over-sized), attachment storage, empty-versus-missing attachments, legacy and measured cell conversion, per-attachment modes and the refusal of an identity-only cell. Bridge tests cover which fixes carry a satellite count and that the NMEA counts follow the modeled epoch instead of a fixed number.

`integration/device/run-environment.mjs ADB SERIAL ARM64_BINARY` collects once on the device, stores the place through an isolated daemon, checks the stored attachments and metadata, verifies that an identity-only attachment is refused, and removes the test place. It does not modify the installed module's configuration.

`integration/device/run-location-continuity.mjs` also asserts, while simulating, that delivered GPS fixes carry a positive satellite count and that network fixes never claim one. That check needs the matching module installed.

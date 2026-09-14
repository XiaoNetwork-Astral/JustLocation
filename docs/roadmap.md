# Remaining verification

This lists the work that is implemented but not yet verified, with the exact command for each step.
It is a checklist, not a substitute for the project memo: the memo holds the reasoning and the
current decisions, this file holds what to run.

Every device step needs the phone connected, the module installed and the probe app present. Device
writes (installing the module, rebooting, changing settings) need an explicit go-ahead each time.

## Phase 9.3 — SDK source matrix

Implemented: `integration/device/run-sdk-matrix.mjs`, resumable per configuration.

```sh
node integration/device/run-sdk-matrix.mjs ADB SERIAL build/tmp/sdk-matrix-full.json 180
node integration/device/run-sdk-matrix.mjs ADB SERIAL build/tmp/sdk-matrix-gps.json 180 --modes gps
node integration/device/run-sdk-matrix.mjs ADB SERIAL build/tmp/sdk-matrix-nocache.json 180 --caches false
```

Each configuration runs one location mode with and without the SDK cache for the requested duration,
inside a single simulation session. Results are written after every configuration, so an interrupted
run keeps what it finished, and a finished configuration clears its capture from the device.

Still missing: the paired comparison against AMap 6.4.9. The probe links 11.2.100 only, so the older
SDK has to be supplied as an artifact before that comparison exists.

## Phase 9.5 — raw motion sensors

Implemented: the six-axis model, the `motion_sensors` switch, the bridge pass-through and native
event synthesis. Verified on the host: the model's physics and its coupling to the step cadence.

```sh
# Enable the channel on the device, then subscribe from an ordinary app.
justlocation steps set --enabled true --motion-sensors true --cadence 2 --movement-linked true --stride-m 0.75
justlocation steps get
```

What has to be checked on a device:

- an ordinary app subscribing to `TYPE_ACCELEROMETER` and `TYPE_GYROSCOPE` receives samples while
  simulation runs, with gravity and a gait that matches the step count it also receives;
- a standstill reports gravity and no rotation;
- apps outside the scope keep receiving the real sensors, and the scope the channel follows is the
  delivered one — the position list for a static session and the route list while a route plays, not
  a list of its own;
- unregistering mid-run and losing the bridge both restore real output;
- two apps at different sampling rates see the same motion, not two independent signals.

Not yet verified anywhere: the native synthesis itself. Its evidence so far is a successful ARM64
build and code review, which is not acceptance.

## Phase 9.6 — cross-channel report

Implemented: `integration/replay-report.mjs` and its self-test.

```sh
node integration/replay-report.test.mjs
node integration/replay-report.mjs build/tmp/replay.json \
  system:device-run:build/tmp/location-continuity-sampled3.json \
  sdk:device-run:build/tmp/capture.json
```

Still missing from the acceptance wording: a run of at least ten minutes and an intermittent
subscription (subscribe, unsubscribe, resubscribe) recorded as a capture, plus the same report over
two different ordinary applications.

## Cross-phase

- The reinstall that carries these changes has not happened: the device holds an earlier build, so
  every device step above also needs a module install and reboot first.
- The location fallback that started the diagnostic work is still unexplained. Phase 9.1 and 9.2
  removed two candidate causes by measurement; nothing so far identifies the target application's
  own path.

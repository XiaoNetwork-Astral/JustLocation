# Captures and offline replay

Location diagnosis produces captures on the device and turns them into one timeline on the host. A
report states what the captures contain, so a finding can be re-checked later without the phone.

## Captures

| File | Written by | Contains |
| ---- | ---------- | -------- |
| `location-continuity.jsonl` | probe app | one row per provider callback or last-known read: provider, coordinates, accuracy, fix time, fix age, mock flag, `extras.satellites` |
| `amap-continuity.jsonl` | probe app | one row per SDK result: mode, cache choice, SDK version, consumer identity, error code, location type, provider, coordinates, accuracy |
| `location-transitions-*.json` | device script | the same rows plus the backend's target positions on the same monotonic clock |
| `sdk-matrix.json` | device script | one summary per SDK mode and cache choice |

A capture is only as good as its labels: a `collection_start` row states when the app began, which is
what separates a slow app start from late delivery.

## Offline replay

```sh
node integration/replay-report.mjs build/tmp/replay.json \
  system:device-run:build/tmp/location-continuity-sampled3.json \
  sdk:device-run:build/tmp/sdk-matrix.json
```

Arguments are `NAME:LABEL:PATH`, and the label must be one of `real-capture`, `synthetic-fixture` or
`device-run`. A capture whose kind is not declared is refused rather than guessed, so a report cannot
present a fixture as a device measurement. `.jsonl` files are read row by row; a JSON report is read
from its `callbacks` or `observations` list.

The report contains, per capture:

- how many rows were fixes, and how many carried a non-zero SDK error code;
- the providers and SDK location types that appeared;
- gaps longer than ten seconds between consecutive fixes of that capture;
- the fix-age range and how many ages were negative, which is what a wrong time base looks like;
- the satellite values that were seen, including the `-1` that means the field was absent.

It then aligns every capture on one monotonic timeline and reports the worst distance between two
captures for pairs whose timestamps are within two seconds of each other. That is the number to look
at when the system providers look right but an SDK reports a different place: a pair divergence
points at the source that took a different path, while a matching pair rules the position out.

Runs are self-tested: `node integration/replay-report.test.mjs` builds a fixture with a known gap, a
known divergent capture and one error row, and checks that the report describes exactly those.

## Boundaries

- The report is a description of captures, not a verdict. A gap in one capture is evidence about
  that consumer, not about the module, until another capture at the same instant is compared.
- Comparison pairs are matched by timestamp window, so a capture at a different rate has fewer
  pairs; `pairs` is reported so a comparison over too few samples is visible.
- Fix age comes from the wall clock an SDK reports, because AMap leaves
  `Location.elapsedRealtimeNanos` at zero; mixing the two clocks would produce meaningless ages.
- Captures hold precise positions. Treat them as private test material: do not commit them, and
  strip them before sharing a report.

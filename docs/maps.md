# Map services and planned routes

Base-map selection and WebService providers are independent. A tile layer does not configure a service key. An Android SDK key is not a WebService key. Keys are stored in the private data directory and are excluded from library backups.

## CLI

Use the root-shell alias from the README. Read a provider key from a private file or stdin:

```sh
justlocation maps key amap --input /private/amap-webservice-key.txt
justlocation maps capabilities
justlocation maps plan --input plan.json --output candidates.json
justlocation route candidates --input candidates.json
justlocation route select "Walk" --input candidates.json --candidate 0
justlocation route select "Walk and start" --input candidates.json --candidate 1 --start --all
```

Example `plan.json` (WGS84; playback speed in metres/second):

```json
{
  "provider": "amap",
  "mode": "walking",
  "origin": { "latitude": 39.907333, "longitude": 116.391083 },
  "destination": { "latitude": 39.912333, "longitude": 116.401083 },
  "waypoints": [],
  "speed": 1.4
}
```

Providers: `amap`, `tencent`, `baidu`. Modes: `walking`, `cycling`, `driving`. The result contains complete WGS84 candidates, distance in metres, estimated duration in seconds and available step metadata. Candidate indices start at zero. Use `route start --id ID` to play a saved route. JSON export/import preserves provider geometry metadata; GPX contains points and segments, not provider navigation metadata.

For an existing manual or GPX route, `route replan ID amap driving --via 10,40 --output candidates.json` requests a new route through the original endpoints and the selected interior point indices. Without `--via`, only endpoints are used. It never overwrites the original or automatically chooses a candidate. This is waypoint planning, not track matching; original GPX gaps are not routing constraints.

## Capabilities and limits

| Service                | Walking / cycling / driving | Via points                                                     | Coordinates at API boundary              |
| ---------------------- | --------------------------- | -------------------------------------------------------------- | ---------------------------------------- |
| AMap v5                | All three                   | Driving: up to 16; default account permission may allow only 1 | GCJ02, longitude first                   |
| Tencent v1             | All three                   | Driving: up to 30                                              | GCJ02, latitude first; compressed output |
| Baidu DirectionLite v1 | All three                   | Driving: up to 10                                              | WGS84 input, explicit GCJ02 output       |

These adapters target the domestic services. Baidu DirectionLite documents mainland China coverage. Overseas services, when available, require their own endpoints and permissions and are not implied by these adapters. Alternative counts depend on the service and the requested endpoints; a successful request need not produce multiple candidates. Unreachable, incomplete and unsupported requests return errors, without a straight-line substitute.

Routes retain all supplied geometry points (only a shared step endpoint is coalesced), up to 100,000 points per candidate. Disconnected step geometry is rejected. Saved plans use the existing route-file mechanism. Planning responses and candidate files have a 64 MiB transport/input budget; control frames remain unchanged.

Provider plans disable corner smoothing and horizontal realism drift. Speed variation can still change time along the path. Road/navigation codes are preserved when supplied; they do not establish road width, floor level, bridge clearance, building footprints or walkable polygons. Building avoidance, arbitrary track matching and level-aware smoothing are currently unsupported. Adding them requires licensed, georeferenced topology and obstacle data, plus a matching/constrained-path implementation.

## Provider setup and references

Checked against official documentation on 2026-09-13. API access, daily quota, QPS and paid permissions vary by account and product. Configure and inspect the intended WebService product in each console; the CLI does not purchase quota or retry into another provider.

- AMap: [route v5](https://developer.amap.com/api/webservice/guide/api/newroute), [key setup](https://console.amap.com/dev/key/app), [quota](https://developer.amap.com/api/webservice/guide/tools/flowlevel).
- Tencent: [official route adapter reference](https://github.com/TencentLBS/tencentmap-webservice-skill/blob/main/references/api-direction.md), [key and account settings](https://lbs.qq.com/dev/console/key/manage).
- Baidu: [coverage](https://lbsyun.baidu.com/docs/webapi?title=directionlite/guide/webservice-lwrouteplanapi), [driving](https://lbsyun.baidu.com/docs/webapi?title=directionlite/guide/webservice-lwrouteplanapi/dirve), [walking](https://lbsyun.baidu.com/docs/webapi?title=directionlite/guide/webservice-lwrouteplanapi/walk), [cycling](https://lbsyun.baidu.com/docs/webapi?title=directionlite/guide/webservice-lwrouteplanapi/cycling), [quota and pricing](https://lbsyun.baidu.com/cashier/quota).

Host tests use synthetic response fixtures to exercise all modes, unit/coordinate conversion, failures, alternatives, storage and path-preserving playback. They are not live-service validation. A configured WebService key and reachable provider are required for real-service acceptance.

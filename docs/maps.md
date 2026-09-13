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

Providers: `amap`, `tencent`, `baidu`, `google`. Modes: `walking`, `cycling`, `driving`. The result contains complete WGS84 candidates, distance in metres, estimated duration in seconds and available step metadata. Candidate indices start at zero. Use `route start --id ID` to play a saved route. JSON export/import preserves provider geometry metadata; GPX contains points and segments, not provider navigation metadata.

For an existing manual or GPX route, `route replan ID amap driving --via 10,40 --output candidates.json` requests a new route through the original endpoints and the selected interior point indices. Without `--via`, only endpoints are used. It never overwrites the original or automatically chooses a candidate. This is waypoint planning, not track matching; original GPX gaps are not routing constraints.

## Capabilities and limits

| Service                | Walking / cycling / driving             | Via points                                                           | Coordinates at API boundary              |
| ---------------------- | --------------------------------------- | -------------------------------------------------------------------- | ---------------------------------------- |
| AMap v5                | All three                               | Driving: up to 16; default account permission may allow only 1       | GCJ02, longitude first                   |
| Tencent v1             | All three                               | Driving: up to 30                                                    | GCJ02, latitude first; compressed output |
| Baidu DirectionLite v1 | All three                               | Driving: up to 10                                                    | WGS84 input, explicit GCJ02 output       |
| Google Routes v2       | All three, subject to regional coverage | Up to 25 in all three modes; alternatives only without intermediates | WGS84 GeoJSON, longitude first           |

The AMap, Tencent and Baidu route adapters target the domestic services. Baidu DirectionLite documents mainland China coverage. Overseas services, when available, require their own endpoints and permissions and are not implied by these adapters. Alternative counts depend on the service and the requested endpoints; a successful request need not produce multiple candidates. Unreachable, incomplete and unsupported requests return errors, without a straight-line substitute.

Routes retain all supplied geometry points (only a shared step endpoint is coalesced), up to 100,000 points per candidate. Disconnected step geometry is rejected. Saved plans use the existing route-file mechanism. Planning responses and candidate files have a 64 MiB transport/input budget; control frames remain unchanged.

Provider plans disable corner smoothing and horizontal realism drift. Speed variation can still change time along the path. Road/navigation codes are preserved when supplied; they do not establish road width, floor level, bridge clearance, building footprints or walkable polygons. Building avoidance, arbitrary track matching and level-aware smoothing are currently unsupported. Adding them requires licensed, georeferenced topology and obstacle data, plus a matching/constrained-path implementation.

## Provider setup and references

Checked against official documentation on 2026-09-13. API access, daily quota, QPS and paid permissions vary by account and product. Configure and inspect the intended WebService product in each console; the CLI does not purchase quota or retry into another provider.

- AMap: [route v5](https://developer.amap.com/api/webservice/guide/api/newroute), [key setup](https://console.amap.com/dev/key/app), [quota](https://developer.amap.com/api/webservice/guide/tools/flowlevel).
- Tencent: [official route adapter reference](https://github.com/TencentLBS/tencentmap-webservice-skill/blob/main/references/api-direction.md), [key and account settings](https://lbs.qq.com/dev/console/key/manage).
- Baidu: [coverage](https://lbsyun.baidu.com/docs/webapi?title=directionlite/guide/webservice-lwrouteplanapi), [driving](https://lbsyun.baidu.com/docs/webapi?title=directionlite/guide/webservice-lwrouteplanapi/dirve), [walking](https://lbsyun.baidu.com/docs/webapi?title=directionlite/guide/webservice-lwrouteplanapi/walk), [cycling](https://lbsyun.baidu.com/docs/webapi?title=directionlite/guide/webservice-lwrouteplanapi/cycling), [quota and pricing](https://lbsyun.baidu.com/cashier/quota).

Host tests use synthetic response fixtures to exercise all modes, unit/coordinate conversion, failures, alternatives, storage and path-preserving playback. They are not live-service validation. A configured WebService key and reachable provider are required for real-service acceptance.

## Reverse geocoding and Google

```sh
justlocation maps reverse amap --lat 31.2304 --lon 121.4737
justlocation maps key google --input /private/google-webservice-key.txt
justlocation maps search google "opera house" --region Sydney
justlocation maps reverse google --lat -33.8568 --lon 151.2153
justlocation maps link google --lat -33.8568 --lon 151.2153
```

Reverse geocoding returns the original query position, address candidates, address components and any requested nearby POIs. An address candidate position is separate from the query and may be absent (AMap). Google Geocoding returns address candidates, not a nearby-POI search. The Google search region is included in the text query; it is not a hard city boundary. Results and geometry remain WGS84.

Google capabilities are separate:

- External map display uses an official Maps URL and requires no API key. The CLI returns the link; it does not launch another application. An embedded Google map/tile layer is not currently provided. The GUI will use an external-open action, with a separate provider for its embedded base map.
- Place search calls Places API (New) Text Search with explicit fields for ID, name, address, location and attributions. The name/location fields invoke its Pro search SKU; no wildcard fields, photos or reviews are requested.
- Coordinate lookup calls Geocoding API v3. Google attribution and structured components are retained.
- Planning calls Routes API v2 Compute Routes with WALK/BICYCLE/DRIVE, high-quality GeoJSON geometry, fixed waypoint order and provider warnings. It uses the same candidate-selection and geometry-preserving playback flow. Google road matching and building constraints are not connected.

Enable the intended APIs and billing in Google Cloud and configure a WebService key with matching API restrictions. A browser-restricted or Android SDK key is not interchangeable with the server API credential used here. IP restrictions must match the actual outbound address; mobile networks can change that address. The project does not deploy a credential proxy or change key restrictions automatically. Separate API SKUs, monthly usage allowances and quotas apply; configure limits in your own Cloud project. Google services must be reachable from the executing device. Network access and regional coverage are different requirements.

Google coverage varies by feature and country: the published table currently lists walking and driving for China, but not cycling. Treat errors or no routes as unsupported/unavailable for that query; there is no fallback straight line. The GUI must retain Google and third-party attribution, display route warnings, and respect the documented map-display/storage rules for Google content. Data results are not automatically painted over another provider's map.

Official references (checked 2026-09-13):

- [AMap reverse geocoding](https://developer.amap.com/api/webservice/guide/api/georegeo), [Tencent reverse geocoding](https://github.com/TencentLBS/tencentmap-webservice-skill/blob/main/references/api-geocoder.md), [Baidu reverse geocoding](https://lbsyun.baidu.com/docs/webapi?title=reverse_geocoding/guide/webservice-geocoding-abroad-base). Baidu overseas reverse geocoding requires separate permissions; this adapter requests GCJ02 output for domestic results.
- Google: [Text Search](https://developers.google.com/maps/documentation/places/web-service/text-search), [reverse geocoding](https://developers.google.com/maps/documentation/geocoding/guides-v3/requests-reverse-geocoding), [Compute Routes](https://developers.google.com/maps/documentation/routes/reference/rest/v2/TopLevel/computeRoutes), [Maps URLs](https://developers.google.com/maps/documentation/urls/get-started), [coverage](https://developers.google.com/maps/coverage).
- Google: [API key restrictions](https://developers.google.com/maps/faq), [pricing](https://developers.google.com/maps/billing-and-pricing/pricing), [Places usage and billing](https://developers.google.com/maps/documentation/places/web-service/usage-and-billing), [Routes usage and billing](https://developers.google.com/maps/documentation/routes/usage-and-billing), [Geocoding usage and billing](https://developers.google.com/maps/documentation/geocoding/usage-and-billing), [Geocoding attribution/display policy](https://developers.google.com/maps/documentation/geocoding/policies).

Live-key verification is still required for all newly added API operations. Fixtures and a loopback HTTP transport test validate parsing and request construction without charging a provider account.

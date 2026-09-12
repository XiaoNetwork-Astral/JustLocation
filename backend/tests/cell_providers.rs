use justlocation_backend::cell_providers::*;
use justlocation_backend::cells::Coordinate;
use serde_json::json;
use std::collections::VecDeque;

#[derive(Default)]
struct MockHttp {
    requests: Vec<HttpRequest>,
    replies: VecDeque<Result<HttpResponse, QueryError>>,
}
impl Http for MockHttp {
    fn send(&mut self, request: HttpRequest) -> Result<HttpResponse, QueryError> {
        self.requests.push(request);
        self.replies.pop_front().expect("unexpected network request")
    }
}
fn reply(value: serde_json::Value) -> Result<HttpResponse, QueryError> {
    Ok(HttpResponse { status: 200, body: value.to_string() })
}
fn query() -> AreaQuery {
    AreaQuery { target: Coordinate { latitude: 0.0, longitude: 0.0 }, radius_m: 500.0 }
}
fn source() -> Provider {
    Provider::OpenCellId { key: "private&key".into() }
}
fn row(id: u64, radio: &str) -> serde_json::Value {
    json!({"lat":0.001,"lon":0.0,"mcc":460,"mnc":1,"lac":12,"cellid":id,"range":800,"radio":radio})
}

#[test]
fn opencellid_normalizes_radio_ids_without_truncating_nr_or_inventing_signal() {
    let mut http = MockHttp {
        replies: [reply(
            json!({"count":3,"cells":[row(68719476735,"NR"),row(123,"LTE"),row(456,"UMTS")]}),
        )]
        .into(),
        ..Default::default()
    };
    let data = fetch(&mut http, &source(), query(), 1000).unwrap();
    assert_eq!(data.region.cells.len(), 3);
    let value = serde_json::to_value(&data).unwrap();
    assert_eq!(value["region"]["cells"][0]["identity"]["nci"], 68719476735_u64);
    assert_eq!(value["region"]["cells"][0]["identity"]["mnc"], "01");
    assert!(value["region"]["cells"][1]["identity"].get("pci").is_none());
    assert!(!data.incomplete);
    assert!(data.attribution.source.contains("opencellid.org"));
    assert_eq!(
        http.requests[0].query.iter().find(|(key, _)| key == "key").unwrap().1,
        "private&key"
    );
}

#[test]
fn empty_success_does_not_fallback_but_service_failure_does_without_leaking_keys() {
    let custom = Provider::Custom {
        endpoint: "https://cells.example/query".into(),
        token: Some("my-token".into()),
    };
    let mut http =
        MockHttp { replies: [reply(json!({"count":0,"cells":[]}))].into(), ..Default::default() };
    let result = fetch_with_fallback(&mut http, &source(), Some(&custom), query(), 1000).unwrap();
    assert!(result.region.cells.is_empty());
    assert_eq!(http.requests.len(), 1);
    http.replies=[Ok(HttpResponse { status:401, body:json!({"code":2,"error":"API Key not known: private&key"}).to_string() }),reply(json!({"version":1,"cells":[],"incomplete":false,"attribution":{"text":"My data","source":"https://cells.example","license":null,"changes":null}}))].into();
    let result = fetch_with_fallback(&mut http, &source(), Some(&custom), query(), 1000).unwrap();
    assert_eq!(result.provider, ProviderKind::Custom);
    assert_eq!(result.failures[0].error, QueryError::Unauthorized);
    assert!(!serde_json::to_string(&result).unwrap().contains("private&key"));
    assert_eq!(http.requests.last().unwrap().body.as_ref().unwrap()["target"]["latitude"], 0.0);
}

#[test]
fn a_quota_error_is_reported_as_a_quota_error_even_though_it_arrives_as_http_400() {
    // OpenCellID reports exhausted quotas as HTTP 400 with code 7.
    let mut http = MockHttp {
        replies: [Ok(HttpResponse {
            status: 400,
            body: json!({"error":"Rate limit exceeded","code":7}).to_string(),
        })]
        .into(),
        ..Default::default()
    };
    assert_eq!(fetch(&mut http, &source(), query(), 0).unwrap_err(), QueryError::RateLimited);

    let mut http = MockHttp {
        replies: [Ok(HttpResponse {
            status: 400,
            body: json!({"error":"Invalid input data","code":3}).to_string(),
        })]
        .into(),
        ..Default::default()
    };
    assert_eq!(fetch(&mut http, &source(), query(), 0).unwrap_err(), QueryError::InvalidQuery);
}

#[test]
fn pagination_filters_box_corners_and_marks_unsupported_rows_without_claiming_empty_coverage() {
    let mut first: Vec<_> = (0..50).map(|id| row(id, "LTE")).collect();
    first[0]["lat"] = json!(0.004);
    first[0]["lon"] = json!(0.004);
    first[1]["radio"] = json!("NBIOT");
    let mut http = MockHttp {
        replies: [
            reply(json!({"count":50,"cells":first})),
            reply(json!({"count":1,"cells":[row(50,"LTE")]})),
        ]
        .into(),
        ..Default::default()
    };
    let data = fetch(&mut http, &source(), query(), 0).unwrap();
    assert_eq!(data.region.cells.len(), 49);
    assert_eq!(data.skipped, 1);
    assert!(data.incomplete);
    assert_eq!(http.requests[1].query.iter().find(|(k, _)| k == "offset").unwrap().1, "50");
    let mut http = MockHttp {
        replies: [reply(json!({"code":1,"error":"temporarily unavailable"}))].into(),
        ..Default::default()
    };
    assert!(fetch(&mut http, &source(), query(), 0).is_err());
}

#[test]
fn boxes_cover_date_line_poles_and_respect_provider_area_limit() {
    for (lat, lon, radius) in
        [(0.0, 179.999, 500.0), (89.999, 20.0, 500.0), (-90.0, 0.0, 500.0), (30.0, 120.0, 5000.0)]
    {
        let area =
            AreaQuery { target: Coordinate { latitude: lat, longitude: lon }, radius_m: radius };
        let boxes = area.boxes().unwrap();
        assert!(!boxes.is_empty());
        for bbox in &boxes {
            assert!(bbox.south >= -90.0 && bbox.north <= 90.0 && bbox.south <= bbox.north);
            assert!(bbox.west >= -180.0 && bbox.east <= 180.0 && bbox.west < bbox.east);
            assert!(bbox.area_m2() <= 4_000_000.0, "{bbox:?}");
        }
        assert!(boxes.iter().any(|b| b.contains(area.target)));
        if lon > 179.0 {
            assert!(boxes.iter().any(|b| b.west == -180.0));
        }
    }
}

#[test]
fn provider_kind_serialises_to_the_names_the_panel_sends() {
    // Provider names are part of the client protocol.
    assert_eq!(json!(ProviderKind::OpenCellId), json!("open_cell_id"));
    assert_eq!(json!(ProviderKind::Custom), json!("custom"));
}

#[test]
fn local_cache_preserves_attribution_and_requires_matching_provider_area_and_age() {
    let mut http = MockHttp {
        replies: [reply(json!({"count":1,"cells":[row(1,"LTE")]}))].into(),
        ..Default::default()
    };
    let data = fetch(&mut http, &source(), query(), 1000).unwrap();
    let mut cache = CellCache::default();
    cache.insert(data.clone()).unwrap();
    let hit = cache.lookup(&source(), query(), 1500, 1000).unwrap();
    assert_eq!(hit.attribution, data.attribution);
    assert!(
        cache
            .lookup(
                &Provider::Custom { endpoint: "https://other.example/query".into(), token: None },
                query(),
                1500,
                1000
            )
            .is_none()
    );
    assert!(cache.lookup(&source(), query(), 3000, 1000).is_none());
    let elsewhere = AreaQuery { target: Coordinate { latitude: 1.0, longitude: 1.0 }, ..query() };
    assert!(cache.lookup(&source(), elsewhere, 1500, 1000).is_none());
    let dir = std::env::temp_dir().join(format!("justlocation-cells-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cache.json");
    cache.save(&path).unwrap();
    assert!(CellCache::open(&path).unwrap().lookup(&source(), query(), 1500, 1000).is_some());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

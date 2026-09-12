use justlocation_backend::cell_providers::{
    Downloader, Http, HttpRequest, HttpResponse, QueryError,
};
use justlocation_backend::cell_service::CellService;
use serde_json::{Value, json};

struct NoNetwork;
impl Http for NoNetwork {
    fn send(&mut self, _: HttpRequest) -> Result<HttpResponse, QueryError> {
        panic!("this operation must not connect to a supplier")
    }
}
impl Downloader for NoNetwork {
    /// Online query tests must not enter the dataset download channel.
    fn download(&mut self, _: &str, _: &str) -> Result<Box<dyn std::io::Read + Send>, QueryError> {
        panic!("this operation must not download a dataset")
    }
}
fn send(service: &CellService, value: Value) -> Value {
    service.handle(&mut NoNetwork, &value.to_string(), 1000)
}

#[test]
fn settings_default_to_opencellid_and_keep_saved_credentials_out_of_responses() {
    let dir =
        std::env::temp_dir().join(format!("justlocation-cell-settings-{}", std::process::id()));
    let service = CellService::new(&dir);
    let status = send(&service, json!({"version":1,"op":"settings"}));
    assert_eq!(status["settings"]["primary"], "open_cell_id");
    assert_eq!(status["settings"]["fallback"], "custom");
    assert!(status["settings"].get("fake_location_ready").is_none());
    let update = json!({"version":1,"op":"configure","settings":{"primary":"open_cell_id","fallback":"custom","opencellid_key":"user-secret","custom_endpoint":"https://cells.example/query","custom_token":"token-secret"}});
    let status = send(&service, update);
    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["settings"]["opencellid_configured"], true);
    let status = send(
        &service,
        json!({"version":1,"op":"configure","settings":{"primary":"custom","fallback":null,"custom_endpoint":"https://cells.example/query"}}),
    );
    assert_eq!(status["settings"]["custom_token_configured"], true);
    assert!(!status.to_string().contains("secret"));
    let invalid = send(
        &service,
        json!({"version":1,"op":"configure","settings":{"primary":"custom","fallback":null,"custom_endpoint":"https://cells.example/query?key=oops"}}),
    );
    assert_eq!(invalid["ok"], false);
    assert_eq!(
        send(&service, json!({"version":1,"op":"settings"}))["settings"]["custom_endpoint"],
        "https://cells.example/query"
    );
    std::fs::remove_file(dir.join("cell-providers.json")).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn a_stored_file_naming_the_removed_supplier_still_loads_the_rest_of_the_settings() {
    // Removed provider names fall back without losing the rest of the saved settings.
    let dir = std::env::temp_dir().join(format!("justlocation-cell-legacy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("cell-providers.json"),
        r#"{"primary":"fake_location","fallback":"fake_location","opencellid_key":"kept","custom_endpoint":"https://cells.example/query","custom_token":null}"#,
    )
    .unwrap();
    let service = CellService::new(&dir);
    let status = send(&service, json!({"version":1,"op":"settings"}));
    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["settings"]["primary"], "open_cell_id");
    assert_eq!(status["settings"]["fallback"], "open_cell_id");
    assert_eq!(status["settings"]["opencellid_configured"], true);
    std::fs::remove_file(dir.join("cell-providers.json")).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn imported_dataset_is_queryable_offline_without_credentials_or_a_running_daemon() {
    let dir =
        std::env::temp_dir().join(format!("justlocation-cell-offline-{}", std::process::id()));
    let service = CellService::new(&dir);
    let dataset = json!({"provider":"open_cell_id","origin":"https://opencellid.org","region":{"center":{"latitude":0,"longitude":0},"radius_m":1000,"source":"OpenCellID","fetched_at_ms":0,"cells":[]},"attribution":{"text":"OpenCellID","source":"https://opencellid.org","license":"https://creativecommons.org/licenses/by-sa/4.0/","changes":null},"incomplete":false,"skipped":0,"failures":[]});
    assert_eq!(send(&service, json!({"version":1,"op":"import","dataset":dataset}))["ok"], true);
    let request = json!({"version":1,"op":"query","area":{"target":{"latitude":0,"longitude":0},"radius_m":500},"mode":"offline"});
    let result = service.handle(&mut NoNetwork, &request.to_string(), 8 * 86400000);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["cached"], true);
    assert_eq!(result["stale"], true);
    assert_eq!(result["dataset"]["region"]["radius_m"], 500.0);
    let result = send(
        &service,
        json!({"version":1,"op":"query","area":{"target":{"latitude":1,"longitude":0},"radius_m":500},"mode":"offline"}),
    );
    assert_eq!(result["ok"], false);
    assert!(result["error"].as_str().unwrap().contains("no offline cell data"));
    assert_eq!(send(&service, json!({"version":1,"op":"clear_cache"}))["ok"], true);
    std::fs::remove_file(dir.join("cell-cache.json")).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn changing_provider_preserves_dataset_preferences_and_clears_old_credentials() {
    let directory =
        std::env::temp_dir().join(format!("justlocation-provider-update-{}", std::process::id()));
    let service = CellService::new(&directory);
    let first = send(
        &service,
        json!({"version":1,"op":"configure","settings":{
            "primary":"custom","fallback":null,"custom_endpoint":"https://first.example/cells",
            "custom_token":"first-secret","dataset_auto_update":true,"dataset_mcc":460
        }}),
    );
    assert_eq!(first["ok"], true, "{first}");
    let next = send(
        &service,
        json!({"version":1,"op":"configure","settings":{
            "primary":"custom","fallback":null,"custom_endpoint":"https://second.example/cells"
        }}),
    );
    assert_eq!(next["ok"], true, "{next}");
    assert_eq!(next["settings"]["custom_token_configured"], false);
    assert_eq!(next["settings"]["dataset_auto_update"], true);
    assert_eq!(next["settings"]["dataset_mcc"], 460);

    let path = directory.join("cell-providers.json");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(!saved.contains("first-secret"));
    let rejected = send(
        &service,
        json!({"version":1,"op":"configure","settings":{
            "primary":"custom","fallback":null,"custom_endpoint":"https://third.example/cells",
            "custom_token":"invalid\ncredential"
        }}),
    );
    assert_eq!(rejected["ok"], false);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

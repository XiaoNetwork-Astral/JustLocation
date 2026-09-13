//! Private WebService keys and provider-specific place search, shared by CLI and panel.
use crate::{
    Position,
    cell_providers::{Http, HttpRequest},
    coordinates::{CoordinateSystem, convert},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapProvider {
    Amap,
    Tencent,
    Baidu,
}

impl MapProvider {
    pub fn console(self) -> &'static str {
        match self {
            Self::Amap => "https://console.amap.com/dev/key/app",
            Self::Tencent => "https://lbs.qq.com/dev/console/key/manage",
            Self::Baidu => "https://lbsyun.baidu.com/apiconsole/key",
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Keys {
    #[serde(default)]
    keys: BTreeMap<MapProvider, String>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Settings,
    Capabilities,
    Plan { plan: crate::routing::PlanRequest },
    ConfigureKey { provider: MapProvider, key: String },
    Search { provider: MapProvider, query: String, region: String },
}
#[derive(Deserialize)]
struct Request {
    version: u32,
    #[serde(flatten)]
    command: Command,
}

pub struct MapService {
    directory: PathBuf,
}
impl MapService {
    pub fn new(directory: &Path) -> Self {
        Self { directory: directory.to_owned() }
    }
    fn load(&self) -> Result<Keys, String> {
        match fs::read(self.directory.join("map-keys.json")) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| "cannot read map key settings".into())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Keys::default()),
            Err(_) => Err("cannot read map key settings".into()),
        }
    }
    pub fn handle(&self, http: &mut impl Http, input: &str) -> Value {
        match self.run(http, input) {
            Ok(mut output) => {
                output["version"] = json!(1);
                output["ok"] = json!(true);
                output
            }
            Err(error) => json!({"version":1,"ok":false,"error":error}),
        }
    }
    fn run(&self, http: &mut impl Http, input: &str) -> Result<Value, String> {
        let request: Request = serde_json::from_str(input).map_err(|_| "invalid map request")?;
        if request.version != 1 {
            return Err("unsupported map protocol version".into());
        }
        let mut keys = self.load()?;
        match request.command {
            Command::Settings => Ok(public_settings(&keys)),
            Command::Capabilities => Ok(crate::routing::capabilities()),
            Command::Plan { plan } => {
                plan.validate()?;
                let key = keys
                    .keys
                    .get(&plan.provider)
                    .ok_or("missing WebService key for route provider")?;
                let response = http
                    .send(crate::routing::request(&plan, key)?)
                    .map_err(|_| "route service network request failed")?;
                if response.status != 200 {
                    return Err(format!(
                        "route service HTTP {}; check key permissions and quota",
                        response.status
                    ));
                }
                let data = serde_json::from_str(&response.body)
                    .map_err(|_| "invalid route service JSON")?;
                let candidates = crate::routing::parse(&plan, &data)?;
                let output = json!({"coordinate_system":"wgs84","candidates":candidates});
                if serde_json::to_vec(&output).map_err(|_| "cannot encode route candidates")?.len()
                    > crate::route_store::MAX_FILE - 64
                {
                    return Err("route candidates exceed 64 MiB".into());
                }
                Ok(output)
            }
            Command::ConfigureKey { provider, key } => {
                let key = key.trim();
                if key.len() > 512 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
                    return Err(
                        "map key must contain at most 512 printable ASCII characters".into()
                    );
                }
                if key.is_empty() {
                    keys.keys.remove(&provider);
                } else {
                    keys.keys.insert(provider, key.into());
                }
                fs::create_dir_all(&self.directory)
                    .map_err(|_| "cannot create map settings directory")?;
                crate::storage::atomic_save(
                    &self.directory.join("map-keys.json"),
                    &serde_json::to_vec(&keys).map_err(|_| "cannot encode map settings")?,
                )
                .map_err(|_| "cannot save map key settings")?;
                Ok(public_settings(&keys))
            }
            Command::Search { provider, query, region } => {
                let query = query.trim();
                let region = region.trim();
                if query.is_empty()
                    || query.len() > 96
                    || region.is_empty()
                    || region.len() > 96
                    || query.chars().any(char::is_control)
                    || region.contains(['(', ')', ','])
                    || region.chars().any(char::is_control)
                {
                    return Err(
                        "supply a keyword and city/region, each at most 96 UTF-8 bytes".into()
                    );
                }
                let key =
                    keys.keys.get(&provider).filter(|key| !key.is_empty()).ok_or_else(|| {
                        format!(
                            "missing {provider:?} WebService key; configure it at {}",
                            provider.console()
                        )
                    })?;
                let request = search_request(provider, key, query, region);
                let response =
                    http.send(request).map_err(|_| "map service network request failed")?;
                if response.status == 401 || response.status == 403 {
                    return Err("map key rejected; check its WebService permissions".into());
                }
                if response.status == 429 {
                    return Err("map service quota exceeded".into());
                }
                if response.status != 200 {
                    return Err(format!("map service HTTP {}", response.status));
                }
                let data: Value =
                    serde_json::from_str(&response.body).map_err(|_| "invalid map service JSON")?;
                let places = parse_places(provider, &data)?;
                Ok(json!({"provider":provider,"coordinate_system":"wgs84","places":places}))
            }
        }
    }
}

fn public_settings(keys: &Keys) -> Value {
    let providers = [MapProvider::Amap,MapProvider::Tencent,MapProvider::Baidu].map(|provider|
        json!({"provider":provider,"configured":keys.keys.get(&provider).is_some_and(|key|!key.is_empty()),
            "key_type":"WebService","console_url":provider.console()}));
    json!({"providers":providers})
}

fn search_request(provider: MapProvider, key: &str, query: &str, region: &str) -> HttpRequest {
    let (url, params): (&str, Vec<(&str, String)>) = match provider {
        MapProvider::Amap => (
            "https://restapi.amap.com/v3/place/text",
            vec![
                ("key", key.into()),
                ("keywords", query.into()),
                ("city", region.into()),
                ("citylimit", "true".into()),
                ("offset", "20".into()),
                ("page", "1".into()),
                ("output", "JSON".into()),
            ],
        ),
        MapProvider::Tencent => (
            "https://apis.map.qq.com/ws/place/v1/search",
            vec![
                ("key", key.into()),
                ("keyword", query.into()),
                ("boundary", format!("region({region},0)")),
                ("page_size", "20".into()),
                ("page_index", "1".into()),
                ("output", "json".into()),
            ],
        ),
        MapProvider::Baidu => (
            "https://api.map.baidu.com/place/v2/search",
            vec![
                ("ak", key.into()),
                ("query", query.into()),
                ("region", region.into()),
                ("city_limit", "true".into()),
                ("page_size", "20".into()),
                ("page_num", "0".into()),
                ("ret_coordtype", "gcj02ll".into()),
                ("output", "json".into()),
            ],
        ),
    };
    HttpRequest {
        url: url.into(),
        query: params.into_iter().map(|(name, value)| (name.into(), value)).collect(),
        body: None,
        bearer: None,
    }
}

fn parse_places(provider: MapProvider, data: &Value) -> Result<Vec<Value>, String> {
    let (status, items) = match provider {
        MapProvider::Amap => (data["status"] == "1", data.get("pois")),
        MapProvider::Tencent => (data["status"] == 0, data.get("data")),
        MapProvider::Baidu => (data["status"] == 0, data.get("results")),
    };
    if !status {
        return Err("map service rejected the request; check key permissions and quota".into());
    }
    let items = items.and_then(Value::as_array).ok_or("map service returned no place list")?;
    let mut places = Vec::new();
    for item in items.iter().take(20) {
        let (latitude, longitude) = if provider == MapProvider::Amap {
            let location = item["location"].as_str().ok_or("invalid map coordinates")?;
            let (lon, lat) = location.split_once(',').ok_or("invalid map coordinates")?;
            (
                lat.parse::<f64>().map_err(|_| "invalid map latitude")?,
                lon.parse::<f64>().map_err(|_| "invalid map longitude")?,
            )
        } else {
            (
                item["location"]["lat"].as_f64().ok_or("invalid map latitude")?,
                item["location"]["lng"].as_f64().ok_or("invalid map longitude")?,
            )
        };
        let position = convert(
            &Position::new(latitude, longitude),
            CoordinateSystem::Gcj02,
            CoordinateSystem::Wgs84,
        )
        .map_err(str::to_owned)?;
        let name = if provider == MapProvider::Tencent { &item["title"] } else { &item["name"] };
        let name = name.as_str().ok_or("map place has no name")?;
        places.push(json!({"id":item.get("id").or_else(||item.get("uid")).and_then(Value::as_str).unwrap_or(""),
            "name":name,"address":item["address"].as_str().unwrap_or(""),"position":position}));
    }
    Ok(places)
}

pub fn request(encoded: &str) -> Result<String, String> {
    let input = crate::transport::decode_request(encoded)?;
    Ok(format!(
        "{}\n",
        MapService::new(Path::new(crate::transport::DATA_DIR))
            .handle(&mut crate::cell_http::Network::maps(), &input)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell_providers::{HttpResponse, QueryError};
    struct Mock {
        requests: Vec<HttpRequest>,
        body: Value,
    }
    impl Http for Mock {
        fn send(&mut self, request: HttpRequest) -> Result<HttpResponse, QueryError> {
            self.requests.push(request);
            Ok(HttpResponse { status: 200, body: self.body.to_string() })
        }
    }
    #[test]
    fn keys_are_separate_private_and_used_by_the_correct_provider() {
        let directory = std::env::temp_dir().join(format!("jl-map-keys-{}", std::process::id()));
        let service = MapService::new(&directory);
        let mut http = Mock { requests: Vec::new(), body: json!({}) };
        let search = |provider| {
            json!({"version":1,"op":"search","provider":provider,"query":"测试","region":"上海"})
                .to_string()
        };
        let missing = service.handle(&mut http, &search("amap"));
        assert_eq!(missing["ok"], false);
        assert!(http.requests.is_empty());
        for (provider, key) in
            [("amap", "AMAP-PRIVATE"), ("tencent", "TENCENT-PRIVATE"), ("baidu", "BAIDU-PRIVATE")]
        {
            let output = service.handle(
                &mut http,
                &json!({"version":1,"op":"configure_key","provider":provider,"key":key})
                    .to_string(),
            );
            assert_eq!(output["ok"], true);
            assert!(!output.to_string().contains("PRIVATE"));
        }
        let fixtures = [
            (
                "amap",
                "AMAP-PRIVATE",
                json!({"status":"1","pois":[{"id":"a","name":"地点","address":"地址","location":"121.47822305927693,31.22845773757727"}]}),
            ),
            (
                "tencent",
                "TENCENT-PRIVATE",
                json!({"status":0,"data":[{"id":"b","title":"地点","location":{"lat":31.22845773757727,"lng":121.47822305927693}}]}),
            ),
            (
                "baidu",
                "BAIDU-PRIVATE",
                json!({"status":0,"results":[{"uid":"c","name":"地点","location":{"lat":31.22845773757727,"lng":121.47822305927693}}]}),
            ),
        ];
        for (provider, key, body) in fixtures {
            http.body = body;
            let output = service.handle(&mut http, &search(provider));
            assert_eq!(output["ok"], true, "{output}");
            assert!(!output.to_string().contains("PRIVATE"));
            let point: Position =
                serde_json::from_value(output["places"][0]["position"].clone()).unwrap();
            assert!(crate::route::distance(&point, &Position::new(31.2304, 121.4737)) < 0.01);
            assert!(http.requests.last().unwrap().query.iter().any(|(_, value)| value == key));
        }
        assert!(http.requests[2].query.contains(&("ret_coordtype".into(), "gcj02ll".into())));
        let output = service
            .handle(&mut http, r#"{"version":1,"op":"configure_key","provider":"amap","key":""}"#);
        assert_eq!(output["providers"][0]["configured"], false);
        assert_eq!(output["providers"][1]["configured"], true);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failures_and_bad_coordinates_are_not_valid_empty_results() {
        assert!(
            parse_places(MapProvider::Amap, &json!({"status":"0","info":"PRIVATE"}))
                .unwrap_err()
                .find("PRIVATE")
                .is_none()
        );
        assert!(
            parse_places(
                MapProvider::Tencent,
                &json!({"status":0,"data":[{"title":"x","location":{"lat":999,"lng":0}}]})
            )
            .is_err()
        );
        assert_eq!(
            parse_places(MapProvider::Baidu, &json!({"status":0,"results":[]})).unwrap(),
            Vec::<Value>::new()
        );
    }
}

//! Independent CLI requests for cell acquisition and private supplier settings.
use crate::cell_providers::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const FRESH_MS: u64 = 7 * 86_400_000;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    primary: ProviderKind,
    fallback: Option<ProviderKind>,
    opencellid_key: String,
    custom_endpoint: String,
    custom_token: Option<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            primary: ProviderKind::OpenCellId,
            fallback: Some(ProviderKind::FakeLocation),
            opencellid_key: String::new(),
            custom_endpoint: String::new(),
            custom_token: None,
        }
    }
}
impl Settings {
    fn provider(&self, kind: ProviderKind) -> Provider {
        match kind {
            ProviderKind::OpenCellId => Provider::OpenCellId {
                key: self.opencellid_key.clone(),
            },
            ProviderKind::FakeLocation => Provider::FakeLocation,
            ProviderKind::Custom => Provider::Custom {
                endpoint: self.custom_endpoint.clone(),
                token: self.custom_token.clone(),
            },
        }
    }
    fn public(&self) -> Value {
        json!({"primary":self.primary,"fallback":self.fallback,"opencellid_configured":!self.opencellid_key.is_empty(),"custom_endpoint":self.custom_endpoint,"custom_token_configured":self.custom_token.is_some(),"fake_location_ready":false})
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsUpdate {
    primary: ProviderKind,
    fallback: Option<ProviderKind>,
    opencellid_key: Option<String>,
    custom_endpoint: String,
    custom_token: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum QueryMode {
    PreferCache,
    Refresh,
    Offline,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Settings,
    Configure { settings: SettingsUpdate },
    Query { area: AreaQuery, mode: QueryMode },
    Import { dataset: Dataset },
    ClearCache,
}
#[derive(Deserialize)]
struct Request {
    version: u32,
    #[serde(flatten)]
    command: Command,
}

pub struct CellService {
    directory: PathBuf,
}
impl CellService {
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.to_owned(),
        }
    }
    fn settings(&self) -> Result<Settings, String> {
        match fs::read(self.directory.join("cell-providers.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "供应商设置无法读取".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(_) => Err("供应商设置无法读取".into()),
        }
    }
    pub fn handle(&self, http: &mut impl Http, input: &str, now_ms: u64) -> Value {
        match self.run(http, input, now_ms) {
            Ok(mut value) => {
                value["version"] = json!(1);
                value["ok"] = json!(true);
                value
            }
            Err(error) => json!({"version":1,"ok":false,"error":error}),
        }
    }
    fn run(&self, http: &mut impl Http, input: &str, now_ms: u64) -> Result<Value, String> {
        if input.len() >= crate::protocol::MAX_FRAME as usize {
            return Err("基站请求太大".into());
        }
        let request: Request = serde_json::from_str(input).map_err(|_| "基站请求格式不正确")?;
        if request.version != 1 {
            return Err("基站请求版本不兼容".into());
        }
        let mut settings = self.settings()?;
        if matches!(request.command, Command::Settings) {
            return Ok(json!({"settings":settings.public()}));
        }
        fs::create_dir_all(&self.directory).map_err(|_| "无法创建基站数据目录")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))
                .map_err(|_| "无法设置数据目录权限")?;
        }
        let cache_path = self.directory.join("cell-cache.json");
        match request.command {
            Command::Settings => unreachable!(),
            Command::Configure { settings: update } => {
                if !update.custom_endpoint.is_empty() {
                    crate::cell_http::validate_endpoint(&update.custom_endpoint)
                        .map_err(|_| "请填写 HTTP(S) 地址，凭据请单独填写")?;
                }
                if (update.primary == ProviderKind::Custom
                    || update.fallback == Some(ProviderKind::Custom))
                    && update.custom_endpoint.is_empty()
                {
                    return Err("请填写自定义供应商地址".into());
                }
                if update.fallback == Some(update.primary) {
                    return Err("首选和备用供应商不能相同".into());
                }
                settings.primary = update.primary;
                settings.fallback = update.fallback;
                // Changing servers must not silently send the old server's credential elsewhere.
                if settings.custom_endpoint != update.custom_endpoint {
                    settings.custom_token = None;
                }
                settings.custom_endpoint = update.custom_endpoint;
                if let Some(key) = update.opencellid_key {
                    settings.opencellid_key = credential(key)?;
                }
                if let Some(token) = update.custom_token {
                    let token = credential(token)?;
                    settings.custom_token = (!token.is_empty()).then_some(token);
                }
                atomic_save(
                    &self.directory.join("cell-providers.json"),
                    &serde_json::to_vec(&settings).map_err(|_| "供应商设置无法保存")?,
                )
                .map_err(|_| "供应商设置无法保存")?;
                Ok(json!({"settings":settings.public()}))
            }
            Command::ClearCache => {
                CellCache::default()
                    .save(&cache_path)
                    .map_err(|_| "缓存清除失败")?;
                Ok(json!({}))
            }
            Command::Import { dataset } => {
                let mut cache = CellCache::open(&cache_path).map_err(|_| "基站缓存无法读取")?;
                cache.insert(dataset).map_err(|_| "离线基站文件内容无效")?;
                cache
                    .save(&cache_path)
                    .map_err(|_| "离线基站数据保存失败")?;
                Ok(json!({}))
            }
            Command::Query { area, mode } => {
                area.validate().map_err(|e| e.to_string())?;
                let primary = settings.provider(settings.primary);
                let fallback = settings.fallback.map(|kind| settings.provider(kind));
                let mut cache = CellCache::open(&cache_path).map_err(|_| "基站缓存无法读取")?;
                if !matches!(mode, QueryMode::Refresh) {
                    let max_age = if matches!(mode, QueryMode::Offline) {
                        u64::MAX
                    } else {
                        FRESH_MS
                    };
                    if let Some(data) = cache.lookup(&primary, area, now_ms, max_age) {
                        return Ok(query_result(data, true, now_ms));
                    }
                    // Offline mode may use an explicitly configured fallback's saved data.
                    if matches!(mode, QueryMode::Offline) {
                        if let Some(data) = fallback
                            .as_ref()
                            .and_then(|p| cache.lookup(p, area, now_ms, max_age))
                        {
                            return Ok(query_result(data, true, now_ms));
                        }
                        return Err("这个位置还没有离线基站数据，请先联网查询或导入".into());
                    }
                }
                let data = match fetch(http, &primary, area, now_ms) {
                    Ok(data) => data,
                    Err(first) => {
                        // Preserve the useful primary error while the legacy adapter is unavailable.
                        let Some(second) = fallback.as_ref().filter(|p| {
                            p.kind() != ProviderKind::FakeLocation && p.origin() != primary.origin()
                        }) else {
                            return Err(first.to_string());
                        };
                        if matches!(first, QueryError::InvalidQuery | QueryError::AreaTooLarge) {
                            return Err(first.to_string());
                        }
                        if !matches!(mode, QueryMode::Refresh) {
                            if let Some(mut data) = cache.lookup(second, area, now_ms, FRESH_MS) {
                                data.failures = vec![ProviderFailure {
                                    provider: primary.kind(),
                                    error: first,
                                }];
                                return Ok(query_result(data, true, now_ms));
                            }
                        }
                        let mut data = fetch(http, second, area, now_ms)
                            .map_err(|e| format!("首选：{first}；备用：{e}"))?;
                        data.failures.push(ProviderFailure {
                            provider: primary.kind(),
                            error: first,
                        });
                        data
                    }
                };
                cache.insert(data.clone()).map_err(|e| e.to_string())?;
                let saved = cache.save(&cache_path).is_ok();
                let mut result = query_result(data, false, now_ms);
                if !saved {
                    result["warning"] = json!("查询成功，但缓存保存失败");
                }
                Ok(result)
            }
        }
    }
}
fn credential(value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err("凭据格式不正确".into());
    }
    Ok(value)
}
fn query_result(data: Dataset, cached: bool, now_ms: u64) -> Value {
    let stale = now_ms
        .checked_sub(data.region.fetched_at_ms)
        .is_none_or(|age| age > FRESH_MS);
    json!({"dataset":data,"cached":cached,"stale":stale})
}

pub fn request(encoded: &str) -> Result<String, String> {
    let input = crate::transport::decode_request(encoded)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "系统时间无效")?
        .as_millis() as u64;
    let result = CellService::new(Path::new(crate::transport::DATA_DIR)).handle(
        &mut crate::cell_http::Network::default(),
        &input,
        now,
    );
    Ok(format!("{result}\n"))
}

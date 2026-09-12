//! Independent CLI requests for cell acquisition and private supplier settings.
use crate::cell_providers::*;
use crate::cell_providers::MAX_CELLS;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const FRESH_MS: u64 = 7 * 86_400_000;

#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct Settings {
    #[serde(deserialize_with = "crate::cell_providers::lenient_kind")]
    primary: ProviderKind,
    #[serde(default, deserialize_with = "crate::cell_providers::lenient_optional_kind")]
    fallback: Option<ProviderKind>,
    opencellid_key: String,
    custom_endpoint: String,
    custom_token: Option<String>,
    /// 是否每天自动拉一次增量。**默认关**：它会联网、消耗上游额度，该由使用者显式打开。
    /// 老配置文件没有这一段，靠 `serde(default)` 读成 false。
    #[serde(default)]
    dataset_auto_update: bool,
    /// 自动更新针对哪个国家（MCC）。0 表示还没选。
    #[serde(default)]
    dataset_mcc: u16,
    /// 上次成功检查的 UTC 日期（`YYYY-MM-DD`）；同一天不重复检查。
    #[serde(default)]
    dataset_last_check_day: Option<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            primary: ProviderKind::OpenCellId,
            fallback: Some(ProviderKind::Custom),
            opencellid_key: String::new(),
            custom_endpoint: String::new(),
            custom_token: None,
            dataset_auto_update: false,
            dataset_mcc: 0,
            dataset_last_check_day: None,
        }
    }
}
impl Settings {
    fn provider(&self, kind: ProviderKind) -> Provider {
        match kind {
            ProviderKind::OpenCellId => Provider::OpenCellId {
                key: self.opencellid_key.clone(),
            },
            ProviderKind::Custom => Provider::Custom {
                endpoint: self.custom_endpoint.clone(),
                token: self.custom_token.clone(),
            },
        }
    }
    fn public(&self) -> Value {
        json!({
            "primary": self.primary,
            "fallback": self.fallback,
            "opencellid_configured": !self.opencellid_key.is_empty(),
            "custom_endpoint": self.custom_endpoint,
            "custom_token_configured": self.custom_token.is_some(),
            // 数据集那三项不敏感，直接报出来，面板与命令行都能看到现在的状态。
            "dataset_auto_update": self.dataset_auto_update,
            "dataset_mcc": self.dataset_mcc,
            "dataset_last_check_day": self.dataset_last_check_day,
        })
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
    /// 数据集的自动更新开关与目标国家。**都是"给才改"**：老调用方不带这两个字段时，
    /// 不能把它们重置成默认值，否则改一次 API Key 就会把每日更新关掉。
    #[serde(default)]
    dataset_auto_update: Option<bool>,
    #[serde(default)]
    dataset_mcc: Option<u16>,
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
    Query {
        area: AreaQuery,
        mode: QueryMode,
        /// 数据集查询要用哪个国家（MCC）。**由调用方给**：装置上报的订阅里带 MCC，
        /// 而基站服务本身看不到那些订阅。省略时回落到设置里的 `dataset_mcc`。
        #[serde(default)]
        mcc: Option<u16>,
    },
    Import { dataset: Dataset },
    ClearCache,
    /// 数据集的下载与查看。**凭据由调用方从设置里取**，请求体里不带 token，
    /// 免得它出现在命令行历史、日志或崩溃报告里。
    DatasetStatus,
    DatasetDownload {
        mcc: u16,
        /// `full` 拉整国导出，`diff` 只拉某一天的增量（默认今天）。
        #[serde(default)]
        mode: DatasetMode,
        #[serde(default)]
        date_utc: Option<String>,
        /// 只在测试或离线校验时用；省略时取设置里的 OpenCellID token。
        #[serde(default)]
        token: Option<String>,
    },
    /// 每日增量检查：今天已经查过就直接返回 `skipped`，不重复消耗额度。
    DatasetUpdate {
        #[serde(default)]
        mcc: u16,
        #[serde(default)]
        token: Option<String>,
    },
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum DatasetMode {
    #[default]
    Full,
    Diff,
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
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "cannot read provider settings".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(_) => Err("cannot read provider settings".into()),
        }
    }
    pub fn handle(&self, http: &mut (impl Http + Downloader), input: &str, now_ms: u64) -> Value {
        match self.run(http, input, now_ms) {
            Ok(mut value) => {
                value["version"] = json!(1);
                value["ok"] = json!(true);
                value
            }
            Err(error) => json!({"version":1,"ok":false,"error":error}),
        }
    }
    fn run(&self, http: &mut (impl Http + Downloader), input: &str, now_ms: u64) -> Result<Value, String> {
        if input.len() >= crate::protocol::MAX_FRAME as usize {
            return Err("cell request too large".into());
        }
        let request: Request = serde_json::from_str(input).map_err(|_| "malformed cell request")?;
        if request.version != 1 {
            return Err("incompatible cell request version".into());
        }
        let mut settings = self.settings()?;
        if matches!(request.command, Command::Settings) {
            return Ok(json!({"settings":settings.public()}));
        }
        fs::create_dir_all(&self.directory).map_err(|_| "cannot create the cell data directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))
                .map_err(|_| "cannot set permissions on the data directory")?;
        }
        let cache_path = self.directory.join("cell-cache.json");
        match request.command {
            Command::Settings => unreachable!(),
            Command::Configure { settings: update } => {
                if !update.custom_endpoint.is_empty() {
                    crate::cell_http::validate_endpoint(&update.custom_endpoint)
                        .map_err(|_| "enter an HTTP(S) address; keep credentials in their own field")?;
                }
                if (update.primary == ProviderKind::Custom
                    || update.fallback == Some(ProviderKind::Custom))
                    && update.custom_endpoint.is_empty()
                {
                    return Err("enter the custom provider address".into());
                }
                if update.fallback == Some(update.primary) {
                    return Err("the primary and fallback providers must differ".into());
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
                if let Some(enabled) = update.dataset_auto_update {
                    settings.dataset_auto_update = enabled;
                }
                if let Some(mcc) = update.dataset_mcc {
                    settings.dataset_mcc = mcc;
                }
                self.save_settings(&settings)?;
                Ok(json!({"settings":settings.public()}))
            }
            Command::ClearCache => {
                CellCache::default()
                    .save(&cache_path)
                    .map_err(|_| "cannot clear the cache")?;
                Ok(json!({}))
            }
            Command::Import { dataset } => {
                let mut cache = CellCache::open(&cache_path).map_err(|_| "cannot read the cell cache")?;
                cache.insert(dataset).map_err(|_| "invalid offline cell data")?;
                cache
                    .save(&cache_path)
                    .map_err(|_| "cannot save offline cell data")?;
                Ok(json!({}))
            }
            Command::Query { area, mode, mcc } => {
                area.validate().map_err(|e| e.to_string())?;
                let primary = settings.provider(settings.primary);
                let fallback = settings.fallback.map(|kind| settings.provider(kind));
                // **本地数据集优先**：它离线、不吃额度、也不受在线查询那 5 公里的半径限制。
                // 顺序是"数据集 → 在线（含缓存）"，这样额度用完时仍然能答，
                // 而数据集没覆盖到的国家/地区才回落到联网查询。
                if let Some(mut data) = self.dataset_region(area, mcc)? {
                    data.failures = Vec::new();
                    let mut result = query_result(data, false, now_ms);
                    result["offline"] = json!(true);
                    return Ok(result);
                }
                let mut cache = CellCache::open(&cache_path).map_err(|_| "cannot read the cell cache")?;
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
                        return Err(
                            "no offline cell data for this location; query online or import a dataset first"
                                .into(),
                        );
                    }
                }
                let data = match fetch(http, &primary, area, now_ms) {
                    Ok(data) => data,
                    Err(first) => {
                        // Preserve the useful primary error while the fallback is unavailable.
                        let Some(second) = fallback.as_ref().filter(|p| p.origin() != primary.origin())
                        else {
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
                            .map_err(|e| format!("primary: {first}; fallback: {e}"))?;
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
                    result["warning"] = json!("query succeeded but the cache could not be saved");
                }
                Ok(result)
            }
            Command::DatasetStatus => {
                let dataset = crate::dataset::Dataset::new(&self.directory);
                // 报体积与条数，**不设上限也不拒绝**：多大是使用者自己的选择。
                // `estimated_import_bytes` 只是把"导入时大致要吃多少内存"如实摆出来。
                let installed: Vec<Value> = dataset
                    .installed()
                    .into_iter()
                    .map(|entry| {
                        json!({
                            "mcc": format!("{:03}", entry.mcc),
                            "records": entry.records,
                            "bytes": entry.bytes,
                            "estimated_import_bytes": entry.estimated_import_bytes(),
                        })
                    })
                    .collect();
                let day = utc_day(now_ms);
                Ok(json!({
                    "installed": installed,
                    "auto_update": settings.dataset_auto_update,
                    "last_check_day": settings.dataset_last_check_day,
                    "checked_today": settings.dataset_last_check_day.as_deref() == Some(day.as_str()),
                    "today": day,
                    "note": "datasets are not size-limited; check records and bytes before downloading a large country",
                }))
            }
            Command::DatasetDownload { mcc, mode, date_utc, token } => {
                let token = dataset_token(&settings, token)?;
                let dataset = crate::dataset::Dataset::new(&self.directory);
                match mode {
                    DatasetMode::Full => {
                        let url = crate::dataset::country_url(mcc, &token);
                        let report = import_download(http, &dataset, mcc, &url, false)?;
                        Ok(json!({
                            "mode": "full", "mcc": format!("{:03}", report.mcc),
                            "records": report.records, "skipped": report.skipped,
                        }))
                    }
                    DatasetMode::Diff => {
                        let date = date_utc.unwrap_or_else(|| utc_day(now_ms));
                        let url = crate::dataset::diff_url(&date, &token)
                            .ok_or_else(|| "invalid diff date (expected YYYY-MM-DD)".to_string())?;
                        let report = import_download(http, &dataset, mcc, &url, true)?;
                        Ok(json!({
                            "mode": "diff", "date": date, "mcc": format!("{:03}", report.mcc),
                            "records": report.records, "skipped": report.skipped,
                        }))
                    }
                }
            }
            Command::DatasetUpdate { mcc, token } => {
                let day = utc_day(now_ms);
                if settings.dataset_last_check_day.as_deref() == Some(day.as_str()) {
                    return Ok(json!({"skipped": "already checked today", "day": day}));
                }
                let mcc = if mcc == 0 { settings.dataset_mcc } else { mcc };
                if mcc == 0 {
                    return Err("no country code configured for dataset updates".into());
                }
                if !settings.dataset_auto_update {
                    return Ok(json!({"skipped": "automatic dataset updates are off", "day": day}));
                }
                let token = dataset_token(&settings, token)?;
                // 增量文件是"那一天全世界的变动"，按 MCC 过滤后并进已有数据集；
                // 还没下过整国数据时先拉一次全量，否则增量没有底子可合。
                let dataset = crate::dataset::Dataset::new(&self.directory);
                let have = dataset
                    .installed()
                    .iter()
                    .any(|entry| entry.mcc == mcc && entry.records > 0);
                let url = if have {
                    crate::dataset::diff_url(&day, &token)
                        .ok_or_else(|| "invalid diff date".to_string())?
                } else {
                    crate::dataset::country_url(mcc, &token)
                };
                let report = import_download(http, &dataset, mcc, &url, have)?;
                // 只有真的成功才记"今天查过"：失败不留痕，下次心跳还会再试。
                let mut updated = settings.clone();
                updated.dataset_last_check_day = Some(day.clone());
                self.save_settings(&updated)?;
                Ok(json!({
                    "mode": if have { "diff" } else { "full" }, "day": day,
                    "mcc": format!("{:03}", report.mcc), "records": report.records,
                }))
            }
        }
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        atomic_save(
            &self.directory.join("cell-providers.json"),
            &serde_json::to_vec(settings).map_err(|_| "cannot save provider settings".to_string())?,
        )
        .map_err(|_| "cannot save provider settings".to_string())
    }

    /// 从本地数据集里凑一个区域出来。
    ///
    /// <p>数据集按 MCC 分文件，所以要先把"该用哪个 MCC"定下来：请求里带上就用它，
    /// 否则用 `dataset_mcc`（每日更新那项设置）。**定不下来就不猜**——猜错国家会返回
    /// 一片完全无关的小区，那比"查不到"更糟。
    ///
    /// <p>返回 `None` 表示"数据集里这个位置没有数据"，交给调用方回落到在线查询。
    fn dataset_region(&self, area: AreaQuery, mcc: Option<u16>) -> Result<Option<Dataset>, String> {
        let settings = self.settings()?;
        let mcc = mcc.or((settings.dataset_mcc != 0).then_some(settings.dataset_mcc));
        let Some(mcc) = mcc else {
            return Ok(None);
        };
        let dataset = crate::dataset::Dataset::new(&self.directory);
        // 上限沿用在线那条路径的 128：面板与装置两边都不该被一次查询塞爆。
        let cells = dataset
            .nearby(mcc, area.target, area.radius_m, MAX_CELLS)
            .map_err(|e| e.to_string())?;
        if cells.is_empty() {
            return Ok(None);
        }
        let region = crate::cells::CellRegion {
            center: area.target,
            radius_m: area.radius_m,
            source: format!("OpenCellID dataset (offline, MCC {mcc:03})"),
            fetched_at_ms: 0,
            cells,
        };
        region.validate().map_err(|e| e.to_string())?;
        Ok(Some(Dataset {
            provider: ProviderKind::OpenCellId,
            origin: "https://opencellid.org".into(),
            region,
            attribution: crate::cell_providers::Attribution {
                text: "Cell tower data from OpenCellID".into(),
                source: "https://opencellid.org".into(),
                license: Some("https://creativecommons.org/licenses/by-sa/4.0/".into()),
                changes: Some(
                    "Downloaded country dataset; selected cells within the requested circle.".into(),
                ),
            },
            incomplete: false,
            skipped: 0,
            failures: Vec::new(),
        }))
    }
}

/// 数据集下载用的凭据：优选用请求里给的（测试与离线校验），否则取设置里的。
fn dataset_token(settings: &Settings, provided: Option<String>) -> Result<String, String> {
    let token = provided.unwrap_or_else(|| settings.opencellid_key.clone());
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err("enter an API key for the provider first".into());
    }
    Ok(token)
}

/// 下载 → 解压 → 导入/合并。**先解压再喂给解析器**，全程流式，不整包进内存。
fn import_download(
    http: &mut impl Downloader,
    dataset: &crate::dataset::Dataset,
    mcc: u16,
    url: &str,
    merge: bool,
) -> Result<crate::dataset::ImportReport, String> {
    let stream = http
        .download(url, "application/gzip, application/octet-stream")
        .map_err(|error| error.to_string())?;
    let decoder = flate2::read::GzDecoder::new(stream);
    if merge {
        dataset.merge_csv(mcc, decoder).map_err(|e| e.to_string())
    } else {
        dataset.import_csv(mcc, decoder).map_err(|e| e.to_string())
    }
}

/// UTC 的 `YYYY-MM-DD`。用它而不是本地日期：上游的增量文件按 UTC 命名。
fn utc_day(now_ms: u64) -> String {
    let days = (now_ms / 86_400_000) as i64;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant 的 `civil_from_days`：把"1970-01-01 起的天数"换算成公历年月日。
/// 自己算是为了不引依赖，而且这段算法是纯整数、没有时区与闰秒的坑。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
fn credential(value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err("invalid credential format".into());
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
        .map_err(|_| "invalid system time")?
        .as_millis() as u64;
    let result = CellService::new(Path::new(crate::transport::DATA_DIR)).handle(
        &mut crate::cell_http::Network::default(),
        &input,
        now,
    );
    Ok(format!("{result}\n"))
}

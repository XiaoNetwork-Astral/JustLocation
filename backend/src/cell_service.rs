//! Independent CLI requests for cell acquisition and private supplier settings.
use crate::cell_providers::{
    AreaQuery, CellCache, Dataset, Downloader, Http, MAX_CELLS, ProviderFailure, ProviderKind,
    QueryError, fetch,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod settings;
use settings::{Settings, SettingsUpdate};

const FRESH_MS: u64 = 7 * 86_400_000;

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
    Configure {
        settings: SettingsUpdate,
    },
    Query {
        area: AreaQuery,
        mode: QueryMode,
        /// Country MCC supplied by the caller, falling back to the saved dataset MCC.
        #[serde(default)]
        mcc: Option<u16>,
    },
    Import {
        dataset: Dataset,
    },
    ClearCache,
    /// Inspect locally installed datasets and update status.
    DatasetStatus,
    DatasetDownload {
        mcc: u16,
        /// Download a full country export or one day's changes.
        #[serde(default)]
        mode: DatasetMode,
        #[serde(default)]
        date_utc: Option<String>,
        /// Optional override; otherwise use the saved OpenCellID key.
        #[serde(default)]
        token: Option<String>,
    },
    /// Skip automatic updates already completed on the same UTC day.
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
        Self { directory: directory.to_owned() }
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
    fn run(
        &self,
        http: &mut (impl Http + Downloader),
        input: &str,
        now_ms: u64,
    ) -> Result<Value, String> {
        if input.len() >= crate::protocol::MAX_FRAME as usize {
            return Err("cell request too large".into());
        }
        let request: Request = serde_json::from_str(input).map_err(|_| "malformed cell request")?;
        if request.version != 1 {
            return Err("incompatible cell request version".into());
        }
        let mut settings = Settings::load(&self.directory)?;
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
                settings.update(update)?;
                settings.save(&self.directory)?;
                Ok(json!({"settings":settings.public()}))
            }
            Command::ClearCache => {
                CellCache::default().save(&cache_path).map_err(|_| "cannot clear the cache")?;
                Ok(json!({}))
            }
            Command::Import { dataset } => {
                let mut cache =
                    CellCache::open(&cache_path).map_err(|_| "cannot read the cell cache")?;
                cache.insert(dataset).map_err(|_| "invalid offline cell data")?;
                cache.save(&cache_path).map_err(|_| "cannot save offline cell data")?;
                Ok(json!({}))
            }
            Command::Query { area, mode, mcc } => {
                area.validate().map_err(|e| e.to_string())?;
                let primary = settings.provider(settings.primary);
                let fallback = settings.fallback.map(|kind| settings.provider(kind));
                // Prefer local country data before online queries and their cache.
                if let Some(mut data) = self.dataset_region(
                    area,
                    mcc.or((settings.dataset_mcc != 0).then_some(settings.dataset_mcc)),
                )? {
                    data.failures = Vec::new();
                    let mut result = query_result(data, false, now_ms);
                    result["offline"] = json!(true);
                    return Ok(result);
                }
                let mut cache =
                    CellCache::open(&cache_path).map_err(|_| "cannot read the cell cache")?;
                if !matches!(mode, QueryMode::Refresh) {
                    let max_age =
                        if matches!(mode, QueryMode::Offline) { u64::MAX } else { FRESH_MS };
                    if let Some(data) = cache.lookup(&primary, area, now_ms, max_age) {
                        return Ok(query_result(data, true, now_ms));
                    }
                    // Offline mode may use an explicitly configured fallback's saved data.
                    if matches!(mode, QueryMode::Offline) {
                        if let Some(data) =
                            fallback.as_ref().and_then(|p| cache.lookup(p, area, now_ms, max_age))
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
                        let Some(second) =
                            fallback.as_ref().filter(|p| p.origin() != primary.origin())
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
                        data.failures
                            .push(ProviderFailure { provider: primary.kind(), error: first });
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
                // Report size and estimated import memory without imposing a dataset size limit.
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
                // Daily changes cover all countries. Import a full country first if none is installed.
                let dataset = crate::dataset::Dataset::new(&self.directory);
                let have =
                    dataset.installed().iter().any(|entry| entry.mcc == mcc && entry.records > 0);
                let url = if have {
                    crate::dataset::diff_url(&day, &token)
                        .ok_or_else(|| "invalid diff date".to_string())?
                } else {
                    crate::dataset::country_url(mcc, &token)
                };
                let report = import_download(http, &dataset, mcc, &url, have)?;
                // Only successful imports count as today's check, so failures remain retryable.
                let mut updated = settings.clone();
                updated.dataset_last_check_day = Some(day.clone());
                updated.save(&self.directory)?;
                Ok(json!({
                    "mode": if have { "diff" } else { "full" }, "day": day,
                    "mcc": format!("{:03}", report.mcc), "records": report.records,
                }))
            }
        }
    }

    /// Query the requested or configured MCC. No data leaves online fallback to the caller.
    fn dataset_region(&self, area: AreaQuery, mcc: Option<u16>) -> Result<Option<Dataset>, String> {
        let Some(mcc) = mcc else {
            return Ok(None);
        };
        let dataset = crate::dataset::Dataset::new(&self.directory);
        // Use the same response limit as online queries.
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
                    "Downloaded country dataset; selected cells within the requested circle."
                        .into(),
                ),
            },
            incomplete: false,
            skipped: 0,
            failures: Vec::new(),
        }))
    }
}

/// Use an explicit download credential when supplied, otherwise the saved key.
fn dataset_token(settings: &Settings, provided: Option<String>) -> Result<String, String> {
    let token = provided.unwrap_or_else(|| settings.opencellid_key.clone());
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err("enter an API key for the provider first".into());
    }
    Ok(token)
}

/// Stream decompression into the CSV importer or merger.
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

/// Use UTC dates because upstream change files are named by UTC day.
fn utc_day(now_ms: u64) -> String {
    let days = (now_ms / 86_400_000) as i64;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's civil_from_days algorithm, using days since 1970-01-01.
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
fn query_result(data: Dataset, cached: bool, now_ms: u64) -> Value {
    let stale = now_ms.checked_sub(data.region.fetched_at_ms).is_none_or(|age| age > FRESH_MS);
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

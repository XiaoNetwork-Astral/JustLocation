//! Cell suppliers return one portable dataset. Fetching runs outside the control daemon:
//! network timeouts must never delay its stop command or system_server heartbeat.
use crate::cells::{Cell, CellIdentity, CellRegion, Coordinate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashSet, fs, io, path::Path};

const EARTH: f64 = 6_371_008.8;
const MAX_REQUESTS: usize = 64;
/// 一次查询返回的小区上限。**在线与离线两条路径共用**：面板与装置都不该被一次查询塞爆。
pub const MAX_CELLS: usize = 128;
/// 单个查询格子允许的最大球面面积。
///
/// <p>OpenCellID 的 `getInArea` 拒绝超过 4,000,000 m² 的包围盒（实测：直接回
/// `BBOX too big - Limit to 4,000,000 sq.mts.`），而我们的实现把这种回绝当成
/// "无法连接基站供应商"，界面上根本看不出原因。这里先留 20% 余量，再由
/// [`AreaQuery::boxes`] 收尾自检，保证发出去的每一格都在上限之内。
const MAX_CELL_AREA_M2: f64 = 3_200_000.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    OpenCellId,
    Custom,
}

/// 读供应商名，并容忍已删除的旧名字。
///
/// <p>历史上这里有过一个指向 Fake Location 远程服务的候选项，它从未真正可用（一旦被选中就
/// 直接返回 `NotReady`），已按用户要求删除。但用户存下的 `cell-providers.json` 里可能还写着
/// 那个名字；直接反序列化会让整份设置读取失败（`deny_unknown_fields` + 枚举），
/// 于是把"选了一个不存在的供应商"降级成默认值，而不是让面板失去全部配置。
pub fn lenient_kind<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ProviderKind, D::Error> {
    let name = String::deserialize(deserializer)?;
    match name.as_str() {
        "open_cell_id" => Ok(ProviderKind::OpenCellId),
        "custom" => Ok(ProviderKind::Custom),
        "fake_location" => Ok(ProviderKind::OpenCellId),
        other => Err(serde::de::Error::custom(format!("unknown provider: {other}"))),
    }
}

/// [`lenient_kind`] 的可选版本，给 `fallback` 用。
pub fn lenient_optional_kind<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ProviderKind>, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?
        .map(|name| match name.as_str() {
            "open_cell_id" => Ok(ProviderKind::OpenCellId),
            "custom" => Ok(ProviderKind::Custom),
            "fake_location" => Ok(ProviderKind::OpenCellId),
            other => Err(serde::de::Error::custom(format!("unknown provider: {other}"))),
        })
        .transpose()?)
}

// Intentionally no Debug: configuration contains credentials.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Provider {
    OpenCellId {
        key: String,
    },
    Custom {
        endpoint: String,
        token: Option<String>,
    },
}
impl Default for Provider {
    fn default() -> Self {
        Self::OpenCellId { key: String::new() }
    }
}
impl Provider {
    pub fn kind(&self) -> ProviderKind {
        match self {
            Self::OpenCellId { .. } => ProviderKind::OpenCellId,
            Self::Custom { .. } => ProviderKind::Custom,
        }
    }
    pub fn origin(&self) -> &str {
        match self {
            Self::OpenCellId { .. } => "https://opencellid.org",
            Self::Custom { endpoint, .. } => endpoint,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaQuery {
    pub target: Coordinate,
    pub radius_m: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct BoundingBox {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}
impl BoundingBox {
    pub fn area_m2(self) -> f64 {
        EARTH.powi(2)
            * (self.east - self.west).to_radians()
            * (self.north.to_radians().sin() - self.south.to_radians().sin())
    }
    pub fn contains(self, p: Coordinate) -> bool {
        p.latitude >= self.south
            && p.latitude <= self.north
            && p.longitude >= self.west
            && p.longitude <= self.east
    }
    fn parameter(self) -> String {
        format!("{},{},{},{}", self.south, self.west, self.north, self.east)
    }
}
impl AreaQuery {
    pub fn validate(self) -> Result<(), QueryError> {
        self.target
            .validate()
            .map_err(|_| QueryError::InvalidQuery)?;
        if !self.radius_m.is_finite() || !(1.0..=200_000.0).contains(&self.radius_m) {
            return Err(QueryError::InvalidQuery);
        }
        Ok(())
    }
    pub fn boxes(self) -> Result<Vec<BoundingBox>, QueryError> {
        self.validate()?;
        // Keep one interactive lookup bounded. Larger regions belong in offline datasets.
        if self.radius_m > 5000.0 {
            return Err(QueryError::AreaTooLarge);
        }
        let delta = (self.radius_m / EARTH).to_degrees();
        let south = (self.target.latitude - delta).max(-90.0);
        let north = (self.target.latitude + delta).min(90.0);
        let longitude_delta = if south == -90.0 || north == 90.0 {
            180.0
        } else {
            ((self.radius_m / EARTH).sin() / self.target.latitude.to_radians().cos())
                .clamp(-1.0, 1.0)
                .asin()
                .to_degrees()
        };
        let west = self.target.longitude - longitude_delta;
        let east = self.target.longitude + longitude_delta;
        let spans = if longitude_delta == 180.0 {
            vec![(-180.0, 180.0)]
        } else if west < -180.0 {
            vec![(-180.0, east), (west + 360.0, 180.0)]
        } else if east > 180.0 {
            vec![(west, 180.0), (-180.0, east - 360.0)]
        } else {
            vec![(west, east)]
        };
        // 切分的每格必须落在供应商的面积上限之内。**按球面面积反推**，不要按"边长"拍脑袋：
        // 球面上一个小格的面积是 `R²·Δλ·(sin φ₂ − sin φ₁)`。
        // 先切纬度带，再**逐格**算它自己的经度跨度——同一行里不同经度上的格宽度并不相同，
        // 按"整行统一列数"算会让最靠极点的那一格超标（这正是自检要拦的情况）。
        let target = MAX_CELL_AREA_M2.sqrt();
        let rows = (((north - south).to_radians() * EARTH / target).ceil() as usize).max(1);
        let mut boxes = Vec::new();
        for row in 0..rows {
            let s = south + (north - south) * row as f64 / rows as f64;
            let n = south + (north - south) * (row + 1) as f64 / rows as f64;
            let band = (s.to_radians().sin() - n.to_radians().sin()).abs().max(1e-15);
            for &(w, e) in &spans {
                // 该纬度带上，一格允许的最大经度跨度（弧度）。
                // 以前这里用的是"离赤道最近"的纬度，格子在远离赤道的一侧被拉宽，
                // 西安（34°N）这种中纬度就会超出供应商上限，表现是查询回
                // "无法连接基站供应商"——其实是供应商回绝了（2026-09-12 实测）。
                let column = (MAX_CELL_AREA_M2 / (EARTH.powi(2) * band)).min(std::f64::consts::PI);
                let span = (e - w).abs().to_radians();
                let cols = ((span / column).ceil() as usize).max(1);
                for col in 0..cols {
                    boxes.push(BoundingBox {
                        south: s,
                        north: n,
                        west: w + (e - w) * col as f64 / cols as f64,
                        east: w + (e - w) * (col + 1) as f64 / cols as f64,
                    });
                }
            }
        }
        if boxes.len() > MAX_REQUESTS {
            return Err(QueryError::AreaTooLarge);
        }
        // 最后一道自检：算出来的格子必须真的在上限之内，否则宁可报"范围太大"，
        // 也不要发一个注定被供应商回绝的请求（那种失败在界面上看不出原因）。
        if boxes.iter().any(|b| b.area_m2() > MAX_CELL_AREA_M2) {
            return Err(QueryError::AreaTooLarge);
        }
        Ok(boxes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryError {
    MissingCredential,
    Unauthorized,
    RateLimited,
    InvalidQuery,
    AreaTooLarge,
    Unavailable,
    InvalidResponse,
    Network,
}
impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::MissingCredential => "enter an API key for the provider first",
                Self::Unauthorized => "the provider rejected the credential; check the API key",
                Self::RateLimited => "the provider query quota is used up",
                Self::InvalidQuery => "invalid query parameters",
                Self::AreaTooLarge => {
                    "OpenCellID online queries are limited to a 5 km radius; narrow the area or import offline data"
                }
                Self::Unavailable => "the provider is temporarily unavailable",
                Self::InvalidResponse => "the provider returned an incompatible data format",
                Self::Network => "cannot reach the cell provider",
            }
        )
    }
}
impl std::error::Error for QueryError {}

// Do not log these objects: URLs, query parameters and headers may contain credentials.
pub struct HttpRequest {
    pub url: String,
    pub query: Vec<(String, String)>,
    pub bearer: Option<String>,
    pub body: Option<Value>,
}
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}
pub trait Http {
    fn send(&mut self, request: HttpRequest) -> Result<HttpResponse, QueryError>;
}

/// 流式下载一个**大文件**（国家数据集）。
///
/// <p>单独一个 trait 而不是给 [`Http`] 加方法：那条通道的契约是"小 JSON 整包读"，
/// 而数据集是几十 MB 的 gz 流，两者的超时、体积与错误处理都不一样。给一个**默认实现**
/// （直接报"暂时无法查询"）是为了让既有的测试替身不必跟着实现下载——下载那条路
/// 另有自己的测试，不该把它拖进每个查询用例里。
pub trait Downloader {
    /// 返回一个可读流。调用方负责解压与解析，**不要整包读进内存**。
    fn download(&mut self, url: &str, accept: &str) -> Result<Box<dyn std::io::Read + Send>, QueryError> {
        let _ = (url, accept);
        Err(QueryError::Unavailable)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attribution {
    pub text: String,
    pub source: String,
    pub license: Option<String>,
    pub changes: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderFailure {
    pub provider: ProviderKind,
    pub error: QueryError,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dataset {
    pub provider: ProviderKind,
    pub origin: String,
    pub region: CellRegion,
    pub attribution: Attribution,
    /// The request budget, result cap, unsupported rows or upstream truncation lost data.
    pub incomplete: bool,
    pub skipped: usize,
    pub failures: Vec<ProviderFailure>,
}
impl Dataset {
    pub fn validate(&self) -> Result<(), QueryError> {
        self.region
            .validate()
            .map_err(|_| QueryError::InvalidResponse)?;
        if self.origin.is_empty()
            || self.origin.len() > 2048
            || self.attribution.text.trim().is_empty()
            || self.attribution.text.len() > 2048
            || self.attribution.source.len() > 2048
            || self
                .attribution
                .license
                .as_ref()
                .is_some_and(|v| v.len() > 2048)
            || self
                .attribution
                .changes
                .as_ref()
                .is_some_and(|v| v.len() > 2048)
        {
            return Err(QueryError::InvalidResponse);
        }
        Ok(())
    }
}

fn body(response: HttpResponse) -> Result<Value, QueryError> {
    // **先读响应体，再按状态码兜底**：OpenCellID 的"额度用完"是 `HTTP 400` 加
    // `{"code":7}`，只按状态码判会把它当成"查询参数无效"，界面上再被包装成
    // "无法连接基站供应商"——2026-09-12 就是这句话把人带偏了整整一轮。
    let parsed = serde_json::from_str::<Value>(&response.body).ok();
    // 报错体：不论 HTTP 状态是 2xx 还是 4xx，都以供应商的 `code` 为准。
    if parsed.as_ref().is_some_and(|value| value.get("error").is_some()) {
        return Err(classify(parsed.as_ref()));
    }
    match response.status {
        401 | 403 => return Err(QueryError::Unauthorized),
        429 => return Err(QueryError::RateLimited),
        400 => return Err(QueryError::InvalidQuery),
        200..=299 => {}
        _ => return Err(QueryError::Unavailable),
    }
    parsed.ok_or(QueryError::InvalidResponse)
}

/// 供应商的 `code` 字段：2 凭据无效、3 参数无效、7 额度用完。
fn classify(value: Option<&Value>) -> QueryError {
    match value.and_then(|v| v.get("code")).and_then(Value::as_u64) {
        Some(2) => QueryError::Unauthorized,
        Some(3) => QueryError::InvalidQuery,
        Some(7) => QueryError::RateLimited,
        _ => QueryError::Unavailable,
    }
}
fn integer(value: &Value, name: &str) -> Result<u64, QueryError> {
    value
        .get(name)
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
        .ok_or(QueryError::InvalidResponse)
}
fn small(value: &Value, name: &str) -> Result<u32, QueryError> {
    u32::try_from(integer(value, name)?).map_err(|_| QueryError::InvalidResponse)
}
fn digits(value: &Value, name: &str, width: usize) -> Result<String, QueryError> {
    // Preserve an explicit string width. Numeric MNCs only imply a minimum of two digits;
    // the source cannot distinguish numeric 1 originating from 01 versus 001.
    if let Some(text) = value.get(name).and_then(Value::as_str) {
        return Ok(text.to_owned());
    }
    Ok(format!("{:0width$}", integer(value, name)?))
}
fn open_cell(value: &Value) -> Result<Cell, QueryError> {
    let radio = value
        .get("radio")
        .and_then(Value::as_str)
        .ok_or(QueryError::InvalidResponse)?;
    let identity = if radio == "CDMA" {
        CellIdentity::Cdma {
            sid: small(value, "mnc")?,
            nid: small(value, "lac")?,
            bid: small(value, "cellid")?,
        }
    } else {
        let mcc = digits(value, "mcc", 3)?;
        let mnc = digits(value, "mnc", 2)?;
        let area = small(value, "lac")?;
        match radio {
            "GSM" => CellIdentity::Gsm {
                mcc,
                mnc,
                lac: area,
                cid: small(value, "cellid")?,
                arfcn: None,
                bsic: None,
            },
            "UMTS" | "WCDMA" => CellIdentity::Wcdma {
                mcc,
                mnc,
                lac: area,
                cid: small(value, "cellid")?,
                psc: None,
                uarfcn: None,
            },
            "LTE" => CellIdentity::Lte {
                mcc,
                mnc,
                tac: area,
                ci: small(value, "cellid")?,
                pci: None,
                earfcn: None,
            },
            "NR" => CellIdentity::Nr {
                mcc,
                mnc,
                tac: area,
                nci: integer(value, "cellid")?,
                pci: None,
                nrarfcn: None,
            },
            _ => return Err(QueryError::InvalidResponse),
        }
    };
    identity
        .validate()
        .map_err(|_| QueryError::InvalidResponse)?;
    let position = Coordinate {
        latitude: value["lat"].as_f64().ok_or(QueryError::InvalidResponse)?,
        longitude: value["lon"].as_f64().ok_or(QueryError::InvalidResponse)?,
    };
    position
        .validate()
        .map_err(|_| QueryError::InvalidResponse)?;
    let range_m = match value.get("range") {
        None | Some(Value::Null) => 0.0,
        Some(v) => v.as_f64().ok_or(QueryError::InvalidResponse)?,
    };
    if !range_m.is_finite() || !(0.0..=200_000.0).contains(&range_m) {
        return Err(QueryError::InvalidResponse);
    }
    Ok(Cell {
        identity,
        position,
        range_m,
    })
}

pub fn fetch(
    http: &mut impl Http,
    provider: &Provider,
    area: AreaQuery,
    now_ms: u64,
) -> Result<Dataset, QueryError> {
    area.validate()?;
    let (cells, attribution, incomplete, skipped) = match provider {
        Provider::OpenCellId { key } => {
            if key.trim().is_empty() {
                return Err(QueryError::MissingCredential);
            }
            let mut cells = Vec::new();
            let mut seen = HashSet::new();
            let mut skipped = 0;
            let mut requests = 0;
            let mut incomplete = false;
            'boxes: for bbox in area.boxes()? {
                let mut offset = 0;
                loop {
                    if requests == MAX_REQUESTS {
                        incomplete = true;
                        break 'boxes;
                    }
                    let value = body(http.send(HttpRequest {
                        url: "https://opencellid.org/cell/getInArea".into(),
                        query: vec![
                            ("key".into(), key.clone()),
                            ("BBOX".into(), bbox.parameter()),
                            ("format".into(), "json".into()),
                            ("limit".into(), "50".into()),
                            ("offset".into(), offset.to_string()),
                        ],
                        bearer: None,
                        body: None,
                    })?)?;
                    requests += 1;
                    let rows = value
                        .get("cells")
                        .and_then(Value::as_array)
                        .ok_or(QueryError::InvalidResponse)?;
                    if value.get("count").and_then(Value::as_u64).is_none() || rows.len() > 50 {
                        return Err(QueryError::InvalidResponse);
                    }
                    for row in rows {
                        match open_cell(row) {
                            Ok(cell)
                                if area.target.distance_to(cell.position) <= area.radius_m
                                    && seen.insert(cell.identity.key()) =>
                            {
                                cells.push(cell)
                            }
                            Ok(_) => {}
                            Err(_) => {
                                skipped += 1;
                                incomplete = true;
                            }
                        }
                    }
                    if rows.len() < 50 {
                        break;
                    }
                    offset += 50;
                }
            }
            (cells,Attribution { text:"Cell tower data from OpenCellID".into(),source:"https://opencellid.org".into(),license:Some("https://creativecommons.org/licenses/by-sa/4.0/".into()),changes:Some("Normalized identities; selected cells within the requested circle. Numeric MNCs use at least two digits; their original width is unavailable.".into()) },incomplete,skipped)
        }
        Provider::Custom { endpoint, token } => {
            let value = body(http.send(HttpRequest {
                url: endpoint.clone(),
                query: vec![],
                bearer: token.clone(),
                body: Some(json!({"version":1,"target":area.target,"radius_m":area.radius_m})),
            })?)?;
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct CustomResponse {
                version: u32,
                cells: Vec<Cell>,
                attribution: Attribution,
                incomplete: bool,
            }
            let result: CustomResponse =
                serde_json::from_value(value).map_err(|_| QueryError::InvalidResponse)?;
            if result.version != 1 || result.cells.len() > MAX_CELLS {
                return Err(QueryError::InvalidResponse);
            }
            (result.cells, result.attribution, result.incomplete, 0)
        }
    };
    let mut cells = cells;
    cells.sort_by(|a, b| {
        area.target
            .distance_to(a.position)
            .total_cmp(&area.target.distance_to(b.position))
    });
    let incomplete = incomplete || cells.len() > MAX_CELLS;
    cells.truncate(MAX_CELLS);
    let data = Dataset {
        provider: provider.kind(),
        origin: provider.origin().into(),
        region: CellRegion {
            center: area.target,
            radius_m: area.radius_m,
            source: match provider.kind() {
                ProviderKind::OpenCellId => "OpenCellID",
                ProviderKind::Custom => "Custom",
            }
            .into(),
            fetched_at_ms: now_ms,
            cells,
        },
        attribution,
        incomplete,
        skipped,
        failures: vec![],
    };
    data.validate()?;
    Ok(data)
}

pub fn fetch_with_fallback(
    http: &mut impl Http,
    primary: &Provider,
    fallback: Option<&Provider>,
    area: AreaQuery,
    now_ms: u64,
) -> Result<Dataset, QueryError> {
    // Validate locally before sending the selected coordinates to either supplier.
    area.validate()?;
    match fetch(http, primary, area, now_ms) {
        Ok(data) => Ok(data),
        Err(error) => match fallback {
            Some(second)
                if second.origin() != primary.origin()
                    && !matches!(error, QueryError::InvalidQuery | QueryError::AreaTooLarge) =>
            {
                let mut data = fetch(http, second, area, now_ms)?;
                data.failures.push(ProviderFailure {
                    provider: primary.kind(),
                    error,
                });
                Ok(data)
            }
            _ => Err(error),
        },
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellCache {
    entries: Vec<Dataset>,
}
impl CellCache {
    pub fn insert(&mut self, data: Dataset) -> Result<(), QueryError> {
        data.validate()?;
        self.entries.retain(|e| {
            e.origin != data.origin
                || e.region.center != data.region.center
                || e.region.radius_m != data.region.radius_m
        });
        self.entries.insert(0, data);
        self.entries.truncate(32);
        Ok(())
    }
    pub fn lookup(
        &self,
        provider: &Provider,
        area: AreaQuery,
        now_ms: u64,
        max_age_ms: u64,
    ) -> Option<Dataset> {
        area.validate().ok()?;
        self.entries
            .iter()
            .find(|data| {
                data.provider == provider.kind()
                    && data.origin == provider.origin()
                    && now_ms
                        .checked_sub(data.region.fetched_at_ms)
                        .is_some_and(|age| age <= max_age_ms)
                    && data.region.center.distance_to(area.target) + area.radius_m
                        <= data.region.radius_m + 0.01
            })
            .map(|data| {
                let mut found = data.clone();
                found
                    .region
                    .cells
                    .retain(|cell| area.target.distance_to(cell.position) <= area.radius_m);
                found.region.center = area.target;
                found.region.radius_m = area.radius_m;
                found.region.cells.sort_by(|a, b| {
                    area.target
                        .distance_to(a.position)
                        .total_cmp(&area.target.distance_to(b.position))
                });
                found
            })
    }
    pub fn open(path: &Path) -> io::Result<Self> {
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        let cache: Self = serde_json::from_slice(&data).map_err(io::Error::other)?;
        if cache.entries.len() > 32 || cache.entries.iter().any(|d| d.validate().is_err()) {
            return Err(io::Error::other("invalid cell cache"));
        }
        Ok(cache)
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        atomic_save(path, &serde_json::to_vec(self).map_err(io::Error::other)?)
    }
}

pub fn atomic_save(path: &Path, bytes: &[u8]) -> io::Result<()> {    use std::{
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod area_tests {
    use super::*;

    fn target(latitude: f64, longitude: f64, radius_m: f64) -> AreaQuery {
        AreaQuery {
            target: Coordinate { latitude, longitude },
            radius_m,
        }
    }

    /// 每一格都必须落在供应商的 4,000,000 m² 上限之内，否则请求会被直接回绝，
    /// 而界面上只会看到"无法连接基站供应商"（2026-09-12 实测踩过）。
    #[test]
    fn every_box_stays_under_the_provider_area_limit() {
        for &(latitude, longitude, radius) in &[
            (34.3546, 108.9360, 3000.0), // 西安未央区：中纬度，改造前会超标
            (31.2304, 121.4737, 2000.0), // 上海：低纬度
            (45.0, 126.6, 5000.0),       // 哈尔滨：改造前必然超标
            (0.0, 0.0, 5000.0),          // 赤道
            (-69.3733, 76.3767, 2000.0), // 南极：高纬度，格子会变窄
        ] {
            let boxes = target(latitude, longitude, radius).boxes().unwrap();
            assert!(!boxes.is_empty(), "({latitude},{longitude}) should produce at least one box");
            for b in &boxes {
                assert!(
                    b.area_m2() <= 4_000_000.0,
                    "({latitude},{longitude}) box area {} m² exceeds the provider limit",
                    b.area_m2()
                );
            }
        }
    }

    /// 覆盖整个请求圆的性质不能被"只求不超标"牺牲掉。
    #[test]
    fn every_box_keeps_the_requested_circle_covered() {
        let query = target(34.3546, 108.9360, 3000.0);
        let boxes = query.boxes().unwrap();
        for bearing in (0..360).step_by(15) {
            let radians = (bearing as f64).to_radians();
            let point = Coordinate {
                latitude: query.target.latitude + (query.radius_m / EARTH).to_degrees() * radians.cos(),
                longitude: query.target.longitude
                    + (query.radius_m / EARTH).to_degrees() * radians.sin()
                        / query.target.latitude.to_radians().cos(),
            };
            assert!(
                boxes.iter().any(|b| b.contains(point)),
                "the edge point at bearing {bearing}° is not covered by any box"
            );
        }
    }
}

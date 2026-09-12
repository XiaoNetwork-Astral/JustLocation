//! Private immutable route files. The library contains references, never large point arrays.
use crate::{
    Position,
    route::{MAX_POINTS, Playback, Route},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::Instant,
};

pub const MAX_FILE: usize = 64 * 1024 * 1024;
pub const PAGE_SIZE: usize = 128;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub offset: usize,
    pub total: usize,
    pub next: Option<usize>,
    pub points: Vec<Position>,
    /// Absolute indices in the complete track, including a break at this page's first point.
    pub breaks: Vec<usize>,
}

pub fn page(points: &[Position], breaks: &[usize], offset: usize, limit: usize) -> Result<Page> {
    if !(1..=PAGE_SIZE).contains(&limit) || offset > points.len() {
        return Err("page limit must be 1..128 and offset must be within the track".into());
    }
    let end = (offset + limit).min(points.len());
    Ok(Page {
        offset,
        total: points.len(),
        next: (end < points.len()).then_some(end),
        points: points[offset..end].to_vec(),
        breaks: breaks
            [breaks.partition_point(|&i| i < offset)..breaks.partition_point(|&i| i < end)]
            .to_vec(),
    })
}

pub fn valid_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("invalid route file ID".into());
    }
    Ok(())
}

fn path(directory: &Path, id: &str) -> Result<PathBuf> {
    valid_id(id)?;
    Ok(directory.join("routes").join(format!("{id}.json")))
}

pub fn load(directory: &Path, id: &str) -> Result<Route> {
    let file =
        fs::File::open(path(directory, id)?).map_err(|e| format!("cannot read route: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE as u64 {
        return Err("route file exceeds 64 MiB".into());
    }
    let route: Route =
        serde_json::from_reader(std::io::BufReader::new(file).take(MAX_FILE as u64 + 1))
            .map_err(|e| format!("invalid route file: {e}"))?;
    Playback::new(route.clone(), Instant::now())?;
    Ok(route)
}

pub fn save(directory: &Path, route: &Route) -> Result<String> {
    Playback::new(route.clone(), Instant::now())?;
    let id = crate::scode::new_id()?;
    let path = path(directory, &id)?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(route).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE {
        return Err("route file exceeds 64 MiB".into());
    }
    crate::storage::atomic_save(&path, &bytes).map_err(|e| format!("cannot save route: {e}"))?;
    Ok(id)
}

pub fn remove(directory: &Path, id: &str) -> Result<()> {
    match fs::remove_file(path(directory, id)?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Uploads use numbered atomic chunks. Retrying the same chunk is safe; finish requires all chunks.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upload {
    pub point_count: usize,
    pub speed: f64,
    #[serde(default = "once")]
    pub repeat_count: u32,
    #[serde(default)]
    pub repeat_delay: f64,
}
fn once() -> u32 {
    1
}
fn upload_dir(directory: &Path, id: &str) -> Result<PathBuf> {
    valid_id(id)?;
    Ok(directory.join("route-uploads").join(id))
}
pub fn begin(directory: &Path, upload: Upload) -> Result<String> {
    if !(2..=MAX_POINTS).contains(&upload.point_count) {
        return Err("upload needs 2 to 100000 points".into());
    }
    Playback::new(
        Route {
            points: vec![Position::new(0., 0.), Position::new(0., 0.001)],
            breaks: vec![],
            speed: upload.speed,
            repeat_count: upload.repeat_count,
            repeat_delay: upload.repeat_delay,
        },
        Instant::now(),
    )?;
    let id = crate::scode::new_id()?;
    let dir = upload_dir(directory, &id)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    crate::storage::atomic_save(&dir.join("upload.json"), &serde_json::to_vec(&upload).unwrap())
        .map_err(|e| e.to_string())?;
    Ok(id)
}
fn upload(directory: &Path, id: &str) -> Result<Upload> {
    serde_json::from_slice(
        &fs::read(upload_dir(directory, id)?.join("upload.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
pub fn append(directory: &Path, id: &str, page: Page) -> Result<()> {
    let upload = upload(directory, id)?;
    let end = page.offset.checked_add(page.points.len()).ok_or("invalid chunk offset")?;
    if page.offset % PAGE_SIZE != 0
        || page.offset >= upload.point_count
        || page.points.len() != PAGE_SIZE.min(upload.point_count - page.offset)
        || page.total != upload.point_count
    {
        return Err("chunk must contain the next aligned block of up to 128 points".into());
    }
    for point in &page.points {
        point.validate()?;
    }
    if page.breaks.iter().any(|&i| i == 0 || i < page.offset || i >= end)
        || page.breaks.windows(2).any(|p| p[0] >= p[1])
    {
        return Err("invalid chunk segment breaks".into());
    }
    crate::storage::atomic_save(
        &upload_dir(directory, id)?.join(format!("{}.json", page.offset)),
        &serde_json::to_vec(&page).unwrap(),
    )
    .map_err(|e| e.to_string())
}
pub fn finish(directory: &Path, id: &str) -> Result<Route> {
    let upload = upload(directory, id)?;
    let mut route = Route {
        points: Vec::with_capacity(upload.point_count),
        breaks: vec![],
        speed: upload.speed,
        repeat_count: upload.repeat_count,
        repeat_delay: upload.repeat_delay,
    };
    for offset in (0..upload.point_count).step_by(PAGE_SIZE) {
        let file = fs::File::open(upload_dir(directory, id)?.join(format!("{offset}.json")))
            .map_err(|e| format!("missing upload chunk at {offset}: {e}"))?;
        let page: Page = serde_json::from_reader(std::io::BufReader::new(file).take(64 * 1024))
            .map_err(|e| e.to_string())?;
        if page.offset != offset
            || page.total != upload.point_count
            || page.points.len() != PAGE_SIZE.min(upload.point_count - offset)
        {
            return Err("invalid upload chunk".into());
        }
        route.points.extend(page.points);
        route.breaks.extend(page.breaks);
    }
    Playback::new(route.clone(), Instant::now())?;
    Ok(route)
}
pub fn abort(directory: &Path, id: &str) -> Result<()> {
    let upload = upload(directory, id)?;
    let dir = upload_dir(directory, id)?;
    for offset in (0..upload.point_count).step_by(PAGE_SIZE) {
        match fs::remove_file(dir.join(format!("{offset}.json"))) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    fs::remove_file(dir.join("upload.json")).map_err(|e| e.to_string())?;
    fs::remove_dir(dir).map_err(|e| e.to_string())
}

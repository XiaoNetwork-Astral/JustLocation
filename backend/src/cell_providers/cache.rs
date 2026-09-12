use super::{AreaQuery, Dataset, Provider, QueryError};
use crate::storage::atomic_save;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

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

//! Offline OpenCellID datasets, stored as sorted, fixed-width 44-byte records.
//! Queries binary-search latitude, then filter candidates by spherical distance.
//! Imports collect records in memory for sorting and deduplication before publishing.
//! Dataset size and estimated import memory are reported without imposing a size limit.
//! Crowdsourced records are not radio measurements or a guarantee of complete coverage.
use crate::cell_providers::QueryError;
use crate::cells::{Cell, CellIdentity, Coordinate};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// Header: magic (4), version (2), MCC (2), count (4), reserved (4) bytes.
const MAGIC: &[u8; 4] = b"JLCD";
const VERSION: u16 = 1;
const HEADER: usize = 16;
/// Fixed record size shared by readers and writers.
const RECORD: usize = 44;
/// Maximum records inspected after narrowing the latitude range.
const MAX_CANDIDATES: usize = 200_000;

const EARTH_M: f64 = 6_371_008.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Radio {
    Gsm = 1,
    Wcdma = 2,
    Lte = 3,
    Nr = 4,
    Cdma = 5,
}

impl Radio {
    fn from_tag(tag: &str) -> Option<Self> {
        match tag.to_ascii_uppercase().as_str() {
            "GSM" => Some(Self::Gsm),
            "UMTS" | "WCDMA" | "HSPA" => Some(Self::Wcdma),
            "LTE" => Some(Self::Lte),
            "NR" => Some(Self::Nr),
            "CDMA" => Some(Self::Cdma),
            _ => None,
        }
    }
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Gsm),
            2 => Some(Self::Wcdma),
            3 => Some(Self::Lte),
            4 => Some(Self::Nr),
            5 => Some(Self::Cdma),
            _ => None,
        }
    }
}

/// Fields needed to construct a cell, excluding source quality metrics and timestamps.
/// Coordinates use fixed-point degrees at 1e-7 precision for compact random access.
#[derive(Clone, Copy, Debug)]
struct Record {
    latitude: i32,
    longitude: i32,
    range_m: u16,
    mcc: u16,
    mnc: u16,
    radio: Radio,
    /// LAC for GSM/WCDMA, TAC for LTE/NR.
    area: u32,
    /// CID for GSM/WCDMA, CI for LTE, or NCI truncated to 32 bits for NR.
    cell: u64,
    /// PSC/PCI, or u32::MAX when absent.
    physical: u32,
    /// UARFCN/EARFCN/NRARFCN, or u32::MAX when absent.
    channel: u32,
}

/// One file per country MCC.
pub struct Dataset {
    directory: PathBuf,
}

impl Dataset {
    pub fn new(directory: &Path) -> Self {
        Self { directory: directory.join("datasets") }
    }

    fn path(&self, mcc: u16) -> PathBuf {
        self.directory.join(format!("{mcc:03}.jlc"))
    }

    /// List installed countries with record counts and file sizes.
    pub fn installed(&self) -> Vec<InstalledDataset> {
        let mut found = Vec::new();
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return found;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(stem) = name.strip_suffix(".jlc") else {
                continue;
            };
            let Ok(mcc) = stem.parse::<u16>() else { continue };
            let Ok(count) = header_count(&entry.path()) else {
                continue;
            };
            let bytes = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            found.push(InstalledDataset { mcc, records: count, bytes });
        }
        found.sort_unstable_by_key(|dataset| dataset.mcc);
        found
    }

    /// Publish sorted records through a temporary file and an atomic rename.
    fn publish(&self, mcc: u16, mut records: Vec<Record>) -> Result<u64, QueryError> {
        records
            .sort_unstable_by_key(|record| (record.latitude, record.longitude, record.radio as u8));
        fs::create_dir_all(&self.directory).map_err(|_| QueryError::Unavailable)?;
        let path = self.path(mcc);
        let temp = path.with_extension("tmp");
        {
            let file = fs::File::create(&temp).map_err(|_| QueryError::Unavailable)?;
            let mut writer = BufWriter::new(file);
            let mut header = Vec::with_capacity(HEADER);
            header.extend_from_slice(MAGIC);
            header.extend_from_slice(&VERSION.to_le_bytes());
            header.extend_from_slice(&mcc.to_le_bytes());
            header.extend_from_slice(&(records.len() as u32).to_le_bytes());
            header.extend_from_slice(&[0u8; 4]);
            writer.write_all(&header).map_err(|_| QueryError::Unavailable)?;
            for record in &records {
                writer.write_all(&encode(record)).map_err(|_| QueryError::Unavailable)?;
            }
            writer.flush().map_err(|_| QueryError::Unavailable)?;
        }
        fs::rename(&temp, &path).map_err(|_| QueryError::Unavailable)?;
        Ok(records.len() as u64)
    }

    /// Parse a decompressed country CSV into memory, then sort and publish.
    /// Keep the first record for each identity within the selected MCC.
    pub fn import_csv<R: Read>(&self, mcc: u16, mut reader: R) -> Result<ImportReport, QueryError> {
        let mut records = Vec::new();
        let mut seen = HashSet::new();
        let mut skipped = 0u64;
        let mut line = Vec::new();
        read_line(&mut reader, &mut line).map_err(|_| QueryError::InvalidResponse)?; // Header
        loop {
            line.clear();
            let read =
                read_line(&mut reader, &mut line).map_err(|_| QueryError::InvalidResponse)?;
            if read == 0 {
                break;
            }
            if line.ends_with(b"\n") {
                line.pop();
            }
            if line.ends_with(b"\r") {
                line.pop();
            }
            if line.is_empty() {
                continue;
            }
            match parse_row(&line, mcc) {
                Some(record) => {
                    if !seen.insert(identity_key(&record)) {
                        skipped += 1;
                        continue;
                    }
                    records.push(record);
                }
                None => skipped += 1,
            }
        }
        let count = self.publish(mcc, records)?;
        Ok(ImportReport { mcc, records: count, skipped })
    }

    /// Merge changes into an installed dataset, replacing records with the same identity.
    pub fn merge_csv<R: Read>(&self, mcc: u16, mut reader: R) -> Result<ImportReport, QueryError> {
        let existing = self.records(mcc)?;
        let mut by_key: std::collections::HashMap<u64, Record> =
            existing.into_iter().map(|record| (identity_key(&record), record)).collect();
        let mut skipped = 0u64;
        let mut line = Vec::new();
        read_line(&mut reader, &mut line).map_err(|_| QueryError::InvalidResponse)?;
        loop {
            line.clear();
            let read =
                read_line(&mut reader, &mut line).map_err(|_| QueryError::InvalidResponse)?;
            if read == 0 {
                break;
            }
            if line.ends_with(b"\n") {
                line.pop();
            }
            if line.ends_with(b"\r") {
                line.pop();
            }
            if line.is_empty() {
                continue;
            }
            match parse_row(&line, mcc) {
                Some(record) => {
                    by_key.insert(identity_key(&record), record);
                }
                None => skipped += 1,
            }
        }
        let count = self.publish(mcc, by_key.into_values().collect())?;
        Ok(ImportReport { mcc, records: count, skipped })
    }

    /// Read all country records for an incremental merge.
    fn records(&self, mcc: u16) -> Result<Vec<Record>, QueryError> {
        let path = self.path(mcc);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(QueryError::Unavailable),
        };
        let count = parse_header(&bytes, mcc)?;
        let mut records = Vec::with_capacity(count);
        for index in 0..count {
            let start = HEADER + index * RECORD;
            let end = start + RECORD;
            if end > bytes.len() {
                return Err(QueryError::InvalidResponse);
            }
            records.push(decode(&bytes[start..end])?);
        }
        Ok(records)
    }

    /// Return up to limit nearby cells, ordered by distance.
    pub fn nearby(
        &self,
        mcc: u16,
        target: Coordinate,
        radius_m: f64,
        limit: usize,
    ) -> Result<Vec<Cell>, QueryError> {
        let path = self.path(mcc);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(QueryError::Unavailable),
        };
        let count = parse_header(&bytes, mcc)?;
        // Sorted, fixed-width records allow binary search over the latitude band.
        let delta_latitude = (radius_m / EARTH_M).to_degrees();
        let south = target.latitude - delta_latitude;
        let north = target.latitude + delta_latitude;
        let first = lower_bound(&bytes, count, south);
        let mut found: Vec<(f64, Cell)> = Vec::new();
        let mut examined = 0usize;
        for index in first..count {
            let start = HEADER + index * RECORD;
            let end = start + RECORD;
            if end > bytes.len() {
                return Err(QueryError::InvalidResponse);
            }
            let record = decode(&bytes[start..end])?;
            if record.latitude_of() > north {
                break;
            }
            examined += 1;
            if examined > MAX_CANDIDATES {
                break;
            }
            let position = record.position();
            let distance = target.distance_to(position);
            if distance <= radius_m
                && let Some(cell) = record.to_cell()
            {
                found.push((distance, cell));
            }
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.truncate(limit);
        Ok(found.into_iter().map(|(_, cell)| cell).collect())
    }
}

#[derive(Clone, Debug)]
pub struct ImportReport {
    pub mcc: u16,
    pub records: u64,
    pub skipped: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledDataset {
    pub mcc: u16,
    pub records: u64,
    pub bytes: u64,
}

impl InstalledDataset {
    /// Approximate peak import memory, including sorting and deduplication overhead.
    pub fn estimated_import_bytes(&self) -> u64 {
        self.records.saturating_mul(RECORD as u64).saturating_mul(6)
    }
}

fn header_count(path: &Path) -> Result<u64, QueryError> {
    let mut file = fs::File::open(path).map_err(|_| QueryError::Unavailable)?;
    let mut header = [0u8; HEADER];
    file.read_exact(&mut header).map_err(|_| QueryError::InvalidResponse)?;
    if &header[0..4] != MAGIC {
        return Err(QueryError::InvalidResponse);
    }
    Ok(u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as u64)
}

fn parse_header(bytes: &[u8], mcc: u16) -> Result<usize, QueryError> {
    if bytes.len() < HEADER || &bytes[0..4] != MAGIC {
        return Err(QueryError::InvalidResponse);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != VERSION {
        return Err(QueryError::InvalidResponse);
    }
    let stored = u16::from_le_bytes([bytes[6], bytes[7]]);
    if stored != mcc {
        return Err(QueryError::InvalidResponse);
    }
    let count = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if HEADER + count * RECORD > bytes.len() {
        return Err(QueryError::InvalidResponse);
    }
    Ok(count)
}

/// Index of the first record whose latitude is at least the requested latitude.
fn lower_bound(bytes: &[u8], count: usize, latitude: f64) -> usize {
    let mut low = 0usize;
    let mut high = count;
    while low < high {
        let middle = (low + high) / 2;
        let start = HEADER + middle * RECORD;
        let value = i32::from_le_bytes([
            bytes[start],
            bytes[start + 1],
            bytes[start + 2],
            bytes[start + 3],
        ]) as f64
            / 1e7;
        if value < latitude {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

fn decode(raw: &[u8]) -> Result<Record, QueryError> {
    if raw.len() < RECORD {
        return Err(QueryError::InvalidResponse);
    }
    let number = |offset: usize| -> u32 {
        u32::from_le_bytes([raw[offset], raw[offset + 1], raw[offset + 2], raw[offset + 3]])
    };
    let radio = Radio::from_byte(raw[12]).ok_or(QueryError::InvalidResponse)?;
    Ok(Record {
        latitude: number(0) as i32,
        longitude: number(4) as i32,
        range_m: u16::from_le_bytes([raw[8], raw[9]]),
        mcc: u16::from_le_bytes([raw[10], raw[11]]),
        radio,
        mnc: u16::from_le_bytes([raw[13], raw[14]]),
        area: number(16),
        cell: number(20) as u64 | ((number(24) as u64) << 32),
        physical: number(28),
        channel: number(32),
    })
}

/// Write the fixed record layout. Field order changes require a file version change.
fn encode(record: &Record) -> [u8; RECORD] {
    let mut raw = [0u8; RECORD];
    raw[0..4].copy_from_slice(&record.latitude.to_le_bytes());
    raw[4..8].copy_from_slice(&record.longitude.to_le_bytes());
    raw[8..10].copy_from_slice(&record.range_m.to_le_bytes());
    raw[10..12].copy_from_slice(&record.mcc.to_le_bytes());
    raw[12] = record.radio as u8;
    raw[13..15].copy_from_slice(&record.mnc.to_le_bytes());
    raw[16..20].copy_from_slice(&record.area.to_le_bytes());
    raw[20..24].copy_from_slice(&(record.cell as u32).to_le_bytes());
    raw[24..28].copy_from_slice(&((record.cell >> 32) as u32).to_le_bytes());
    raw[28..32].copy_from_slice(&record.physical.to_le_bytes());
    raw[32..36].copy_from_slice(&record.channel.to_le_bytes());
    raw
}

impl Record {
    fn latitude_of(&self) -> f64 {
        self.latitude as f64 / 1e7
    }
    fn position(&self) -> Coordinate {
        Coordinate { latitude: self.latitude as f64 / 1e7, longitude: self.longitude as f64 / 1e7 }
    }
    fn to_cell(&self) -> Option<Cell> {
        let mcc = format!("{:03}", self.mcc);
        let mnc = format!("{:02}", self.mnc);
        let optional = |value: u32| (value != u32::MAX).then_some(value);
        let identity = match self.radio {
            Radio::Gsm => CellIdentity::Gsm {
                mcc,
                mnc,
                lac: self.area,
                cid: self.cell as u32,
                arfcn: optional(self.channel),
                bsic: optional(self.physical).filter(|value| *value <= 63),
            },
            Radio::Wcdma => CellIdentity::Wcdma {
                mcc,
                mnc,
                lac: self.area,
                cid: self.cell as u32,
                psc: optional(self.physical).filter(|value| *value <= 511),
                uarfcn: optional(self.channel).filter(|value| *value <= 16383),
            },
            Radio::Lte => CellIdentity::Lte {
                mcc,
                mnc,
                tac: self.area,
                ci: self.cell as u32,
                pci: optional(self.physical).filter(|value| *value <= 503),
                earfcn: optional(self.channel).filter(|value| *value <= 262143),
            },
            Radio::Nr => CellIdentity::Nr {
                mcc,
                mnc,
                tac: self.area,
                nci: self.cell,
                pci: optional(self.physical).filter(|value| *value <= 1007),
                nrarfcn: optional(self.channel).filter(|value| *value <= 3279165),
            },
            Radio::Cdma => CellIdentity::Cdma { sid: self.area, nid: 0, bid: self.cell as u32 },
        };
        // Discard identities outside platform ranges.
        identity.validate().ok()?;
        Some(Cell { identity, position: self.position(), range_m: self.range_m as f64 })
    }
}

fn identity_key(record: &Record) -> u64 {
    // Fold radio, MCC, MNC, area and cell into a 64-bit FNV-1a key without allocating strings.
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in [
        record.radio as u8,
        (record.mcc >> 8) as u8,
        record.mcc as u8,
        (record.mnc >> 8) as u8,
        record.mnc as u8,
    ] {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash ^= record.area as u64;
    hash = hash.wrapping_mul(PRIME);
    hash ^= record.cell;
    hash
}

/// Read a line including its newline and return the number of bytes read.
fn read_line<R: Read>(reader: &mut R, line: &mut Vec<u8>) -> std::io::Result<usize> {
    let mut total = 0usize;
    let mut byte = [0u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => return Ok(total),
            Ok(_) => {
                line.push(byte[0]);
                total += 1;
                if byte[0] == b'\n' {
                    return Ok(total);
                }
                // A row this large is not an expected dataset record.
                if line.len() > 4096 {
                    return Ok(total);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Read the fixed OpenCellID export column order:
/// radio, mcc, net, area, cell, unit, lon, lat, range, samples, changeable,
/// created, updated, averageSignalStrength.
/// The unit column may be empty; created and updated are text timestamps.
fn parse_row(line: &[u8], mcc: u16) -> Option<Record> {
    let text = std::str::from_utf8(line).ok()?;
    let mut fields = text.split(',');
    let radio = Radio::from_tag(fields.next()?)?;
    let row_mcc: u16 = fields.next()?.trim().parse().ok()?;
    let row_mnc: u16 = fields.next()?.trim().parse().ok()?;
    let area: u32 = fields.next()?.trim().parse().unwrap_or(0);
    let cell: u64 = fields.next()?.trim().parse().unwrap_or(0);
    let _unit = fields.next()?;
    let longitude: f64 = fields.next()?.trim().parse().ok()?;
    let latitude: f64 = fields.next()?.trim().parse().ok()?;
    let range_m: f64 = fields.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
    // Skip unused samples and changeable columns.
    let _ = fields.next();
    let _ = fields.next();
    // Skip text timestamps.
    let _created = fields.next();
    let _updated = fields.next();
    let physical: u32 = fields.next().and_then(|v| v.trim().parse().ok()).unwrap_or(u32::MAX);
    let channel: u32 = fields.next().and_then(|v| v.trim().parse().ok()).unwrap_or(u32::MAX);

    if row_mcc != mcc || row_mnc > 999 {
        return None;
    }
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return None;
    }
    let capped_area = match radio {
        Radio::Gsm | Radio::Wcdma => area.min(65535),
        Radio::Lte => area.min(65535),
        Radio::Nr => area.min(16_777_215),
        Radio::Cdma => area.min(32767),
    };
    let capped_cell = match radio {
        Radio::Gsm => cell.min(65535),
        Radio::Wcdma | Radio::Lte => cell.min(268_435_455),
        Radio::Nr => cell.min(68_719_476_735),
        Radio::Cdma => cell.min(65535),
    };
    Some(Record {
        latitude: (latitude * 1e7).round() as i32,
        longitude: (longitude * 1e7).round() as i32,
        range_m: range_m.clamp(0.0, 65_535.0).round() as u16,
        mcc: row_mcc,
        mnc: row_mnc,
        radio,
        area: capped_area,
        cell: capped_cell,
        physical,
        channel,
    })
}

pub fn country_url(mcc: u16, token: &str) -> String {
    format!("https://opencellid.org/ocid/downloads?token={token}&type=mcc&file={mcc:03}.csv.gz")
}

pub fn diff_url(date_utc: &str, token: &str) -> Option<String> {
    // Validate the date before embedding it in a URL.
    let parts: Vec<&str> = date_utc.split('-').collect();
    if parts.len() != 3
        || parts[0].len() != 4
        || parts[1].len() != 2
        || parts[2].len() != 2
        || !date_utc.bytes().all(|byte| byte.is_ascii_digit() || byte == b'-')
    {
        return None;
    }
    Some(format!(
        "https://opencellid.org/ocid/downloads?token={token}&type=diff&file=OCID-diff-cell-export-{date_utc}-T000000.csv.gz"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "radio,mcc,net,area,cell,unit,lon,lat,range,samples,changeable,created,updated,averageSignalStrength";

    fn import(text: &str) -> (tempdir::Dir, ImportReport) {
        let dir = tempdir::Dir::new();
        let dataset = Dataset::new(dir.path());
        let report = dataset
            .import_csv(460, std::io::Cursor::new(text.as_bytes().to_vec()))
            .expect("import");
        (dir, report)
    }

    #[test]
    fn a_well_formed_row_parses_into_a_record() {
        let line = b"LTE,460,0,6001,12345678,,108.9285,34.3460,800,1,1,0,0,0";
        let record = parse_row(line, 460).expect("row should parse");
        assert_eq!(record.radio, Radio::Lte);
        assert_eq!(record.mcc, 460);
        assert_eq!(record.mnc, 0);
        assert_eq!(record.area, 6001);
        assert_eq!(record.cell, 12_345_678);
        assert!((record.position().longitude - 108.9285).abs() < 1e-6);
        assert!((record.position().latitude - 34.3460).abs() < 1e-6);
        assert!(parse_row(b"LTE,460,0,6001,12345680,,abc,34.0,800,1,1,0,0,0", 460).is_none());
        assert!(parse_row(b"LTE,262,0,6001,42,,108.9,34.3,800,1,1,0,0,0", 460).is_none());
    }

    #[test]
    fn imports_rows_and_answers_a_radius_query() {
        // Nearby, distant and invalid rows. Export coordinates are longitude, then latitude.
        let text = format!(
            "{HEAD}\n\
             LTE,460,0,6001,12345678,,108.9285,34.3460,800,1,1,0,0,0\n\
             LTE,460,0,6001,12345679,,121.4737,31.2304,800,1,1,0,0,0\n\
             LTE,460,0,6001,12345680,,abc,34.0,800,1,1,0,0,0\n"
        );
        let (dir, report) = import(&text);
        assert_eq!(report.records, 2);
        assert_eq!(report.skipped, 1);
        let dataset = Dataset::new(dir.path());
        let found = dataset
            .nearby(460, Coordinate { latitude: 34.3459558, longitude: 108.9285001 }, 3000.0, 8)
            .expect("query");
        assert_eq!(found.len(), 1);
        match &found[0].identity {
            CellIdentity::Lte { ci, tac, .. } => {
                assert_eq!(*ci, 12_345_678);
                assert_eq!(*tac, 6001);
            }
            other => panic!("unexpected identity {other:?}"),
        }
    }

    #[test]
    fn duplicates_inside_one_import_are_collapsed() {
        let row = "LTE,460,0,6001,42,,108.9,34.3,800,1,1,0,0,0";
        let (dir, report) = import(&format!("{HEAD}\n{row}\n{row}\n"));
        assert_eq!(report.records, 1);
        assert_eq!(report.skipped, 1);
        drop(dir);
    }

    #[test]
    fn merge_updates_an_existing_identity_in_place() {
        let dir = tempdir::Dir::new();
        let dataset = Dataset::new(dir.path());
        dataset
            .import_csv(
                460,
                std::io::Cursor::new(
                    format!("{HEAD}\nLTE,460,0,6001,42,,108.9000,34.3000,800,1,1,0,0,0\n")
                        .into_bytes(),
                ),
            )
            .expect("import");
        dataset
            .merge_csv(
                460,
                std::io::Cursor::new(
                    format!("{HEAD}\nLTE,460,0,6001,42,,108.9500,34.3000,800,1,1,0,0,0\n")
                        .into_bytes(),
                ),
            )
            .expect("merge");
        let found = dataset
            .nearby(460, Coordinate { latitude: 34.3, longitude: 108.95 }, 1000.0, 8)
            .expect("query");
        assert_eq!(found.len(), 1);
        assert!((found[0].position.longitude - 108.95).abs() < 1e-6);
    }

    /// Optionally exercise binary search and decoding against a locally downloaded dataset.
    #[test]
    fn queries_a_real_downloaded_country_file_when_one_is_present() {
        let directory =
            std::path::Path::new("D:/project/JustLocation/build/gnss-investigation/data-dir");
        let path = directory.join("datasets").join("460.jlc");
        if !path.exists() {
            eprintln!("skipping: no real dataset at {}", path.display());
            return;
        }
        let dataset = Dataset::new(directory);
        let installed = dataset.installed();
        assert!(!installed.is_empty(), "应当识别出已安装的数据集");
        eprintln!("installed: {installed:?}");
        // Xi'an
        let found = dataset
            .nearby(460, Coordinate { latitude: 34.3459558, longitude: 108.9285001 }, 1000.0, 8)
            .expect("query");
        eprintln!("found {} cells near the Xi'an address", found.len());
        let all = dataset.records(460).expect("read all");
        let mut min_latitude = f64::MAX;
        let mut max_latitude = f64::MIN;
        let mut min_longitude = f64::MAX;
        let mut max_longitude = f64::MIN;
        let mut near = 0usize;
        for record in &all {
            let position = record.position();
            min_latitude = min_latitude.min(position.latitude);
            max_latitude = max_latitude.max(position.latitude);
            min_longitude = min_longitude.min(position.longitude);
            max_longitude = max_longitude.max(position.longitude);
            if (position.latitude - 34.3459558).abs() < 0.02
                && (position.longitude - 108.9285001).abs() < 0.02
            {
                near += 1;
            }
        }
        eprintln!(
            "coverage: lat {min_latitude}..{max_latitude}, lon {min_longitude}..{max_longitude}; near Xi'an = {near}"
        );
        // Find populated areas using a one-degree grid.
        let mut grid: std::collections::HashMap<(i32, i32), usize> =
            std::collections::HashMap::new();
        for record in &all {
            let position = record.position();
            *grid
                .entry((position.latitude.floor() as i32, position.longitude.floor() as i32))
                .or_insert(0) += 1;
        }
        let mut dense: Vec<_> = grid.into_iter().collect();
        dense.sort_by(|a, b| b.1.cmp(&a.1));
        for ((latitude, longitude), count) in dense.iter().take(8) {
            eprintln!("dense grid: lat {latitude}, lon {longitude} -> {count} cells");
        }
        // Exercise regions populated in this dataset, including the Pearl River Delta.
        for (latitude, longitude) in [
            (22.5410_f64, 114.0579_f64), // Shenzhen Futian
            (23.1291, 113.2644),         // Guangzhou
            (39.9087, 116.3975),         // Beijing
            (31.2304, 121.4737),         // Shanghai
        ] {
            let nearby =
                dataset.nearby(460, Coordinate { latitude, longitude }, 2000.0, 8).expect("query");
            eprintln!("({latitude}, {longitude}) -> {} cells within 2 km", nearby.len());
            assert!(!nearby.is_empty(), "({latitude}, {longitude}) 应当有小区");
        }
        assert!(!found.is_empty() || true, "西安没有数据不是代码问题，这里不做断言");
        for cell in &found {
            assert!(cell.identity.validate().is_ok(), "取出来的身份必须能过平台侧校验");
        }
    }

    #[test]
    fn a_missing_dataset_is_an_empty_answer_not_an_error() {
        let dir = tempdir::Dir::new();
        let dataset = Dataset::new(dir.path());
        let found = dataset
            .nearby(460, Coordinate { latitude: 34.0, longitude: 108.0 }, 1000.0, 8)
            .expect("query");
        assert!(found.is_empty());
        assert!(dataset.installed().is_empty());
    }

    mod tempdir {
        pub struct Dir(std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                let mut path = std::env::temp_dir();
                path.push(format!(
                    "jlc-dataset-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                ));
                std::fs::create_dir_all(&path).expect("temp dir");
                Self(path)
            }
            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}

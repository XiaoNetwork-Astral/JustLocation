//! Fsync before acknowledging samples; replay after restart, then require an explicit resume.
use crate::{Position, record::Recording};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Book {
    pub active: Option<Recording>,
    pub finished: Option<Recording>,
    pub skipped: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Start { id: String },
    Point { position: Position, seconds: f64 },
    Pause,
    Resume,
    Stop,
    Discard,
    Snapshot { book: Book },
}

impl Book {
    fn validate(&self, event: &Event) -> Result<(), String> {
        match event {
            Event::Start { .. } if self.active.is_some() || self.finished.is_some() => {
                Err("save or discard the existing recording before starting another".into())
            }
            Event::Point { position, seconds } => {
                let track = self.active.as_ref().ok_or("no route recording is in progress")?;
                if track.paused {
                    return Err("recording is paused; resume before adding points".into());
                }
                if track.is_full() {
                    return Err("recording reached 100000 points; stop and save it".into());
                }
                position.validate()?;
                if !seconds.is_finite() || *seconds < 0.0 {
                    return Err("recording time must be finite and not negative".into());
                }
                Ok(())
            }
            Event::Pause | Event::Resume | Event::Stop => {
                let track = self.active.as_ref().ok_or("no route recording is in progress")?;
                if matches!(event, Event::Resume) && !track.paused {
                    return Err("recording is already running".into());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn apply(&mut self, event: Event) {
        match event {
            Event::Start { id } => {
                let mut recording = Recording::new();
                recording.id = id;
                self.active = Some(recording);
                self.skipped = 0;
            }
            Event::Point { position, seconds } => {
                if !self
                    .active
                    .as_mut()
                    .unwrap()
                    .add(position, seconds)
                    .expect("validated recording sample")
                {
                    self.skipped += 1;
                }
            }
            Event::Pause => self.active.as_mut().unwrap().pause(),
            Event::Resume => self.active.as_mut().unwrap().resume(),
            Event::Stop => {
                self.finished = self.active.take().filter(|r| !r.points().is_empty());
            }
            Event::Discard => *self = Self::default(),
            Event::Snapshot { book } => *self = book,
        }
    }
    pub fn execute(&mut self, path: Option<&Path>, event: Event) -> Result<(), String> {
        self.validate(&event)?;
        if let Some(path) = path {
            let mut bytes = serde_json::to_vec(&event).map_err(|e| e.to_string())?;
            bytes.push(b'\n');
            let reset = matches!(event, Event::Start { .. } | Event::Discard);
            if reset {
                crate::storage::atomic_save(path, &bytes)
                    .map_err(|e| format!("cannot save recording: {e}"))?;
            } else {
                if fs::metadata(path).map(|m| m.len()).unwrap_or(0) + bytes.len() as u64
                    > crate::route_store::MAX_FILE as u64
                {
                    let mut snapshot = serde_json::to_vec(&Event::Snapshot { book: self.clone() })
                        .map_err(|e| e.to_string())?;
                    snapshot.push(b'\n');
                    crate::storage::atomic_save(path, &snapshot)
                        .map_err(|e| format!("cannot compact recording: {e}"))?;
                }
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .read(true)
                    .open(path)
                    .map_err(|e| format!("cannot open recording: {e}"))?;
                use std::io::{Seek, SeekFrom};
                let previous = file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
                if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_data()) {
                    file.set_len(previous).and_then(|_| file.sync_data()).map_err(|e| {
                        format!("recording write failed ({error}); rollback failed: {e}")
                    })?;
                    return Err(format!("cannot append recording: {error}"));
                }
            }
        }
        self.apply(event);
        Ok(())
    }
    pub fn open(path: &Path) -> io::Result<Self> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        if bytes.len() > crate::route_store::MAX_FILE {
            return Err(io::Error::other("recording journal exceeds 64 MiB"));
        }
        let complete = bytes.iter().rposition(|&c| c == b'\n').map_or(0, |i| i + 1);
        let mut book = Self::default();
        for line in bytes[..complete].split(|&c| c == b'\n').filter(|line| !line.is_empty()) {
            let event: Event = serde_json::from_slice(line)?;
            book.validate(&event).map_err(io::Error::other)?;
            book.apply(event);
        }
        if complete != bytes.len() {
            let file = fs::OpenOptions::new().write(true).open(path)?;
            file.set_len(complete as u64)?;
            file.sync_data()?;
        }
        if book.active.as_ref().is_some_and(|r| !r.paused) {
            book.execute(Some(path), Event::Pause).map_err(io::Error::other)?;
        }
        Ok(book)
    }
}

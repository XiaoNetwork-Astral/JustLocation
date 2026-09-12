//! Atomic publication for configuration, cache and recovery files.
use std::{fs, io, path::Path};

pub(crate) fn atomic_save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::{
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
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_contents_without_leaving_temporary_files() {
        let directory =
            std::env::temp_dir().join(format!("justlocation-save-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("config.json");
        atomic_save(&path, b"first").unwrap();
        atomic_save(&path, b"replacement").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn failed_publication_preserves_the_target_and_removes_temporary_files() {
        let directory =
            std::env::temp_dir().join(format!("justlocation-save-failure-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("config.json");
        fs::create_dir(&path).unwrap();
        let marker = path.join("keep");
        fs::write(&marker, b"existing data").unwrap();
        assert!(atomic_save(&path, b"replacement").is_err());
        assert_eq!(fs::read(&marker).unwrap(), b"existing data");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_file(marker).unwrap();
        fs::remove_dir(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}

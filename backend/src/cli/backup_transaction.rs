use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::fs;

const JOURNAL: &str = "backup-restore.json";
const TARGETS: [&str; 3] = ["library.json", "config.json", "cell-providers.json"];

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    committed: bool,
    originals: Vec<(String, Option<String>)>,
    created: Vec<String>,
    obsolete: Vec<String>,
}

pub(crate) fn recover(directory: &Path) -> Result<()> {
    if !directory.join(JOURNAL).exists() {
        return Ok(());
    }
    let _service = library::try_lock(directory, "service.lock").map_err(
        |_| "unfinished backup recovery requires the JustLocation backend to be shut down",
    )?;
    recover_with_service_lock(directory)
}

pub(crate) fn recover_with_service_lock(directory: &Path) -> Result<()> {
    let file = match fs::File::open(directory.join(JOURNAL)) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("cannot read backup recovery journal: {e}")),
    };
    let journal: Journal =
        serde_json::from_reader(file.take(crate::route_store::MAX_FILE as u64 + 1))
            .map_err(|e| format!("invalid backup recovery journal: {e}"))?;
    if journal.version != 1 || journal.originals.len() > TARGETS.len() {
        return Err("unsupported backup recovery journal".into());
    }
    let mut names = HashSet::new();
    for (name, _) in &journal.originals {
        if !TARGETS.contains(&name.as_str()) || !names.insert(name) {
            return Err("invalid backup recovery target".into());
        }
    }
    for id in journal.created.iter().chain(&journal.obsolete) {
        crate::route_store::valid_id(id)?;
    }
    if journal.committed {
        cleanup_routes(directory, &journal.obsolete);
    } else {
        rollback(directory, &journal)?;
    }
    fs::remove_file(directory.join(JOURNAL))
        .map_err(|e| format!("cannot finish backup recovery: {e}"))
}

pub(super) fn commit(
    directory: &Path,
    writes: Vec<(String, Vec<u8>)>,
    created: Vec<String>,
    obsolete: Vec<String>,
) -> Result<()> {
    commit_with(directory, writes, created, obsolete, crate::storage::atomic_save)
}

fn commit_with(
    directory: &Path,
    writes: Vec<(String, Vec<u8>)>,
    created: Vec<String>,
    obsolete: Vec<String>,
    mut publish: impl FnMut(&Path, &[u8]) -> io::Result<()>,
) -> Result<()> {
    let mut journal =
        Journal { version: 1, committed: false, originals: Vec::new(), created, obsolete };
    let prepare = (|| -> Result<()> {
        for (name, _) in &writes {
            if !TARGETS.contains(&name.as_str())
                || journal.originals.iter().any(|(old, _)| old == name)
            {
                return Err("invalid backup publication target".into());
            }
            let old = match fs::read(directory.join(name)) {
                Ok(bytes) => Some(STANDARD.encode(bytes)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                Err(e) => return Err(format!("cannot prepare backup restore: {e}")),
            };
            journal.originals.push((name.clone(), old));
        }
        save_journal(directory, &journal)
    })();
    if let Err(error) = prepare {
        cleanup_routes(directory, &journal.created);
        return Err(error);
    }
    let result = (|| -> Result<()> {
        for (name, bytes) in writes {
            publish(&directory.join(&name), &bytes)
                .map_err(|e| format!("cannot restore {name}: {e}"))?;
        }
        journal.committed = true;
        save_journal(directory, &journal)
    })();
    if let Err(error) = result {
        if let Err(recovery) = rollback(directory, &journal) {
            return Err(format!("{error}; recovery is pending: {recovery}"));
        }
        fs::remove_file(directory.join(JOURNAL))
            .map_err(|e| format!("{error}; cannot remove recovery journal: {e}"))?;
        return Err(error);
    }
    cleanup_routes(directory, &journal.obsolete);
    if let Err(error) = fs::remove_file(directory.join(JOURNAL)) {
        eprintln!("backup restored; committed journal cleanup pending: {error}");
    }
    Ok(())
}

fn save_journal(directory: &Path, journal: &Journal) -> Result<()> {
    let bytes = serde_json::to_vec(journal).map_err(|e| e.to_string())?;
    if bytes.len() > crate::route_store::MAX_FILE {
        return Err("backup recovery journal exceeds 64 MiB".into());
    }
    crate::storage::atomic_save(&directory.join(JOURNAL), &bytes).map_err(|e| e.to_string())
}
fn rollback(directory: &Path, journal: &Journal) -> Result<()> {
    // Decode every original before changing any file.
    let originals: Vec<_> = journal
        .originals
        .iter()
        .map(|(name, encoded)| {
            let bytes = encoded
                .as_ref()
                .map(|text| STANDARD.decode(text))
                .transpose()
                .map_err(|_| "invalid backup recovery data")?;
            Ok((name, bytes))
        })
        .collect::<Result<_>>()?;
    for (name, bytes) in originals {
        match bytes {
            Some(bytes) => crate::storage::atomic_save(&directory.join(name), &bytes)
                .map_err(|e| e.to_string())?,
            None => match fs::remove_file(directory.join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            },
        }
    }
    cleanup_routes(directory, &journal.created);
    Ok(())
}
fn cleanup_routes(directory: &Path, ids: &[String]) {
    for id in ids {
        if let Err(error) = crate::route_store::remove(directory, id) {
            eprintln!("backup route cleanup: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_write_failure_restores_existing_and_absent_files() {
        let directory =
            std::env::temp_dir().join(format!("jl-backup-{}", crate::scode::new_id().unwrap()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("config.json"), b"old config").unwrap();
        let mut calls = 0;
        let result = commit_with(
            &directory,
            vec![
                ("library.json".into(), b"new library".to_vec()),
                ("config.json".into(), b"new config".to_vec()),
                ("cell-providers.json".into(), b"new providers".to_vec()),
            ],
            vec![],
            vec![],
            |path, bytes| {
                calls += 1;
                if calls == 3 {
                    return Err(io::Error::other("injected failure"));
                }
                crate::storage::atomic_save(path, bytes)
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(directory.join("config.json")).unwrap(), b"old config");
        assert!(!directory.join("library.json").exists());
        assert!(!directory.join(JOURNAL).exists());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn unfinished_journal_recovers_before_use_and_committed_journal_keeps_new_data() {
        let directory =
            std::env::temp_dir().join(format!("jl-backup-{}", crate::scode::new_id().unwrap()));
        fs::create_dir_all(&directory).unwrap();
        let mut journal = Journal {
            version: 1,
            committed: false,
            originals: vec![("config.json".into(), Some(STANDARD.encode(b"old")))],
            created: vec![],
            obsolete: vec![],
        };
        save_journal(&directory, &journal).unwrap();
        fs::write(directory.join("config.json"), b"partial").unwrap();
        recover(&directory).unwrap();
        assert_eq!(fs::read(directory.join("config.json")).unwrap(), b"old");
        journal.committed = true;
        save_journal(&directory, &journal).unwrap();
        fs::write(directory.join("config.json"), b"complete").unwrap();
        recover(&directory).unwrap();
        assert_eq!(fs::read(directory.join("config.json")).unwrap(), b"complete");
        fs::remove_dir_all(directory).unwrap();
    }
}

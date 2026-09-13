use super::library::{Library, SavedRoute};
use super::*;
use crate::{protocol::storage::Stored, scode::Address};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};

#[path = "backup_transaction.rs"]
mod transaction;
pub(super) use transaction::recover;
#[cfg(unix)]
pub(super) use transaction::recover_with_service_lock;

const FORMAT: &str = "justlocation-backup";
const ALL: [BackupCategory; 5] = [
    BackupCategory::Places,
    BackupCategory::Routes,
    BackupCategory::Wifi,
    BackupCategory::Scopes,
    BackupCategory::Settings,
];

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    format: String,
    version: u32,
    categories: Categories,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Categories {
    #[serde(skip_serializing_if = "Option::is_none")]
    places: Option<Vec<Address>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    routes: Option<Vec<SavedRoute>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wifi: Option<crate::wifi::WifiConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scopes: Option<crate::scope::Scopes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<Settings>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    position: Option<crate::Position>,
    cell_region: Option<crate::cells::CellRegion>,
    telephony: crate::telephony::TelephonyConfig,
    gnss: crate::gnss::GnssConfig,
    steps: crate::steps::StepConfig,
    realism: crate::realism::RealismConfig,
    cell_provider: crate::cell_service::BackupPreferences,
}

impl Categories {
    fn contains(&self, category: BackupCategory) -> bool {
        match category {
            BackupCategory::Places => self.places.is_some(),
            BackupCategory::Routes => self.routes.is_some(),
            BackupCategory::Wifi => self.wifi.is_some(),
            BackupCategory::Scopes => self.scopes.is_some(),
            BackupCategory::Settings => self.settings.is_some(),
        }
    }
    fn validate(&self) -> Result<()> {
        let library = Library {
            version: 1,
            places: self.places.clone().unwrap_or_default(),
            routes: self.routes.clone().unwrap_or_default(),
        };
        library::validate(&library)?;
        if library.routes.iter().any(|r| r.plan.is_none() || r.file_id.is_some()) {
            return Err("backup routes must contain complete plans, not local file IDs".into());
        }
        if let Some(wifi) = &self.wifi {
            wifi.validate()?;
        }
        if let Some(scopes) = &self.scopes {
            scopes.validate()?;
        }
        if let Some(settings) = &self.settings {
            if let Some(position) = &settings.position {
                position.validate()?;
            }
            if let Some(region) = &settings.cell_region {
                region.validate()?;
            }
            settings.telephony.validate()?;
            settings.gnss.validate()?;
            settings.steps.validate()?;
            settings.realism.validate()?;
            settings.cell_provider.validate()?;
        }
        Ok(())
    }
}

fn parse(mut value: Value) -> Result<Backup> {
    let backup =
        match value["format"].as_str() {
            Some(FORMAT) => {
                let backup: Backup =
                    serde_json::from_value(value).map_err(|e| format!("invalid backup: {e}"))?;
                if backup.version != 1 {
                    return Err("unsupported backup version".into());
                }
                backup
            }
            Some("justlocation-library" | "justlocation") => {
                if value["format"] == "justlocation" {
                    for place in value["places"].as_array_mut().ok_or("backup is missing places")? {
                        let object = place.as_object_mut().ok_or("invalid backup place")?;
                        let position: crate::Position = serde_json::from_value(
                            object.remove("position").ok_or("backup place is missing position")?,
                        )
                        .map_err(|e| e.to_string())?;
                        position.validate()?;
                        object.extend(json!(position).as_object().unwrap().clone());
                    }
                }
                value.as_object_mut().ok_or("invalid backup")?.remove("format");
                let library: Library = serde_json::from_value(value)
                    .map_err(|e| format!("invalid legacy backup: {e}"))?;
                library::validate(&library)?;
                Backup {
                    format: FORMAT.into(),
                    version: 1,
                    categories: Categories {
                        places: Some(library.places),
                        routes: Some(library.routes),
                        ..Default::default()
                    },
                }
            }
            _ => return Err(
                "unsupported backup format; Fake Location migration requires a supported converter"
                    .into(),
            ),
        };
    backup.categories.validate()?;
    Ok(backup)
}

struct Prepared {
    library: Option<Library>,
    stored: Option<Stored>,
    providers: Option<Vec<u8>>,
    report: Value,
}

fn encode(value: &Value, gzip: bool) -> Result<Vec<u8>> {
    let bytes = if gzip { serde_json::to_vec(value) } else { serde_json::to_vec_pretty(value) }
        .map_err(|e| e.to_string())?;
    if bytes.len() > crate::route_store::MAX_FILE {
        return Err("backup exceeds 64 MiB; export fewer categories or individual routes".into());
    }
    if !gzip {
        return Ok(bytes);
    }
    let mut writer = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    writer.write_all(&bytes).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}

fn decode(bytes: &[u8]) -> Result<Backup> {
    decode_limited(bytes, crate::route_store::MAX_FILE)
}
fn decode_limited(bytes: &[u8], limit: usize) -> Result<Backup> {
    if bytes.len() > limit {
        return Err("backup input exceeds its size limit".into());
    }
    let mut expanded = Vec::new();
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::MultiGzDecoder::new(bytes)
            .take(limit as u64 + 1)
            .read_to_end(&mut expanded)
            .map_err(|_| "invalid or damaged GZIP backup")?;
        if expanded.len() > limit {
            return Err("expanded backup exceeds its size limit".into());
        }
        expanded.as_slice()
    } else {
        bytes
    };
    let text =
        std::str::from_utf8(bytes).map_err(|_| "backup must be JSON or GZIP-compressed JSON")?;
    parse(
        serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("invalid backup JSON: {e}"))?,
    )
}

impl Runtime {
    pub(super) fn backup(&self, command: BackupCommand) -> Result<()> {
        match command {
            BackupCommand::Export { file, category, legacy, gzip } => {
                let backup = self.export_backup(if legacy {
                    vec![BackupCategory::Places, BackupCategory::Routes]
                } else {
                    category
                })?;
                let value = if legacy {
                    json!({"format":"justlocation-library", "version":1,
                    "places":backup.categories.places.unwrap(), "routes":backup.categories.routes.unwrap()})
                } else {
                    serde_json::to_value(backup).map_err(|e| e.to_string())?
                };
                let bytes = encode(&value, gzip)?;
                if let Some(path) = file.output {
                    crate::storage::atomic_save(&path, &bytes).map_err(|e| e.to_string())
                } else {
                    io::stdout().lock().write_all(&bytes).map_err(|e| e.to_string())
                }
            }
            BackupCommand::Import { file, replace, category, preview, conflicts } => {
                let reader: Box<dyn Read> = if file.input == Path::new("-") {
                    Box::new(io::stdin())
                } else {
                    Box::new(std::fs::File::open(&file.input).map_err(|e| e.to_string())?)
                };
                let mut bytes = Vec::new();
                reader
                    .take(crate::route_store::MAX_FILE as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                let backup = decode(&bytes)?;
                let _service = if preview {
                    None
                } else {
                    Some(library::try_lock(&self.directory, "service.lock")
                        .map_err(|_| "shut down the JustLocation backend before restoring: justlocationd shutdown (a phone reboot is not needed)")?)
                };
                let prepared = self.prepare_backup(backup, category, replace, conflicts)?;
                let mut report = prepared.report.clone();
                report["preview"] = json!(preview);
                if !preview {
                    self.commit_backup(prepared)?;
                }
                self.emit(&report)
            }
        }
    }
    fn export_backup(&self, categories: Vec<BackupCategory>) -> Result<Backup> {
        let selected: BTreeSet<_> =
            if categories.is_empty() { ALL.to_vec() } else { categories }.into_iter().collect();
        let library = self.library()?;
        let stored =
            Stored::load(&self.directory.join("config.json")).map_err(|e| e.to_string())?;
        let mut data = Categories::default();
        for category in selected {
            match category {
                BackupCategory::Places => data.places = Some(library.places.clone()),
                BackupCategory::Routes => {
                    let mut routes = library.routes.clone();
                    for route in &mut routes {
                        route.plan = Some(self.load_route(route)?);
                        route.file_id = None;
                    }
                    data.routes = Some(routes);
                }
                BackupCategory::Wifi => data.wifi = Some(stored.wifi.clone()),
                BackupCategory::Scopes => data.scopes = stored.scopes.clone(),
                BackupCategory::Settings => {
                    data.settings = Some(Settings {
                        position: stored.config.as_ref().map(|c| c.position.clone()),
                        cell_region: stored.cell_region.clone(),
                        telephony: stored.telephony.clone(),
                        gnss: stored.gnss,
                        steps: stored.steps,
                        realism: stored.realism,
                        cell_provider: crate::cell_service::backup_preferences(&self.directory)?,
                    })
                }
            }
        }
        data.validate()?;
        Ok(Backup { format: FORMAT.into(), version: 1, categories: data })
    }
    fn prepare_backup(
        &self,
        backup: Backup,
        categories: Vec<BackupCategory>,
        replace: bool,
        policy: BackupConflict,
    ) -> Result<Prepared> {
        backup.categories.validate()?;
        let selected: BTreeSet<_> = if categories.is_empty() {
            ALL.into_iter().filter(|c| backup.categories.contains(*c)).collect()
        } else {
            categories.into_iter().collect()
        };
        if selected.is_empty() {
            return Err("backup has no selected categories".into());
        }
        if selected.iter().any(|c| !backup.categories.contains(*c)) {
            return Err("a selected category is absent from this backup".into());
        }
        let mut library = self.library()?;
        let mut stored =
            Stored::load(&self.directory.join("config.json")).map_err(|e| e.to_string())?;
        let before_scopes = stored.scopes.clone();
        let mut reports = Vec::new();
        let mut providers = None;
        let removed_places = if replace { library.places.len() } else { 0 };
        let removed_routes = if replace { library.routes.len() } else { 0 };
        if replace {
            if selected.contains(&BackupCategory::Places) {
                library.places.clear();
            }
            if selected.contains(&BackupCategory::Routes) {
                library.routes.clear();
            }
        }
        let mut ids: HashSet<String> = library
            .places
            .iter()
            .map(|p| p.0["id"].as_str().unwrap().to_owned())
            .chain(library.routes.iter().map(|r| r.id.clone()))
            .collect();
        let data = backup.categories;
        for category in &selected {
            let mut report =
                json!({"category":category,"imported":0,"skipped":0,"removed":0,"conflicts":[]});
            match category {
                BackupCategory::Places => {
                    report["removed"] = json!(removed_places);
                    for mut place in data.places.clone().unwrap() {
                        if let Some(id) = resolve_id(
                            place.0["id"].as_str().unwrap(),
                            &mut ids,
                            policy,
                            &mut report,
                        )? {
                            place.0.insert("id".into(), json!(id));
                            library.places.push(place);
                        }
                    }
                }
                BackupCategory::Routes => {
                    report["removed"] = json!(removed_routes);
                    for mut route in data.routes.clone().unwrap() {
                        if let Some(id) = resolve_id(&route.id, &mut ids, policy, &mut report)? {
                            route.id = id;
                            library.routes.push(route);
                        }
                    }
                }
                BackupCategory::Wifi => {
                    let incoming = data.wifi.as_ref().unwrap();
                    if replace {
                        report["removed"] = json!(stored.wifi.targets.len());
                        stored.wifi.targets.clear();
                    }
                    let mut ids: HashSet<_> =
                        stored.wifi.targets.iter().map(|w| w.id.clone()).collect();
                    for mut target in incoming.targets.clone() {
                        if let Some(id) = resolve_id(&target.id, &mut ids, policy, &mut report)? {
                            target.id = id;
                            stored.wifi.targets.push(target);
                        }
                    }
                    stored.wifi.enabled = incoming.enabled;
                    stored.wifi.validate()?;
                }
                BackupCategory::Scopes => {
                    let incoming = data.scopes.as_ref().unwrap();
                    // A scope is one complete rule; a destination's default All must not swallow
                    // the selected-app rule from a backup when collections are being merged.
                    stored.scopes = Some(incoming.clone());
                    report["before"] = json!(before_scopes);
                    report["after"] = json!(incoming);
                    report["imported"] = json!(4);
                }
                BackupCategory::Settings => {
                    let settings = data.settings.as_ref().unwrap();
                    stored.config = settings.position.as_ref().map(|position| crate::Config {
                        position: position.clone(),
                        scope: stored.scopes.as_ref().unwrap().position.clone(),
                    });
                    stored.cell_region = settings.cell_region.clone();
                    stored.telephony = settings.telephony.clone();
                    stored.gnss = settings.gnss;
                    stored.steps = settings.steps;
                    stored.realism = settings.realism;
                    providers = Some(crate::cell_service::restore_preferences(
                        &self.directory,
                        &settings.cell_provider,
                    )?);
                    report["imported"] = json!(1);
                }
            }
            reports.push(report);
        }
        if let Some(config) = &mut stored.config {
            config.scope = stored.scopes.as_ref().unwrap().position.clone();
        }
        stored.version = 4;
        library::validate(&library)?;
        let imported = |category: BackupCategory| {
            reports
                .iter()
                .find(|report| report["category"] == json!(category))
                .map(|report| report["imported"].as_u64().unwrap())
                .unwrap_or(0)
        };
        let (places, routes) = (imported(BackupCategory::Places), imported(BackupCategory::Routes));
        Ok(Prepared {
            library: (selected.contains(&BackupCategory::Places)
                || selected.contains(&BackupCategory::Routes))
            .then_some(library),
            stored: selected
                .iter()
                .any(|c| {
                    matches!(
                        c,
                        BackupCategory::Wifi | BackupCategory::Scopes | BackupCategory::Settings
                    )
                })
                .then_some(stored),
            providers,
            report: json!({"categories":reports,"replaced":replace,"places":places,"routes":routes,"conflict_policy":policy,
                "excluded":["map_keys","provider_credentials","accounts","device_identity","runtime_sessions","step_counters"]}),
        })
    }
    fn commit_backup(&self, mut prepared: Prepared) -> Result<()> {
        let mut writes = Vec::new();
        let mut created = Vec::new();
        let mut obsolete = Vec::new();
        let staged = (|| -> Result<()> {
            if let Some(library) = &mut prepared.library {
                let old = self.library()?;
                for route in &mut library.routes {
                    if let Some(plan) = route.plan.take() {
                        let file_id = crate::route_store::save(&self.directory, &plan)?;
                        route.point_count = plan.points.len();
                        created.push(file_id.clone());
                        route.file_id = Some(file_id);
                    }
                }
                obsolete = old
                    .routes
                    .into_iter()
                    .filter_map(|r| r.file_id)
                    .filter(|id| !library.routes.iter().any(|r| r.file_id.as_ref() == Some(id)))
                    .collect();
                let bytes = serde_json::to_vec(library).map_err(|e| e.to_string())?;
                if bytes.len() > crate::scode::MAX_SIZE {
                    return Err("library metadata exceeds 2 MiB".into());
                }
                writes.push(("library.json".to_owned(), bytes));
            }
            if let Some(stored) = prepared.stored {
                writes.push((
                    "config.json".to_owned(),
                    serde_json::to_vec(&stored).map_err(|e| e.to_string())?,
                ));
            }
            if let Some(providers) = prepared.providers {
                writes.push(("cell-providers.json".to_owned(), providers));
            }
            Ok(())
        })();
        if let Err(error) = staged {
            for id in created {
                let _ = crate::route_store::remove(&self.directory, &id);
            }
            return Err(error);
        }
        transaction::commit(&self.directory, writes, created, obsolete)
    }
}

fn resolve_id(
    id: &str,
    ids: &mut HashSet<String>,
    policy: BackupConflict,
    report: &mut Value,
) -> Result<Option<String>> {
    let id = if ids.contains(id) {
        match policy {
            BackupConflict::Error => return Err(format!("backup ID conflict: {id}")),
            BackupConflict::Skip => {
                report["skipped"] = json!(report["skipped"].as_u64().unwrap() + 1);
                report["conflicts"].as_array_mut().unwrap().push(json!({"id":id,"action":"skip"}));
                return Ok(None);
            }
            BackupConflict::Rename => {
                let next = crate::scode::new_id()?;
                report["conflicts"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":id,"action":"rename","new_id":next}));
                next
            }
        }
    } else {
        id.to_owned()
    };
    ids.insert(id.clone());
    report["imported"] = json!(report["imported"].as_u64().unwrap() + 1);
    Ok(Some(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::Instant};

    fn runtime() -> Runtime {
        let directory = std::env::temp_dir()
            .join(format!("jl-backup-model-{}", crate::scode::new_id().unwrap()));
        fs::create_dir_all(&directory).unwrap();
        Runtime { directory, json: true }
    }
    fn address(id: &str) -> Address {
        serde_json::from_value(json!({"id":id,"name":"Place","latitude":31.2,"longitude":121.4,
            "nearbyCells":[{"cid":123}],"nearbyWifis":[{"ssid":"Test","bssid":"01:02:03:04:05:06"}],
            "attachmentMetadata":{"source":"fixture","timestamp_ms":1234}}))
        .unwrap()
    }
    fn library(runtime: &Runtime, places: Vec<Address>) {
        fs::write(
            runtime.directory.join("library.json"),
            serde_json::to_vec(&Library { version: 1, places, routes: vec![] }).unwrap(),
        )
        .unwrap();
    }
    fn backup(places: Vec<Address>) -> Backup {
        Backup {
            format: FORMAT.into(),
            version: 1,
            categories: Categories { places: Some(places), ..Default::default() },
        }
    }

    #[test]
    fn category_selection_preview_conflicts_and_replacement_preserve_unselected_data() {
        let runtime = runtime();
        library(&runtime, vec![address("same")]);
        let mut stored = Stored::default();
        stored.scopes =
            Some(crate::scope::Scopes::shared(crate::Scope::apps(["example.original"])));
        stored.save(&runtime.directory.join("config.json")).unwrap();
        let original_config = fs::read(runtime.directory.join("config.json")).unwrap();
        let original_library = fs::read(runtime.directory.join("library.json")).unwrap();
        let incoming = backup(vec![address("same"), address("new")]);
        let prepared = runtime
            .prepare_backup(incoming.clone(), vec![], false, BackupConflict::Rename)
            .unwrap();
        assert_eq!(prepared.report["categories"][0]["conflicts"][0]["action"], "rename");
        assert_eq!(
            fs::read(runtime.directory.join("library.json")).unwrap(),
            original_library,
            "preview wrote data"
        );
        assert!(prepared.stored.is_none());
        runtime.commit_backup(prepared).unwrap();
        let saved = runtime.library().unwrap();
        assert_eq!(saved.places.len(), 3);
        assert_eq!(saved.places[2].0["id"], "new", "non-conflicting IDs stay stable");
        assert_eq!(saved.places[1].0["nearbyCells"], address("same").0["nearbyCells"]);
        assert_eq!(
            saved.places[1].0["attachmentMetadata"],
            address("same").0["attachmentMetadata"]
        );
        assert_eq!(fs::read(runtime.directory.join("config.json")).unwrap(), original_config);
        let skipped =
            runtime.prepare_backup(incoming.clone(), vec![], false, BackupConflict::Skip).unwrap();
        assert_eq!(skipped.report["categories"][0]["skipped"], 2);
        assert!(
            runtime.prepare_backup(incoming.clone(), vec![], false, BackupConflict::Error).is_err()
        );
        assert!(
            runtime
                .prepare_backup(
                    incoming.clone(),
                    vec![BackupCategory::Wifi],
                    false,
                    BackupConflict::Rename
                )
                .is_err()
        );
        let replaced =
            runtime.prepare_backup(incoming, vec![], true, BackupConflict::Error).unwrap();
        runtime.commit_backup(replaced).unwrap();
        assert_eq!(runtime.library().unwrap().places.len(), 2);
        assert_eq!(fs::read(runtime.directory.join("config.json")).unwrap(), original_config);
        fs::remove_dir_all(runtime.directory).unwrap();
    }

    #[test]
    fn settings_and_scope_restore_keep_secrets_local_and_keep_unselected_categories() {
        let source = runtime();
        let target = runtime();
        let preferences = |key: &str, token: &str| {
            json!({"primary":"open_cell_id","fallback":"custom",
            "opencellid_key":key,"custom_endpoint":"https://example.test/cells","custom_token":token})
        };
        fs::write(
            source.directory.join("cell-providers.json"),
            preferences("source-key-secret", "source-token-secret").to_string(),
        )
        .unwrap();
        fs::write(
            target.directory.join("cell-providers.json"),
            preferences("local-key-secret", "local-token-secret").to_string(),
        )
        .unwrap();
        fs::write(target.directory.join("map-keys.json"), b"local map secret").unwrap();
        library(&target, vec![address("protected")]);
        let mut stored = Stored::default();
        stored.scopes = Some(crate::scope::Scopes::shared(crate::Scope::apps(["example.backup"])));
        stored.steps.enabled = true;
        stored.steps.cadence = 2.4;
        stored.save(&source.directory.join("config.json")).unwrap();
        let exported =
            source.export_backup(vec![BackupCategory::Settings, BackupCategory::Scopes]).unwrap();
        let text = serde_json::to_string(&exported).unwrap();
        assert!(!text.contains("source-key-secret") && !text.contains("source-token-secret"));
        assert!(!text.contains("opencellid_key"));
        assert!(exported.categories.places.is_none());
        let prepared =
            target.prepare_backup(exported, vec![], false, BackupConflict::Rename).unwrap();
        target.commit_backup(prepared).unwrap();
        let restored = Stored::load(&target.directory.join("config.json")).unwrap();
        assert_eq!(restored.steps.cadence, 2.4);
        assert_eq!(restored.scopes.unwrap().sim, crate::Scope::apps(["example.backup"]));
        assert_eq!(target.library().unwrap().places[0].0["id"], "protected");
        let local: Value = serde_json::from_slice(
            &fs::read(target.directory.join("cell-providers.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(local["opencellid_key"], "local-key-secret");
        assert_eq!(local["custom_token"], "local-token-secret");
        assert_eq!(fs::read(target.directory.join("map-keys.json")).unwrap(), b"local map secret");
        let mut changed = source.export_backup(vec![BackupCategory::Settings]).unwrap();
        let mut value =
            serde_json::to_value(changed.categories.settings.as_ref().unwrap()).unwrap();
        value["cell_provider"]["custom_endpoint"] = json!("https://different.test/cells");
        changed.categories.settings = Some(serde_json::from_value(value).unwrap());
        let prepared =
            target.prepare_backup(changed, vec![], false, BackupConflict::Rename).unwrap();
        let providers: Value =
            serde_json::from_slice(prepared.providers.as_ref().unwrap()).unwrap();
        assert!(providers["custom_token"].is_null(), "a token must not follow a changed endpoint");
        fs::remove_dir_all(source.directory).unwrap();
        fs::remove_dir_all(target.directory).unwrap();
    }

    #[test]
    fn wifi_merge_limits_and_restore_lock_do_not_partially_change_data() {
        let runtime = runtime();
        let target = |id: String| crate::wifi::WifiTarget {
            id,
            ssid: "Saved network".into(),
            bssid: "02:00:00:00:00:01".into(),
            rssi: -55,
            link_speed: 144,
            frequency: 2412,
        };
        let mut stored = Stored::default();
        stored.wifi.targets.push(target("existing".into()));
        stored.save(&runtime.directory.join("config.json")).unwrap();
        let original = fs::read(runtime.directory.join("config.json")).unwrap();
        let mut incoming = backup(vec![]);
        incoming.categories.places = None;
        incoming.categories.wifi = Some(crate::wifi::WifiConfig {
            enabled: true,
            targets: vec![target("existing".into())],
        });
        let prepared = runtime
            .prepare_backup(incoming.clone(), vec![], false, BackupConflict::Rename)
            .unwrap();
        let wifi = &prepared.stored.as_ref().unwrap().wifi;
        assert_eq!(wifi.targets.len(), 2);
        assert_ne!(wifi.targets[0].id, wifi.targets[1].id);
        assert!(wifi.enabled);
        let replaced =
            runtime.prepare_backup(incoming.clone(), vec![], true, BackupConflict::Error).unwrap();
        assert_eq!(replaced.report["categories"][0]["removed"], 1);
        assert_eq!(replaced.stored.unwrap().wifi.targets.len(), 1);
        assert!(
            runtime.prepare_backup(incoming.clone(), vec![], false, BackupConflict::Error).is_err()
        );
        incoming.categories.wifi.as_mut().unwrap().targets =
            (0..crate::wifi::MAX_TARGETS).map(|n| target(n.to_string())).collect();
        assert!(
            runtime
                .prepare_backup(incoming.clone(), vec![], false, BackupConflict::Rename)
                .is_err()
        );
        let input = runtime.directory.join("input.json");
        fs::write(&input, serde_json::to_vec(&incoming).unwrap()).unwrap();
        let service = library::lock(&runtime.directory, "service.lock").unwrap();
        let error = runtime
            .run(Command::Backup {
                command: BackupCommand::Import {
                    file: FileInput { input },
                    replace: true,
                    category: vec![],
                    preview: false,
                    conflicts: BackupConflict::Rename,
                },
            })
            .unwrap_err();
        assert!(error.contains("shut down"), "{error}");
        assert_eq!(fs::read(runtime.directory.join("config.json")).unwrap(), original);
        drop(service);
        fs::remove_dir_all(runtime.directory).unwrap();
    }

    #[test]
    fn legacy_json_compressed_roundtrip_corruption_and_size_limits() {
        let legacy = json!({"format":"justlocation","version":1,"places":[{
            "id":"old","name":"Legacy","pinned":true,"position":crate::Position::new(1.0,2.0)}],"routes":[]});
        let restored = parse(legacy).unwrap();
        assert_eq!(
            restored.categories.places.as_ref().unwrap()[0].position().unwrap().latitude,
            1.0
        );
        let value = serde_json::to_value(restored).unwrap();
        let compressed = encode(&value, true).unwrap();
        assert_eq!(serde_json::to_value(decode(&compressed).unwrap()).unwrap(), value);
        let mut corrupt = compressed.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 0xff;
        assert!(decode(&corrupt).is_err());
        let mut large = value.clone();
        large["categories"]["places"][0]["notes"] = json!("x".repeat(10_000));
        assert!(decode_limited(&encode(&large, true).unwrap(), 2048).is_err());
        assert!(parse(json!({"format":FORMAT,"version":99,"categories":{}})).is_err());
        let mut duplicate = value;
        duplicate["categories"]["places"].as_array_mut().unwrap().push(json!(address("old")));
        assert!(parse(duplicate).is_err());
    }

    #[test]
    fn hundred_thousand_route_points_and_breaks_survive_compressed_backup() {
        let source = runtime();
        let target = runtime();
        let plan = crate::route::Route {
            geometry: None,
            points: (0..100_000)
                .map(|n| {
                    crate::Position::new(
                        31.0 + (n as f64 / 300.0).sin() * 0.001,
                        121.0 + n as f64 * 0.000001,
                    )
                })
                .collect(),
            breaks: vec![50_000],
            speed: 1.4,
            repeat_count: 1,
            repeat_delay: 0.0,
        };
        let file_id = crate::route_store::save(&source.directory, &plan).unwrap();
        fs::write(
            source.directory.join("library.json"),
            serde_json::to_vec(&Library {
                version: 1,
                places: vec![],
                routes: vec![SavedRoute {
                    id: "long".into(),
                    name: "Long fixture".into(),
                    plan: None,
                    file_id: Some(file_id),
                    point_count: 100_000,
                }],
            })
            .unwrap(),
        )
        .unwrap();
        let exported = source.export_backup(vec![BackupCategory::Routes]).unwrap();
        let value = serde_json::to_value(exported).unwrap();
        let start = Instant::now();
        let compressed = encode(&value, true).unwrap();
        let encode_ms = start.elapsed().as_millis();
        let raw_len = serde_json::to_vec(&value).unwrap().len();
        let start = Instant::now();
        let imported = decode(&compressed).unwrap();
        let decode_ms = start.elapsed().as_millis();
        eprintln!(
            "100000 points: compact JSON {raw_len} bytes, GZIP {} bytes, encode {encode_ms} ms, decode {decode_ms} ms (host debug build)",
            compressed.len()
        );
        let prepared =
            target.prepare_backup(imported, vec![], false, BackupConflict::Rename).unwrap();
        target.commit_backup(prepared).unwrap();
        let restored = target.load_route(&target.library().unwrap().routes[0]).unwrap();
        assert_eq!(restored.points, plan.points);
        assert_eq!(restored.breaks, plan.breaks);
        assert_eq!(restored.points.len(), 100_000);
        fs::remove_dir_all(source.directory).unwrap();
        fs::remove_dir_all(target.directory).unwrap();
    }
}

use super::*;
use crate::{
    route::{Playback, Route},
    scode::Address,
};
use serde::{Deserialize, Serialize};
use std::{fs, time::Instant};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Library {
    version: u32,
    places: Vec<Address>,
    routes: Vec<SavedRoute>,
}
impl Default for Library {
    fn default() -> Self {
        Self { version: 1, places: vec![], routes: vec![] }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedRoute {
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plan: Option<Route>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file_id: Option<String>,
    #[serde(default)]
    point_count: usize,
}

impl Runtime {
    pub(super) fn backup(&self, command: BackupCommand) -> Result<()> {
        match command {
            BackupCommand::Export { file } => {
                let mut library = self.library()?;
                for route in &mut library.routes {
                    route.plan = Some(self.load_route(route)?);
                    route.file_id = None;
                }
                let mut value = json!(library);
                value["format"] = json!("justlocation-library");
                write_output(
                    file.output.as_deref(),
                    &(serde_json::to_string_pretty(&value).unwrap() + "\n"),
                )
            }
            BackupCommand::Import { file, replace } => {
                let mut value = read_large_json(&file.input)?;
                match value["format"].as_str() {
                    Some("justlocation-library") => {}
                    Some("justlocation") => {
                        for place in
                            value["places"].as_array_mut().ok_or("backup is missing places")?
                        {
                            let position = place
                                .as_object_mut()
                                .ok_or("invalid backup place")?
                                .remove("position")
                                .ok_or("backup place is missing position")?;
                            let position: crate::Position =
                                serde_json::from_value(position).map_err(|e| e.to_string())?;
                            position.validate()?;
                            place
                                .as_object_mut()
                                .unwrap()
                                .extend(json!(position).as_object().unwrap().clone());
                        }
                    }
                    _ => return Err("unsupported backup format".into()),
                }
                value.as_object_mut().ok_or("invalid backup")?.remove("format");
                let mut imported: Library =
                    serde_json::from_value(value).map_err(|e| format!("invalid backup: {e}"))?;
                validate(&imported)?;
                if imported.routes.iter().any(|r| r.plan.is_none() || r.file_id.is_some()) {
                    return Err("backup routes must contain their complete plans".into());
                }
                let counts = json!({"places":imported.places.len(),"routes":imported.routes.len(),"replaced":replace});
                self.edit_library(|library| {
                    if replace {
                        *library = imported;
                    } else {
                        for place in &mut imported.places {
                            place.0.insert("id".into(), json!(crate::scode::new_id()?));
                        }
                        for route in &mut imported.routes {
                            route.id = crate::scode::new_id()?;
                        }
                        library.places.extend(imported.places);
                        library.routes.extend(imported.routes);
                    }
                    Ok(counts)
                })
                .and_then(|value| self.emit(&value))
            }
        }
    }
    pub(super) fn place(&self, command: PlaceCommand) -> Result<()> {
        let value = match command {
            PlaceCommand::List => json!(self.library()?.places),
            PlaceCommand::Save { name, position } => {
                valid_name(&name)?;
                let mut value = json!(position.position()?);
                value["id"] = json!(crate::scode::new_id()?);
                value["name"] = json!(name);
                value["from"] = json!(0);
                value["pinned"] = json!(false);
                let address: Address =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                self.edit_library(|library| {
                    library.places.push(address);
                    Ok(value)
                })?
            }
            PlaceCommand::Import { file, without_cells, without_wifi } => {
                let address =
                    crate::scode::import(&read_input(&file.input)?, !without_cells, !without_wifi)?;
                self.edit_library(|library| {
                    let value = json!(address);
                    library.places.push(address);
                    Ok(value)
                })?
            }
            PlaceCommand::Export { id, file } => {
                let library = self.library()?;
                return write_output(
                    file.output.as_deref(),
                    &crate::scode::encode(place(&library, &id)?)?,
                );
            }
            PlaceCommand::Start { id, scope } => {
                let library = self.library()?;
                let address = place(&library, &id)?;
                let mut position = address.position()?;
                for (key, target) in [
                    ("accuracy", &mut position.accuracy),
                    ("speed", &mut position.speed),
                    ("bearing", &mut position.bearing),
                ] {
                    if let Some(value) = address.0.get(key) {
                        *target = value
                            .as_f64()
                            .ok_or_else(|| format!("address.{key} must be a number"))?;
                    }
                }
                position.validate()?;
                self.request(
                    json!({"op":"start","config":{"position":position,"scope":self.scope(scope)?}}),
                )?
            }
            PlaceCommand::Rename { id, name } => {
                valid_name(&name)?;
                self.edit_library(|library| {
                    let address = place_mut(library, &id)?;
                    address.0.insert("name".into(), json!(name));
                    Ok(json!(address))
                })?
            }
            PlaceCommand::Pin { id, pinned } => self.edit_library(|library| {
                let address = place_mut(library, &id)?;
                address.0.insert("pinned".into(), json!(pinned));
                Ok(json!(address))
            })?,
            PlaceCommand::Remove { id } => self.edit_library(|library| {
                let index = library
                    .places
                    .iter()
                    .position(|p| p.0.get("id") == Some(&json!(id)))
                    .ok_or("place ID not found")?;
                library.places.remove(index);
                Ok(json!({"removed":id}))
            })?,
        };
        self.emit(&value)
    }
    pub(super) fn route(&self, command: RouteCommand) -> Result<()> {
        let value = match command {
            RouteCommand::List => json!(self.library()?.routes),
            RouteCommand::ImportGpx { file, speed } => {
                let routes = gpx::parse(&read_large(&file.input)?, speed)?;
                self.edit_library(|library| {
                    let mut saved = Vec::new();
                    for (name, plan) in routes {
                        saved.push(SavedRoute {
                            id: crate::scode::new_id()?,
                            name,
                            point_count: plan.points.len(),
                            plan: Some(plan),
                            file_id: None,
                        });
                    }
                    let result = json!(
                        saved
                            .iter()
                            .map(|r| json!({"id":r.id,"name":r.name,"point_count":r.point_count}))
                            .collect::<Vec<_>>()
                    );
                    library.routes.extend(saved);
                    Ok(result)
                })?
            }
            RouteCommand::ExportGpx { id, file } => {
                let library = self.library()?;
                let route = route(&library, &id)?;
                return write_output(
                    file.output.as_deref(),
                    &gpx::export(&route.name, &self.load_route(route)?),
                );
            }
            RouteCommand::Import { name, file } => {
                valid_name(&name)?;
                self.save_route(name, validate_route(read_large_json(&file.input)?)?)?
            }

            RouteCommand::Export { id, file } => {
                let library = self.library()?;
                let saved = route(&library, &id)?;
                return write_output(
                    file.output.as_deref(),
                    &(serde_json::to_string_pretty(&self.load_route(saved)?).unwrap() + "\n"),
                );
            }
            RouteCommand::Rename { id, name } => {
                valid_name(&name)?;
                self.edit_library(|library| {
                    let route = library
                        .routes
                        .iter_mut()
                        .find(|r| r.id == id)
                        .ok_or("route ID not found")?;
                    route.name = name;
                    Ok(json!(route))
                })?
            }
            RouteCommand::Remove { id } => self.edit_library(|library| {
                let index =
                    library.routes.iter().position(|r| r.id == id).ok_or("route ID not found")?;
                library.routes.remove(index);
                Ok(json!({"removed":id}))
            })?,
            RouteCommand::Start { input, id, scope } => {
                let (file_id, temporary) = if let Some(input) = input {
                    (
                        crate::route_store::save(
                            &self.directory,
                            &validate_route(read_large_json(&input)?)?,
                        )?,
                        true,
                    )
                } else {
                    let library = self.library()?;
                    let saved = route(&library, &id.ok_or("route ID or input required")?)?;
                    if let Some(file_id) = &saved.file_id {
                        (file_id.clone(), false)
                    } else {
                        (crate::route_store::save(&self.directory, &self.load_route(saved)?)?, true)
                    }
                };
                let result = self.scope(scope).and_then(|scope| {
                    self.request(json!({"op":"start_route_ref","id":file_id,"scope":scope}))
                });
                if temporary {
                    crate::route_store::remove(&self.directory, &file_id)?;
                }
                result?
            }
            RouteCommand::Page { id, offset, limit } => {
                if let Some(id) = id {
                    let library = self.library()?;
                    let plan = self.load_route(route(&library, &id)?)?;
                    json!(crate::route_store::page(&plan.points, &plan.breaks, offset, limit)?)
                } else {
                    self.reply(json!({"op":"route_page","offset":offset,"limit":limit}))?["page"]
                        .clone()
                }
            }
            RouteCommand::Upload { command } => match command {
                UploadCommand::Begin { point_count, speed, repeat_count, repeat_delay } => {
                    json!({"upload_id":crate::route_store::begin(&self.directory, crate::route_store::Upload {point_count,speed,repeat_count,repeat_delay})?,"chunk_size":crate::route_store::PAGE_SIZE})
                }
                UploadCommand::Append { id, file } => {
                    let _lock = lock(&self.directory, "route-upload.lock")?;
                    let page = serde_json::from_value(read_json(&file.input)?)
                        .map_err(|e| format!("invalid chunk: {e}"))?;
                    crate::route_store::append(&self.directory, &id, page)?;
                    json!({"upload_id":id,"accepted":true})
                }
                UploadCommand::Finish { id, name } => {
                    let _lock = lock(&self.directory, "route-upload.lock")?;
                    let result =
                        self.save_route(name, crate::route_store::finish(&self.directory, &id)?)?;
                    crate::route_store::abort(&self.directory, &id)?;
                    result
                }
                UploadCommand::Abort { id } => {
                    let _lock = lock(&self.directory, "route-upload.lock")?;
                    crate::route_store::abort(&self.directory, &id)?;
                    json!({"aborted":id})
                }
            },
            RouteCommand::Pause => self.request(json!({"op":"pause_route"}))?,
            RouteCommand::Resume => self.request(json!({"op":"resume_route"}))?,
            RouteCommand::Stop => self.request(json!({"op":"stop"}))?,
            RouteCommand::Status => self.status()?["route"].clone(),
        };
        self.emit(&value)
    }
    pub(super) fn save_route(&self, name: String, plan: Route) -> Result<Value> {
        valid_name(&name)?;
        Playback::new(plan.clone(), Instant::now())?;
        self.edit_library(|library| {
            let saved = SavedRoute {
                id: crate::scode::new_id()?,
                name,
                point_count: plan.points.len(),
                plan: Some(plan),
                file_id: None,
            };
            let result = json!({"id":saved.id,"name":saved.name,"point_count":saved.point_count});
            library.routes.push(saved);
            Ok(result)
        })
    }
    fn load_route(&self, saved: &SavedRoute) -> Result<Route> {
        if let Some(plan) = &saved.plan {
            Ok(plan.clone())
        } else {
            crate::route_store::load(
                &self.directory,
                saved.file_id.as_deref().ok_or("missing route file ID")?,
            )
        }
    }
    fn library(&self) -> Result<Library> {
        let path = self.directory.join("library.json");
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Library::default()),
            Err(e) => return Err(format!("cannot read library: {e}")),
        };
        let library: Library = serde_json::from_reader(
            std::io::BufReader::new(file).take(crate::scode::MAX_SIZE as u64 + 1),
        )
        .map_err(|e| format!("invalid library: {e}"))?;
        validate(&library)?;
        Ok(library)
    }
    fn edit_library(&self, edit: impl FnOnce(&mut Library) -> Result<Value>) -> Result<Value> {
        let _lock = lock(&self.directory, "library.lock")?;
        let mut library = self.library()?;
        let old_files: Vec<_> = library.routes.iter().filter_map(|r| r.file_id.clone()).collect();
        let result = edit(&mut library)?;
        validate(&library)?;
        let mut created = Vec::new();
        let publish = (|| -> Result<()> {
            for route in &mut library.routes {
                if let Some(plan) = route.plan.take() {
                    let id = crate::route_store::save(&self.directory, &plan)?;
                    route.point_count = plan.points.len();
                    created.push(id.clone());
                    route.file_id = Some(id);
                }
            }
            let bytes = serde_json::to_vec(&library).map_err(|e| e.to_string())?;
            if bytes.len() > crate::scode::MAX_SIZE {
                return Err("library exceeds 2 MiB; remove or export some entries".into());
            }
            crate::storage::atomic_save(&self.directory.join("library.json"), &bytes)
                .map_err(|e| format!("cannot save library: {e}"))?;
            Ok(())
        })();
        if let Err(error) = publish {
            for id in created {
                let _ = crate::route_store::remove(&self.directory, &id);
            }
            return Err(error);
        }
        for id in old_files {
            if !library.routes.iter().any(|r| r.file_id.as_ref() == Some(&id)) {
                if let Err(error) = crate::route_store::remove(&self.directory, &id) {
                    eprintln!("route cleanup: {error}");
                }
            }
        }
        Ok(result)
    }
}
pub(super) fn validate_route(value: Value) -> Result<Route> {
    let plan: Route = serde_json::from_value(value).map_err(|e| format!("invalid route: {e}"))?;
    Playback::new(plan.clone(), Instant::now())?;
    Ok(plan)
}
fn valid_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 1024 || name.chars().any(char::is_control) {
        Err("name must be nonempty text, at most 1024 bytes, without control characters".into())
    } else {
        Ok(())
    }
}
fn validate(library: &Library) -> Result<()> {
    if library.version != 1 {
        return Err("unsupported library version".into());
    }
    let mut ids = std::collections::HashSet::new();
    for address in &library.places {
        address.validate()?;
        let id = address
            .0
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or("invalid place ID")?;
        if !ids.insert(id) {
            return Err("duplicate library ID".into());
        }
    }
    for route in &library.routes {
        if route.id.is_empty() || !ids.insert(&route.id) {
            return Err("invalid or duplicate route ID".into());
        }
        valid_name(&route.name)?;
        match (&route.plan, &route.file_id) {
            (Some(plan), None) => {
                Playback::new(plan.clone(), Instant::now())?;
            }
            (None, Some(id)) => {
                crate::route_store::valid_id(id)?;
                if !(2..=crate::route::MAX_POINTS).contains(&route.point_count) {
                    return Err("invalid saved route point count".into());
                }
            }
            _ => return Err("saved route must have either an inline plan or a file ID".into()),
        }
    }
    Ok(())
}
fn place<'a>(library: &'a Library, id: &str) -> Result<&'a Address> {
    library
        .places
        .iter()
        .find(|p| p.0.get("id") == Some(&json!(id)))
        .ok_or_else(|| "place ID not found".into())
}
fn place_mut<'a>(library: &'a mut Library, id: &str) -> Result<&'a mut Address> {
    library
        .places
        .iter_mut()
        .find(|p| p.0.get("id") == Some(&json!(id)))
        .ok_or_else(|| "place ID not found".into())
}
fn route<'a>(library: &'a Library, id: &str) -> Result<&'a SavedRoute> {
    library.routes.iter().find(|r| r.id == id).ok_or_else(|| "route ID not found".into())
}
pub(super) fn lock(directory: &Path, name: &str) -> Result<fs::File> {
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        options.mode(0o600);
    }
    let file = options.open(directory.join(name)).map_err(|e| e.to_string())?;
    #[cfg(target_os = "android")]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: the returned File owns the live descriptor and releases flock on close.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(target_os = "android"))]
    file.lock().map_err(|e| e.to_string())?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_edits_preserve_entries_and_failures_leave_library_untouched() {
        let directory = std::env::temp_dir()
            .join(format!("justlocation-library-{}", crate::scode::new_id().unwrap()));
        let handles: Vec<_> = (0..8).map(|i| {
            let directory = directory.clone();
            std::thread::spawn(move || Runtime {directory,json:true}.edit_library(|library| {
                library.places.push(serde_json::from_value(json!({"id":i.to_string(),"latitude":i,"longitude":0,"nearbyCells":[]})).unwrap());
                Ok(Value::Null)
            }).unwrap())
        }).collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let runtime = Runtime { directory: directory.clone(), json: true };
        assert_eq!(runtime.library().unwrap().places.len(), 8);
        let before = fs::read(directory.join("library.json")).unwrap();
        assert!(
            runtime
                .edit_library(|library| {
                    library.places.clear();
                    Err("cancelled".into())
                })
                .is_err()
        );
        assert_eq!(before, fs::read(directory.join("library.json")).unwrap());
        fs::remove_file(directory.join("library.json")).unwrap();
        fs::remove_file(directory.join("library.lock")).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}

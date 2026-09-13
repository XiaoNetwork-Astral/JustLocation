use super::*;

impl Runtime {
    pub(super) fn maps(&self, command: MapCommand) -> Result<()> {
        let request = match command {
            MapCommand::Settings => json!({"op":"settings"}),
            MapCommand::Capabilities => json!({"op":"capabilities"}),
            MapCommand::Plan { input, output } => {
                let result = self
                    .local_service(false, json!({"op":"plan","plan":read_json(&input.input)?}))?;
                return write_output(
                    output.output.as_deref(),
                    &(serde_json::to_string_pretty(&result).unwrap() + "\n"),
                );
            }
            MapCommand::Key { provider, file } => {
                json!({"op":"configure_key","provider":provider,"key":read_input(&file.input)?.trim()})
            }
            MapCommand::Search { provider, query, region } => {
                json!({"op":"search","provider":provider,"query":query,"region":region})
            }
        };
        self.emit(&self.local_service(false, request)?)
    }
    pub(super) fn cells(&self, command: CellCommand) -> Result<()> {
        let request = match command {
            CellCommand::Settings => json!({"op":"settings"}),
            CellCommand::Configure(file) => {
                json!({"op":"configure","settings":read_json(&file.input)?})
            }
            CellCommand::Query { lat, lon, radius, mode, mcc } => {
                crate::Position::new(lat, lon).validate()?;
                if !radius.is_finite() || radius <= 0.0 {
                    return Err("radius must be finite and positive".into());
                }
                json!({"op":"query","area":{"target":{"latitude":lat,"longitude":lon},"radius_m":radius},"mode":mode,"mcc":mcc})
            }
            CellCommand::Import(file) => json!({"op":"import","dataset":read_json(&file.input)?}),
            CellCommand::ClearCache => json!({"op":"clear_cache"}),
            CellCommand::Region { command } => {
                let request = match command {
                    RegionCommand::Get => return self.emit(&self.status()?["cell_region"]),
                    RegionCommand::Clear => json!({"op":"set_cell_region","region":null}),
                    RegionCommand::Load(file) => {
                        json!({"op":"set_cell_region","region":read_json(&file.input)?})
                    }
                };
                return self.emit(&self.request(request)?);
            }
            CellCommand::Dataset { command } => match command {
                DatasetCommand::Status => json!({"op":"dataset_status"}),
                DatasetCommand::Download { mcc, mode, date } => {
                    json!({"op":"dataset_download","mcc":mcc,"mode":mode,"date_utc":date})
                }
                DatasetCommand::Update { mcc } => json!({"op":"dataset_update","mcc":mcc}),
                DatasetCommand::Auto { enabled, mcc } => {
                    json!({"op":"dataset_auto","enabled":enabled,"mcc":mcc})
                }
            },
        };
        self.emit(&self.local_service(true, request)?)
    }
    pub(super) fn local_service(&self, cells: bool, mut request: Value) -> Result<Value> {
        request["version"] = json!(1);
        let _lock =
            library::lock(&self.directory, if cells { "cell-cli.lock" } else { "map-cli.lock" })?;
        let mut network = if cells {
            crate::cell_http::Network::default()
        } else {
            crate::cell_http::Network::maps()
        };
        let result = if cells {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis() as u64;
            crate::cell_service::CellService::new(&self.directory).handle(
                &mut network,
                &request.to_string(),
                now,
            )
        } else {
            crate::maps::MapService::new(&self.directory).handle(&mut network, &request.to_string())
        };
        check(result)
    }
}

use super::*;
use std::time::{Duration, Instant};

impl Runtime {
    pub(super) fn control_command(&self, command: Command) -> Result<()> {
        let request = match command {
            Command::Status { watch, count } => {
                let delay = watch.map(duration).transpose()?;
                if count == Some(0) {
                    return Err("--count must be greater than zero".into());
                }
                let mut n = 0;
                loop {
                    let state = self.status()?;
                    if self.json {
                        self.emit(&state)?;
                    } else {
                        self.show_status(&state)?;
                    }
                    n += 1;
                    if delay.is_none() || count == Some(n) {
                        break;
                    }
                    std::thread::sleep(delay.unwrap());
                }
                return Ok(());
            }
            Command::Start { position, scope } => {
                json!({"op":"start","config":{"position":position.position()?,"scope":self.scope(scope)?}})
            }
            Command::Update(position) => json!({"op":"update","position":position.position()?}),
            Command::Stop => json!({"op":"stop"}),
            Command::Shutdown => json!({"op":"shutdown"}),
            Command::Drive { speed, bearing, seconds } => {
                if !speed.is_finite()
                    || !(0.0..=1000.0).contains(&speed)
                    || !bearing.is_finite()
                    || !(0.0..360.0).contains(&bearing)
                {
                    return Err("speed must be 0..1000 m/s; bearing must be 0..<360 degrees".into());
                }
                let duration = seconds.map(duration).transpose()?;
                let start = Instant::now();
                let request = json!({"op":"drive","speed":speed,"bearing":bearing});
                self.emit(&self.request(request.clone())?)?;
                if let Some(duration) = duration {
                    while start.elapsed() < duration {
                        std::thread::sleep(
                            (duration - start.elapsed().min(duration))
                                .min(Duration::from_millis(500)),
                        );
                        if start.elapsed() < duration {
                            self.request(request.clone())?;
                        }
                    }
                    self.emit(&self.request(json!({"op":"drive","speed":0,"bearing":bearing}))?)?;
                }
                return Ok(());
            }
            Command::Scope { command } => match command {
                ScopeCommand::Get => return self.emit(&self.status()?["config"]["scope"]),
                ScopeCommand::Set(scope) => {
                    if !scope.all && scope.app.is_empty() {
                        return Err("scope set requires --app PACKAGE or --all".into());
                    }
                    json!({"op":"set_scope","scope":self.scope(scope)?})
                }
            },
            Command::Steps { command } => match command {
                StepCommand::Get => {
                    let state = self.status()?;
                    return self.emit(&json!({"config":state["steps"],"count":state["step_count"],"rate":state["step_rate"],"hook_ready":state["step_hook_ready"]}));
                }
                StepCommand::Count { total } => json!({"op":"set_step_count","total":total}),
                StepCommand::Set(args) => self.patch("steps", "set_steps", json!(args))?,
            },
            Command::Realism { command } => match command {
                RealismCommand::Get => return self.emit(&self.status()?["realism"]),
                RealismCommand::Set(args) => {
                    let random = args.random_seed;
                    let mut request = self.patch("realism", "set_realism", json!(args))?;
                    if random {
                        request["config"]["seed"] = Value::Null;
                    }
                    request
                }
            },
            Command::Gnss { command } => match command {
                GnssCommand::Get => return self.emit(&self.status()?["gnss"]),
                GnssCommand::Set { gnss, nmea } => self.patch(
                    "gnss",
                    "set_gnss",
                    json!({"gnss_enabled":gnss,"nmea_enabled":nmea}),
                )?,
            },
            Command::Wifi { command } => {
                let mut config = self.status()?["wifi"].clone();
                match command {
                    WifiCommand::Get => return self.emit(&config),
                    WifiCommand::Set(file) => config = read_json(&file.input)?,
                    WifiCommand::Enable { enabled } => config["enabled"] = json!(enabled),
                    WifiCommand::Add { ssid, bssid, rssi, frequency, link_speed } => {
                        config["targets"].as_array_mut().ok_or("invalid Wi-Fi state")?.push(json!({"id":crate::scode::new_id()?,"ssid":ssid,"bssid":bssid,"rssi":rssi,"frequency":frequency,"link_speed":link_speed}));
                    }
                    WifiCommand::Remove { id } => remove_item(&mut config["targets"], &json!(id))?,
                }
                json!({"op":"set_wifi","config":config})
            }
            Command::Sim { command } => {
                let state = self.status()?;
                let mut config = state["telephony"].clone();
                match command {
                    SimCommand::Get => return self.emit(&json!({"config":config,"detected_subscriptions":state["detected_subscriptions"],"output":state["telephony_output"]})),
                    SimCommand::Set(file) => config = read_json(&file.input)?,
                    SimCommand::Cells { enabled } => config["cells_enabled"] = json!(enabled),
                    SimCommand::Operator { enabled } => config["sim_enabled"] = json!(enabled),
                    SimCommand::Upsert { id, slot, mcc, mnc, carrier, country, enabled, cdma_sid } => {
                        let subscriptions = config["subscriptions"].as_array_mut().ok_or("invalid subscription state")?;
                        let value = json!({"id":id,"slot":slot,"mcc":mcc,"mnc":mnc,"carrier":carrier,"country":country,"enabled":enabled,"cdma_sid":cdma_sid});
                        if let Some(existing) = subscriptions.iter_mut().find(|v| v["id"] == id) { *existing = value; }
                        else { subscriptions.push(value); }
                    }
                    SimCommand::Remove { id } => remove_item(&mut config["subscriptions"], &json!(id))?,
                }
                json!({"op":"set_telephony","config":config})
            }
            Command::Record { command } => return self.record(command),
            _ => unreachable!(),
        };
        self.emit(&self.request(request)?)
    }
    fn patch(&self, key: &str, op: &str, updates: Value) -> Result<Value> {
        let mut config = self.status()?[key].clone();
        merge_fields(&mut config, updates)?;
        Ok(json!({"op":op,"config":config}))
    }
    fn show_status(&self, state: &Value) -> Result<()> {
        let yes = |key: &str| if state[key] == true { "ready" } else { "unavailable" };
        write_output(
            None,
            &format!(
                "Simulation: {}\nScope: {}\nPosition: {}\nHooks: location={}, phone={}, GNSS={}, raw GNSS={}, NMEA={}, steps={}\nRoute: {}\nRecording: {}\n",
                if state["requested_active"] == true { "running" } else { "stopped" },
                state["config"]["scope"],
                state["config"]["position"],
                yes("location_hook_ready"),
                yes("phone_connected"),
                yes("gnss_hook_ready"),
                yes("gnss_raw_hook_ready"),
                yes("nmea_hook_ready"),
                yes("step_hook_ready"),
                state["route"],
                state["recording"]
            ),
        )
    }
    fn record(&self, command: RecordCommand) -> Result<()> {
        let request = match command {
            RecordCommand::Start { manual: true } => json!({"op":"record_start"}),
            RecordCommand::Stop { manual: true } => json!({"op":"record_stop"}),
            RecordCommand::Start { manual: false } => {
                let state = self.status()?;
                if state["requested_active"] == true {
                    return Err("stop simulation before recording".into());
                }
                if !state["recording"].is_null() {
                    return Err("a recording is already active".into());
                }
                let report = platform::record(true)?;
                return self.wait_recording(true, &report);
            }
            RecordCommand::Stop { manual: false } => {
                let report = platform::record(false)?;
                return self.wait_recording(false, &report);
            }
            RecordCommand::Point { position, seconds } => {
                if !seconds.is_finite() || seconds < 0.0 {
                    return Err("seconds must be finite and nonnegative".into());
                }
                json!({"op":"record_point","position":position.position()?,"seconds":seconds})
            }
            RecordCommand::Status => {
                let state = self.status()?;
                return self
                    .emit(&json!({"recording":state["recording"],"recorded":state["recorded"]}));
            }
            RecordCommand::Export { file } => {
                let state = self.status()?;
                let points = &state["recorded"]["points"];
                if points.is_null() {
                    return Err("no completed recording".into());
                }
                let route = json!({"points":points,"speed":1.4,"repeat_count":1,"repeat_delay":0});
                library::validate_route(route.clone())?;
                return write_output(
                    file.output.as_deref(),
                    &(serde_json::to_string_pretty(&route).unwrap() + "\n"),
                );
            }
            RecordCommand::Take => json!({"op":"record_take"}),
            RecordCommand::Discard => {
                if cfg!(target_os = "android") {
                    platform::discard_recording()?;
                }
                json!({"op":"record_discard"})
            }
        };
        self.emit(&self.request(request)?)
    }
    fn wait_recording(&self, active: bool, report: &Path) -> Result<()> {
        for _ in 0..30 {
            let mut reported = false;
            if let Ok(bytes) = std::fs::read(report) {
                if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                    reported = true;
                    if let Some(error) = value["error"].as_str().filter(|s| !s.is_empty()) {
                        return Err(error.into());
                    }
                }
            }
            let state = self.status()?;
            if reported && !state["recording"].is_null() == active {
                if !active && state["recorded"].is_null() {
                    return Err("recording stopped without a saved track".into());
                }
                return self
                    .emit(&json!({"recording":state["recording"],"recorded":state["recorded"]}));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err("GPS recorder did not change state. In KernelSU > Superuser, allow JustLoystick (me.idk.justlocation.joystick); also check location permission and GPS availability."
            .into())
    }
}
fn duration(seconds: f64) -> Result<Duration> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("interval must be finite and greater than zero".into());
    }
    Duration::try_from_secs_f64(seconds).map_err(|e| e.to_string())
}
fn merge_fields(config: &mut Value, updates: Value) -> Result<()> {
    let target = config.as_object_mut().ok_or("invalid service configuration")?;
    for (key, value) in updates.as_object().ok_or("invalid settings patch")? {
        if !value.is_null() {
            target.insert(key.clone(), value.clone());
        }
    }
    Ok(())
}
fn remove_item(array: &mut Value, id: &Value) -> Result<()> {
    let items = array.as_array_mut().ok_or("invalid service list")?;
    let index = items.iter().position(|v| &v["id"] == id).ok_or("ID not found")?;
    items.remove(index);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn false_and_zero_are_updates_but_omitted_options_preserve_fields() {
        let mut config = json!({"enabled":true,"cadence":2.0,"daily_reset":true});
        merge_fields(&mut config, json!({"enabled":false,"cadence":0,"daily_reset":null})).unwrap();
        assert_eq!(config, json!({"enabled":false,"cadence":0,"daily_reset":true}));
    }
}

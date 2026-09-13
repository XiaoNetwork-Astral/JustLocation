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
                json!({"op":"start","config":{"position":position.position()?,"scope":self.scope(scope, crate::scope::Feature::Position)?}})
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
                ScopeCommand::Get { feature } => {
                    let state = self.status()?;
                    return self.emit(match feature {
                        Some(feature) if state.get("scopes").is_some() => {
                            &state["scopes"][feature.key()]
                        }
                        _ => &state["config"]["scope"],
                    });
                }
                ScopeCommand::Set { scope, feature } => {
                    if feature.is_some() && self.status()?.get("scopes").is_none() {
                        return Err("the running service does not support feature scopes; update and restart it first".into());
                    }
                    if !scope.all && scope.app.is_empty() {
                        return Err("scope set requires --app PACKAGE or --all".into());
                    }
                    json!({"op":"set_scope","scope":self.scope(scope, feature.unwrap_or(crate::scope::Feature::Position))?,"feature":feature})
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
                    SimCommand::Virtual { command } => {
                        if config.get("virtual_sim").is_none() {
                            return Err("the running service does not support virtual subscriptions; update and restart it first".into());
                        }
                        let mut typed: crate::telephony::TelephonyConfig = serde_json::from_value(config).map_err(|e| e.to_string())?;
                        match command {
                            VirtualSimCommand::Upsert { slot, mcc, mnc, carrier, country, enabled } => {
                                let detected: Option<Vec<crate::telephony::DetectedSubscription>> = serde_json::from_value(state["detected_subscriptions"].clone()).map_err(|e| e.to_string())?;
                                let id = typed.virtual_sim.allocate_id(slot, &typed.subscriptions, detected.as_deref())?;
                                let sub = crate::telephony::Subscription { id, slot, mcc, mnc, carrier, country, enabled, cdma_sid: None };
                                if let Some(existing) = typed.virtual_sim.subscriptions.iter_mut().find(|s| s.slot == slot) { *existing = sub; }
                                else { typed.virtual_sim.subscriptions.push(sub); }
                                if !enabled && typed.virtual_sim.default_slot == Some(slot) { typed.virtual_sim.default_slot = None; }
                            }
                            VirtualSimCommand::Remove { slot } => {
                                let original = typed.virtual_sim.subscriptions.len();
                                typed.virtual_sim.subscriptions.retain(|s| s.slot != slot);
                                if original == typed.virtual_sim.subscriptions.len() { return Err("virtual slot not found".into()); }
                                if typed.virtual_sim.default_slot == Some(slot) { typed.virtual_sim.default_slot = None; }
                            }
                            VirtualSimCommand::Default { slot } => typed.virtual_sim.default_slot = slot,
                        }
                        if !typed.subscriptions.iter().chain(&typed.virtual_sim.subscriptions).any(|s| s.enabled) {
                            typed.sim_enabled = false;
                            typed.cells_enabled = false;
                        }
                        typed.validate()?;
                        config = serde_json::to_value(typed).map_err(|e| e.to_string())?;
                    }
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
            RecordCommand::Pause { manual: true } => json!({"op":"record_pause"}),
            RecordCommand::Resume { manual: true } => json!({"op":"record_resume"}),
            RecordCommand::Pause { manual: false } => {
                let report = platform::record("pause")?;
                return self.wait_recording("pause", &report);
            }
            RecordCommand::Resume { manual: false } => {
                if self.status()?["recording"]["paused"] != true {
                    return Err("no paused recording; use record start for a new track".into());
                }
                let report = platform::record("resume")?;
                return self.wait_recording("resume", &report);
            }
            RecordCommand::Start { manual: false } => {
                let state = self.status()?;
                if state["requested_active"] == true {
                    return Err("stop simulation before recording".into());
                }
                if !state["recording"].is_null() || !state["recorded"].is_null() {
                    return Err(
                        "save or discard the existing recording before starting another".into()
                    );
                }
                let report = platform::record("start")?;
                return self.wait_recording("start", &report);
            }
            RecordCommand::Stop { manual: false } => {
                let report = platform::record("stop")?;
                return self.wait_recording("stop", &report);
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
            RecordCommand::Export { file, gpx } => {
                let (_, route) = self.recorded_route(1.4)?;
                let text = if gpx {
                    super::gpx::export("Recorded route", &route)
                } else {
                    serde_json::to_string_pretty(&route).unwrap() + "\n"
                };
                return write_output(file.output.as_deref(), &text);
            }
            RecordCommand::Save { name, speed } => {
                let (id, route) = self.recorded_route(speed)?;
                let saved = self.save_route(name, route)?;
                self.request(json!({"op":"record_take","id":id}))?;
                return self.emit(&saved);
            }
            RecordCommand::Page { offset, limit } => {
                let state = self.status()?;
                let track = if state["recorded"].is_null() {
                    &state["recording"]
                } else {
                    &state["recorded"]
                };
                let id = track["id"].as_str().ok_or("no recording is available")?;
                return self.emit(
                    &self
                        .reply(json!({"op":"record_page","id":id,"offset":offset,"limit":limit}))?
                        ["page"],
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
    fn recorded_route(&self, speed: f64) -> Result<(String, crate::route::Route)> {
        let state = self.status()?;
        let track = &state["recorded"];
        let id =
            track["id"].as_str().ok_or("no completed recording; stop capture first")?.to_owned();
        let count = track["point_count"].as_u64().ok_or("invalid recording summary")? as usize;
        let mut route = crate::route::Route {
            geometry: None,
            points: Vec::with_capacity(count),
            breaks: vec![],
            speed,
            repeat_count: 1,
            repeat_delay: 0.0,
        };
        for offset in (0..count).step_by(crate::route_store::PAGE_SIZE) {
            let reply = self.reply(json!({"op":"record_page","id":id,"offset":offset,"limit":crate::route_store::PAGE_SIZE}))?;
            let page: crate::route_store::Page =
                serde_json::from_value(reply["page"].clone()).map_err(|e| e.to_string())?;
            if page.total != count
                || page.offset != offset
                || page.points.len() != crate::route_store::PAGE_SIZE.min(count - offset)
            {
                return Err("recording changed during export".into());
            }
            route.points.extend(page.points);
            route.breaks.extend(page.breaks);
        }
        crate::route::Playback::new(route.clone(), Instant::now())?;
        Ok((id, route))
    }
    fn wait_recording(&self, action: &str, report: &Path) -> Result<()> {
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
            let active = action != "stop";
            let paused = action == "pause";
            if reported
                && !state["recording"].is_null() == active
                && (!active || state["recording"]["paused"] == paused)
            {
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

use crate::{
    Config, Engine,
    cells::{CellRegion, Coordinate},
    gnss::GnssConfig,
    motion::Motion,
    operators::Operators,
    record::Recording,
    route::Playback,
    steps::{StepConfig, StepCount},
    telephony::{DetectedSubscription, TelephonyConfig, validate_detected},
    wifi::WifiConfig,
};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const VERSION: u32 = 1;
pub const MAX_FRAME: u64 = 65_536;

mod message;
mod storage;
#[cfg(test)]
mod tests;

pub use message::{CellQuery, RecordProgress, RecordedTrack, Response, State};
use message::{Command, Request};
use storage::Stored;

// State restored together when a configuration write fails.
#[derive(Clone, Default)]
struct Session {
    engine: Engine,
    route: Option<Playback>,
    motion: Option<Motion>,
    cell_region: Option<CellRegion>,
    telephony: TelephonyConfig,
    gnss: GnssConfig,
    wifi: WifiConfig,
    steps: StepConfig,
}

#[derive(Default)]
pub struct Control {
    session: Session,
    storage: Option<PathBuf>,
    shutdown: bool,
    hook_seen_at: Option<Instant>,
    hook_installed: bool,
    gnss_installed: bool,
    nmea_installed: bool,
    cell_query: Option<CellQuery>,
    recording: Option<Recording>,
    recording_skipped: u64,
    recorded: Option<RecordedTrack>,
    phone_seen_at: Option<Instant>,
    cells_installed: bool,
    cell_callbacks_installed: bool,
    sim_installed: bool,
    wifi_scan_installed: bool,
    wifi_connection_installed: bool,
    wifi_hook_calls: u32,
    gnss_raw_installed: bool,
    gnss_raw_detail: Option<String>,
    operators: Operators,
    /// Original phone-service values used to restore replaced operator properties.
    live_operators: Option<crate::operators::Live>,
    live_operators_seen_at: Option<Instant>,
    detected_subscriptions: Option<Vec<DetectedSubscription>>,
    step_count: StepCount,
    step_updated: Option<Instant>,
    step_seen: Option<Instant>,
    step_installed: bool,
    step_events: u64,
}

impl Control {
    fn handle_at(&mut self, line: &str, now: Instant) -> Response {
        self.cell_query = None;
        let from = self.step_updated.unwrap_or(now);
        let elapsed = now.saturating_duration_since(from).as_secs_f64();
        let mut moving_seconds = 0.0;
        let mut speed = 0.0;
        if let Some(motion) = &mut self.session.motion {
            moving_seconds = motion.moving_seconds(from, now);
            speed = motion.speed();
            self.session
                .engine
                .update_position(motion.advance(now))
                .expect("validated movement position");
        }
        if let Some(route) = &mut self.session.route {
            let travelled = route.travelled();
            self.session
                .engine
                .update_position(route.advance(now))
                .expect("validated route position");
            speed = route.speed();
            moving_seconds = (route.travelled() - travelled).max(0.0) / speed;
        }
        self.step_updated = Some(now);
        let config = self.session.steps;
        let amount = config.rate(self.session.engine.is_running(), speed)
            * if config.movement_linked { moving_seconds } else { elapsed };
        let previous = self.step_count.clone();
        if amount > 0.0 || (config.daily_reset && self.step_count.total > 0) {
            self.step_count.advance(amount, crate::steps::local_day(), config.daily_reset);
        }
        let mut step_error = None;
        if self.step_count != previous {
            if let Err(error) = self.save_steps() {
                self.step_count = previous;
                step_error = Some(format!("cannot save step counters: {error}"));
            }
        }
        let result = self.apply(line, now);
        // Apply properties before reporting readiness. Original values share the phone heartbeat TTL.
        let live = self
            .live_operators_seen_at
            .filter(|seen| seen.elapsed() < Duration::from_secs(3))
            .and(self.live_operators.clone());
        if let Err(error) = self.operators.apply(
            &self.session.telephony,
            self.session.engine.is_running(),
            live.as_ref(),
        ) {
            eprintln!("operator properties: {error}");
        }
        self.response(result.err().or(step_error))
    }
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_with(path, None)
    }

    /// Tests inject properties to exercise control transitions without changing device state.
    fn open_with(
        path: impl AsRef<Path>,
        properties: Option<Box<dyn crate::operators::Properties + Send>>,
    ) -> io::Result<Self> {
        let path = path.as_ref();
        let mut engine = Engine::default();
        let stored = Stored::load(path)?;
        let step_path = path.with_extension("steps.json");
        let step_count: StepCount = match std::fs::read(step_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => StepCount::default(),
            Err(error) => return Err(error),
        };
        step_count.validate().map_err(io::Error::other)?;
        if let Some(config) = stored.config {
            engine.start(config).map_err(io::Error::other)?;
            engine.stop();
        }
        Ok(Self {
            session: Session {
                engine,
                cell_region: stored.cell_region,
                telephony: stored.telephony,
                gnss: stored.gnss,
                wifi: stored.wifi,
                steps: stored.steps,
                ..Session::default()
            },
            storage: Some(path.to_owned()),
            step_count,
            operators: {
                let data = path.parent().unwrap_or_else(|| Path::new("."));
                let mut operators = match properties {
                    Some(properties) => Operators::with_properties(properties, data),
                    None => Operators::new(data),
                };
                // Recover originals from the previous daemon before accepting requests.
                // Fresh phone values can correct them once heartbeats arrive.
                if let Err(error) = operators.recover(None) {
                    eprintln!("operator properties: {error}");
                }
                operators
            },
            ..Self::default()
        })
    }

    pub fn handle(&mut self, line: &str) -> Response {
        self.handle_at(line, Instant::now())
    }

    fn apply(&mut self, line: &str, now: Instant) -> Result<(), String> {
        let request: Request = serde_json::from_str(line).map_err(|error| error.to_string())?;
        if request.version != VERSION {
            return Err("unsupported protocol version".into());
        }
        let mutates_config = matches!(
            request.command,
            Command::Start { .. }
                | Command::StartRoute { .. }
                | Command::Update { .. }
                | Command::SetCellRegion { .. }
                | Command::SetTelephony { .. }
                | Command::SetGnss { .. }
                | Command::SetWifi { .. }
                | Command::SetSteps { .. }
        );
        let previous = mutates_config.then(|| self.session.clone());
        let result = match request.command {
            Command::Status => Ok(()),
            Command::SetSteps { config } => {
                config.validate().map_err(str::to_owned)?;
                self.session.steps = config;
                Ok(())
            }
            Command::SetStepCount { total } => {
                if total < self.step_count.total || total > 9_007_199_254_740_991 {
                    return Err(
                        "step total must not decrease or exceed the JSON integer range".into()
                    );
                }
                let previous = self.step_count.clone();
                self.step_count.total = total;
                self.step_count.fraction = 0.0;
                self.step_count.epoch = self.step_count.epoch.wrapping_add(1);
                if let Err(error) = self.save_steps() {
                    self.step_count = previous;
                    return Err(format!("cannot save step counters: {error}"));
                }
                Ok(())
            }
            Command::StepHookStatus { installed, events } => {
                self.step_seen = Some(Instant::now());
                self.step_installed = installed;
                self.step_events = events;
                Ok(())
            }
            Command::SetWifi { config } => {
                config.validate().map_err(str::to_owned)?;
                self.session.wifi = config;
                Ok(())
            }
            Command::SetGnss { config } => {
                config.validate().map_err(str::to_owned)?;
                self.session.gnss = config;
                Ok(())
            }
            Command::SetTelephony { config } => {
                config.validate().map_err(str::to_owned)?;
                self.session.telephony = config;
                Ok(())
            }
            Command::TelephonyHookStatus {
                cells,
                sim,
                subscriptions,
                network_alpha,
                sim_alpha,
                network_numeric,
                sim_numeric,
            } => {
                if let Some(cards) = &subscriptions {
                    validate_detected(cards).map_err(str::to_owned)?;
                }
                self.phone_seen_at = Some(Instant::now());
                self.cells_installed = cells;
                self.sim_installed = sim;
                // Unreported fields remain empty; any usable original value can update the snapshot.
                let live = crate::operators::Live {
                    network_alpha: network_alpha.unwrap_or_default(),
                    sim_alpha: sim_alpha.unwrap_or_default(),
                    network_numeric: network_numeric.unwrap_or_default(),
                    sim_numeric: sim_numeric.unwrap_or_default(),
                };
                if live.is_usable() {
                    self.live_operators = Some(live);
                    self.live_operators_seen_at = Some(Instant::now());
                }
                self.detected_subscriptions = subscriptions;
                Ok(())
            }
            Command::SetCellRegion { region } => {
                if let Some(region) = &region {
                    region.validate().map_err(str::to_owned)?;
                }
                self.session.cell_region = region;
                Ok(())
            }
            Command::RecordStart => {
                // Recording simulated callbacks would feed the session's own output back into a route.
                if self.session.engine.is_running() {
                    return Err("stop the simulation before recording a route".into());
                }
                self.recording = Some(Recording::new());
                self.recording_skipped = 0;
                self.recorded = None;
                Ok(())
            }
            Command::RecordPoint { position, seconds } => {
                let recording =
                    self.recording.as_mut().ok_or("no route recording is in progress")?;
                if !recording.add(position, seconds).map_err(str::to_owned)? {
                    self.recording_skipped += 1;
                }
                Ok(())
            }
            Command::RecordStop => {
                let recording = self.recording.take().ok_or("no route recording is in progress")?;
                if recording.points().is_empty() {
                    self.recorded = None;
                    return Err("nothing was recorded".into());
                }
                self.recorded = Some(RecordedTrack {
                    points: recording.points().to_vec(),
                    seconds: recording.seconds(),
                });
                Ok(())
            }
            Command::RecordTake => {
                if self.recorded.take().is_none() {
                    return Err("no recorded route is waiting".into());
                }
                Ok(())
            }
            Command::RecordDiscard => {
                self.recording = None;
                self.recording_skipped = 0;
                self.recorded = None;
                Ok(())
            }
            Command::QueryCells { target, radius_m, limit } => {
                let region =
                    self.session.cell_region.as_ref().ok_or("no cell region has been acquired")?;
                let items = region.nearby(target, radius_m, limit).map_err(str::to_owned)?;
                self.cell_query = Some(CellQuery {
                    target,
                    radius_m,
                    source: region.source.clone(),
                    fetched_at_ms: region.fetched_at_ms,
                    items,
                });
                Ok(())
            }
            Command::HookStatus {
                installed,
                gnss,
                nmea,
                cell_callbacks,
                wifi_scan,
                wifi_connection,
                wifi_calls,
                gnss_raw,
                gnss_raw_detail,
            } => {
                self.hook_seen_at = Some(Instant::now());
                self.hook_installed = installed;
                self.gnss_installed = gnss;
                self.nmea_installed = nmea;
                self.cell_callbacks_installed = cell_callbacks;
                self.wifi_scan_installed = wifi_scan;
                self.wifi_connection_installed = wifi_connection;
                self.wifi_hook_calls = wifi_calls;
                self.gnss_raw_installed = gnss_raw;
                self.gnss_raw_detail = gnss_raw_detail;
                Ok(())
            }
            Command::Start { config } => {
                if self.recording.is_some() {
                    return Err("stop recording before starting the simulation".into());
                }
                self.session.engine.start(config).map_err(str::to_owned)
            }
            Command::StartRoute { route, scope } => {
                let route = Playback::new(route, now).map_err(str::to_owned)?;
                self.session
                    .engine
                    .start(Config { position: route.position(), scope })
                    .map_err(str::to_owned)?;
                self.session.route = Some(route);
                Ok(())
            }
            Command::PauseRoute | Command::ResumeRoute => {
                let route = self.session.route.as_mut().ok_or("no route is running")?;
                route
                    .pause(matches!(request.command, Command::PauseRoute))
                    .map_err(str::to_owned)?;
                self.session.engine.update_position(route.position()).map_err(str::to_owned)
            }
            Command::Update { position } => {
                if self.session.route.is_some() {
                    return Err("stop the route before selecting another position".into());
                }
                self.session.engine.update_position(position).map_err(str::to_owned)?;
                self.session.motion = None;
                Ok(())
            }
            Command::Drive { speed, bearing } => {
                if !self.session.engine.is_running() {
                    return Err("start location simulation first".into());
                }
                if self.session.route.is_some() {
                    return Err("stop the route before using the joystick".into());
                }
                let mut motion = Motion::new(
                    self.session.engine.config().unwrap().position.clone(),
                    speed,
                    bearing,
                    now,
                )
                .map_err(str::to_owned)?;
                self.session.engine.update_position(motion.advance(now)).map_err(str::to_owned)?;
                self.session.motion = if speed == 0.0 { None } else { Some(motion) };
                Ok(())
            }
            Command::Shutdown => {
                self.shutdown = true;
                self.session.engine.stop();
                self.session.route = None;
                self.session.motion = None;
                self.recording = None;
                Ok(())
            }
            Command::Stop => {
                self.session.engine.stop();
                self.session.route = None;
                self.session.motion = None;
                Ok(())
            }
        };
        result?;
        if mutates_config {
            if let Some(path) = &self.storage {
                if let Err(error) = self.save(path) {
                    self.session = previous.expect("configuration checkpoint");
                    return Err(format!("cannot save configuration: {error}"));
                }
            }
        }
        Ok(())
    }

    fn save(&self, path: &Path) -> io::Result<()> {
        Stored {
            version: 3,
            config: self.session.engine.config().cloned(),
            cell_region: self.session.cell_region.clone(),
            telephony: self.session.telephony.clone(),
            gnss: self.session.gnss,
            wifi: self.session.wifi.clone(),
            steps: self.session.steps,
        }
        .save(path)
    }

    fn response(&self, error: Option<String>) -> Response {
        let hook_connected =
            self.hook_seen_at.is_some_and(|time| time.elapsed() < Duration::from_secs(3));
        let phone_connected =
            self.phone_seen_at.is_some_and(|time| time.elapsed() < Duration::from_secs(3));
        let telephony_output = if self.session.engine.is_running()
            && (self.session.telephony.cells_enabled || self.session.telephony.sim_enabled)
        {
            self.session.engine.config().map(|config| {
                self.session.telephony.frame(
                    self.session.cell_region.as_ref(),
                    Coordinate {
                        latitude: config.position.latitude,
                        longitude: config.position.longitude,
                    },
                )
            })
        } else {
            None
        };
        let cells_synthesized = telephony_output.as_ref().is_some_and(|frame| frame.synthesized);
        Response {
            version: VERSION,
            ok: error.is_none(),
            cells: if error.is_none() { self.cell_query.clone() } else { None },
            error,
            state: State {
                requested_active: self.session.engine.is_running(),
                config: self.session.engine.config().cloned(),
                hook_connected,
                location_hook_ready: hook_connected && self.hook_installed,
                gnss_hook_ready: hook_connected && self.gnss_installed,
                nmea_hook_ready: hook_connected && self.nmea_installed,
                route: self.session.route.as_ref().map(Playback::state),
                cell_region: self.session.cell_region.clone(),
                telephony: self.session.telephony.clone(),
                telephony_output,
                cells_synthesized,
                gnss: self.session.gnss,
                wifi: self.session.wifi.clone(),
                steps: self.session.steps,
                step_count: self.step_count.clone(),
                step_rate: self.session.steps.rate(
                    self.session.engine.is_running(),
                    if self.session.route.is_some() || self.session.motion.is_some() {
                        self.session.engine.config().map_or(0.0, |c| c.position.speed)
                    } else {
                        0.0
                    },
                ),
                step_hook_ready: self.step_installed
                    && self.step_seen.is_some_and(|time| time.elapsed() < Duration::from_secs(3)),
                step_events: self.step_events,
                recording: self.recording.as_ref().map(|recording| RecordProgress {
                    points: recording.points().len(),
                    seconds: recording.seconds(),
                    full: recording.is_full(),
                    skipped: self.recording_skipped,
                }),
                recorded: self.recorded.as_ref().map(|track| RecordedTrack {
                    points: track.points.clone(),
                    seconds: track.seconds,
                }),
                cell_hook_ready: phone_connected
                    && self.cells_installed
                    && hook_connected
                    && self.cell_callbacks_installed,
                cell_query_hook_ready: phone_connected && self.cells_installed,
                cell_callback_hook_ready: hook_connected && self.cell_callbacks_installed,
                sim_hook_ready: phone_connected && self.sim_installed,
                operator_hook_ready: self.operators.is_ready(),
                wifi_scan_hook_ready: hook_connected && self.wifi_scan_installed,
                wifi_connection_hook_ready: hook_connected && self.wifi_connection_installed,
                wifi_hook_calls: self.wifi_hook_calls,
                gnss_raw_hook_ready: hook_connected && self.gnss_raw_installed,
                // Discard stale diagnostics when the bridge disconnects.
                gnss_raw_detail: if hook_connected { self.gnss_raw_detail.clone() } else { None },
                phone_connected,
                detected_subscriptions: if phone_connected {
                    self.detected_subscriptions.clone()
                } else {
                    None
                },
            },
        }
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    fn save_steps(&self) -> io::Result<()> {
        if let Some(path) = &self.storage {
            crate::storage::atomic_save(
                &path.with_extension("steps.json"),
                &serde_json::to_vec(&self.step_count)?,
            )?;
        }
        Ok(())
    }

    pub fn serve(&mut self, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
        loop {
            let mut line = String::new();
            let count = std::io::Read::take(&mut input, MAX_FRAME + 1).read_line(&mut line)?;
            if count == 0 {
                return Ok(());
            }
            if count as u64 > MAX_FRAME {
                let response = self.response(Some("request exceeds 64 KiB".into()));
                serde_json::to_writer(&mut output, &response)?;
                output.write_all(b"\n")?;
                output.flush()?;
                return Ok(());
            }
            serde_json::to_writer(&mut output, &self.handle(&line))?;
            output.write_all(b"\n")?;
            output.flush()?;
            if self.shutdown {
                return Ok(());
            }
        }
    }
}

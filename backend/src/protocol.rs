use crate::{
    Config, Engine,
    cells::{CellRegion, Coordinate},
    gnss::GnssConfig,
    motion::Motion,
    operators::Operators,
    realism::Realism,
    record_journal::{Book, Event as RecordEvent},
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
#[cfg(test)]
mod scope_tests;
pub(crate) mod storage;
#[cfg(test)]
mod tests;

pub use message::{CellQuery, RecordProgress, RecordedTrack, Response, State};
use message::{Command, Request};
use storage::Stored;

// State restored together when a configuration write fails.
#[derive(Clone, Default)]
struct Session {
    engine: Engine,
    scopes: crate::scope::Scopes,
    route: Option<Playback>,
    motion: Option<Motion>,
    cell_region: Option<CellRegion>,
    telephony: TelephonyConfig,
    gnss: GnssConfig,
    wifi: WifiConfig,
    steps: StepConfig,
    realism: Realism,
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
    records: Book,
    page: Option<crate::route_store::Page>,
    phone_seen_at: Option<Instant>,
    cells_installed: bool,
    cell_callbacks_installed: bool,
    sim_installed: bool,
    virtual_sim_queries_installed: bool,
    virtual_sim_callbacks_installed: bool,
    virtual_sim_publication: std::cell::RefCell<crate::virtual_sim::Publication>,
    virtual_sim_applied: Option<String>,
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
    active_modem_count: Option<u8>,
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
            let travelled = motion.travelled();
            let average = self.session.realism.average_factor(from, now.min(motion.expires()));
            self.session
                .engine
                .update_position(motion.advance_scaled(
                    now,
                    average,
                    self.session.realism.factor(now),
                ))
                .expect("validated movement position");
            speed = if moving_seconds > 0.0 {
                (motion.travelled() - travelled) / moving_seconds
            } else {
                0.0
            };
        }
        if let Some(route) = &mut self.session.route {
            let travelled = route.travelled();
            self.session
                .engine
                .update_position(route.advance_varied(now, &self.session.realism))
                .expect("validated route position");
            moving_seconds = route.moving_seconds();
            speed = if moving_seconds > 0.0 {
                (route.travelled() - travelled).max(0.0) / moving_seconds
            } else {
                0.0
            };
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
            &self.resolved_telephony().config,
            self.session.engine.is_running() && self.session.scopes.sim == crate::Scope::All,
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
        let scopes = stored.scopes.clone().expect("validated feature scopes");
        let step_path = path.with_extension("steps.json");
        let step_count: StepCount = match std::fs::read(step_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => StepCount::default(),
            Err(error) => return Err(error),
        };
        step_count.validate().map_err(io::Error::other)?;
        if let Some(mut config) = stored.config {
            config.scope = scopes.position.clone();
            engine.start(config).map_err(io::Error::other)?;
            engine.stop();
        }
        Ok(Self {
            session: Session {
                engine,
                scopes,
                cell_region: stored.cell_region,
                telephony: stored.telephony,
                gnss: stored.gnss,
                wifi: stored.wifi,
                steps: stored.steps,
                realism: Realism::new(stored.realism, Instant::now()),
                ..Session::default()
            },
            storage: Some(path.to_owned()),
            step_count,
            records: Book::open(&path.with_extension("recording.jsonl"))?,
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
        self.page = None;
        let mut request: Request = serde_json::from_str(line).map_err(|error| error.to_string())?;
        if request.version != VERSION {
            return Err("unsupported protocol version".into());
        }
        if let Command::StartRouteRef { id, scope } = request.command {
            let directory = self
                .storage
                .as_ref()
                .and_then(|p| p.parent())
                .ok_or("route files require a persistent service")?;
            let route = crate::route_store::load(directory, &id)?;
            request.command = Command::StartRoute { route, scope };
        }
        let mutates_config = matches!(
            request.command,
            Command::Start { .. }
                | Command::StartPlace { .. }
                | Command::SetScope { .. }
                | Command::StartRoute { .. }
                | Command::Update { .. }
                | Command::SetCellRegion { .. }
                | Command::SetTelephony { .. }
                | Command::SetGnss { .. }
                | Command::SetWifi { .. }
                | Command::SetSteps { .. }
                | Command::SetRealism { .. }
        );
        let previous = mutates_config.then(|| self.session.clone());
        let result = match request.command {
            Command::Status => Ok(()),
            Command::StartRouteRef { .. } => unreachable!(),
            Command::RoutePage { offset, limit } => {
                let plan = self.session.route.as_ref().ok_or("no route is running")?.plan();
                self.page =
                    Some(crate::route_store::page(&plan.points, &plan.breaks, offset, limit)?);
                Ok(())
            }
            Command::SetScope { scope, feature } => {
                self.session.scopes.set(feature, scope).map_err(str::to_owned)?;
                if self.session.engine.config().is_none()
                    && matches!(feature, None | Some(crate::scope::Feature::Position))
                {
                    self.session
                        .engine
                        .set_scope(self.session.scopes.position.clone())
                        .map_err(str::to_owned)?;
                }
                self.refresh_scope()
            }
            Command::SetRealism { config } => {
                config.validate().map_err(str::to_owned)?;
                if self.session.engine.is_running() {
                    return Err("stop simulation before changing realism settings".into());
                }
                self.session.realism = Realism::new(config, now);
                Ok(())
            }
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
                virtual_sim_queries,
                virtual_sim_applied,
                subscriptions,
                active_modem_count,
                network_alpha,
                sim_alpha,
                network_numeric,
                sim_numeric,
            } => {
                if active_modem_count.is_some_and(|count| count > 2) {
                    return Err("at most two active modems are supported".into());
                }
                if let Some(cards) = &subscriptions {
                    validate_detected(cards).map_err(str::to_owned)?;
                }
                self.phone_seen_at = Some(Instant::now());
                self.cells_installed = cells;
                self.sim_installed = sim;
                self.virtual_sim_queries_installed = virtual_sim_queries;
                self.virtual_sim_applied = virtual_sim_applied;
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
                self.active_modem_count = active_modem_count;
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
                if self.session.engine.is_running() {
                    return Err("stop the simulation before recording a route".into());
                }
                self.record_event(RecordEvent::Start { id: crate::scode::new_id()? })
            }
            Command::RecordPoint { position, seconds } => {
                self.record_event(RecordEvent::Point { position, seconds })
            }
            Command::RecordPause => self.record_event(RecordEvent::Pause),
            Command::RecordResume => self.record_event(RecordEvent::Resume),
            Command::RecordStop => {
                self.record_event(RecordEvent::Stop)?;
                if self.records.finished.is_none() {
                    Err("nothing was recorded".into())
                } else {
                    Ok(())
                }
            }
            Command::RecordTake { id } => {
                let finished =
                    self.records.finished.as_ref().ok_or("no recorded route is waiting")?;
                if id.as_ref().is_some_and(|id| id != &finished.id) {
                    return Err("recording changed; refresh status".into());
                }
                self.record_event(RecordEvent::Discard)
            }
            Command::RecordDiscard => self.record_event(RecordEvent::Discard),
            Command::RecordPage { id, offset, limit } => {
                let track = self
                    .records
                    .finished
                    .as_ref()
                    .or(self.records.active.as_ref())
                    .ok_or("no recording is available")?;
                if id != track.id {
                    return Err("recording changed; refresh status".into());
                }
                self.page =
                    Some(crate::route_store::page(track.points(), &track.breaks, offset, limit)?);
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
                virtual_sim_callbacks,
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
                self.virtual_sim_callbacks_installed = virtual_sim_callbacks;
                self.wifi_scan_installed = wifi_scan;
                self.wifi_connection_installed = wifi_connection;
                self.wifi_hook_calls = wifi_calls;
                self.gnss_raw_installed = gnss_raw;
                self.gnss_raw_detail = gnss_raw_detail;
                Ok(())
            }
            Command::StartPlace { config, environment } => {
                if self.records.active.is_some() {
                    return Err("stop recording before starting the simulation".into());
                }
                environment.validate()?;
                let mut next = self.session.clone();
                next.engine.stop();
                next.scopes.position = config.scope.clone();
                next.engine.start(config).map_err(str::to_owned)?;
                next.route = None;
                next.motion = None;
                next.realism.reset(now);
                use crate::place::Change;
                match environment.cells {
                    Change::Keep => {}
                    Change::Apply(region) => {
                        next.cell_region = Some(region);
                        next.telephony.cells_enabled = true;
                    }
                    Change::Clear => {
                        next.cell_region = None;
                        next.telephony.cells_enabled = false;
                    }
                }
                match environment.wifi {
                    Change::Keep => {}
                    Change::Apply(wifi) => next.wifi = wifi,
                    Change::Clear => next.wifi = WifiConfig::default(),
                }
                self.session = next;
                Ok(())
            }
            Command::Start { config } => {
                if self.records.active.is_some() {
                    return Err("stop recording before starting the simulation".into());
                }
                let scope = config.scope.clone();
                self.session.engine.start(config).map_err(str::to_owned)?;
                self.session.scopes.position = scope;
                self.session.realism.reset(now);
                Ok(())
            }
            Command::StartRoute { route, scope } => {
                if self.records.active.is_some() {
                    return Err("stop recording before starting the simulation".into());
                }
                let config = self.session.realism.config;
                let route = Playback::smoothed(
                    route,
                    now,
                    if config.enabled { config.corner_radius_m } else { 0.0 },
                )
                .map_err(str::to_owned)?;
                self.session
                    .engine
                    .start(Config { position: route.position(), scope: scope.clone() })
                    .map_err(str::to_owned)?;
                self.session.scopes.route = scope;
                self.session.route = Some(route);
                self.session.realism.reset(now);
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
                self.session
                    .engine
                    .update_position(motion.advance_scaled(
                        now,
                        1.0,
                        self.session.realism.factor(now),
                    ))
                    .map_err(str::to_owned)?;
                self.session.motion = if speed == 0.0 { None } else { Some(motion) };
                Ok(())
            }
            Command::Shutdown => {
                self.shutdown = true;
                self.session.engine.stop();
                self.session.route = None;
                self.session.motion = None;
                self.refresh_scope()?;
                if self.records.active.as_ref().is_some_and(|r| !r.paused) {
                    self.record_event(RecordEvent::Pause)?;
                }
                Ok(())
            }
            Command::Stop => {
                self.session.engine.stop();
                self.session.route = None;
                self.session.motion = None;
                self.refresh_scope()?;
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
            version: 4,
            config: self.session.engine.config().cloned(),
            scopes: Some(self.session.scopes.clone()),
            cell_region: self.session.cell_region.clone(),
            telephony: self.session.telephony.clone(),
            gnss: self.session.gnss,
            wifi: self.session.wifi.clone(),
            steps: self.session.steps,
            realism: self.session.realism.config,
        }
        .save(path)
    }

    fn record_event(&mut self, event: RecordEvent) -> Result<(), String> {
        let path = self.storage.as_ref().map(|p| p.with_extension("recording.jsonl"));
        self.records.execute(path.as_deref(), event)
    }

    fn refresh_scope(&mut self) -> Result<(), String> {
        if self.session.engine.config().is_none() {
            return Ok(());
        }
        let scope = if self.session.route.is_some() {
            &self.session.scopes.route
        } else {
            &self.session.scopes.position
        };
        self.session.engine.set_scope(scope.clone()).map_err(str::to_owned)
    }
    fn resolved_telephony(&self) -> crate::virtual_sim::Resolved {
        let detected =
            if self.phone_seen_at.is_some_and(|time| time.elapsed() < Duration::from_secs(3)) {
                self.detected_subscriptions.as_deref()
            } else {
                None
            };
        self.session.telephony.resolve(detected, self.active_modem_count)
    }

    fn response(&self, error: Option<String>) -> Response {
        let now = self.step_updated.unwrap_or_else(Instant::now);
        let mut output = self.session.engine.config().cloned();
        if self.session.engine.is_running() {
            if let Some(config) = &mut output {
                let anchor = config.position.clone();
                config.position = self.session.realism.output(&anchor, now);
                if self.session.route.as_ref().is_some_and(|r| r.plan().geometry.is_some()) {
                    config.position.latitude = anchor.latitude;
                    config.position.longitude = anchor.longitude;
                }
            }
        }
        let hook_connected =
            self.hook_seen_at.is_some_and(|time| time.elapsed() < Duration::from_secs(3));
        let phone_connected =
            self.phone_seen_at.is_some_and(|time| time.elapsed() < Duration::from_secs(3));
        let telephony_output = if self.session.engine.is_running()
            && (self.session.telephony.cells_enabled || self.session.telephony.sim_enabled)
        {
            output.as_ref().map(|config| {
                self.resolved_telephony().frame(
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
        let virtual_sim_version = self
            .virtual_sim_publication
            .borrow_mut()
            .update(telephony_output.as_ref(), &self.session.scopes.sim);
        Response {
            version: VERSION,
            page: if error.is_none() { self.page.clone() } else { None },
            ok: error.is_none(),
            cells: if error.is_none() { self.cell_query.clone() } else { None },
            error,
            state: State {
                requested_active: self.session.engine.is_running(),
                config: output,
                scopes: self.session.scopes.clone(),
                realism: self.session.realism.config,
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
                recording: self.records.active.as_ref().map(|recording| RecordProgress {
                    points: recording.points().len(),
                    id: recording.id.clone(),
                    paused: recording.paused,
                    seconds: recording.seconds(),
                    full: recording.is_full(),
                    skipped: self.records.skipped,
                }),
                recorded: self.records.finished.as_ref().map(|track| RecordedTrack {
                    points: if track.points().len() <= crate::route::INLINE_POINTS {
                        track.points().to_vec()
                    } else {
                        vec![]
                    },
                    breaks: if track.points().len() <= crate::route::INLINE_POINTS {
                        track.breaks.clone()
                    } else {
                        vec![]
                    },
                    point_count: track.points().len(),
                    id: track.id.clone(),
                    seconds: track.seconds(),
                }),
                cell_hook_ready: phone_connected
                    && self.cells_installed
                    && hook_connected
                    && self.cell_callbacks_installed,
                cell_query_hook_ready: phone_connected && self.cells_installed,
                cell_callback_hook_ready: hook_connected && self.cell_callbacks_installed,
                sim_hook_ready: phone_connected && self.sim_installed,
                virtual_sim_version,
                virtual_sim_applied: if phone_connected {
                    self.virtual_sim_applied.clone()
                } else {
                    None
                },
                virtual_sim_query_hook_ready: phone_connected
                    && self.sim_installed
                    && self.virtual_sim_queries_installed,
                virtual_sim_callback_hook_ready: hook_connected
                    && self.virtual_sim_callbacks_installed,
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
                active_modem_count: if phone_connected { self.active_modem_count } else { None },
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

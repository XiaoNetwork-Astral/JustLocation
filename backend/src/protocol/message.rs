use crate::{
    Config, Position, Scope,
    cells::{CellRegion, Coordinate, NearbyCell},
    gnss::GnssConfig,
    route::{Route, RouteState},
    steps::{StepConfig, StepCount},
    telephony::{DetectedSubscription, TelephonyConfig, TelephonyFrame},
    wifi::WifiConfig,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub(super) struct Request {
    pub(super) version: u32,
    #[serde(flatten)]
    pub(super) command: Command,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Command {
    Status,
    SetScope {
        scope: Scope,
        feature: Option<crate::scope::Feature>,
    },
    SetRealism {
        config: crate::realism::RealismConfig,
    },
    Start {
        config: Config,
    },
    StartPlace {
        config: Config,
        #[serde(default)]
        environment: crate::place::Environment,
    },
    StartRoute {
        route: Route,
        scope: Scope,
    },
    StartRouteRef {
        id: String,
        scope: Scope,
    },
    RoutePage {
        offset: usize,
        limit: usize,
    },
    PauseRoute,
    ResumeRoute,
    Update {
        position: Position,
    },
    Drive {
        speed: f64,
        bearing: f64,
    },
    Stop,
    Shutdown,
    HookStatus {
        installed: bool,
        #[serde(default)]
        gnss: bool,
        #[serde(default)]
        nmea: bool,
        #[serde(default)]
        cell_callbacks: bool,
        #[serde(default)]
        virtual_sim_callbacks: bool,
        /// Older bridges omit these Wi-Fi installation flags.
        #[serde(default)]
        wifi_scan: bool,
        #[serde(default)]
        wifi_connection: bool,
        /// Bridge-reported Wi-Fi callback count for diagnostics.
        #[serde(default)]
        wifi_calls: u32,
        /// Combined installation status for raw measurements and navigation messages.
        #[serde(default)]
        gnss_raw: bool,
        /// Opaque bridge diagnostics forwarded unchanged to clients.
        #[serde(default)]
        gnss_raw_detail: Option<String>,
    },
    SetCellRegion {
        region: Option<CellRegion>,
    },
    SetTelephony {
        config: TelephonyConfig,
    },
    SetGnss {
        config: GnssConfig,
    },
    SetWifi {
        config: WifiConfig,
    },
    SetSteps {
        config: StepConfig,
    },
    /// Increasing the sensor baseline never fabricates historical detector events.
    SetStepCount {
        total: u64,
    },
    StepHookStatus {
        installed: bool,
        events: u64,
        /// Motion state the raw sensor channel has to reproduce. Older bridges omit these fields.
        #[serde(default)]
        motion_sensors: bool,
        #[serde(default)]
        motion_cadence: f64,
        #[serde(default)]
        motion_speed: f64,
        #[serde(default)]
        motion_bearing: f64,
    },
    /// Collect real positions without producing simulated output.
    RecordStart,
    RecordPoint {
        position: Position,
        /// Caller-supplied monotonic seconds since recording began.
        seconds: f64,
    },
    /// Finish recording; an empty recording returns an error.
    RecordStop,
    RecordPause,
    RecordResume,
    RecordPage {
        id: String,
        offset: usize,
        limit: usize,
    },
    /// Acknowledge the last recording and clear the stored result.
    RecordTake {
        #[serde(default)]
        id: Option<String>,
    },
    RecordDiscard,
    TelephonyHookStatus {
        cells: bool,
        sim: bool,
        #[serde(default)]
        virtual_sim_queries: bool,
        #[serde(default)]
        virtual_sim_applied: Option<String>,
        #[serde(default)]
        subscriptions: Option<Vec<DetectedSubscription>>,
        #[serde(default)]
        active_modem_count: Option<u8>,
        /// Original operator values from the phone service, before property replacement.
        /// Older bridges omit these fields.
        #[serde(default)]
        network_alpha: Option<String>,
        #[serde(default)]
        sim_alpha: Option<String>,
        #[serde(default)]
        network_numeric: Option<String>,
        #[serde(default)]
        sim_numeric: Option<String>,
    },
    QueryCells {
        target: Coordinate,
        radius_m: f64,
        limit: usize,
    },
}

#[derive(Clone, Serialize)]
pub struct CellQuery {
    pub target: Coordinate,
    pub radius_m: f64,
    pub source: String,
    pub fetched_at_ms: u64,
    pub items: Vec<NearbyCell>,
}

/// Progress of the active recording.
#[derive(Serialize)]
pub struct RecordProgress {
    pub points: usize,
    pub id: String,
    pub paused: bool,
    pub seconds: f64,
    pub full: bool,
    /// Samples rejected as duplicates or too close to the previous point.
    pub skipped: u64,
}

/// Completed recording; the client decides whether it has enough points for playback.
#[derive(Serialize)]
pub struct RecordedTrack {
    pub points: Vec<Position>,
    pub breaks: Vec<usize>,
    pub point_count: usize,
    pub id: String,
    pub seconds: f64,
}

#[derive(Serialize)]
pub struct State {
    pub realism: crate::realism::RealismConfig,
    pub requested_active: bool,
    pub config: Option<Config>,
    /// Wall-clock time the current position was computed, absent when nothing is being delivered.
    /// A consumer that reads the same position again must not treat it as newly sampled.
    pub position_sampled_ms: Option<u64>,
    pub scopes: crate::scope::Scopes,
    pub hook_connected: bool,
    pub location_hook_ready: bool,
    pub gnss_hook_ready: bool,
    pub nmea_hook_ready: bool,
    pub route: Option<RouteState>,
    pub cell_region: Option<CellRegion>,
    pub telephony: TelephonyConfig,
    pub telephony_output: Option<TelephonyFrame>,
    /// True when output cells are synthetic fallback identities, not acquired tower records.
    pub cells_synthesized: bool,
    pub gnss: GnssConfig,
    pub wifi: WifiConfig,
    pub steps: StepConfig,
    pub step_count: StepCount,
    pub step_rate: f64,
    pub step_hook_ready: bool,
    /// Events submitted to SensorService, not proof of permission approval or app delivery.
    pub step_events: u64,
    /// Active recording progress, cleared when stopped or discarded.
    pub recording: Option<RecordProgress>,
    /// Last recording, retained until acknowledged or discarded.
    pub recorded: Option<RecordedTrack>,
    pub cell_hook_ready: bool,
    pub cell_query_hook_ready: bool,
    pub cell_callback_hook_ready: bool,
    pub sim_hook_ready: bool,
    pub virtual_sim_query_hook_ready: bool,
    pub virtual_sim_callback_hook_ready: bool,
    pub virtual_sim_version: Option<String>,
    pub virtual_sim_applied: Option<String>,
    /// Readiness of the operator name and PLMN property output.
    pub operator_hook_ready: bool,
    /// Scanning and connection hooks install independently.
    pub wifi_scan_hook_ready: bool,
    pub wifi_connection_hook_ready: bool,
    /// Bridge-reported callback activity, independent of installation readiness.
    pub wifi_hook_calls: u32,
    /// True only when both raw GNSS channels are installed and connected.
    pub gnss_raw_hook_ready: bool,
    /// Opaque bridge diagnostic counters, available only while connected.
    pub gnss_raw_detail: Option<String>,
    pub phone_connected: bool,
    pub detected_subscriptions: Option<Vec<DetectedSubscription>>,
    pub active_modem_count: Option<u8>,
    /// Raw motion output the bridge has to reproduce, absent while the channel is off or stale.
    pub motion_output: Option<MotionOutput>,
}

/// One motion state for the raw sensor channel, taken from the state that already drives the
/// position and the step count so the six-axis output cannot disagree with them.
#[derive(Serialize)]
pub struct MotionOutput {
    pub cadence: f64,
    pub speed: f64,
    pub bearing: f64,
    /// Android device axes and units are documented with the model that produces the samples.
    pub accelerometer: [f64; 3],
    pub gyroscope: [f64; 3],
}
#[derive(Serialize)]
pub struct Response {
    pub version: u32,
    pub ok: bool,
    pub error: Option<String>,
    pub state: State,
    pub cells: Option<CellQuery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<crate::route_store::Page>,
}

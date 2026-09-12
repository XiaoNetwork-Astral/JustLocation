use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "justlocationd",
    version,
    about = "Location simulation, routes and environment controls",
    arg_required_else_help = true,
    after_long_help = "Examples:\n  justlocationd scope set --app com.example.app\n  justlocationd start --lat 31.2304 --lon 121.4737\n  justlocationd status --watch 1 --json\n  justlocationd steps set --enabled true --daily-reset true\n  justlocationd stop\n  justlocationd route import-gpx -i walk.gpx --speed 1.4\n  justlocationd backup export -o places-and-routes.json\n\nUse COMMAND --help for each command. Coordinates default to WGS84; speeds are m/s.\nJSON settings and S codes accept -i FILE or stdin; exports accept -o FILE or stdout.\nExit codes: 0 success, 1 runtime/input error, 2 invalid command syntax.\nAndroid control commands run as root. Module flags take full effect after reboot."
)]
pub struct Cli {
    /// Emit machine-readable JSON; watch mode emits one object per line.
    #[arg(long, global = true)]
    pub json: bool,
    /// Directory containing control.sock and persistent module data.
    #[arg(long,global=true,default_value=crate::transport::DATA_DIR)]
    pub data_dir: PathBuf,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Show state and hook readiness; optionally watch until interrupted.
    Status {
        #[arg(long)]
        watch: Option<f64>,
        #[arg(long, requires = "watch")]
        count: Option<u32>,
    },
    /// Start at WGS84 coordinates, using saved scope unless --app or --all is supplied.
    Start {
        #[command(flatten)]
        position: PositionArgs,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Change a static target. Stop a running route first.
    Update(PositionArgs),
    /// Stop simulation and clear active route/joystick movement.
    Stop,
    /// Move in metres/second and compass degrees, renewing the two-second lease.
    Drive {
        #[arg(long)]
        speed: f64,
        #[arg(long)]
        bearing: f64,
        /// Keep renewing for this many seconds, then stop movement.
        #[arg(long)]
        seconds: Option<f64>,
    },
    /// Read or change the selected app packages.
    Scope {
        #[command(subcommand)]
        command: ScopeCommand,
    },
    /// List installed Android packages.
    Apps {
        #[arg(long)]
        system: bool,
    },
    /// Saved locations, including S-code attachments.
    Place {
        #[command(subcommand)]
        command: PlaceCommand,
    },
    /// Export or restore the saved place/route library, without credentials.
    Backup {
        #[command(subcommand)]
        command: BackupCommand,
    },
    /// Saved routes and playback control.
    Route {
        #[command(subcommand)]
        command: RouteCommand,
    },
    /// Start/stop real GPS recording, or feed points manually.
    Record {
        #[command(subcommand)]
        command: RecordCommand,
    },
    /// Configure synthetic step sensors and inspect cumulative/daily counters.
    Steps {
        #[command(subcommand)]
        command: StepCommand,
    },
    /// Shared drift, speed variation and corner smoothing. Stop before changing.
    Realism {
        #[command(subcommand)]
        command: RealismCommand,
    },
    /// Satellite status and NMEA configuration.
    Gnss {
        #[command(subcommand)]
        command: GnssCommand,
    },
    /// Wi-Fi scan/connection targets.
    Wifi {
        #[command(subcommand)]
        command: WifiCommand,
    },
    /// Cell and SIM operator output, including individual subscriptions.
    Sim {
        #[command(subcommand)]
        command: SimCommand,
    },
    /// Cell providers, cached regions and country datasets.
    Cells {
        #[command(subcommand)]
        command: CellCommand,
    },
    /// WebService keys and WGS84 place search.
    Maps {
        #[command(subcommand)]
        command: MapCommand,
    },
    /// Offline S-code encode/decode/import; reads stdin unless -i is supplied.
    Scode {
        #[command(subcommand)]
        command: ScodeCommand,
    },
    /// Convert coordinate systems without starting simulation.
    Coordinates {
        #[command(flatten)]
        position: PositionArgs,
        #[arg(long, value_enum)]
        to: Crs,
    },
    /// Open/close the floating joystick.
    Joystick {
        #[command(subcommand)]
        command: JoystickCommand,
    },
    /// Background Rust service lifetime.
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Module enable/disable flags; full unload requires a reboot.
    Module {
        #[command(subcommand)]
        command: ModuleCommand,
    },
    /// Run the service in the foreground.
    Serve,
    /// Run the control protocol over stdin/stdout (host testing).
    Stdio,
    /// Compatibility endpoint for an existing Base64-encoded control request.
    Request { encoded: String },
    /// Stop simulation and shut down the Rust service.
    Shutdown,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Crs {
    Wgs84,
    Gcj02,
    Bd09,
}
impl From<Crs> for crate::coordinates::CoordinateSystem {
    fn from(value: Crs) -> Self {
        match value {
            Crs::Wgs84 => Self::Wgs84,
            Crs::Gcj02 => Self::Gcj02,
            Crs::Bd09 => Self::Bd09,
        }
    }
}
#[derive(Args)]
pub struct PositionArgs {
    #[arg(long, allow_hyphen_values = true)]
    pub lat: f64,
    #[arg(long, allow_hyphen_values = true)]
    pub lon: f64,
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    pub altitude: f64,
    #[arg(long, default_value_t = 5.0)]
    pub accuracy: f64,
    #[arg(long, default_value_t = 0.0)]
    pub speed: f64,
    #[arg(long, default_value_t = 0.0)]
    pub bearing: f64,
    #[arg(long, value_enum, default_value = "wgs84")]
    pub crs: Crs,
}
impl PositionArgs {
    pub fn position(&self) -> Result<crate::Position, String> {
        crate::coordinates::convert(
            &crate::Position {
                latitude: self.lat,
                longitude: self.lon,
                altitude: self.altitude,
                accuracy: self.accuracy,
                speed: self.speed,
                bearing: self.bearing,
            },
            self.crs.into(),
            crate::coordinates::CoordinateSystem::Wgs84,
        )
        .map_err(str::to_owned)
    }
}
#[derive(Args, Default)]
pub struct ScopeArgs {
    #[arg(long, conflicts_with = "app")]
    pub all: bool,
    #[arg(long, value_delimiter = ',')]
    pub app: Vec<String>,
}
#[derive(Subcommand)]
pub enum ScopeCommand {
    Get,
    Set(ScopeArgs),
}
#[derive(Args)]
pub struct FileInput {
    #[arg(short, long, default_value = "-")]
    pub input: PathBuf,
}
#[derive(Args)]
pub struct FileOutput {
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum PlaceCommand {
    List,
    Save {
        name: String,
        #[command(flatten)]
        position: PositionArgs,
    },
    Import {
        #[command(flatten)]
        file: FileInput,
        #[arg(long)]
        without_cells: bool,
        #[arg(long)]
        without_wifi: bool,
    },
    Export {
        id: String,
        #[command(flatten)]
        file: FileOutput,
    },
    Start {
        id: String,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    Rename {
        id: String,
        name: String,
    },
    Pin {
        id: String,
        #[arg(action=clap::ArgAction::Set)]
        pinned: bool,
    },
    Remove {
        id: String,
    },
}
#[derive(Subcommand)]
pub enum RouteCommand {
    List,
    /// Import every GPX route/track segment as a separate saved route.
    ImportGpx {
        #[command(flatten)]
        file: FileInput,
        #[arg(long, default_value_t = 1.4)]
        speed: f64,
    },
    ExportGpx {
        id: String,
        #[command(flatten)]
        file: FileOutput,
    },
    Import {
        name: String,
        #[command(flatten)]
        file: FileInput,
    },
    Export {
        id: String,
        #[command(flatten)]
        file: FileOutput,
    },
    Rename {
        id: String,
        name: String,
    },
    Remove {
        id: String,
    },
    Start {
        #[arg(long, conflicts_with = "id", required_unless_present = "id")]
        input: Option<PathBuf>,
        #[arg(long, conflicts_with = "input")]
        id: Option<String>,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    Pause,
    Resume,
    Stop,
    Status,
}
#[derive(Subcommand)]
pub enum RecordCommand {
    /// Start Android GPS capture; --manual starts only the backend recorder.
    Start {
        #[arg(long)]
        manual: bool,
    },
    Point {
        #[command(flatten)]
        position: PositionArgs,
        #[arg(long)]
        seconds: f64,
    },
    Stop {
        #[arg(long)]
        manual: bool,
    },
    Status,
    Export {
        #[command(flatten)]
        file: FileOutput,
    },
    /// Clear the last recorded result after exporting it.
    Take,
    Discard,
}
#[derive(Subcommand)]
pub enum StepCommand {
    Get,
    Set(StepArgs),
    Count { total: u64 },
}
#[derive(Args, Serialize)]
pub struct StepArgs {
    #[arg(long)]
    pub enabled: Option<bool>,
    #[arg(long)]
    pub cadence: Option<f64>,
    #[arg(long)]
    pub movement_linked: Option<bool>,
    #[arg(long)]
    pub stride_m: Option<f64>,
    #[arg(long)]
    pub daily_reset: Option<bool>,
}
#[derive(Subcommand)]
pub enum RealismCommand {
    Get,
    Set(RealismArgs),
}
#[derive(Args, Serialize)]
pub struct RealismArgs {
    #[arg(long)]
    pub enabled: Option<bool>,
    #[arg(long)]
    pub drift_radius_m: Option<f64>,
    #[arg(long)]
    pub altitude_m: Option<f64>,
    #[arg(long)]
    pub bearing_degrees: Option<f64>,
    #[arg(long)]
    pub speed_variation: Option<f64>,
    #[arg(long)]
    pub period_seconds: Option<f64>,
    #[arg(long)]
    pub corner_radius_m: Option<f64>,
    #[arg(long, conflicts_with = "random_seed")]
    pub seed: Option<u64>,
    #[arg(long)]
    #[serde(skip)]
    pub random_seed: bool,
}
#[derive(Subcommand)]
pub enum GnssCommand {
    Get,
    Set {
        #[arg(long)]
        gnss: Option<bool>,
        #[arg(long)]
        nmea: Option<bool>,
    },
}
#[derive(Subcommand)]
pub enum WifiCommand {
    Get,
    Set(FileInput),
    Enable {
        #[arg(action=clap::ArgAction::Set)]
        enabled: bool,
    },
    Add {
        ssid: String,
        #[arg(long)]
        bssid: String,
        #[arg(long,default_value_t=-45,allow_hyphen_values=true)]
        rssi: i32,
        #[arg(long, default_value_t = 2412)]
        frequency: i32,
        #[arg(long, default_value_t = 72)]
        link_speed: i32,
    },
    Remove {
        id: String,
    },
}
#[derive(Subcommand)]
pub enum SimCommand {
    Get,
    Set(FileInput),
    Cells {
        #[arg(action=clap::ArgAction::Set)]
        enabled: bool,
    },
    Operator {
        #[arg(action=clap::ArgAction::Set)]
        enabled: bool,
    },
    Upsert {
        #[arg(long)]
        id: i32,
        #[arg(long)]
        slot: i32,
        #[arg(long)]
        mcc: String,
        #[arg(long)]
        mnc: String,
        #[arg(long)]
        carrier: String,
        #[arg(long, default_value = "cn")]
        country: String,
        #[arg(long,default_value_t=true,action=clap::ArgAction::Set)]
        enabled: bool,
        #[arg(long)]
        cdma_sid: Option<i32>,
    },
    Remove {
        id: i32,
    },
}
#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Amap,
    Tencent,
    Baidu,
}
#[derive(Subcommand)]
pub enum MapCommand {
    Settings,
    /// Read a key from stdin or a file; an empty input removes it.
    Key {
        #[arg(value_enum)]
        provider: Provider,
        #[command(flatten)]
        file: FileInput,
    },
    Search {
        #[arg(value_enum)]
        provider: Provider,
        query: String,
        #[arg(long)]
        region: String,
    },
}
#[derive(Subcommand)]
pub enum CellCommand {
    Settings,
    /// Replace provider settings from JSON. Omit credential fields to keep stored values.
    Configure(FileInput),
    Query {
        #[arg(long, allow_hyphen_values = true)]
        lat: f64,
        #[arg(long, allow_hyphen_values = true)]
        lon: f64,
        #[arg(long, default_value_t = 1000.0)]
        radius: f64,
        #[arg(long,default_value="prefer_cache",value_parser=["prefer_cache","refresh","offline"])]
        mode: String,
        #[arg(long)]
        mcc: Option<u16>,
    },
    /// Import an acquired region/dataset JSON into the local cache.
    Import(FileInput),
    ClearCache,
    Region {
        #[command(subcommand)]
        command: RegionCommand,
    },
    Dataset {
        #[command(subcommand)]
        command: DatasetCommand,
    },
}
#[derive(Subcommand)]
pub enum RegionCommand {
    Get,
    Load(FileInput),
    Clear,
}
#[derive(Subcommand)]
pub enum DatasetCommand {
    Status,
    Download {
        mcc: u16,
        #[arg(long,default_value="full",value_parser=["full","diff"])]
        mode: String,
        #[arg(long)]
        date: Option<String>,
    },
    Update {
        #[arg(long, default_value_t = 0)]
        mcc: u16,
    },
    Auto {
        #[arg(action=clap::ArgAction::Set)]
        enabled: bool,
        #[arg(long)]
        mcc: Option<u16>,
    },
}
#[derive(Subcommand)]
pub enum ScodeCommand {
    Encode {
        #[command(flatten)]
        input: FileInput,
        #[command(flatten)]
        output: FileOutput,
    },
    Decode {
        #[command(flatten)]
        input: FileInput,
        #[command(flatten)]
        output: FileOutput,
    },
    Import {
        #[command(flatten)]
        input: FileInput,
        #[command(flatten)]
        output: FileOutput,
        #[arg(long)]
        without_cells: bool,
        #[arg(long)]
        without_wifi: bool,
    },
}
#[derive(Subcommand)]
pub enum JoystickCommand {
    Open {
        #[arg(long, default_value_t = 1.5)]
        speed: f64,
    },
    Close,
}
#[derive(Subcommand)]
pub enum ServiceCommand {
    Start,
    Stop,
    Restart,
    Status,
}
#[derive(Subcommand)]
pub enum ModuleCommand {
    Status,
    Enable,
    Disable,
}

#[derive(Subcommand)]
pub enum BackupCommand {
    Export {
        #[command(flatten)]
        file: FileOutput,
    },
    /// Merge with fresh IDs by default; --replace replaces the saved library.
    Import {
        #[command(flatten)]
        file: FileInput,
        #[arg(long)]
        replace: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    #[test]
    fn command_tree_is_valid_and_explicit_booleans_parse() {
        Cli::command().debug_assert();
        for args in [
            vec!["wifi", "enable", "false"],
            vec!["sim", "operator", "false"],
            vec!["place", "pin", "id", "false"],
            vec!["cells", "dataset", "auto", "false"],
            vec!["steps", "set", "--enabled", "false", "--cadence", "0"],
        ] {
            assert!(Cli::try_parse_from(std::iter::once("justlocationd").chain(args)).is_ok());
        }
        assert!(Cli::try_parse_from(["justlocationd", "route", "start"]).is_err());
        assert!(
            Cli::try_parse_from(["justlocationd", "scope", "set", "--all", "--app", "example.app"])
                .is_err()
        );
    }
}

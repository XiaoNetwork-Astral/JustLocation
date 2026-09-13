//! Human-facing commands and stable JSON output over the existing control protocol.
mod args;
mod backup;
mod control;
mod gpx;
mod library;
mod platform;
#[path = "cli/scode.rs"]
mod scode_command;
mod services;
pub use scode_command::scode;

use args::*;
use clap::Parser;
use serde_json::{Value, json};
use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};
type Result<T> = std::result::Result<T, String>;

pub fn entry() -> ExitCode {
    let arguments: Vec<String> = std::env::args().collect();
    // Android clients pass a single Base64 payload to these endpoints.
    let legacy = arguments.len() == 3
        && matches!(arguments[1].as_str(), "cells" | "maps")
        && crate::transport::decode_request(&arguments[2])
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .is_some();
    let result = if legacy {
        let directory = Path::new(crate::transport::DATA_DIR);
        let guard = library::lock(directory, "data.lock");
        let recovered =
            guard.as_ref().map_err(|e| e.clone()).and_then(|_| backup::recover(directory));
        if let Err(error) = recovered {
            eprintln!("error: {error}");
            return ExitCode::from(1);
        }
        (if arguments[1] == "cells" {
            crate::cell_service::request(&arguments[2])
        } else {
            crate::maps::request(&arguments[2])
        })
        .and_then(|s| write_output(None, &s))
    } else {
        let cli = Cli::parse();
        Runtime { directory: cli.data_dir, json: cli.json }.run(cli.command)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error == "broken pipe" => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

struct Runtime {
    directory: PathBuf,
    json: bool,
}
impl Runtime {
    fn run(&self, command: Command) -> Result<()> {
        let _guard = if matches!(
            &command,
            Command::Place { .. }
                | Command::Route { .. }
                | Command::Backup { .. }
                | Command::Cells { .. }
                | Command::Maps { .. }
        ) {
            let guard = library::lock(&self.directory, "data.lock")?;
            backup::recover(&self.directory)?;
            Some(guard)
        } else {
            None
        };
        match command {
            Command::Serve => {
                crate::transport::serve_in(&self.directory).map_err(|e| e.to_string())
            }
            Command::Stdio => crate::protocol::Control::default()
                .serve(io::stdin().lock(), io::stdout().lock())
                .map_err(|e| e.to_string()),
            Command::Request { encoded } => write_output(
                None,
                &crate::transport::request_in(
                    &self.directory,
                    &crate::transport::decode_request(&encoded)?,
                )?,
            ),
            Command::Scode { command } => self.scode(command),
            Command::Coordinates { position, to } => {
                self.emit(&json!(crate::coordinates::convert(
                    &position.position()?,
                    crate::coordinates::CoordinateSystem::Wgs84,
                    to.into()
                )?))
            }
            Command::Place { command } => self.place(command),
            Command::Backup { command } => self.backup(command),
            Command::Route { command } => self.route(command),
            Command::Cells { command } => self.cells(command),
            Command::Maps { command } => self.maps(command),
            Command::Apps { system } => self.emit(&platform::apps(system)?),
            Command::Joystick { command } => self.emit(&platform::joystick(command)?),
            Command::Module { command } => self.emit(&platform::module(command)?),
            Command::Service { command } => self.emit(&self.service(command)?),
            other => self.control_command(other),
        }
    }
    fn emit(&self, value: &Value) -> Result<()> {
        let text = if self.json {
            serde_json::to_string(value)
        } else {
            serde_json::to_string_pretty(value)
        }
        .map_err(|e| e.to_string())?;
        write_output(None, &(text + "\n"))
    }
    fn request(&self, request: Value) -> Result<Value> {
        self.reply(request).map(|value| value["state"].clone())
    }
    fn reply(&self, mut request: Value) -> Result<Value> {
        request["version"] = json!(1);
        let reply = crate::transport::request_in(&self.directory, &request.to_string())?;
        check(serde_json::from_str(&reply).map_err(|e| format!("invalid service response: {e}"))?)
    }
    fn status(&self) -> Result<Value> {
        self.request(json!({"op":"status"}))
    }
    fn scope(&self, args: ScopeArgs, feature: crate::scope::Feature) -> Result<Value> {
        if args.all {
            Ok(json!({"mode":"all"}))
        } else if !args.app.is_empty() {
            if args.app.iter().any(|s| s.trim().is_empty()) {
                return Err("empty app package".into());
            }
            Ok(
                json!({"mode":"apps","packages":args.app.into_iter().collect::<std::collections::BTreeSet<_>>()}),
            )
        } else {
            let state = self.status()?;
            if state["config"].is_null() {
                return Err("select apps with --app PACKAGE or explicitly use --all".into());
            }
            let scope = if state.get("scopes").is_some() {
                &state["scopes"][feature.key()]
            } else {
                &state["config"]["scope"]
            };
            if scope.is_null() {
                Err("select apps with --app PACKAGE or explicitly use --all".into())
            } else {
                Ok(scope.clone())
            }
        }
    }
    fn scode(&self, command: ScodeCommand) -> Result<()> {
        let (input, output, mode, cells, wifi) = match command {
            ScodeCommand::Encode { input, output } => (input, output, "encode", true, true),
            ScodeCommand::Decode { input, output } => (input, output, "decode", true, true),
            ScodeCommand::Import { input, output, without_cells, without_wifi } => {
                (input, output, "import", !without_cells, !without_wifi)
            }
        };
        let mut args = vec![mode.into()];
        if !cells {
            args.push("--without-cells".into());
        }
        if !wifi {
            args.push("--without-wifi".into());
        }
        let mut result = Vec::new();
        scode(&args, read_input(&input.input)?.as_bytes(), &mut result)?;
        write_output(
            output.output.as_deref(),
            std::str::from_utf8(&result).map_err(|e| e.to_string())?,
        )
    }
}
#[cfg(unix)]
pub(crate) fn recover_backup_before_serve(directory: &Path) -> Result<()> {
    let _guard = library::lock(directory, "data.lock")?;
    backup::recover_with_service_lock(directory)
}
fn check(value: Value) -> Result<Value> {
    if value["ok"] == true {
        Ok(value)
    } else {
        Err(value["error"].as_str().unwrap_or("operation failed").into())
    }
}
fn read_input(path: &Path) -> Result<String> {
    read_sized(path, crate::scode::MAX_SIZE)
}
fn read_large(path: &Path) -> Result<String> {
    read_sized(path, crate::route_store::MAX_FILE)
}
fn read_large_json(path: &Path) -> Result<Value> {
    serde_json::from_str(&read_large(path)?).map_err(|e| format!("invalid JSON: {e}"))
}
fn read_sized(path: &Path, limit: usize) -> Result<String> {
    let reader: Box<dyn Read> = if path == Path::new("-") {
        Box::new(io::stdin())
    } else {
        Box::new(std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?)
    };
    let mut text = String::new();
    reader.take(limit as u64 + 1).read_to_string(&mut text).map_err(|e| e.to_string())?;
    if text.len() > limit {
        return Err(format!("input exceeds {} MiB", limit / 1024 / 1024));
    }
    Ok(text.trim_start_matches('\u{feff}').to_owned())
}
fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_str(&read_input(path)?).map_err(|e| format!("invalid JSON: {e}"))
}
fn write_output(path: Option<&Path>, text: &str) -> Result<()> {
    if let Some(path) = path.filter(|p| *p != Path::new("-")) {
        crate::storage::atomic_save(path, text.as_bytes())
            .map_err(|e| format!("{}: {e}", path.display()))
    } else {
        io::stdout().lock().write_all(text.as_bytes()).map_err(|e| {
            if e.kind() == io::ErrorKind::BrokenPipe { "broken pipe".into() } else { e.to_string() }
        })
    }
}

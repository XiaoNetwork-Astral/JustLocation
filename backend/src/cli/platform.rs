use super::*;
use std::{
    fs,
    process::{Command as Process, Stdio},
    time::{Duration, Instant},
};
const PACKAGE: &str = "me.idk.justlocation.joystick";
const PROBE: &str = "me.idk.justlocation.probe";
const PROBE_SNAPSHOT: &str = "environment-snapshot.json";
const PROBE_TIMEOUT: Duration = Duration::from_secs(40);

fn android() -> Result<()> {
    if cfg!(target_os = "android") {
        Ok(())
    } else {
        Err("this command requires Android and root access".into())
    }
}
fn am(args: &[&str]) -> Result<String> {
    android()?;
    let output = Process::new("am").args(args).output().map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if args.first() == Some(&"stopservice") && service_stopped(&stdout, &stderr) {
        return Ok("Service stopped".into());
    }
    if !output.status.success() || stdout.contains("Error:") || stderr.contains("Error:") {
        Err(format!("Android command failed: {} {}", stdout.trim(), stderr.trim()))
    } else {
        Ok(stdout)
    }
}
fn service_stopped(stdout: &str, stderr: &str) -> bool {
    stdout.lines().chain(stderr.lines()).any(|line| {
        matches!(line.trim(), "Service stopped" | "Service not stopped: was not running.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn android_stopservice_uses_service_count_as_exit_status() {
        assert!(service_stopped("Stopping service: Intent {}\nService stopped\n", ""));
        assert!(service_stopped(
            "Stopping service: Intent {}",
            "Service not stopped: was not running."
        ));
        assert!(!service_stopped("", "Error: permission denied"));
    }
}
pub(super) fn apps(system: bool) -> Result<Value> {
    android()?;
    let mut command = Process::new("pm");
    command.args(["list", "packages"]);
    if !system {
        command.arg("-3");
    }
    let output = command.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into());
    }
    let mut packages: Vec<_> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|s| s.strip_prefix("package:"))
        .map(str::to_owned)
        .collect();
    packages.sort();
    Ok(json!(packages))
}
pub(super) fn joystick(command: JoystickCommand) -> Result<Value> {
    let result = match command {
        JoystickCommand::Open { speed } => {
            if !speed.is_finite() || !(0.0..=1000.0).contains(&speed) || speed == 0.0 {
                return Err("joystick speed must be greater than 0 and at most 1000 m/s".into());
            }
            android()?;
            let grant = Process::new("appops")
                .args(["set", PACKAGE, "SYSTEM_ALERT_WINDOW", "allow"])
                .output()
                .map_err(|e| e.to_string())?;
            if !grant.status.success() {
                return Err("allow JustLoystick to display over other apps, then retry".into());
            }
            am(&[
                "start",
                "-W",
                "-n",
                &format!("{PACKAGE}/.CallActivity"),
                "--ef",
                "speed",
                &speed.to_string(),
            ])?
        }
        JoystickCommand::Close => {
            am(&["stopservice", "-n", &format!("{PACKAGE}/.JoystickService")])?
        }
    };
    Ok(json!({"requested":true,"android":result.trim()}))
}
pub(super) fn record(action: &str) -> Result<PathBuf> {
    let user = am(&["get-current-user"])?;
    let user = user
        .trim()
        .parse::<u32>()
        .map_err(|_| "cannot determine the current Android user")?
        .to_string();
    let report = PathBuf::from(format!("/data/user/{user}/{PACKAGE}/files/recording-state.json"));
    match fs::remove_file(&report) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    if matches!(action, "start" | "resume") {
        android()?;
        // A root CLI recording request explicitly enables the companion's location access.
        for permission in
            ["android.permission.ACCESS_COARSE_LOCATION", "android.permission.ACCESS_FINE_LOCATION"]
        {
            let output = Process::new("pm")
                .args(["grant", "--user", &user, PACKAGE, permission])
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "cannot grant recording permission: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }
        am(&[
            "start",
            "-W",
            "-n",
            &format!("{PACKAGE}/.CallActivity"),
            "--ez",
            "record",
            "true",
            "--es",
            "record_action",
            action,
        ])?;
    } else {
        am(&[
            "startservice",
            "-n",
            &format!("{PACKAGE}/.RouteRecordService"),
            "-a",
            if action == "pause" {
                "me.idk.justlocation.joystick.RECORD_PAUSE"
            } else {
                "me.idk.justlocation.joystick.RECORD_STOP"
            },
        ])?;
    }
    Ok(report)
}
/// Ask the probe app for one measured environment snapshot and return its JSON text.
/// The probe is a separate normal app: this only launches its collection activity, grants the
/// runtime permissions the capture needs and reads the file it writes.
pub(super) fn collect_environment() -> Result<String> {
    android()?;
    let user = am(&["get-current-user"])?
        .trim()
        .parse::<u32>()
        .map_err(|_| "cannot determine the current Android user")?
        .to_string();
    let path = PathBuf::from(format!("/data/user/{user}/{PROBE}/files/{PROBE_SNAPSHOT}"));
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    if probe_installed(user.as_str()).is_err() {
        return Err(format!("the probe app {PROBE} is not installed"));
    }
    for permission in [
        "android.permission.ACCESS_COARSE_LOCATION",
        "android.permission.ACCESS_FINE_LOCATION",
        "android.permission.ACCESS_WIFI_STATE",
        "android.permission.READ_PHONE_STATE",
    ] {
        // A permission the platform refuses is reported by the snapshot itself.
        let _ = Process::new("pm")
            .args(["grant", "--user", &user, PROBE, permission])
            .output()
            .map_err(|e| e.to_string())?;
    }
    // The probe collects from onCreate, so a leftover activity would be resumed without
    // collecting anything; stop the app first and always start a fresh instance.
    let _ = Process::new("am")
        .args(["force-stop", PROBE])
        .output()
        .map_err(|e| e.to_string())?;
    am(&[
        "start",
        "-W",
        "-n",
        &format!("{PROBE}/.ChannelCheckActivity"),
        "--ez",
        "autorun",
        "true",
        "--ez",
        "env_only",
        "true",
    ])?;
    // The probe waits up to 12s for a fresh fix, so allow for its own timeout plus startup.
    let start = Instant::now();
    while start.elapsed() < PROBE_TIMEOUT {
        if path.is_file() {
            let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            // The probe writes through a temporary file, so a partial read is unexpected; an
            // unreadable snapshot is reported instead of being treated as a capture.
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!(
        "the probe did not write {PROBE_SNAPSHOT} within {}s; unlock the device, keep the probe installed and check that location is enabled",
        PROBE_TIMEOUT.as_secs()
    ))
}

/// `pm list packages` is the supported existence check; `am list` does not exist on Android 15.
fn probe_installed(user: &str) -> Result<()> {
    let output = Process::new("pm")
        .args(["list", "packages", "--user", user, PROBE])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    if String::from_utf8_lossy(&output.stdout).lines().any(|line| line.trim() == format!("package:{PROBE}"))
    {
        Ok(())
    } else {
        Err("not installed".into())
    }
}

pub(super) fn discard_recording() -> Result<()> {
    am(&["stopservice", "-n", &format!("{PACKAGE}/.RouteRecordService")])?;
    Ok(())
}
pub(super) fn module(command: ModuleCommand) -> Result<Value> {
    android()?;
    let directory = Path::new("/data/adb/modules/justlocation");
    if !directory.join("module.prop").is_file() {
        return Err("JustLocation module is not installed".into());
    }
    let flag = directory.join("disable");
    let changed = match command {
        ModuleCommand::Enable => match fs::remove_file(&flag) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.to_string()),
        },
        ModuleCommand::Disable => {
            let changed = !flag.exists();
            fs::write(&flag, b"").map_err(|e| e.to_string())?;
            changed
        }
        ModuleCommand::Status => false,
    };
    Ok(
        json!({"enabled_next_boot":!flag.exists(),"changed":changed,"reboot_required_for_unload":true}),
    )
}
impl Runtime {
    pub(super) fn service(&self, command: ServiceCommand) -> Result<Value> {
        if matches!(command, ServiceCommand::Status) {
            return match self.status() {
                Ok(state) => Ok(json!({"running":true,"state":state})),
                Err(error) => Ok(json!({"running":false,"error":error})),
            };
        }
        if !cfg!(unix) {
            return Err("background service requires Unix sockets (Android/Linux)".into());
        }
        let _lock = library::lock(&self.directory, "service-cli.lock")?;
        if matches!(command, ServiceCommand::Stop | ServiceCommand::Restart) {
            if self.status().is_ok() {
                self.request(json!({"op":"shutdown"}))?;
            }
            for _ in 0..40 {
                if !self.directory.join("control.sock").exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if self.status().is_ok() {
                return Err("service did not shut down".into());
            }
            if matches!(command, ServiceCommand::Stop) {
                return Ok(json!({"running":false}));
            }
        } else if let Ok(state) = self.status() {
            return Ok(json!({"running":true,"already_running":true,"state":state}));
        }
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let log = options.open(self.directory.join("service.log")).map_err(|e| e.to_string())?;
        let mut command = Process::new(std::env::current_exe().map_err(|e| e.to_string())?);
        command
            .arg("serve")
            .arg("--data-dir")
            .arg(&self.directory)
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        for _ in 0..50 {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                return Err(format!(
                    "service exited ({status}); inspect {}/service.log",
                    self.directory.display()
                ));
            }
            if let Ok(state) = self.status() {
                return Ok(json!({"running":true,"pid":child.id(),"state":state}));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(format!("service not ready; inspect {}/service.log", self.directory.display()))
    }
}

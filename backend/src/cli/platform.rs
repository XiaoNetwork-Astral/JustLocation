use super::*;
use std::{
    fs,
    process::{Command as Process, Stdio},
    time::Duration,
};
const PACKAGE: &str = "me.idk.justlocation.joystick";

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
pub(super) fn record(start: bool) -> Result<PathBuf> {
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
    if start {
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
        am(&["start", "-W", "-n", &format!("{PACKAGE}/.CallActivity"), "--ez", "record", "true"])?;
    } else {
        am(&[
            "startservice",
            "-n",
            &format!("{PACKAGE}/.RouteRecordService"),
            "-a",
            "me.idk.justlocation.joystick.RECORD_STOP",
        ])?;
    }
    Ok(report)
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

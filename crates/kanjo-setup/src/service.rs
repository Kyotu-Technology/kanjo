//! The background service that runs `<exe> serve` for the current user:
//! a systemd user unit on Linux, a LaunchAgent on macOS and a logon entry in
//! `HKCU\...\Run` on Windows (through a windowless `wscript` launcher, so no
//! console window appears and no administrator rights are needed).
//!
//! The file contents are produced by pure functions; every external command
//! goes through `Runner`, so tests never start `systemctl`, `launchctl` or `reg`.

use kanjo_core::paths::Paths;
use kanjo_core::{Error, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const SYSTEMD_UNIT: &str = "kanjo.service";
pub const LAUNCHD_LABEL: &str = "io.kyotu.kanjo";
pub const WINDOWS_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
pub const WINDOWS_RUN_VALUE: &str = "Kanjo";
pub const WINDOWS_LAUNCHER: &str = "kanjo-service.vbs";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    NotInstalled,
    Running,
    Stopped,
    /// Registered, but the platform gives no cheap way to tell whether it runs (Windows).
    Installed,
    /// No service manager Kanjo supports on this system.
    Unsupported,
}

impl ServiceState {
    pub fn is_installed(&self) -> bool {
        matches!(self, ServiceState::Running | ServiceState::Stopped | ServiceState::Installed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOs,
    Windows,
}

impl Platform {
    pub fn current() -> Option<Platform> {
        if cfg!(target_os = "linux") {
            Some(Platform::Linux)
        } else if cfg!(target_os = "macos") {
            Some(Platform::MacOs)
        } else if cfg!(windows) {
            Some(Platform::Windows)
        } else {
            None
        }
    }
}

/// Where the service definition lives: the systemd user unit directory, the
/// LaunchAgents directory or the directory of the Windows launcher script.
#[derive(Debug, Clone)]
pub struct Target {
    pub platform: Platform,
    pub dir: PathBuf,
}

impl Target {
    pub fn current() -> Option<Target> {
        let platform = Platform::current()?;
        let base = kanjo_core::paths::UserDirs::current()?;
        let dir = match platform {
            Platform::Linux => base.config.join("systemd").join("user"),
            Platform::MacOs => base.home.join("Library").join("LaunchAgents"),
            Platform::Windows => base.data_local.join("Kyotu").join("kanjo").join("service"),
        };
        Some(Target { platform, dir })
    }

    /// The file Kanjo writes for this platform.
    pub fn file(&self) -> PathBuf {
        match self.platform {
            Platform::Linux => self.dir.join(SYSTEMD_UNIT),
            Platform::MacOs => self.dir.join(format!("{LAUNCHD_LABEL}.plist")),
            Platform::Windows => self.dir.join(WINDOWS_LAUNCHER),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunOutput {
    /// `None` when the process was killed by a signal.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl RunOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Runs an external program to completion.
pub trait Runner {
    fn run(&self, program: &str, args: &[&str]) -> std::io::Result<RunOutput>;
}

/// Runs programs for real, without a console window on Windows.
pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> std::io::Result<RunOutput> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args).stdin(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(crate::CREATE_NO_WINDOW);
        }
        let out = cmd.output()?;
        Ok(RunOutput {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

fn run_ok(runner: &dyn Runner, program: &str, args: &[&str]) -> Result<RunOutput> {
    let out = runner.run(program, args).map_err(|e| Error::Other(format!("{program}: {e}")))?;
    if out.success() {
        Ok(out)
    } else {
        let msg = if out.stderr.trim().is_empty() { out.stdout.trim() } else { out.stderr.trim() };
        Err(Error::Other(format!("{program} {}: exit {:?}: {msg}", args.join(" "), out.code)))
    }
}

fn env_pairs(paths: &Paths) -> [(&'static str, String); 2] {
    [("KANJO_HOME", paths.home.to_string_lossy().into_owned()), ("KANJO_CLAUDE_DIR", paths.claude_dir.to_string_lossy().into_owned())]
}

/// Quotes a value for a unit file; `ExecStart=` also expands `$`, `Environment=` does not.
fn systemd_quote(s: &str, exec: bool) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '%' => out.push_str("%%"),
            '$' if exec => out.push_str("$$"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The systemd user unit. The data and Claude directories are pinned so the
/// service sees the same locations as the installer.
pub fn systemd_unit(exe: &Path, paths: &Paths) -> String {
    let env: String =
        env_pairs(paths).iter().map(|(k, v)| format!("Environment={}\n", systemd_quote(&format!("{k}={v}"), false))).collect();
    format!(
        "[Unit]\n\
         Description=Kanjo cost and limit monitor for Claude Code\n\
         Documentation=https://github.com/Kyotu-Technology/kanjo\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={} serve\n\
         {env}\
         Restart=on-failure\n\
         RestartSec=10\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        systemd_quote(&exe.to_string_lossy(), true)
    )
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// The LaunchAgent property list.
pub fn launchd_plist(exe: &Path, paths: &Paths) -> String {
    let log = xml_escape(&paths.log_dir().join("service.log").to_string_lossy());
    let env: String =
        env_pairs(paths).iter().map(|(k, v)| format!("        <key>{k}</key>\n        <string>{}</string>\n", xml_escape(v))).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCHD_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>serve</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
{env}    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        xml_escape(&exe.to_string_lossy())
    )
}

fn vbs_string(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// The windowless launcher: sets the pinned directories and starts `<exe> serve` hidden.
pub fn windows_launcher(exe: &Path, paths: &Paths) -> String {
    let mut out = String::from("' Kanjo background service launcher, generated by kanjo. Starts \"kanjo serve\" without a window.\r\n");
    out.push_str("Set shell = CreateObject(\"WScript.Shell\")\r\n");
    out.push_str("Set env = shell.Environment(\"Process\")\r\n");
    for (k, v) in env_pairs(paths) {
        out.push_str(&format!("env(\"{k}\") = {}\r\n", vbs_string(&v)));
    }
    let cmdline = format!("\"{}\" serve", exe.to_string_lossy());
    out.push_str(&format!("shell.Run {}, 0, False\r\n", vbs_string(&cmdline)));
    out
}

/// The value of the `Run` registry entry that starts the launcher at logon.
pub fn windows_run_value(launcher: &Path) -> String {
    format!("wscript.exe //B //Nologo \"{}\"", launcher.to_string_lossy())
}

fn launchctl_domain(runner: &dyn Runner) -> Result<String> {
    let out = run_ok(runner, "id", &["-u"])?;
    Ok(format!("gui/{}", out.stdout.trim()))
}

/// Writes the service definition, registers it and starts it. Reinstalling
/// replaces the definition (e.g. after the executable moved) and restarts.
pub fn install(exe: &Path, paths: &Paths) -> Result<()> {
    let target = Target::current().ok_or_else(|| Error::Other("no supported service manager on this system".into()))?;
    install_with(&target, &SystemRunner, exe, paths)
}

pub fn uninstall() -> Result<()> {
    let target = Target::current().ok_or_else(|| Error::Other("no supported service manager on this system".into()))?;
    uninstall_with(&target, &SystemRunner)
}

/// Installed and running state, asking the service manager.
pub fn status() -> ServiceState {
    match Target::current() {
        Some(t) => status_with(&t, &SystemRunner),
        None => ServiceState::Unsupported,
    }
}

/// True when Kanjo's service definition file exists. Starts no process.
pub fn is_installed() -> bool {
    Target::current().is_some_and(|t| t.file().is_file())
}

pub fn install_with(target: &Target, runner: &dyn Runner, exe: &Path, paths: &Paths) -> Result<()> {
    std::fs::create_dir_all(&target.dir)?;
    let file = target.file();
    match target.platform {
        Platform::Linux => {
            crate::claude::atomic_write(&file, systemd_unit(exe, paths).as_bytes())?;
            run_ok(runner, "systemctl", &["--user", "daemon-reload"])?;
            run_ok(runner, "systemctl", &["--user", "enable", SYSTEMD_UNIT])?;
            run_ok(runner, "systemctl", &["--user", "restart", SYSTEMD_UNIT])?;
        }
        Platform::MacOs => {
            paths.ensure_home()?;
            std::fs::create_dir_all(paths.log_dir())?;
            crate::claude::atomic_write(&file, launchd_plist(exe, paths).as_bytes())?;
            let domain = launchctl_domain(runner)?;
            let service = format!("{domain}/{LAUNCHD_LABEL}");
            let _ = runner.run("launchctl", &["bootout", &service]);
            let plist = file.to_string_lossy();
            run_ok(runner, "launchctl", &["bootstrap", &domain, &plist])?;
            run_ok(runner, "launchctl", &["enable", &service])?;
        }
        Platform::Windows => {
            crate::claude::atomic_write(&file, windows_launcher(exe, paths).as_bytes())?;
            let value = windows_run_value(&file);
            run_ok(runner, "reg", &["add", WINDOWS_RUN_KEY, "/v", WINDOWS_RUN_VALUE, "/t", "REG_SZ", "/d", &value, "/f"])?;
            let launcher = file.to_string_lossy();
            run_ok(runner, "wscript.exe", &["//B", "//Nologo", &launcher])?;
        }
    }
    Ok(())
}

/// Stops and unregisters the service and removes its file. Not installed is not an error.
pub fn uninstall_with(target: &Target, runner: &dyn Runner) -> Result<()> {
    let file = target.file();
    match target.platform {
        Platform::Linux => {
            if file.exists() {
                let _ = runner.run("systemctl", &["--user", "disable", "--now", SYSTEMD_UNIT]);
                std::fs::remove_file(&file)?;
                run_ok(runner, "systemctl", &["--user", "daemon-reload"])?;
            }
        }
        Platform::MacOs => {
            if file.exists() {
                let domain = launchctl_domain(runner)?;
                let _ = runner.run("launchctl", &["bootout", &format!("{domain}/{LAUNCHD_LABEL}")]);
                std::fs::remove_file(&file)?;
            }
        }
        Platform::Windows => {
            let registered = runner.run("reg", &["query", WINDOWS_RUN_KEY, "/v", WINDOWS_RUN_VALUE]).is_ok_and(|o| o.success());
            if registered {
                run_ok(runner, "reg", &["delete", WINDOWS_RUN_KEY, "/v", WINDOWS_RUN_VALUE, "/f"])?;
            }
            if file.exists() {
                std::fs::remove_file(&file)?;
            }
        }
    }
    Ok(())
}

pub fn status_with(target: &Target, runner: &dyn Runner) -> ServiceState {
    let installed = target.file().is_file();
    match target.platform {
        Platform::Linux => {
            if !installed {
                return ServiceState::NotInstalled;
            }
            match runner.run("systemctl", &["--user", "is-active", SYSTEMD_UNIT]) {
                Ok(o) if o.stdout.trim() == "active" => ServiceState::Running,
                Ok(_) => ServiceState::Stopped,
                Err(_) => ServiceState::Installed,
            }
        }
        Platform::MacOs => {
            if !installed {
                return ServiceState::NotInstalled;
            }
            let Ok(domain) = launchctl_domain(runner) else {
                return ServiceState::Installed;
            };
            match runner.run("launchctl", &["print", &format!("{domain}/{LAUNCHD_LABEL}")]) {
                Ok(o) if o.success() && o.stdout.contains("state = running") => ServiceState::Running,
                Ok(_) => ServiceState::Stopped,
                Err(_) => ServiceState::Installed,
            }
        }
        Platform::Windows => {
            let registered = runner.run("reg", &["query", WINDOWS_RUN_KEY, "/v", WINDOWS_RUN_VALUE]).is_ok_and(|o| o.success());
            if installed && registered {
                ServiceState::Installed
            } else {
                ServiceState::NotInstalled
            }
        }
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;

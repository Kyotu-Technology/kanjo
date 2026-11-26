//! End-to-end tests of the `kanjo` binary. Every run points KANJO_HOME,
//! KANJO_CLAUDE_DIR, HOME and the XDG directories into a temporary directory
//! and disables the OS keyring, so nothing on this machine is read or changed.

#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const PAYLOAD: &str = r#"{
  "cwd": "/work/app", "session_id": "sess-1", "session_name": "refactor", "transcript_path": "/x/y.jsonl",
  "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
  "workspace": {"current_dir": "/work/app", "project_dir": "/work/app"},
  "version": "2.1.332",
  "cost": {"total_cost_usd": 1.25, "total_duration_ms": 45000, "total_api_duration_ms": 2300, "total_lines_added": 156, "total_lines_removed": 23},
  "context_window": {"total_input_tokens": 15500, "context_window_size": 200000, "used_percentage": 8},
  "rate_limits": {"five_hour": {"used_percentage": 23.5, "resets_at": 4102444800}, "seven_day": {"used_percentage": 41.2, "resets_at": 4102444800}}
}"#;

struct Env {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    claude: PathBuf,
    port: u16,
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

impl Env {
    fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let home = root.join("kanjo-home");
        let claude = root.join("claude");
        std::fs::create_dir_all(claude.join("projects")).unwrap();
        std::fs::create_dir_all(root.join("user-home")).unwrap();
        let port = free_port();
        let env = Env { _dir: dir, root, home, claude, port };
        env.write_config("");
        env
    }

    fn write_config(&self, extra: &str) {
        std::fs::create_dir_all(&self.home).unwrap();
        std::fs::write(self.home.join("config.toml"), format!("[server]\nport = {}\n{extra}", self.port)).unwrap();
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_kanjo"));
        let user_home = self.root.join("user-home");
        c.args(args)
            .env("KANJO_HOME", &self.home)
            .env("KANJO_CLAUDE_DIR", &self.claude)
            .env("KANJO_NO_KEYRING", "1")
            .env("HOME", &user_home)
            .env("XDG_CONFIG_HOME", user_home.join(".config"))
            .env("XDG_DATA_HOME", user_home.join(".local/share"))
            .env("NO_COLOR", "1")
            .env("LANG", "en_US.UTF-8")
            .env("PATH", "/usr/bin:/bin")
            .env_remove("LC_ALL")
            .env_remove("LC_MESSAGES")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("BROWSER");
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).stdin(Stdio::null()).output().unwrap()
    }

    fn run_stdin(&self, args: &[&str], input: &[u8]) -> Output {
        let mut child = self.cmd(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }

    fn spool_lines(&self) -> Vec<Value> {
        let Ok(dir) = std::fs::read_dir(self.home.join("spool")) else { return Vec::new() };
        let mut out = Vec::new();
        for f in dir.flatten() {
            for line in std::fs::read_to_string(f.path()).unwrap().lines() {
                out.push(serde_json::from_str(line).unwrap());
            }
        }
        out
    }

    fn seed(&self) {
        let o = self.run(&["demo", "--seed-only", "--days", "30"]);
        assert!(o.status.success(), "seed failed: {}", text(&o.stderr));
    }
}

/// Runs a command that may not exit on its own (a foreground server), collects
/// stdout until `marker` appears or `limit` passes, then kills it.
fn run_until(cmd: Command, marker: &str, limit: Duration) -> String {
    let (mut child, seen) = spawn_until(cmd, marker, limit);
    let _ = child.kill();
    let _ = child.wait();
    seen
}

/// Starts a command and collects stdout until `marker` appears or `limit`
/// passes; the process keeps running.
fn spawn_until(mut cmd: Command, marker: &str, limit: Duration) -> (std::process::Child, String) {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let start = Instant::now();
    let mut seen = String::new();
    while let Some(left) = limit.checked_sub(start.elapsed()) {
        match rx.recv_timeout(left) {
            Ok(line) => {
                seen.push_str(&line);
                seen.push('\n');
                if line.contains(marker) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    (child, seen)
}

/// A plain HTTP/1.1 GET on 127.0.0.1; returns the raw response.
#[cfg(feature = "server")]
fn http_get(port: u16, path: &str) -> String {
    use std::io::Read;
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out
}

/// The path and query of a URL on 127.0.0.1.
#[cfg(feature = "server")]
fn path_of(url: &str) -> &str {
    &url[url.find("/?").unwrap()..]
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn ok(o: &Output) -> String {
    assert!(o.status.success(), "exit {:?}\nstdout: {}\nstderr: {}", o.status.code(), text(&o.stdout), text(&o.stderr));
    text(&o.stdout)
}

fn json(o: &Output) -> Value {
    serde_json::from_str(&ok(o)).unwrap()
}

#[test]
fn statusline_records_and_prints_on_valid_input() {
    let env = Env::new();
    let o = env.run_stdin(&["statusline"], PAYLOAD.as_bytes());
    let out = ok(&o);
    assert!(!out.trim().is_empty());
    assert!(out.contains("Opus") || out.contains("opus"), "{out}");
    let lines = env.spool_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["kind"], "statusline");
    assert_eq!(lines[0]["session_id"], "sess-1");
    assert!(lines[0]["session_name"].is_null(), "titles are off by default, so none reaches the spool: {}", lines[0]);
    assert!(!env.home.join("kanjo.db").exists(), "the fast path must not create the database");

    env.write_config("[statusline]\ncapture_titles = true\n");
    ok(&env.run_stdin(&["statusline"], PAYLOAD.as_bytes()));
    let lines = env.spool_lines();
    assert!(lines.iter().any(|l| l["session_name"] == "refactor"), "{lines:?}");
}

#[test]
fn the_home_option_reaches_the_fast_path() {
    let env = Env::new();
    let other = env.root.join("other-home");
    let mut cmd = env.cmd(&["--home", other.to_str().unwrap(), "hook"]);
    cmd.env_remove("KANJO_HOME").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(br#"{"hook_event_name":"Stop","session_id":"s-1"}"#).unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    assert_eq!(std::fs::read_dir(other.join("spool")).unwrap().count(), 1);
    assert!(env.spool_lines().is_empty());
}

#[test]
fn statusline_survives_empty_and_garbage_input() {
    let env = Env::new();
    for input in [&b""[..], b"not json at all", b"{\"session_id\": 5}", b"[1,2,3]", &[0xff, 0xfe, 0x00]] {
        let o = env.run_stdin(&["statusline"], input);
        let out = ok(&o);
        assert!(!out.trim().is_empty(), "nothing printed for {input:?}");
        assert!(o.stderr.is_empty(), "{}", text(&o.stderr));
    }
    assert!(env.spool_lines().is_empty());
}

#[test]
fn statusline_survives_a_broken_config() {
    let env = Env::new();
    std::fs::write(env.home.join("config.toml"), "this is = = not toml").unwrap();
    let o = env.run_stdin(&["statusline"], PAYLOAD.as_bytes());
    let out = ok(&o);
    assert!(!out.trim().is_empty());
    let err = text(&o.stderr);
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("config.toml cannot be read"), "{err}");
}

#[test]
fn statusline_passes_the_chained_output_through() {
    let env = Env::new();
    env.write_config("[statusline]\nchain_command = \"cat > /dev/null; echo chained-line\"\n");
    let out = ok(&env.run_stdin(&["statusline"], PAYLOAD.as_bytes()));
    assert_eq!(out.trim(), "chained-line");
    assert_eq!(env.spool_lines().len(), 1);

    env.write_config("[statusline]\nchain_command = \"exit 3\"\n");
    let out = ok(&env.run_stdin(&["statusline"], PAYLOAD.as_bytes()));
    assert!(!out.trim().is_empty() && !out.contains("chained"), "a failing chain falls back to Kanjo's line: {out}");
}

#[test]
fn hook_appends_a_marker_and_prints_nothing() {
    let env = Env::new();
    let payload = br#"{"hook_event_name":"SessionStart","session_id":"s-9","cwd":"/work/app","source":"startup","model":"claude-opus-5-5","transcript_path":"/x.jsonl"}"#;
    let o = env.run_stdin(&["hook"], payload);
    assert!(o.status.success());
    assert!(o.stdout.is_empty() && o.stderr.is_empty());
    let lines = env.spool_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["kind"], "hook");
    assert_eq!(lines[0]["event"], "SessionStart");
    assert_eq!(lines[0]["source"], "startup");
    assert_eq!(lines[0]["model"], "claude-opus-5-5");

    for bad in [&b""[..], b"garbage", br#"{"session_id":"x"}"#] {
        let o = env.run_stdin(&["hook"], bad);
        assert!(o.status.success() && o.stdout.is_empty());
    }
    assert_eq!(env.spool_lines().len(), 1);
}

#[test]
fn reports_on_demo_data() {
    let env = Env::new();
    env.seed();
    let daily = json(&env.run(&["daily", "--json"]));
    assert!(!daily["daily"].as_array().unwrap().is_empty());
    assert!(daily["totals"]["totalCost"].as_f64().unwrap() > 0.0);

    let table = ok(&env.run(&["daily"]));
    assert!(table.contains("Total") && table.contains("lower bound"), "{table}");
    let breakdown = ok(&env.run(&["daily", "--breakdown", "--since", "20200101"]));
    assert!(breakdown.contains("- "), "{breakdown}");
    assert!(ok(&env.run(&["weekly"])).contains("Week"));
    assert!(ok(&env.run(&["monthly", "--csv"])).contains(','));
    assert!(json(&env.run(&["blocks", "--json"]))["blocks"].is_array());
    assert!(ok(&env.run(&["blocks"])).contains("Window"));
    let sessions = json(&env.run(&["session", "--json"]));
    assert!(!sessions["sessions"].as_array().unwrap().is_empty());

    let status = ok(&env.run(&["status"]));
    assert!(status.contains("5-hour limit") && status.contains("Today"), "{status}");
    assert!(!status.contains("Burn rate") || status.contains("tokens/min"), "{status}");
    let blocks = ok(&env.run(&["blocks", "--breakdown"]));
    assert!(blocks.contains("- project "), "{blocks}");
    let export = ok(&env.run(&["export", "breakdown", "--by", "query-source", "--format", "csv"]));
    assert!(export.contains("repl_main_thread"), "{export}");
    assert!(status.lines().count() <= 16, "{status}");
    assert!(json(&env.run(&["status", "--json"]))["today"].is_object());

    let bad = env.run(&["daily", "--project", "no-such-project"]);
    assert_eq!(bad.status.code(), Some(1));
    let err = text(&bad.stderr);
    assert!(err.contains("Error:") && err.contains("What to do:"), "{err}");
    assert!(!err.contains("panicked"));
}

#[test]
fn doctor_reports_as_json_with_an_exit_code() {
    let env = Env::new();
    env.seed();
    let o = env.run(&["doctor", "--json"]);
    assert!(matches!(o.status.code(), Some(0) | Some(1)), "{}", text(&o.stderr));
    let checks: Value = serde_json::from_slice(&o.stdout).unwrap();
    let checks = checks.as_array().unwrap();
    assert!(checks.iter().any(|c| c["id"] == "database" && c["status"] == "ok"), "{checks:?}");
    let failed = checks.iter().any(|c| c["status"] == "fail");
    assert_eq!(o.status.code(), Some(if failed { 1 } else { 0 }));
    let human = env.run(&["doctor"]);
    assert!(text(&human.stdout).contains("[ok]"));
}

#[test]
fn export_purge_backup_restore_pricing_and_audit() {
    let env = Env::new();
    env.seed();
    let csv = env.root.join("usage.csv");
    let o = env.run(&["export", "usage", "--format", "csv", "-o", csv.to_str().unwrap()]);
    ok(&o);
    assert!(std::fs::read_to_string(&csv).unwrap().lines().count() > 2);
    let sessions: Value = serde_json::from_str(&ok(&env.run(&["export", "sessions", "--format", "json"]))).unwrap();
    assert!(sessions["items"].is_array());

    let id = json(&env.run(&["session", "--json"]))["sessions"][0]["sessionId"].as_str().unwrap().to_string();
    let refused = env.run(&["purge", "--session", &id]);
    assert_eq!(refused.status.code(), Some(1), "no terminal and no --yes must refuse");
    let report = json(&env.run(&["purge", "--session", &id, "--yes", "--json"]));
    assert!(report["requests"].as_i64().unwrap() > 0, "{report}");

    let made = json(&env.run(&["backup", "create", "--json"]));
    let file = made["file"].as_str().unwrap().to_string();
    let list = json(&env.run(&["backup", "list", "--json"]));
    assert!(list.as_array().unwrap().iter().any(|b| b["file"] == file.as_str()));

    let squatter = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let taken = squatter.local_addr().unwrap().port();
    std::fs::write(env.home.join("config.toml"), format!("[server]\nport = {taken}\n[storage]\nencryption = \"off\"\n")).unwrap();
    assert!(ok(&env.run(&["restore", &file])).contains("Restored"), "a foreign listener on the configured port is not Kanjo");
    drop(squatter);

    #[cfg(feature = "server")]
    {
        let port = free_port();
        let (mut server, seen) =
            spawn_until(env.cmd(&["serve", "--port", &port.to_string(), "--no-open"]), "Kanjo dashboard on", Duration::from_secs(60));
        assert!(seen.contains(&format!("http://127.0.0.1:{port}/")), "{seen}");
        let busy = env.run(&["restore", &file]);
        assert_eq!(busy.status.code(), Some(1), "a server on a --port override is found");
        assert!(text(&busy.stderr).contains("running") && text(&busy.stderr).contains(&port.to_string()), "{}", text(&busy.stderr));

        let url = json(&env.run(&["--json", "open"]))["url"].as_str().unwrap().to_string();
        let token = std::fs::read_to_string(env.home.join("access.token")).unwrap();
        assert!(url.starts_with(&format!("http://127.0.0.1:{port}/?token=")), "{url}");
        assert!(!url.contains(token.trim()), "kanjo open never hands out the access token");
        let first = http_get(port, path_of(&url));
        assert!(first.starts_with("HTTP/1.1 303") && first.to_lowercase().contains("set-cookie: kanjo_session="), "{first}");
        assert!(!http_get(port, path_of(&url)).to_lowercase().contains("set-cookie"), "the code works once");

        let _ = server.kill();
        let _ = server.wait();
        assert!(env.home.join("server.json").exists(), "a killed server leaves its record behind");
    }
    let restored = ok(&env.run(&["restore", &file]));
    assert!(restored.contains("Restored"), "a stale record does not block");

    let prices = env.root.join("prices.json");
    std::fs::write(
        &prices,
        r#"{"version":"test-1","source":"test","models":{"claude-opus-5-5":{"input_cost_per_token":0.000004,"output_cost_per_token":0.00002}}}"#,
    )
    .unwrap();
    let unsigned = env.run(&["pricing", "import", prices.to_str().unwrap()]);
    assert_eq!(unsigned.status.code(), Some(1));
    assert!(text(&unsigned.stderr).contains("not signed"));
    let imported = json(&env.run(&["pricing", "import", prices.to_str().unwrap(), "--allow-unsigned", "--json"]));
    assert_eq!(imported["version"], "test-1");
    assert_eq!(imported["signed"], false);
    assert!(ok(&env.run(&["pricing", "reset"])).contains("Price table"));

    ok(&env.run(&["retention", "apply"]));
    assert!(ok(&env.run(&["audit", "verify"])).contains("intact"));
    let audit = json(&env.run(&["audit", "list", "--limit", "50", "--json"]));
    let actions: Vec<&str> = audit["items"].as_array().unwrap().iter().filter_map(|e| e["action"].as_str()).collect();
    for want in ["export", "data.purge", "backup.create", "backup.restore", "pricing.import", "pricing.reset", "retention.apply"] {
        assert!(actions.contains(&want), "{want} missing from {actions:?}");
    }
}

#[test]
fn up_edits_settings_json_and_keeps_a_backup() {
    let env = Env::new();
    let settings = env.claude.join("settings.json");
    std::fs::write(&settings, "{\n  \"model\": \"opus\"\n}\n").unwrap();
    let mut child = env
        .cmd(&["up", "--yes", "--no-service", "--no-open"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let backup = |dir: &Path| {
        std::fs::read_dir(dir).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("settings.json.kanjo-backup-"))
    };
    while start.elapsed() < Duration::from_secs(60) {
        let edited = std::fs::read_to_string(&settings).unwrap_or_default().contains("statusLine");
        if (edited && backup(&env.claude)) || child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let o = child.wait_with_output().unwrap();
    let text_now = std::fs::read_to_string(&settings).unwrap();
    assert!(text_now.contains("statusLine") && text_now.contains("SessionStart"), "{text_now}\n{}", text(&o.stderr));
    let named = format!("--home '{}' statusline", env.home.display());
    assert!(text_now.contains(&named), "a data directory other than the default is named in the command: {text_now}");
    assert!(text_now.contains("\"model\": \"opus\""));
    assert!(backup(&env.claude));
    let out = text(&o.stdout);
    assert!(out.contains("Kanjo found") && out.contains("Kanjo will"), "{out}");

    let before = std::fs::read_to_string(&settings).unwrap();
    let seen = run_until(env.cmd(&["up", "--yes", "--no-service", "--no-open"]), "already set up", Duration::from_secs(60));
    assert!(seen.contains("already set up"), "{seen}");
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), before, "a second run changes nothing");

    let refused = env.run(&["up", "--no-service", "--no-open"]);
    assert_eq!(refused.status.code(), Some(1), "no consent without a terminal");
    assert!(text(&refused.stderr).contains("--yes"));
}

#[test]
fn messages_in_both_languages() {
    let env = Env::new();
    let empty_en = env.run(&["status"]);
    assert_eq!(empty_en.status.code(), Some(1));
    assert!(text(&empty_en.stderr).contains("What to do:"));
    let empty_pl = env.run(&["--lang", "pl", "status"]);
    let err = text(&empty_pl.stderr);
    assert!(err.contains("Błąd:") && err.contains("Co zrobić:"), "{err}");
    let by_env = env.cmd(&["status"]).env("LANG", "pl_PL.UTF-8").stdin(Stdio::null()).output().unwrap();
    assert!(text(&by_env.stderr).contains("Błąd:"));

    env.seed();
    let en = ok(&env.run(&["--lang", "en", "status"]));
    assert!(en.contains("Weekly limit"));
    let pl = ok(&env.run(&["--lang", "pl", "status"]));
    assert!(pl.contains("Limit tygodniowy") && pl.contains("Dzisiaj"), "{pl}");
    let pl_daily = ok(&env.run(&["--lang", "pl", "daily"]));
    assert!(pl_daily.contains("Razem") && pl_daily.contains("dolna granica"), "{pl_daily}");
}

#[test]
fn the_stored_language_setting_beats_lang() {
    let env = Env::new();
    env.seed();
    let paths = kanjo_core::paths::Paths::at(&env.home, &env.claude);
    let config = kanjo_core::config::Config::load(&paths).unwrap();
    let (store, _) = kanjo_ops::open_store_with(&paths, &config, None).unwrap();
    let mut settings = kanjo_core::settings::Settings::load(&store).unwrap();
    settings.language = kanjo_core::settings::Language::Pl;
    settings.save(&store).unwrap();
    drop(store);
    let pl = ok(&env.run(&["status"]));
    assert!(pl.contains("Dzisiaj"), "{pl}");
    let en = ok(&env.run(&["--lang", "en", "status"]));
    assert!(en.contains("Today"), "{en}");
}

#[cfg(feature = "server")]
#[test]
fn demo_serves_on_the_given_port() {
    let env = Env::new();
    let home = env.root.join("demo-home");
    let port = free_port();
    let args = ["--home", home.to_str().unwrap(), "demo", "--port", &port.to_string(), "--no-open", "--days", "3"];
    let (mut child, seen) = spawn_until(env.cmd(&args), "--home", Duration::from_secs(60));
    assert!(seen.contains(&format!("http://127.0.0.1:{port}/")), "{seen}");
    let link = seen.split_whitespace().find(|w| w.contains("?token=")).unwrap_or_else(|| panic!("no sign-in link: {seen}"));
    let token = std::fs::read_to_string(home.join("access.token")).unwrap();
    assert!(!link.contains(token.trim()), "the printed link carries a login code, not the token");
    assert!(http_get(port, path_of(link)).to_lowercase().contains("set-cookie: kanjo_session="));
    assert!(http_get(port, "/api/v1/health").contains("\"ok\""));
    let url = json(&env.run(&["--home", home.to_str().unwrap(), "--json", "open"]))["url"].as_str().unwrap().to_string();
    assert!(http_get(port, path_of(&url)).to_lowercase().contains("set-cookie: kanjo_session="), "kanjo open finds the demo");
    let _ = child.kill();
    let _ = child.wait();

    let daily = json(&env.run(&["--home", home.to_str().unwrap(), "daily", "--json"]));
    let days = daily["daily"].as_array().unwrap().len();
    assert!((1..=4).contains(&days), "--days 3 reaches the server: {days} days");
}

#[cfg(feature = "server")]
#[test]
fn demo_leaves_the_data_directory_of_kanjo_home_alone() {
    let env = Env::new();
    let real = env.root.join("real-home");
    std::fs::create_dir_all(&real).unwrap();
    let port = free_port();
    let mut cmd = env.cmd(&["demo", "--port", &port.to_string(), "--no-open"]);
    cmd.env("KANJO_HOME", &real);
    let seen = run_until(cmd, "Kanjo dashboard on", Duration::from_secs(60));
    assert!(seen.contains("Kanjo dashboard on"), "{seen}");
    assert_eq!(std::fs::read_dir(&real).unwrap().count(), 0, "nothing was written to KANJO_HOME");
}

#[test]
fn demo_refuses_a_real_installation() {
    let env = Env::new();
    let real = env.root.join("real");
    std::fs::create_dir_all(real.join("spool")).unwrap();
    std::fs::write(real.join("kanjo.key"), "k").unwrap();
    let o = env.run(&["--home", real.to_str().unwrap(), "demo", "--no-open"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o.stderr).contains("real Kanjo installation"), "{}", text(&o.stderr));
    std::fs::remove_file(real.join("kanjo.key")).unwrap();
    let o = env.cmd(&["demo", "--seed-only"]).env("KANJO_HOME", &real).stdin(Stdio::null()).output().unwrap();
    assert_eq!(o.status.code(), Some(1), "a home with a spool is in use");
    assert!(!real.join("kanjo.db").exists());
}

#[cfg(feature = "server")]
#[test]
fn a_refused_bind_says_why_instead_of_blaming_the_port() {
    let env = Env::new();
    let o = env.run(&["serve", "--bind", "0.0.0.0", "--port", &free_port().to_string(), "--no-open"]);
    assert_eq!(o.status.code(), Some(1));
    let err = text(&o.stderr);
    assert!(err.contains("tls_cert") && !err.contains("already taken"), "{err}");
    assert!(!text(&o.stdout).contains("Kanjo dashboard on"), "no banner for a server that never started");
}

#[test]
fn a_database_made_without_encryption_says_how_to_open_it() {
    let env = Env::new();
    env.seed();
    env.write_config("");
    let o = env.run(&["status"]);
    assert_eq!(o.status.code(), Some(1));
    let err = text(&o.stderr);
    assert!(err.contains("never encrypted") && err.contains("encryption = \"off\""), "{err}");
}

#[test]
fn open_says_how_to_start_the_dashboard() {
    let env = Env::new();
    let squatter = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std::fs::write(env.home.join("config.toml"), format!("[server]\nport = {}\n", squatter.local_addr().unwrap().port())).unwrap();
    let o = env.run(&["--json", "open"]);
    assert_eq!(o.status.code(), Some(1));
    let err = text(&o.stderr);
    assert!(err.contains("not running") && err.contains("kanjo up"), "{err}");
    assert!(o.stdout.is_empty(), "nothing is handed to whatever holds the port");
    assert!(!env.home.join("access.token").exists());
    drop(squatter);
}

#[test]
fn completions_and_help() {
    let env = Env::new();
    assert!(ok(&env.run(&["completions", "bash"])).contains("kanjo"));
    assert!(ok(&env.run(&["--version"])).contains("kanjo"));
}

/// Kanjo is neuter in Polish, as in the web UI: "Kanjo znalazło", never "Kanjo znalazł".
#[test]
fn polish_texts_treat_kanjo_as_neuter() {
    let sources = [
        include_str!("../src/setup.rs"),
        include_str!("../src/data.rs"),
        include_str!("../src/reports.rs"),
        include_str!("../src/app.rs"),
        include_str!("../src/error.rs"),
        include_str!("../../kanjo-setup/src/doctor.rs"),
        include_str!("../../kanjo-setup/src/claude.rs"),
        include_str!("../../kanjo-setup/src/service.rs"),
    ];
    for src in sources {
        for (i, _) in src.match_indices("Kanjo ") {
            let rest = &src[i + "Kanjo ".len()..];
            let rest = rest.strip_prefix("nie ").unwrap_or(rest);
            let word: String = rest.chars().take_while(|c| c.is_alphabetic()).collect();
            assert!(!word.ends_with('ł') && word != "sam" && word != "gotowy", "masculine form: Kanjo {word}");
        }
    }
}

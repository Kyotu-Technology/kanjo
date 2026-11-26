//! Commands that set Kanjo up on this machine or start the dashboard: up,
//! serve, demo, open, service and uninstall.

use crate::app::{out, App, Global};
use crate::cli::ServiceAction;
#[cfg(feature = "server")]
use crate::error::Failure;
use crate::error::{fail, CliResult};
use crate::i18n::Lang;
use kanjo_core::api::HookInstallRequest;
use kanjo_core::config::{Config, EncryptionMode};
use kanjo_core::paths::Paths;
use kanjo_setup::claude::StatuslineSlot;
use kanjo_setup::service::ServiceState;
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const DEMO_PORT: u16 = 4748;

fn loopback_addr(config: &Config) -> Option<SocketAddr> {
    let ip: IpAddr = match config.server.bind.trim() {
        "" | "0.0.0.0" | "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        "::" => IpAddr::V6(Ipv6Addr::LOCALHOST),
        other => other.trim_start_matches('[').trim_end_matches(']').parse().ok()?,
    };
    ip.is_loopback().then(|| SocketAddr::new(ip, config.server.port))
}

/// True when something answers on the configured loopback port, or the background service reports running.
pub fn server_running(config: &Config) -> bool {
    let port_open = loopback_addr(config).is_some_and(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok());
    port_open || (kanjo_setup::service::is_installed() && kanjo_setup::service::status() == ServiceState::Running)
}

fn wait_for_server(config: &Config, limit: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if loopback_addr(config).is_some_and(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

#[cfg(feature = "server")]
fn dashboard_url(paths: &Paths, config: &Config) -> CliResult<String> {
    Ok(kanjo_server::access_url(paths, config)?)
}

#[cfg(not(feature = "server"))]
fn dashboard_url(paths: &Paths, config: &Config) -> CliResult<String> {
    let scheme = if config.server.tls_cert.is_some() { "https" } else { "http" };
    let host = match loopback_addr(config) {
        Some(SocketAddr::V6(a)) => format!("[{}]", a.ip()),
        Some(SocketAddr::V4(a)) => a.ip().to_string(),
        None => config.server.bind.clone(),
    };
    let base = format!("{scheme}://{host}:{}/", config.server.port);
    Ok(match config.server.auth {
        kanjo_core::config::AuthMode::None => base,
        kanjo_core::config::AuthMode::Token => format!("{base}?token={}", kanjo_ops::token::load_or_create(paths)?),
    })
}

fn open_browser(lang: Lang, url: &str) {
    match open::that(url) {
        Ok(()) => out(lang.t("Opened the dashboard in your browser.", "Otwarto pulpit w przeglądarce.")),
        Err(_) => {
            out(lang.t(
                "No browser could be started. Open this address yourself:",
                "Nie udało się uruchomić przeglądarki. Otwórz ten adres samodzielnie:",
            ));
            out(&format!("  {url}"));
        }
    }
}

#[cfg(feature = "server")]
fn run_server(lang: Lang, paths: Paths, config: Config, open_browser: bool, demo: bool) -> CliResult<()> {
    crate::app::init_logging(tracing_subscriber::filter::LevelFilter::INFO);
    let port = config.server.port;
    out(&tr!(
        lang,
        "Kanjo dashboard on http://{}:{port}/ (Ctrl-C stops it).",
        "Pulpit Kanjo pod adresem http://{}:{port}/ (Ctrl-C zatrzymuje).",
        config.server.bind
    ));
    kanjo_server::run(kanjo_server::ServeOptions { paths, config, open_browser, demo }).map_err(|e| {
        let text = e.to_string().to_lowercase();
        if text.contains("in use") || text.contains("address") {
            fail(
                tr!(lang, "port {port} is already taken ({e}).", "port {port} jest już zajęty ({e})."),
                tr!(
                    lang,
                    "Kanjo may already be running (\"kanjo open\" shows it). Otherwise choose another port with --port N.",
                    "Kanjo może już działać (\"kanjo open\" go pokaże). W przeciwnym razie wybierz inny port opcją --port N."
                ),
            )
        } else {
            Failure::Core(e)
        }
    })
}

#[cfg(not(feature = "server"))]
fn run_server(lang: Lang, paths: Paths, config: Config, open_browser: bool, demo: bool) -> CliResult<()> {
    let _ = (paths, config, open_browser, demo);
    Err(fail(
        lang.t("this build of Kanjo has no dashboard server.", "ta wersja Kanjo nie ma wbudowanego serwera pulpitu.").into(),
        lang.t(
            "install an official release, or build with the \"server\" feature: cargo build -p kanjo --features server.",
            "zainstaluj oficjalne wydanie albo zbuduj z funkcją \"server\": cargo build -p kanjo --features server.",
        )
        .into(),
    ))
}

fn exe(lang: Lang) -> CliResult<PathBuf> {
    std::env::current_exe().map_err(|e| {
        fail(
            tr!(lang, "Kanjo cannot find its own program file ({e}).", "Kanjo nie może znaleźć własnego pliku programu ({e})."),
            lang.t(
                "run Kanjo from where it is installed, not through a temporary link.",
                "uruchom Kanjo z miejsca instalacji, a nie przez tymczasowy skrót.",
            )
            .into(),
        )
    })
}

/// (projects, main transcripts) under `<claude>/projects`.
fn count_transcripts(projects: &Path) -> (usize, usize) {
    let Ok(dirs) = std::fs::read_dir(projects) else { return (0, 0) };
    let mut n_projects = 0;
    let mut n_files = 0;
    for dir in dirs.flatten() {
        if !dir.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let files = std::fs::read_dir(dir.path())
            .map(|it| {
                it.flatten()
                    .filter(|f| f.path().extension().is_some_and(|e| e == "jsonl") && f.file_type().is_ok_and(|t| t.is_file()))
                    .count()
            })
            .unwrap_or(0);
        if files > 0 {
            n_projects += 1;
            n_files += files;
        }
    }
    (n_projects, n_files)
}

fn service_kind(lang: Lang) -> &'static str {
    match std::env::consts::OS {
        "linux" => lang.t("a systemd user service", "usługa użytkownika systemd"),
        "macos" => lang.t("a LaunchAgent", "LaunchAgent"),
        "windows" => lang.t("a sign-in entry", "wpis uruchamiany przy logowaniu"),
        _ => lang.t("a background service", "usługa w tle"),
    }
}

pub fn up(app: &mut App, yes: bool, no_open: bool, no_hooks: bool, no_service: bool) -> CliResult<()> {
    let lang = app.lang;
    let claude = app.paths.claude_dir.clone();
    if !claude.is_dir() {
        return Err(fail(
            tr!(lang, "Claude Code's directory {} does not exist.", "katalog Claude Code {} nie istnieje.", claude.display()),
            lang.t(
                "install Claude Code and use it once, or pass --claude-dir DIR if it lives somewhere else.",
                "zainstaluj Claude Code i uruchom go raz albo podaj --claude-dir KATALOG, jeśli jest gdzie indziej.",
            )
            .into(),
        ));
    }
    let (projects, transcripts) = count_transcripts(&app.paths.claude_projects());
    let status = kanjo_setup::claude::status(&app.paths, &app.config)?;
    out(&app.bold(lang.t("Kanjo found:", "Kanjo znalazł:")));
    out(&tr!(lang, "  Claude Code in {}", "  Claude Code w katalogu {}", claude.display()));
    if transcripts > 0 {
        let (t_word, p_word) =
            (if transcripts == 1 { "transcript" } else { "transcripts" }, if projects == 1 { "project" } else { "projects" });
        out(&tr!(
            lang,
            "  {transcripts} {t_word} in {projects} {p_word}; their costs are read in the background",
            "  transkrypty: {transcripts}, projekty: {projects}; koszty zostaną wczytane w tle"
        ));
    } else {
        out(lang.t(
            "  no transcripts yet; costs appear as soon as you use Claude Code",
            "  brak transkryptów; koszty pojawią się, gdy tylko użyjesz Claude Code",
        ));
    }
    if !no_hooks {
        if let Some(err) = &status.settings_error {
            return Err(fail(
                tr!(
                    lang,
                    "{} is not valid JSON ({err}), so Kanjo will not edit it.",
                    "plik {} nie jest poprawnym JSON-em ({err}), więc Kanjo go nie zmieni.",
                    status.settings_path.display()
                ),
                lang.t(
                    "fix the file in an editor and run \"kanjo up\" again, or add --no-hooks to skip it.",
                    "popraw plik w edytorze i uruchom \"kanjo up\" ponownie albo dodaj --no-hooks, żeby go pominąć.",
                )
                .into(),
            ));
        }
    }
    out("");
    out(&app.bold(lang.t("Kanjo will:", "Co zrobi Kanjo:")));
    if !no_hooks {
        let line = match &status.statusline {
            StatuslineSlot::Missing => lang
                .t(
                    "  add a status line to Claude Code that shows cost and limits",
                    "  doda do Claude Code pasek stanu z kosztem i limitami",
                )
                .to_string(),
            StatuslineSlot::Kanjo { .. } => {
                lang.t("  keep its status line (already there)", "  zostawi swój pasek stanu (już jest)").to_string()
            }
            StatuslineSlot::Foreign { command } => tr!(
                lang,
                "  put itself in front of your status line{}; yours keeps showing, Kanjo only records the data",
                "  stanie przed Twoim paskiem stanu{}; Twój nadal będzie widoczny, Kanjo tylko zapisze dane",
                command.as_deref().map(|c| format!(" ({c})")).unwrap_or_default()
            ),
        };
        out(&line);
        if status.session_hooks_installed() {
            out(lang.t("  keep its three session hooks (already there)", "  zostawi swoje trzy hooki sesji (już są)"));
        } else {
            out(lang.t(
                "  add three hooks (SessionStart, Stop, SessionEnd) that note when sessions start and end",
                "  doda trzy hooki (SessionStart, Stop, SessionEnd), które odnotują początek i koniec sesji",
            ));
        }
        out(&tr!(
            lang,
            "  save a copy of {} before changing it",
            "  zapisze kopię pliku {}, zanim go zmieni",
            status.settings_path.display()
        ));
    }
    if no_service {
        out(lang.t("  run the dashboard in this terminal (Ctrl-C stops it)", "  uruchomi pulpit w tym terminalu (Ctrl-C zatrzymuje)"));
    } else {
        out(&tr!(
            lang,
            "  install {} so Kanjo starts when you log in",
            "  zainstaluje {}, żeby Kanjo startował po zalogowaniu",
            service_kind(lang)
        ));
    }
    if !no_open {
        out(lang.t("  open the dashboard in your browser", "  otworzy pulpit w przeglądarce"));
    }
    out(lang.t("Nothing leaves this computer: Kanjo works offline.", "Nic nie opuszcza tego komputera: Kanjo działa offline."));
    if !yes {
        let ok = app.confirm(
            lang.t("Continue?", "Kontynuować?"),
            true,
            lang.t("run \"kanjo up --yes\" to accept without being asked.", "uruchom \"kanjo up --yes\", żeby zgodzić się bez pytania."),
        )?;
        if !ok {
            out(lang.t("Nothing was changed.", "Nic nie zostało zmienione."));
            return Ok(());
        }
    }
    let exe = exe(lang)?;
    let mut installed = None;
    if !no_hooks {
        let req = HookInstallRequest { statusline: true, session_hooks: true, service: false };
        let mut config = app.config.clone();
        let report = kanjo_setup::claude::install(&app.paths, &mut config, &req, &exe)?;
        app.config = config;
        if report.changed {
            out(&tr!(lang, "Updated {}.", "Zaktualizowano {}.", report.settings_path.display()));
            if let Some(b) = &report.backup {
                out(&tr!(lang, "  The previous version is in {}.", "  Poprzednia wersja jest w {}.", b.display()));
            }
        } else {
            out(lang.t("Claude Code was already set up.", "Claude Code był już skonfigurowany."));
        }
        installed = Some(report);
    }
    if !no_service {
        install_service(app, &exe)?;
    }
    app.store_or_create()?;
    if let Some(report) = &installed {
        let detail = json!({
            "statusline": true,
            "session_hooks": true,
            "service": !no_service,
            "changed": report.changed,
            "settings_backup": report.backup.as_ref().and_then(|b| b.file_name()).map(|n| n.to_string_lossy().into_owned()),
            "via": "cli",
        });
        app.audit("hooks.install", &report.settings_path.display().to_string(), detail);
    } else if !no_service {
        app.audit("service.install", &exe.display().to_string(), json!({ "via": "cli" }));
    }
    app.close();
    if no_service {
        return run_server(lang, app.paths.clone(), app.config.clone(), !no_open, false);
    }
    finish_start(app, no_open)
}

fn install_service(app: &mut App, exe: &Path) -> CliResult<()> {
    let lang = app.lang;
    kanjo_setup::service::install(exe, &app.paths).map_err(|e| {
        fail(
            tr!(lang, "the background service could not be installed ({e}).", "nie udało się zainstalować usługi w tle ({e})."),
            lang.t(
                "run \"kanjo up --no-service\" to keep Kanjo running in this terminal instead.",
                "uruchom \"kanjo up --no-service\", żeby zamiast tego Kanjo działał w tym terminalu.",
            )
            .into(),
        )
    })?;
    out(lang.t(
        "The background service is installed and starts with your session.",
        "Usługa w tle jest zainstalowana i startuje razem z sesją.",
    ));
    Ok(())
}

fn finish_start(app: &mut App, no_open: bool) -> CliResult<()> {
    let lang = app.lang;
    if !wait_for_server(&app.config, Duration::from_secs(15)) {
        out(lang.t(
            "The dashboard does not answer yet. Check with \"kanjo service status\" in a moment, then run \"kanjo open\".",
            "Pulpit jeszcze nie odpowiada. Za chwilę sprawdź \"kanjo service status\", a potem uruchom \"kanjo open\".",
        ));
        return Ok(());
    }
    let url = dashboard_url(&app.paths, &app.config)?;
    if no_open {
        out(&tr!(lang, "Kanjo is running: {url}", "Kanjo działa: {url}"));
    } else {
        open_browser(lang, &url);
    }
    Ok(())
}

pub fn serve(app: &mut App, port: Option<u16>, bind: Option<String>, no_open: bool) -> CliResult<()> {
    let mut config = app.config.clone();
    if let Some(p) = port {
        config.server.port = p;
    }
    if let Some(b) = bind {
        config.server.bind = b;
    }
    app.close();
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdout());
    run_server(app.lang, app.paths.clone(), config, !no_open && interactive, false)
}

pub fn demo(g: &Global, port: Option<u16>, no_open: bool, days: Option<u32>, seed_only: bool, seed: u64) -> CliResult<()> {
    let lang = g.lang.unwrap_or_else(Lang::from_env);
    let explicit = g.home.clone().or_else(|| std::env::var_os("KANJO_HOME").filter(|v| !v.is_empty()).map(PathBuf::from));
    let mut temp = None;
    let home = match explicit {
        Some(h) => h,
        None => {
            let dir = tempfile::Builder::new().prefix("kanjo-demo-").tempdir()?;
            let path = dir.path().to_path_buf();
            temp = Some(dir);
            path
        }
    };
    let claude = g
        .claude_dir
        .clone()
        .or_else(|| std::env::var_os("KANJO_CLAUDE_DIR").filter(|v| !v.is_empty()).map(PathBuf::from))
        .unwrap_or_else(|| home.join("claude-demo"));
    let paths = Paths::at(&home, &claude);
    let mut config = Config::load(&paths)?;
    config.storage.encryption = EncryptionMode::Off;
    match port {
        Some(p) => config.server.port = p,
        None if !paths.config_file().exists() => config.server.port = DEMO_PORT,
        None => {}
    }
    if seed_only {
        let (store, _) = kanjo_ops::open_store_with(&paths, &config, None)?;
        kanjo_ops::demo::populate(&store, kanjo_core::time::now_ms(), days.unwrap_or(30).max(1), seed).map_err(|e| {
            fail(
                tr!(lang, "the demo cannot use {} ({e}).", "demo nie może użyć katalogu {} ({e}).", home.display()),
                lang.t(
                    "point KANJO_HOME at an empty directory, or leave it unset so the demo uses a temporary one.",
                    "wskaż w KANJO_HOME pusty katalog albo go nie ustawiaj, a demo użyje tymczasowego.",
                )
                .into(),
            )
        })?;
        drop(store);
        config.save(&paths)?;
        out(&tr!(lang, "Demo data written to {}.", "Dane demonstracyjne zapisano w {}.", home.display()));
        return Ok(());
    }
    if days.is_some_and(|d| d != 30) {
        out(lang.t(
            "Note: the dashboard server always invents 30 days of history; --days only applies with --seed-only.",
            "Uwaga: serwer pulpitu zawsze generuje 30 dni historii; --days działa tylko z --seed-only.",
        ));
    }
    if temp.is_some() {
        out(&tr!(
            lang,
            "Demo with invented data in {} (deleted when you stop it).",
            "Demo z wymyślonymi danymi w {} (zostaną usunięte po zatrzymaniu).",
            home.display()
        ));
    }
    let result = run_server(lang, paths, config, !no_open, true);
    drop(temp);
    result
}

pub fn open_dashboard(app: &mut App) -> CliResult<()> {
    let lang = app.lang;
    if !server_running(&app.config) {
        return Err(fail(
            lang.t("the dashboard is not running.", "pulpit nie jest uruchomiony.").into(),
            lang.t(
                "start it with \"kanjo up\" (in the background) or \"kanjo serve\" (in this terminal).",
                "uruchom go poleceniem \"kanjo up\" (w tle) albo \"kanjo serve\" (w tym terminalu).",
            )
            .into(),
        ));
    }
    let url = dashboard_url(&app.paths, &app.config)?;
    if app.json {
        return app.print_json(&json!({ "url": url }));
    }
    open_browser(lang, &url);
    Ok(())
}

pub fn service(app: &mut App, action: &ServiceAction) -> CliResult<()> {
    let lang = app.lang;
    match action {
        ServiceAction::Install => {
            let exe = exe(lang)?;
            if app.paths.db().exists() {
                let _ = app.store();
            }
            install_service(app, &exe)?;
            app.audit("service.install", &exe.display().to_string(), json!({ "via": "cli" }));
            app.close();
            finish_start(app, true)
        }
        ServiceAction::Uninstall => {
            if !kanjo_setup::service::is_installed() {
                out(lang.t("The background service is not installed.", "Usługa w tle nie jest zainstalowana."));
                return Ok(());
            }
            kanjo_setup::service::uninstall()?;
            if app.paths.db().exists() && app.store().is_ok() {
                app.audit("service.uninstall", "service", json!({ "via": "cli" }));
            }
            out(lang.t(
                "The background service is stopped and removed. Start Kanjo by hand with \"kanjo serve\" when you need it.",
                "Usługa w tle została zatrzymana i usunięta. Gdy będzie potrzebna, uruchom Kanjo ręcznie poleceniem \"kanjo serve\".",
            ));
            Ok(())
        }
        ServiceAction::Status => {
            let state = kanjo_setup::service::status();
            let answering = server_running(&app.config);
            if app.json {
                return app.print_json(&json!({ "service": state, "dashboard_answering": answering, "port": app.config.server.port }));
            }
            let text = match state {
                ServiceState::Running => lang.t("The background service is running.", "Usługa w tle działa."),
                ServiceState::Stopped => lang.t(
                    "The background service is installed but stopped. \"kanjo service install\" starts it again.",
                    "Usługa w tle jest zainstalowana, ale zatrzymana. \"kanjo service install\" uruchomi ją ponownie.",
                ),
                ServiceState::Installed => lang.t("The background service is installed.", "Usługa w tle jest zainstalowana."),
                ServiceState::NotInstalled => lang.t(
                    "The background service is not installed. \"kanjo up\" sets it up.",
                    "Usługa w tle nie jest zainstalowana. \"kanjo up\" ją skonfiguruje.",
                ),
                ServiceState::Unsupported => lang.t(
                    "This system has no service manager Kanjo supports; run \"kanjo serve\" instead.",
                    "Ten system nie ma menedżera usług obsługiwanego przez Kanjo; użyj \"kanjo serve\".",
                ),
            };
            out(text);
            out(&if answering {
                tr!(lang, "The dashboard answers on port {}.", "Pulpit odpowiada na porcie {}.", app.config.server.port)
            } else {
                tr!(lang, "Nothing answers on port {}.", "Na porcie {} nic nie odpowiada.", app.config.server.port)
            });
            Ok(())
        }
    }
}

fn safe_to_delete(paths: &Paths) -> bool {
    let home = &paths.home;
    let ours = ["kanjo.db", "config.toml", "kanjo.key", "access.token", "spool", "backups"].iter().any(|n| home.join(n).exists());
    let top = home.parent().is_none() || home.parent().and_then(Path::parent).is_none();
    let holds_claude = paths.claude_dir.starts_with(home);
    ours && !top && !holds_claude
}

pub fn uninstall(app: &mut App, purge_data: bool, yes: bool, no_service: bool) -> CliResult<()> {
    let lang = app.lang;
    if !yes {
        let q = if purge_data {
            tr!(
                lang,
                "Remove Kanjo from Claude Code, stop the background service and DELETE all Kanjo data in {}?",
                "Usunąć Kanjo z Claude Code, zatrzymać usługę w tle i SKASOWAĆ wszystkie dane Kanjo z {}?",
                app.paths.home.display()
            )
        } else {
            lang.t(
                "Remove Kanjo from Claude Code and stop the background service? Your data stays.",
                "Usunąć Kanjo z Claude Code i zatrzymać usługę w tle? Dane zostaną.",
            )
            .into()
        };
        if !app.confirm(&q, false, lang.t("add --yes to go ahead without being asked.", "dodaj --yes, żeby kontynuować bez pytania."))? {
            out(lang.t("Nothing was changed.", "Nic nie zostało zmienione."));
            return Ok(());
        }
    }
    let exe = exe(lang)?;
    let before = kanjo_setup::claude::status(&app.paths, &app.config).ok();
    let mut config = app.config.clone();
    let report = kanjo_setup::claude::uninstall(&app.paths, &mut config, &exe)?;
    app.config = config;
    if report.changed {
        out(&tr!(lang, "Removed Kanjo from {}.", "Usunięto Kanjo z {}.", report.settings_path.display()));
        if let Some(cmd) = before.and_then(|s| s.chain_command) {
            out(&tr!(lang, "  Your previous status line is back: {cmd}", "  Twój poprzedni pasek stanu wrócił: {cmd}"));
        }
    } else {
        out(lang.t("Claude Code's settings had nothing from Kanjo.", "Ustawienia Claude Code nie zawierały niczego od Kanjo."));
    }
    if !purge_data && app.paths.db().exists() && app.store().is_ok() {
        app.audit("hooks.uninstall", &report.settings_path.display().to_string(), json!({ "changed": report.changed, "via": "cli" }));
    }
    if !no_service && kanjo_setup::service::is_installed() {
        kanjo_setup::service::uninstall()?;
        out(lang.t("Stopped and removed the background service.", "Zatrzymano i usunięto usługę w tle."));
    }
    if purge_data {
        app.close();
        if !app.paths.home.exists() {
            out(lang.t("There was no data directory to delete.", "Nie było katalogu danych do usunięcia."));
        } else if !safe_to_delete(&app.paths) {
            return Err(fail(
                tr!(
                    lang,
                    "{} does not look like a Kanjo data directory, so it was left alone.",
                    "{} nie wygląda na katalog danych Kanjo, więc go nie ruszono.",
                    app.paths.home.display()
                ),
                lang.t("delete the directory yourself if you are sure.", "usuń ten katalog samodzielnie, jeśli masz pewność.").into(),
            ));
        } else {
            std::fs::remove_dir_all(&app.paths.home)?;
            out(&tr!(lang, "Deleted {}.", "Usunięto {}.", app.paths.home.display()));
            out(lang.t(
                "If the database key was kept in the system keyring, its \"kanjo\" entry is still there; remove it with your keyring tool.",
                "Jeśli klucz bazy był w systemowym pęku kluczy, wpis \"kanjo\" nadal tam jest; usuń go narzędziem do obsługi kluczy.",
            ));
        }
    }
    Ok(())
}

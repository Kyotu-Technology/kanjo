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
use kanjo_setup::server_file::{self, ServerRecord};
use kanjo_setup::service::ServiceState;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const DEMO_PORT: u16 = 4748;

/// Ends the server the background service started. systemd and launchd stop
/// their process when the service is removed; the Windows logon entry has no
/// manager behind it, so there the running server is ended by the process id
/// it announced. The database is crash-safe, so nothing is lost.
fn stop_service_server(paths: &Paths) {
    #[cfg(windows)]
    if let Some(rec) = server_file::live(paths) {
        use std::process::{Command, Stdio};
        let _ = Command::new("taskkill")
            .args(["/PID", &rec.pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let deadline = Instant::now() + Duration::from_secs(5);
        while server_file::live(paths).is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    #[cfg(not(windows))]
    let _ = paths;
}

fn service_running() -> bool {
    kanjo_setup::service::is_installed() && kanjo_setup::service::status() == ServiceState::Running
}

/// True when a server uses this data directory (it announced itself in
/// server.json and is still alive), or the background service reports running.
pub fn server_running(paths: &Paths) -> bool {
    server_file::live(paths).is_some() || service_running()
}

/// The server of this data directory, once it accepts connections.
fn wait_for_server(paths: &Paths, limit: Duration) -> Option<ServerRecord> {
    let start = Instant::now();
    loop {
        if let Some(rec) = server_file::answering(paths) {
            return Some(rec);
        }
        if start.elapsed() >= limit {
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// A sign-in URL for `rec`: a single-use login code valid for two minutes,
/// never the access token itself.
fn sign_in_url(paths: &Paths, rec: &ServerRecord) -> CliResult<String> {
    Ok(server_file::login_url(paths, rec, kanjo_core::time::now_ms())?)
}

/// Opens `url`. A sign-in URL goes through an owner-only local page, so the
/// single-use code never appears on the opener's command line; the page is
/// removed once the browser has used it. When the browser has not used it in
/// time (a sandboxed browser may be unable to read the page), the address is
/// printed instead.
fn open_browser(lang: Lang, paths: &Paths, url: &str) {
    let page = server_file::login_code_of(url).and_then(|code| server_file::launch_page(paths, url).ok().map(|p| (p, code)));
    let opened = match &page {
        Some((p, _)) => open::that(p),
        None => open::that(url),
    };
    let redeemed = match (&opened, &page) {
        (Ok(()), Some((p, code))) => server_file::retire_launch_page(paths, p, code, server_file::LAUNCH_PAGE_TTL),
        (Ok(()), None) => true,
        (Err(_), Some((p, _))) => {
            let _ = std::fs::remove_file(p);
            false
        }
        (Err(_), None) => false,
    };
    if redeemed {
        out(lang.t("Opened the dashboard in your browser.", "Otwarto pulpit w przeglądarce."));
        return;
    }
    out(if opened.is_ok() {
        lang.t(
            "The browser did not sign in. If it shows no dashboard, open this address yourself:",
            "Przeglądarka się nie zalogowała. Jeśli nie widać pulpitu, otwórz ten adres samodzielnie:",
        )
    } else {
        lang.t(
            "No browser could be started. Open this address yourself:",
            "Nie udało się uruchomić przeglądarki. Otwórz ten adres samodzielnie:",
        )
    });
    out(&format!("  {url}"));
}

/// The failure for a data directory another server already uses.
#[cfg(feature = "server")]
fn already_running(lang: Lang, paths: &Paths, rec: &ServerRecord) -> Failure {
    fail(
        tr!(
            lang,
            "Kanjo is already running for {} at {} (process {}).",
            "Kanjo już działa dla katalogu {} pod adresem {} (proces {}).",
            paths.home.display(),
            rec.url,
            rec.pid
        ),
        lang.t(
            "open it with \"kanjo open\", or stop it before starting another one.",
            "otwórz go poleceniem \"kanjo open\" albo zatrzymaj go, zanim uruchomisz kolejny.",
        )
        .into(),
    )
}

#[cfg(feature = "server")]
fn run_server(lang: Lang, paths: Paths, config: Config, open_browser: bool, demo: bool, demo_days: Option<u32>) -> CliResult<()> {
    if let Err(e) = kanjo_server::check_bind(&config) {
        return Err(fail(
            tr!(lang, "Kanjo cannot listen as configured ({e}).", "Kanjo nie może nasłuchiwać tak, jak skonfigurowano ({e})."),
            lang.t(
                "to reach the dashboard from other machines, set server.tls_cert and server.tls_key (and keep auth = \"token\") in config.toml; otherwise leave --bind out so it stays on 127.0.0.1.",
                "żeby pulpit był dostępny z innych komputerów, ustaw server.tls_cert i server.tls_key (i zostaw auth = \"token\") w config.toml; w przeciwnym razie pomiń --bind, a pulpit zostanie na 127.0.0.1.",
            )
            .into(),
        ));
    }
    if let Some(rec) = server_file::live(&paths) {
        return Err(already_running(lang, &paths, &rec));
    }
    if let Some(f) = crate::app::plain_database(lang, &paths, &config) {
        return Err(f);
    }
    crate::app::init_logging(tracing_subscriber::filter::LevelFilter::INFO);
    let port = config.server.port;
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let home = paths.home.clone();
    let ready = |server: &kanjo_server::Server| {
        out(&tr!(lang, "Kanjo dashboard on {} (Ctrl-C stops it).", "Pulpit Kanjo pod adresem {} (Ctrl-C zatrzymuje).", server.url()));
        if demo || (interactive && !open_browser) {
            let link = server.login_url();
            if link != server.url() {
                out(&tr!(
                    lang,
                    "Sign in with this link (it works once, within two minutes): {link}",
                    "Zaloguj się tym linkiem (działa raz, przez dwie minuty): {link}"
                ));
                out(&tr!(
                    lang,
                    "Later, \"kanjo --home {} open\" signs you in again.",
                    "Później \"kanjo --home {} open\" zaloguje Cię ponownie.",
                    home.display()
                ));
            }
        }
    };
    let opts = kanjo_server::ServeOptions { paths: paths.clone(), config, open_browser, demo, demo_days };
    kanjo_server::run_with(opts, ready).map_err(|e| match &e {
        kanjo_core::Error::Io(io) if io.kind() == std::io::ErrorKind::AddrInUse => fail(
            tr!(lang, "port {port} is already taken ({e}).", "port {port} jest już zajęty ({e})."),
            tr!(
                lang,
                "another program holds it; choose another port with --port N or set server.port in config.toml.",
                "zajmuje go inny program; wybierz inny port opcją --port N albo ustaw server.port w config.toml."
            ),
        ),
        _ => match server_file::live(&paths) {
            Some(rec) => already_running(lang, &paths, &rec),
            None => Failure::Core(e),
        },
    })
}

#[cfg(not(feature = "server"))]
fn run_server(lang: Lang, paths: Paths, config: Config, open_browser: bool, demo: bool, demo_days: Option<u32>) -> CliResult<()> {
    let _ = (paths, config, open_browser, demo, demo_days);
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
    kanjo_setup::current_exe().map_err(|e| {
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
    out(&app.bold(lang.t("Kanjo found:", "Kanjo znalazło:")));
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
            "  zainstaluje {}, żeby Kanjo startowało po zalogowaniu",
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
        return run_server(lang, app.paths.clone(), app.config.clone(), !no_open, false, None);
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
                "uruchom \"kanjo up --no-service\", żeby zamiast tego Kanjo działało w tym terminalu.",
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
    let Some(rec) = wait_for_server(&app.paths, Duration::from_secs(15)) else {
        out(lang.t(
            "The dashboard does not answer yet. Check with \"kanjo service status\" in a moment, then run \"kanjo open\".",
            "Pulpit jeszcze nie odpowiada. Za chwilę sprawdź \"kanjo service status\", a potem uruchom \"kanjo open\".",
        ));
        return Ok(());
    };
    if no_open {
        out(&tr!(lang, "Kanjo is running: {} (\"kanjo open\" signs you in).", "Kanjo działa: {} (\"kanjo open\" Cię zaloguje).", rec.url));
    } else {
        open_browser(lang, &app.paths, &sign_in_url(&app.paths, &rec)?);
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
    run_server(app.lang, app.paths.clone(), config, !no_open && interactive, false, None)
}

/// Why `paths` belongs to a real installation, which a demo must not use.
fn real_installation(paths: &Paths) -> Option<&'static str> {
    let default = kanjo_core::paths::UserDirs::current().map(|d| d.kanjo_data_dir());
    if default.as_deref() == Some(paths.home.as_path()) {
        return Some("default");
    }
    if paths.key_file().exists() || paths.spool_dir().exists() || paths.backups_dir().exists() {
        return Some("files");
    }
    if paths.db().exists() && !kanjo_setup::doctor::is_plaintext_db(&paths.db()) {
        return Some("encrypted");
    }
    None
}

/// `kanjo demo`. The live demo uses `--home` when given and a temporary
/// directory otherwise; `KANJO_HOME` (usually the real data directory) is only
/// honoured by the hidden `--seed-only`. Either way a directory that holds a
/// real installation is refused.
pub fn demo(g: &Global, port: Option<u16>, no_open: bool, days: Option<u32>, seed_only: bool, seed: u64) -> CliResult<()> {
    let lang = g.lang.unwrap_or_else(Lang::from_env);
    let from_env = || std::env::var_os("KANJO_HOME").filter(|v| !v.is_empty()).map(PathBuf::from);
    let explicit = g.home.clone().or_else(|| if seed_only { from_env() } else { None });
    let mut temp = None;
    let home = match explicit {
        Some(h) => std::path::absolute(&h).unwrap_or(h),
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
    if temp.is_none() {
        if let Some(why) = real_installation(&paths) {
            let message = match why {
                "default" => tr!(
                    lang,
                    "{} is Kanjo's own data directory, so the demo will not use it.",
                    "{} to własny katalog danych Kanjo, więc demo z niego nie skorzysta.",
                    home.display()
                ),
                _ => tr!(
                    lang,
                    "{} holds a real Kanjo installation, so the demo will not use it.",
                    "w katalogu {} jest prawdziwa instalacja Kanjo, więc demo z niego nie skorzysta.",
                    home.display()
                ),
            };
            return Err(fail(
                message,
                lang.t(
                    "leave --home out (and KANJO_HOME unset for --seed-only) so the demo uses a temporary directory, or point it at an empty one.",
                    "pomiń --home (a przy --seed-only nie ustawiaj KANJO_HOME), żeby demo użyło katalogu tymczasowego, albo wskaż pusty katalog.",
                )
                .into(),
            ));
        }
    }
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
    config.save(&paths)?;
    if temp.is_some() {
        out(&tr!(
            lang,
            "Demo with invented data in {} (deleted when you stop it).",
            "Demo z wymyślonymi danymi w {} (zostaną usunięte po zatrzymaniu).",
            home.display()
        ));
    }
    let result = run_server(lang, paths, config, !no_open, true, days);
    drop(temp);
    result
}

pub fn open_dashboard(app: &mut App) -> CliResult<()> {
    let lang = app.lang;
    let limit = if service_running() { Duration::from_secs(5) } else { Duration::ZERO };
    let Some(rec) = wait_for_server(&app.paths, limit) else {
        return Err(fail(
            lang.t("the dashboard is not running.", "pulpit nie jest uruchomiony.").into(),
            lang.t(
                "start it with \"kanjo up\" (in the background) or \"kanjo serve\" (in this terminal).",
                "uruchom go poleceniem \"kanjo up\" (w tle) albo \"kanjo serve\" (w tym terminalu).",
            )
            .into(),
        ));
    };
    let url = sign_in_url(&app.paths, &rec)?;
    if app.json {
        return app.print_json(&json!({ "url": url }));
    }
    open_browser(lang, &app.paths, &url);
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
            stop_service_server(&app.paths);
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
            let rec = server_file::answering(&app.paths);
            let answering = rec.is_some();
            let port = rec.as_ref().map_or(app.config.server.port, |r| r.addr.port());
            if app.json {
                return app
                    .print_json(&json!({ "service": state, "dashboard_answering": answering, "port": port, "url": rec.map(|r| r.url) }));
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
                tr!(lang, "The dashboard answers on port {port}.", "Pulpit odpowiada na porcie {port}.")
            } else {
                tr!(lang, "No dashboard is running for {}.", "Dla katalogu {} nie działa żaden pulpit.", app.paths.home.display())
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
        stop_service_server(&app.paths);
        out(lang.t("Stopped and removed the background service.", "Zatrzymano i usunięto usługę w tle."));
    }
    if purge_data {
        app.close();
        if let Some(rec) = server_file::live(&app.paths) {
            return Err(fail(
                tr!(
                    lang,
                    "Kanjo is still running at {} (process {}), so its data cannot be deleted now.",
                    "Kanjo nadal działa pod adresem {} (proces {}), więc jego danych nie można teraz usunąć.",
                    rec.url,
                    rec.pid
                ),
                crate::data::stop_hint(lang),
            ));
        }
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

use crate::error::{fail, CliResult, Failure};
use crate::i18n::Lang;
use kanjo_core::api::KeySource;
use kanjo_core::config::Config;
use kanjo_core::paths::Paths;
use kanjo_core::settings::Settings;
use kanjo_core::{Ctx, Store};
use std::io::{BufRead, IsTerminal, Write};
use std::sync::Arc;

pub struct Global {
    pub home: Option<std::path::PathBuf>,
    pub claude_dir: Option<std::path::PathBuf>,
    pub lang: Option<Lang>,
    pub no_color: bool,
    pub json: bool,
}

/// Everything a command needs: locations, bootstrap config, language, and the
/// store, opened on first use.
pub struct App {
    pub paths: Paths,
    pub config: Config,
    pub lang: Lang,
    pub color: bool,
    pub json: bool,
    lang_flag: Option<Lang>,
    store: Option<(Arc<Store>, KeySource)>,
    settings: Option<Settings>,
}

impl App {
    pub fn new(g: &Global, settings_language: bool) -> CliResult<App> {
        let lang = g.lang.unwrap_or_else(Lang::from_env);
        let paths = Paths::resolve(g.home.clone(), g.claude_dir.clone()).map_err(|e| {
            fail(
                tr!(lang, "Kanjo cannot work out where to keep its data ({e}).", "Kanjo nie może ustalić, gdzie trzymać dane ({e})."),
                tr!(
                    lang,
                    "pass the data directory with --home DIR or set KANJO_HOME.",
                    "podaj katalog danych opcją --home KATALOG albo ustaw zmienną KANJO_HOME."
                ),
            )
        })?;
        let config = Config::load(&paths).map_err(|e| {
            fail(
                tr!(lang, "the configuration file cannot be read ({e}).", "nie można odczytać pliku konfiguracji ({e})."),
                tr!(
                    lang,
                    "fix or delete {} and run the command again.",
                    "popraw lub usuń plik {} i uruchom polecenie ponownie.",
                    paths.config_file().display()
                ),
            )
        })?;
        let color = !g.no_color && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()) && std::io::stdout().is_terminal();
        let mut app = App { paths, config, lang, color, json: g.json, lang_flag: g.lang, store: None, settings: None };
        if settings_language && g.lang.is_none() && app.paths.db().exists() {
            let _ = app.open(false);
        }
        Ok(app)
    }

    fn open(&mut self, create: bool) -> CliResult<()> {
        if self.store.is_some() {
            return Ok(());
        }
        if !create && !self.paths.db().exists() {
            return Err(self.no_data());
        }
        let (store, source) = kanjo_ops::open_store(&self.paths, &self.config)?;
        let settings = Settings::load(&store)?;
        if self.lang_flag.is_none() {
            if let Some(l) = Lang::from_settings(settings.language) {
                self.lang = l;
            }
        }
        self.settings = Some(settings);
        self.store = Some((Arc::new(store), source));
        Ok(())
    }

    pub fn no_data(&self) -> Failure {
        fail(
            tr!(self.lang, "there is no Kanjo data in {} yet.", "w katalogu {} nie ma jeszcze danych Kanjo.", self.paths.home.display()),
            tr!(
                self.lang,
                "run \"kanjo up\" to connect Claude Code, or \"kanjo import\" to read the transcripts you already have.",
                "uruchom \"kanjo up\", żeby podłączyć Claude Code, albo \"kanjo import\", żeby wczytać istniejące transkrypty."
            ),
        )
    }

    /// The store of an existing database; a friendly error when there is none yet.
    pub fn store(&mut self) -> CliResult<Arc<Store>> {
        self.open(false)?;
        self.store.as_ref().map(|(s, _)| s.clone()).ok_or_else(|| self.no_data())
    }

    /// The store, creating the database when it does not exist.
    pub fn store_or_create(&mut self) -> CliResult<Arc<Store>> {
        self.open(true)?;
        self.store.as_ref().map(|(s, _)| s.clone()).ok_or_else(|| self.no_data())
    }

    pub fn key_source(&self) -> Option<KeySource> {
        self.store.as_ref().map(|(_, k)| *k)
    }

    /// Closes the database (restore, purge of the data directory, or a server in this process).
    pub fn close(&mut self) {
        self.store = None;
    }

    pub fn settings(&self) -> Settings {
        self.settings.clone().unwrap_or_default()
    }

    pub fn ctx(&self) -> Ctx {
        let mut ctx = Ctx::system();
        if let Some(s) = &self.settings {
            ctx.week_start = s.week_start.min(6);
        }
        ctx
    }

    /// Records an audit entry when the database is open; failures are reported but do not undo the action.
    pub fn audit(&mut self, action: &str, target: &str, detail: serde_json::Value) {
        let Some((store, _)) = &self.store else { return };
        if let Err(e) = kanjo_ops::audit::record(store, action, target, detail) {
            eprintln!(
                "{}",
                tr!(
                    self.lang,
                    "Warning: the audit log could not be written ({e}).",
                    "Uwaga: nie udało się zapisać dziennika audytu ({e})."
                )
            );
        }
    }

    pub fn bold(&self, s: &str) -> String {
        if self.color {
            format!("\x1b[1m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn paint(&self, s: &str, code: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn print_json<T: serde::Serialize>(&self, value: &T) -> CliResult<()> {
        let text = serde_json::to_string_pretty(value).map_err(kanjo_core::Error::from)?;
        out(&text);
        Ok(())
    }

    /// Asks a yes/no question; Enter means `default`. Refuses without a terminal.
    pub fn confirm(&self, question: &str, default: bool, yes_flag_hint: &str) -> CliResult<bool> {
        self.require_terminal(yes_flag_hint)?;
        let suffix = match (self.lang, default) {
            (Lang::En, true) => "[Y/n]",
            (Lang::En, false) => "[y/N]",
            (Lang::Pl, true) => "[T/n]",
            (Lang::Pl, false) => "[t/N]",
        };
        let answer = prompt(&format!("{question} {suffix} "))?;
        let a = answer.trim().to_lowercase();
        Ok(match a.as_str() {
            "" => default,
            "y" | "yes" | "t" | "tak" => true,
            _ => false,
        })
    }

    pub fn require_terminal(&self, yes_flag_hint: &str) -> CliResult<()> {
        if std::io::stdin().is_terminal() {
            return Ok(());
        }
        Err(fail(
            tr!(
                self.lang,
                "this step needs your confirmation, but there is no terminal to ask in.",
                "ten krok wymaga Twojego potwierdzenia, ale nie ma terminala, w którym można o nie zapytać."
            ),
            yes_flag_hint.to_string(),
        ))
    }
}

pub fn prompt(text: &str) -> CliResult<String> {
    let mut err = std::io::stderr();
    let _ = write!(err, "{text}");
    let _ = err.flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line)
}

/// Engine log lines go to stderr (the service log when running in the background). `KANJO_DEBUG=1` shows everything.
pub fn init_logging(level: tracing_subscriber::filter::LevelFilter) {
    let level =
        if std::env::var_os("KANJO_DEBUG").is_some_and(|v| !v.is_empty()) { tracing_subscriber::filter::LevelFilter::DEBUG } else { level };
    let _ =
        tracing_subscriber::fmt().with_max_level(level).with_writer(std::io::stderr).with_ansi(std::io::stderr().is_terminal()).try_init();
}

/// Prints to stdout and ignores a closed pipe (`kanjo daily | head`).
pub fn out(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{text}");
}

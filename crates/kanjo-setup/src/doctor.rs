//! `kanjo doctor`: diagnostics with stable ids, in English and Polish.
//!
//! The caller passes what it knows about the database and the running agent in
//! `DoctorInput`; this module only looks at the filesystem (Claude's directory,
//! settings.json, Kanjo's data directory) and never opens the database.

use crate::claude::{self, StatuslineSlot, HOOK_EVENTS};
use crate::service::ServiceState;
use kanjo_core::api::{CheckStatus, DatabaseInfo, DoctorCheck, KeySource, PricingInfo};
use kanjo_core::config::{Config, EncryptionMode};
use kanjo_core::paths::Paths;
use kanjo_core::pricing::PriceTable;
use kanjo_core::store::SCHEMA_VERSION;
use kanjo_core::time::{DAY_MS, HOUR_MS, MINUTE_MS};
use std::path::Path;

/// First Claude Code version whose statusline payload carries prompt cache statistics.
pub const MIN_CC_CACHE_STATS: &str = "2.1.251";
/// First Claude Code version that reports prompt cache miss causes.
pub const MIN_CC_MISS_CAUSES: &str = "2.1.260";
/// Claude Code's default for `cleanupPeriodDays`.
pub const DEFAULT_CLEANUP_DAYS: i64 = 30;

/// Stable check ids, in output order.
pub const CHECK_IDS: [&str; 16] = [
    "claude_dir",
    "projects",
    "settings_json",
    "statusline",
    "chain",
    "session_hooks",
    "statusline_payload",
    "data_dir",
    "database",
    "encryption",
    "schema",
    "pricing",
    "quarantine",
    "service",
    "claude_code_version",
    "cleanup_period",
];

/// What the caller knows. Database facts are optional: `None` means unknown
/// (not opened), with `database_error` saying why when opening failed.
#[derive(Debug, Clone)]
pub struct DoctorInput<'a> {
    pub paths: &'a Paths,
    pub config: &'a Config,
    /// Epoch milliseconds.
    pub now: i64,
    /// The running executable; entries in settings.json pointing elsewhere are reported.
    pub exe: Option<&'a Path>,
    pub database: Option<DatabaseInfo>,
    pub database_error: Option<String>,
    pub last_statusline_at: Option<i64>,
    pub pricing: Option<PricingInfo>,
    pub quarantined_records: Option<i64>,
    /// Latest Claude Code version seen in transcripts or statusline payloads.
    pub claude_code_version: Option<String>,
    pub service: ServiceState,
}

impl<'a> DoctorInput<'a> {
    pub fn new(paths: &'a Paths, config: &'a Config, now: i64) -> DoctorInput<'a> {
        DoctorInput {
            paths,
            config,
            now,
            exe: None,
            database: None,
            database_error: None,
            last_statusline_at: None,
            pricing: None,
            quarantined_records: None,
            claude_code_version: None,
            service: ServiceState::NotInstalled,
        }
    }
}

struct Lang {
    pl: bool,
}

impl Lang {
    fn t<'s>(&self, en: &'s str, pl: &'s str) -> &'s str {
        if self.pl {
            pl
        } else {
            en
        }
    }
}

fn check(id: &str, status: CheckStatus, title: &str, detail: String, fix: Option<String>) -> DoctorCheck {
    DoctorCheck { id: id.to_string(), status, title: title.to_string(), detail, fix }
}

/// Parses the leading `major.minor.patch` of a version string.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let token = v.trim().split(|c: char| !(c.is_ascii_digit() || c == '.')).next()?;
    let mut it = token.split('.').map(|p| p.parse::<u64>());
    let major = it.next()?.ok()?;
    let minor = it.next().and_then(|r| r.ok()).unwrap_or(0);
    let patch = it.next().and_then(|r| r.ok()).unwrap_or(0);
    Some((major, minor, patch))
}

fn ago(l: &Lang, ms: i64) -> String {
    let ms = ms.max(0);
    if ms < MINUTE_MS {
        l.t("just now", "przed chwilą").to_string()
    } else if ms < HOUR_MS {
        format!("{} {}", ms / MINUTE_MS, l.t("min ago", "min temu"))
    } else if ms < DAY_MS {
        format!("{} {}", ms / HOUR_MS, l.t("h ago", "godz. temu"))
    } else {
        let d = ms / DAY_MS;
        format!("{d} {}", if l.pl { pl_plural(d, "dzień temu", "dni temu", "dni temu") } else { en_plural(d, "day ago", "days ago") })
    }
}

fn en_plural<'s>(n: i64, one: &'s str, many: &'s str) -> &'s str {
    if n == 1 {
        one
    } else {
        many
    }
}

/// Polish noun form for a count: 1 rekord, 2-4 rekordy (but 12-14 rekordów), 5+ rekordów.
fn pl_plural<'s>(n: i64, one: &'s str, few: &'s str, many: &'s str) -> &'s str {
    let n = n.abs();
    if n == 1 {
        one
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        few
    } else {
        many
    }
}

fn count_transcripts(dir: &Path) -> std::io::Result<(usize, usize)> {
    fn walk(dir: &Path, depth: u8, files: &mut usize) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else {
                continue;
            };
            if ft.is_dir() && depth < 4 {
                walk(&e.path(), depth + 1, files);
            } else if ft.is_file() && e.path().extension().is_some_and(|x| x == "jsonl") {
                *files += 1;
            }
        }
    }
    let mut projects = 0;
    let mut files = 0;
    for e in std::fs::read_dir(dir)?.flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            projects += 1;
            walk(&e.path(), 1, &mut files);
        }
    }
    Ok((projects, files))
}

#[cfg(unix)]
fn loose_permissions(paths: &Paths) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;
    let mut bad = Vec::new();
    let entries = [
        (paths.home.clone(), 0o077),
        (paths.db(), 0o077),
        (paths.key_file(), 0o077),
        (paths.token_file(), 0o077),
        (paths.config_file(), 0o077),
    ];
    for (p, mask) in entries {
        if let Ok(m) = std::fs::metadata(&p) {
            if m.permissions().mode() & mask != 0 {
                bad.push(format!("{} ({:o})", p.display(), m.permissions().mode() & 0o777));
            }
        }
    }
    bad
}

#[cfg(not(unix))]
fn loose_permissions(_paths: &Paths) -> Vec<String> {
    Vec::new()
}

/// Runs every check. `lang` is `pl` for Polish, anything else gives English.
pub fn run(input: &DoctorInput, lang: &str) -> Vec<DoctorCheck> {
    let l = Lang { pl: lang.eq_ignore_ascii_case("pl") || lang.to_ascii_lowercase().starts_with("pl-") };
    let p = input.paths;
    let mut out = Vec::with_capacity(CHECK_IDS.len());
    let up = "kanjo up";

    let title = l.t("Claude Code directory", "Katalog Claude Code");
    if p.claude_dir.is_dir() {
        out.push(check("claude_dir", CheckStatus::Ok, title, format!("{}", p.claude_dir.display()), None));
    } else {
        out.push(check(
            "claude_dir",
            CheckStatus::Fail,
            title,
            format!("{}: {}", p.claude_dir.display(), l.t("not found", "nie istnieje")),
            Some(
                l.t(
                    "Start Claude Code once, or point KANJO_CLAUDE_DIR (or CLAUDE_CONFIG_DIR) at its configuration directory.",
                    "Uruchom raz Claude Code albo wskaż jego katalog konfiguracji w KANJO_CLAUDE_DIR (lub CLAUDE_CONFIG_DIR).",
                )
                .into(),
            ),
        ));
    }

    let title = l.t("Session transcripts", "Transkrypty sesji");
    let projects = p.claude_projects();
    match count_transcripts(&projects) {
        Ok((0, _)) | Ok((_, 0)) => out.push(check(
            "projects",
            CheckStatus::Warn,
            title,
            format!("{}: {}", projects.display(), l.t("no transcripts yet", "brak transkryptów")),
            Some(
                l.t(
                    "Run a Claude Code session; Kanjo picks up new transcripts automatically.",
                    "Przeprowadź sesję w Claude Code; Kanjo sam wczyta nowe transkrypty.",
                )
                .into(),
            ),
        )),
        Ok((dirs, files)) => out.push(check(
            "projects",
            CheckStatus::Ok,
            title,
            if l.pl {
                format!(
                    "{files} {} w {dirs} {}",
                    pl_plural(files as i64, "transkrypt", "transkrypty", "transkryptów"),
                    if dirs == 1 { "projekcie" } else { "projektach" }
                )
            } else {
                format!(
                    "{files} {} in {dirs} {}",
                    en_plural(files as i64, "transcript", "transcripts"),
                    en_plural(dirs as i64, "project", "projects")
                )
            },
            None,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => out.push(check(
            "projects",
            CheckStatus::Warn,
            title,
            format!("{}: {}", projects.display(), l.t("not found", "nie istnieje")),
            Some(
                l.t(
                    "Run a Claude Code session; the directory is created with the first transcript.",
                    "Przeprowadź sesję w Claude Code; katalog powstanie wraz z pierwszym transkryptem.",
                )
                .into(),
            ),
        )),
        Err(e) => out.push(check(
            "projects",
            CheckStatus::Fail,
            title,
            format!("{}: {e}", projects.display()),
            Some(if l.pl {
                format!("Nadaj swojemu użytkownikowi prawo odczytu katalogu {}.", projects.display())
            } else {
                format!("Give your user read access to {}.", projects.display())
            }),
        )),
    }

    let integration = claude::status(p, input.config);
    let title = l.t("Claude Code settings.json", "Plik settings.json Claude Code");
    let settings_path = p.claude_settings();
    let st = match &integration {
        Ok(st) if st.settings_error.is_some() => {
            out.push(check(
                "settings_json",
                CheckStatus::Fail,
                title,
                st.settings_error.clone().unwrap_or_default(),
                Some(
                    l.t(
                        "Fix the JSON syntax of the file. Kanjo does not change a file it cannot parse.",
                        "Popraw składnię JSON w tym pliku. Kanjo nie zmienia pliku, którego nie umie odczytać.",
                    )
                    .into(),
                ),
            ));
            None
        }
        Ok(st) if !st.settings_exists => {
            out.push(check(
                "settings_json",
                CheckStatus::Warn,
                title,
                format!("{}: {}", settings_path.display(), l.t("does not exist yet", "jeszcze nie istnieje")),
                Some(if l.pl {
                    format!("Uruchom {up}; utworzy plik z wpisami Kanjo.")
                } else {
                    format!("Run {up}; it creates the file with Kanjo's entries.")
                }),
            ));
            Some(st)
        }
        Ok(st) => {
            out.push(check("settings_json", CheckStatus::Ok, title, format!("{}", settings_path.display()), None));
            Some(st)
        }
        Err(e) => {
            out.push(check("settings_json", CheckStatus::Fail, title, format!("{}: {e}", settings_path.display()), None));
            None
        }
    };

    let title = l.t("Statusline", "Statusline");
    let chain_title = l.t("Statusline chain", "Łańcuch statusline");
    let unknown = l.t("cannot be checked while settings.json is unreadable", "nie da się sprawdzić, dopóki settings.json jest nieczytelny");
    match st.map(|s| &s.statusline) {
        None => {
            out.push(check("statusline", CheckStatus::Warn, title, unknown.into(), None));
            out.push(check("chain", CheckStatus::Warn, chain_title, unknown.into(), None));
        }
        Some(StatuslineSlot::Kanjo { command }) => {
            let words = crate::shell::split_words(command);
            let program = words.first().map(String::as_str).unwrap_or("");
            let missing = !program.is_empty() && !Path::new(program).exists();
            let elsewhere = input.exe.is_some_and(|e| Path::new(program) != e && !missing);
            if missing {
                out.push(check(
                    "statusline",
                    CheckStatus::Fail,
                    title,
                    if l.pl {
                        format!("wpis wskazuje nieistniejący plik {program}")
                    } else {
                        format!("the entry points to a missing file {program}")
                    },
                    Some(if l.pl {
                        format!("Uruchom {up}, aby wskazać bieżący plik wykonywalny.")
                    } else {
                        format!("Run {up} to point it at the current executable.")
                    }),
                ));
            } else if elsewhere {
                out.push(check(
                    "statusline",
                    CheckStatus::Warn,
                    title,
                    if l.pl {
                        format!("zainstalowany, ale uruchamia inny plik: {program}")
                    } else {
                        format!("installed, but runs a different executable: {program}")
                    },
                    Some(if l.pl {
                        format!("Uruchom {up}, jeśli to ta kopia Kanjo ma obsługiwać statusline.")
                    } else {
                        format!("Run {up} if this copy of Kanjo should handle the statusline.")
                    }),
                ));
            } else {
                out.push(check("statusline", CheckStatus::Ok, title, l.t("installed", "zainstalowany").into(), None));
            }
            let chain = st.and_then(|s| s.chain_command.clone());
            match chain {
                Some(c) => out.push(check(
                    "chain",
                    CheckStatus::Ok,
                    chain_title,
                    if l.pl {
                        format!("Kanjo przekazuje dane do Twojej poprzedniej komendy: {c}")
                    } else {
                        format!("Kanjo forwards the payload to your previous command: {c}")
                    },
                    None,
                )),
                None => out.push(check(
                    "chain",
                    CheckStatus::Ok,
                    chain_title,
                    l.t(
                        "no previous statusline command; Kanjo shows its own line",
                        "brak poprzedniej komendy statusline; Kanjo pokazuje własną linię",
                    )
                    .into(),
                    None,
                )),
            }
        }
        Some(StatuslineSlot::Foreign { command }) => {
            let cmd = command.clone().unwrap_or_default();
            out.push(check(
                "statusline",
                CheckStatus::Warn,
                title,
                if l.pl {
                    format!("skonfigurowana jest inna komenda ({cmd}), więc Kanjo nie dostaje oficjalnych procentów limitów")
                } else {
                    format!("another command is configured ({cmd}), so Kanjo gets no official limit percentages")
                },
                Some(if l.pl {
                    format!("Uruchom {up}; Kanjo zajmie miejsce tej komendy i dalej będzie ją wywoływać.")
                } else {
                    format!("Run {up}; Kanjo takes its place and keeps running it after itself.")
                }),
            ));
            out.push(check("chain", CheckStatus::Warn, chain_title, l.t("not chained yet", "jeszcze nie w łańcuchu").into(), None));
        }
        Some(StatuslineSlot::Missing) => {
            out.push(check(
                "statusline",
                CheckStatus::Warn,
                title,
                l.t("not installed; limit percentages are estimates only", "niezainstalowany; procenty limitów są tylko szacunkiem").into(),
                Some(if l.pl {
                    format!("Uruchom {up}, aby zainstalować statusline.")
                } else {
                    format!("Run {up} to install the statusline.")
                }),
            ));
            out.push(check("chain", CheckStatus::Ok, chain_title, l.t("nothing to chain", "nie ma czego łączyć").into(), None));
        }
    }

    let title = l.t("Session hooks", "Hooki sesji");
    match st {
        None => out.push(check("session_hooks", CheckStatus::Warn, title, unknown.into(), None)),
        Some(s) if s.session_hooks_installed() => {
            out.push(check("session_hooks", CheckStatus::Ok, title, HOOK_EVENTS.join(", "), None));
        }
        Some(s) => {
            let missing: Vec<&str> = HOOK_EVENTS.iter().zip(s.hooks).filter(|(_, h)| !*h).map(|(e, _)| *e).collect();
            out.push(check(
                "session_hooks",
                CheckStatus::Warn,
                title,
                format!("{}: {}", l.t("missing", "brakuje"), missing.join(", ")),
                Some(if l.pl {
                    format!("Uruchom {up}, aby dodać hooki SessionStart, Stop i SessionEnd.")
                } else {
                    format!("Run {up} to add the SessionStart, Stop and SessionEnd hooks.")
                }),
            ));
        }
    }

    let title = l.t("Last statusline payload", "Ostatnie dane ze statusline");
    match input.last_statusline_at {
        Some(ts) if input.now - ts <= DAY_MS => {
            out.push(check("statusline_payload", CheckStatus::Ok, title, ago(&l, input.now - ts), None))
        }
        Some(ts) => out.push(check(
            "statusline_payload",
            CheckStatus::Warn,
            title,
            ago(&l, input.now - ts),
            Some(
                l.t(
                    "Normal if Claude Code has not run since; otherwise check the statusline entry above.",
                    "To normalne, jeśli od tego czasu nie używano Claude Code; w przeciwnym razie sprawdź wpis statusline powyżej.",
                )
                .into(),
            ),
        )),
        None => out.push(check(
            "statusline_payload",
            CheckStatus::Warn,
            title,
            l.t("none received yet", "jeszcze nic nie dotarło").into(),
            Some(
                l.t(
                    "Start a Claude Code session and send one message; the payload arrives after the first response.",
                    "Uruchom sesję Claude Code i wyślij jedną wiadomość; dane przychodzą po pierwszej odpowiedzi.",
                )
                .into(),
            ),
        )),
    }

    let title = l.t("Data directory", "Katalog danych");
    if !p.home.is_dir() {
        out.push(check(
            "data_dir",
            CheckStatus::Warn,
            title,
            format!("{}: {}", p.home.display(), l.t("not created yet", "jeszcze nie utworzony")),
            Some(
                l.t(
                    "Run kanjo serve once; it creates the directory with owner-only permissions.",
                    "Uruchom raz kanjo serve; utworzy katalog z uprawnieniami tylko dla właściciela.",
                )
                .into(),
            ),
        ));
    } else {
        let bad = loose_permissions(p);
        if bad.is_empty() {
            out.push(check(
                "data_dir",
                CheckStatus::Ok,
                title,
                format!("{} ({})", p.home.display(), l.t("owner-only access", "dostęp tylko dla właściciela")),
                None,
            ));
        } else {
            out.push(check(
                "data_dir",
                CheckStatus::Warn,
                title,
                format!("{}: {}", l.t("readable by other users", "dostępne dla innych użytkowników"), bad.join(", ")),
                Some(format!("chmod 700 '{0}' && chmod 600 '{0}'/kanjo.* '{0}'/access.token '{0}'/config.toml", p.home.display())),
            ));
        }
    }

    let title = l.t("Database", "Baza danych");
    let enc_title = l.t("Encryption", "Szyfrowanie");
    let db_exists = p.db().is_file();
    match (&input.database, &input.database_error) {
        (Some(db), _) => {
            out.push(check(
                "database",
                CheckStatus::Ok,
                title,
                format!("{} ({:.1} MB)", p.db().display(), db.size_bytes as f64 / 1_048_576.0),
                None,
            ));
            let source = match db.key_source {
                KeySource::Keyring => l.t("key in the system keyring", "klucz w systemowym pęku kluczy"),
                KeySource::Keyfile => l.t("key in an owner-only key file", "klucz w pliku dostępnym tylko dla właściciela"),
                KeySource::None => "",
            };
            if db.encrypted {
                out.push(check("encryption", CheckStatus::Ok, enc_title, format!("SQLCipher, {source}"), None));
            } else {
                let off = input.config.storage.encryption == EncryptionMode::Off;
                out.push(check(
                    "encryption",
                    CheckStatus::Warn,
                    enc_title,
                    if off {
                        l.t("not encrypted (encryption = \"off\" in config.toml)", "niezaszyfrowana (encryption = \"off\" w config.toml)")
                            .into()
                    } else {
                        l.t("not encrypted", "niezaszyfrowana").into()
                    },
                    Some(if l.pl {
                        format!("Ustaw encryption = \"auto\" w sekcji [storage] pliku {}.", p.config_file().display())
                    } else {
                        format!("Set encryption = \"auto\" in the [storage] section of {}.", p.config_file().display())
                    }),
                ));
            }
        }
        (None, Some(err)) => {
            out.push(check(
                "database",
                CheckStatus::Fail,
                title,
                format!("{}: {err}", p.db().display()),
                Some(if l.pl {
                    format!(
                        "Sprawdź, czy klucz jest dostępny (odblokowany pęk kluczy albo plik {}); uszkodzoną bazę odtwórz z kopii w {}.",
                        p.key_file().display(),
                        p.backups_dir().display()
                    )
                } else {
                    format!(
                        "Check that the key is available (unlocked keyring or {}); restore a damaged database from a backup in {}.",
                        p.key_file().display(),
                        p.backups_dir().display()
                    )
                }),
            ));
            out.push(check(
                "encryption",
                CheckStatus::Warn,
                enc_title,
                l.t("unknown while the database does not open", "nieznane, dopóki baza się nie otwiera").into(),
                None,
            ));
        }
        (None, None) if !db_exists => {
            out.push(check(
                "database",
                CheckStatus::Warn,
                title,
                format!("{}: {}", p.db().display(), l.t("not created yet", "jeszcze nie utworzona")),
                Some(
                    l.t(
                        "Run kanjo serve; the database is created on first start.",
                        "Uruchom kanjo serve; baza powstanie przy pierwszym starcie.",
                    )
                    .into(),
                ),
            ));
            out.push(check("encryption", CheckStatus::Warn, enc_title, l.t("no database yet", "jeszcze brak bazy").into(), None));
        }
        (None, None) => {
            out.push(check(
                "database",
                CheckStatus::Warn,
                title,
                format!("{}: {}", p.db().display(), l.t("not opened by this command", "to polecenie jej nie otworzyło")),
                None,
            ));
            out.push(check("encryption", CheckStatus::Warn, enc_title, l.t("unknown", "nieznane").into(), None));
        }
    }

    let title = l.t("Schema version", "Wersja schematu");
    match &input.database {
        Some(db) if db.schema_version == SCHEMA_VERSION => {
            out.push(check("schema", CheckStatus::Ok, title, format!("{}", db.schema_version), None));
        }
        Some(db) if db.schema_version < SCHEMA_VERSION => out.push(check(
            "schema",
            CheckStatus::Warn,
            title,
            if l.pl {
                format!("{} (bieżąca {SCHEMA_VERSION})", db.schema_version)
            } else {
                format!("{} (current {SCHEMA_VERSION})", db.schema_version)
            },
            Some(
                l.t(
                    "Start kanjo serve; it migrates the database and makes a backup first.",
                    "Uruchom kanjo serve; zmigruje bazę, najpierw robiąc kopię.",
                )
                .into(),
            ),
        )),
        Some(db) => out.push(check(
            "schema",
            CheckStatus::Fail,
            title,
            if l.pl {
                format!("{} jest nowsza niż obsługiwana przez tę wersję Kanjo ({SCHEMA_VERSION})", db.schema_version)
            } else {
                format!("{} is newer than this Kanjo supports ({SCHEMA_VERSION})", db.schema_version)
            },
            Some(
                l.t(
                    "Update Kanjo to the version that last wrote this database.",
                    "Zaktualizuj Kanjo do wersji, która ostatnio zapisywała tę bazę.",
                )
                .into(),
            ),
        )),
        None => out.push(check("schema", CheckStatus::Warn, title, l.t("unknown", "nieznana").into(), None)),
    }

    let title = l.t("Price table", "Cennik");
    let (version, unpriced) = match &input.pricing {
        Some(info) => (info.version.clone(), info.unpriced_models.clone()),
        None => (PriceTable::embedded().version.clone(), Vec::new()),
    };
    if unpriced.is_empty() {
        out.push(check("pricing", CheckStatus::Ok, title, version, None));
    } else {
        out.push(check(
            "pricing",
            CheckStatus::Warn,
            title,
            if l.pl {
                format!("{version}; modele bez ceny (ich koszt nie jest liczony): {}", unpriced.join(", "))
            } else {
                format!("{version}; models without a price (their cost is not counted): {}", unpriced.join(", "))
            },
            Some(
                l.t(
                    "Update Kanjo or import a signed price table that covers these models.",
                    "Zaktualizuj Kanjo albo zaimportuj podpisany cennik obejmujący te modele.",
                )
                .into(),
            ),
        ));
    }

    let title = l.t("Unrecognised records", "Nierozpoznane rekordy");
    match input.quarantined_records {
        Some(0) => out.push(check("quarantine", CheckStatus::Ok, title, l.t("none", "brak").into(), None)),
        Some(n) => out.push(check(
            "quarantine",
            CheckStatus::Warn,
            title,
            if l.pl {
                format!(
                    "odłożono na bok {n} {} o nieznanym kształcie (zapisano tylko typ i sygnaturę kluczy)",
                    pl_plural(n, "rekord", "rekordy", "rekordów")
                )
            } else {
                format!("set aside {n} {} of unknown shape (only the type and key signature are kept)", en_plural(n, "record", "records"))
            },
            Some(
                l.t(
                    "Usually a newer Claude Code format; update Kanjo. The list is in Settings -> System.",
                    "Zwykle to nowszy format Claude Code; zaktualizuj Kanjo. Lista jest w Ustawienia -> System.",
                )
                .into(),
            ),
        )),
        None => out.push(check("quarantine", CheckStatus::Warn, title, l.t("unknown", "nieznane").into(), None)),
    }

    let title = l.t("Background service", "Usługa w tle");
    let (status, detail, fix) = match input.service {
        ServiceState::Running => (CheckStatus::Ok, l.t("running", "działa"), None),
        ServiceState::Installed => (CheckStatus::Ok, l.t("starts at logon", "startuje po zalogowaniu"), None),
        ServiceState::Stopped => (
            CheckStatus::Warn,
            l.t("installed but not running", "zainstalowana, ale nie działa"),
            Some(if l.pl {
                format!("Uruchom {up} ponownie albo sprawdź logi w {}.", p.log_dir().display())
            } else {
                format!("Run {up} again or check the logs in {}.", p.log_dir().display())
            }),
        ),
        ServiceState::NotInstalled => (
            CheckStatus::Warn,
            l.t(
                "not installed; data is collected only while kanjo serve runs",
                "niezainstalowana; dane zbierane są tylko, gdy działa kanjo serve",
            ),
            Some(if l.pl {
                format!("Uruchom {up}, aby Kanjo startował razem z systemem.")
            } else {
                format!("Run {up} to start Kanjo with your session.")
            }),
        ),
        ServiceState::Unsupported => (
            CheckStatus::Warn,
            l.t("no supported service manager on this system", "brak obsługiwanego menedżera usług w tym systemie"),
            None,
        ),
    };
    out.push(check("service", status, title, detail.into(), fix));

    let title = l.t("Claude Code version", "Wersja Claude Code");
    let update = l.t("Update Claude Code (claude update).", "Zaktualizuj Claude Code (claude update).");
    match input.claude_code_version.as_deref().and_then(|v| parse_version(v).map(|p| (v, p))) {
        None => out.push(check(
            "claude_code_version",
            CheckStatus::Warn,
            title,
            l.t("not seen yet", "jeszcze nieznana").into(),
            Some(
                l.t(
                    "Run a Claude Code session; the version is read from its data.",
                    "Przeprowadź sesję w Claude Code; wersja zostanie odczytana z jej danych.",
                )
                .into(),
            ),
        )),
        Some((v, ver)) => {
            let cache = parse_version(MIN_CC_CACHE_STATS).unwrap_or_default();
            let causes = parse_version(MIN_CC_MISS_CAUSES).unwrap_or_default();
            if ver < cache {
                out.push(check(
                    "claude_code_version",
                    CheckStatus::Warn,
                    title,
                    if l.pl {
                        format!("{v}; statystyki prompt cache wymagają {MIN_CC_CACHE_STATS}, a przyczyny chybień {MIN_CC_MISS_CAUSES}")
                    } else {
                        format!("{v}; prompt cache statistics need {MIN_CC_CACHE_STATS} and miss causes {MIN_CC_MISS_CAUSES}")
                    },
                    Some(update.into()),
                ));
            } else if ver < causes {
                out.push(check(
                    "claude_code_version",
                    CheckStatus::Warn,
                    title,
                    if l.pl {
                        format!("{v}; przyczyny chybień cache wymagają {MIN_CC_MISS_CAUSES}")
                    } else {
                        format!("{v}; cache miss causes need {MIN_CC_MISS_CAUSES}")
                    },
                    Some(update.into()),
                ));
            } else {
                out.push(check("claude_code_version", CheckStatus::Ok, title, v.to_string(), None));
            }
        }
    }

    let title = l.t("Transcript retention", "Przechowywanie transkryptów");
    let (days, set) = match st.and_then(|s| s.cleanup_period_days) {
        Some(d) => (d, true),
        None => (DEFAULT_CLEANUP_DAYS, false),
    };
    let origin = if set { "cleanupPeriodDays" } else { l.t("Claude Code default", "domyślnie w Claude Code") };
    let detail = if l.pl {
        format!("Claude Code usuwa transkrypty starsze niż {days} dni ({origin}); Kanjo trzyma własne agregaty, więc historia kosztów zostaje, ale podgląd transkryptów i ponowne skanowanie sięgają tylko {days} dni wstecz")
    } else {
        format!("Claude Code deletes transcripts older than {days} days ({origin}); Kanjo keeps its own aggregates, so cost history stays, but the transcript viewer and rescans only reach {days} days back")
    };
    if days < 7 {
        out.push(check(
            "cleanup_period",
            CheckStatus::Warn,
            title,
            detail,
            Some(if l.pl {
                format!("Zwiększ cleanupPeriodDays w {}, jeśli Kanjo bywa wyłączony dłużej niż {days} dni.", settings_path.display())
            } else {
                format!("Raise cleanupPeriodDays in {} if Kanjo is sometimes off for longer than {days} days.", settings_path.display())
            }),
        ));
    } else {
        out.push(check("cleanup_period", CheckStatus::Ok, title, detail, None));
    }

    let order = |c: &DoctorCheck| CHECK_IDS.iter().position(|id| *id == c.id).unwrap_or(CHECK_IDS.len());
    out.sort_by_key(order);
    out
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;

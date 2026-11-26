//! Commands that read or change the database: status, doctor, import, backup,
//! restore, pricing, purge, retention and audit.

use crate::app::{out, App};
use crate::cli::{AuditAction, BackupAction, PricingAction, PurgeTarget};
use crate::error::{fail, CliResult, Failure};
use crate::fmt::{self, Align, Table};
use crate::i18n::Lang;
use crate::reports;
use kanjo_core::api::{CheckStatus, DatabaseInfo, LimitWindow, PeriodSummary, PurgeReport, PurgeRequest, Query};
use kanjo_setup::doctor::DoctorInput;
use serde_json::json;
use std::io::{IsTerminal, Write};
use std::path::Path;

pub fn status(app: &mut App) -> CliResult<()> {
    let store = app.store()?;
    let ctx = app.ctx();
    let o = kanjo_analytics::overview(&store, &ctx)?;
    if app.json {
        return app.print_json(&o);
    }
    let lang = app.lang;
    let label = |en: &'static str, pl: &'static str| format!("{:<18}", lang.t(en, pl));
    out(&app.bold(&tr!(lang, "Kanjo, {}", "Kanjo, {}", fmt::date_time(&ctx, o.generated_at))));
    let window = |name: String, w: &Option<LimitWindow>| -> String {
        let Some(w) = w else { return format!("{name}{}", lang.t("no reading yet", "brak odczytu")) };
        let mut s = format!(
            "{name}{}  {}",
            fmt::pct(lang, w.used_pct),
            tr!(lang, "resets in {}", "reset za {}", fmt::duration(lang, w.resets_at - ctx.now))
        );
        if let Some(at) = w.exhausts_at.filter(|_| w.will_exhaust) {
            let warn = tr!(
                lang,
                "at this pace runs out at {} (in {})",
                "w tym tempie skończy się o {} (za {})",
                fmt::when(&ctx, at),
                fmt::duration(lang, at - ctx.now)
            );
            s.push_str(&format!("  {}", app.paint(&warn, "33")));
        } else if w.rate_pct_per_hour.is_some() {
            s.push_str(&format!("  {}", lang.t("lasts until the reset at this pace", "w tym tempie wystarczy do resetu")));
        }
        if w.stale {
            s.push_str(&format!(" ({})", lang.t("last reading over 15 min old", "ostatni odczyt starszy niż 15 min")));
        }
        s
    };
    out(&window(label("5-hour limit", "Limit 5-godz."), &o.limits.five_hour));
    out(&window(label("Weekly limit", "Limit tygodniowy"), &o.limits.seven_day));
    if let Some(sp) = &o.limits.spend {
        let mut s = format!("{}{}", label("Spend limit", "Limit wydatków"), fmt::pct(lang, sp.used_pct));
        if let (Some(used), Some(limit)) = (sp.used_usd, sp.limit_usd) {
            s.push_str(&format!("  ({} / {})", fmt::money(lang, used), fmt::money(lang, limit)));
        }
        out(&s);
    }
    if o.limits.five_hour.is_none() {
        if let Some(e) = &o.limits.estimate {
            out(&format!(
                "{}{}",
                label("5-hour estimate", "Szacunek 5-godz."),
                tr!(
                    lang,
                    "{} of a typical {} block ({}, from {} past blocks)",
                    "{} z typowego bloku {} ({}, na podstawie {} bloków)",
                    fmt::money(lang, e.current_block_usd),
                    fmt::money(lang, e.p90_block_usd),
                    fmt::pct(lang, e.current_block_pct),
                    e.blocks_observed
                )
            ));
        }
    }
    let period = |name: String, p: &PeriodSummary, prev: &str| -> String {
        let mut s = format!("{name}{}", fmt::money(lang, p.usage.cost_usd));
        if let Some(d) = p.delta_pct {
            let sign = if d >= 0.0 { "+" } else { "" };
            s.push_str(&format!("  ({sign}{} {prev})", fmt::pct(lang, d)));
        }
        s
    };
    out(&period(label("Today", "Dzisiaj"), &o.today, lang.t("vs yesterday at this time", "wobec wczoraj o tej porze")));
    out(&period(label("This week", "Ten tydzień"), &o.week, lang.t("vs last week", "wobec zeszłego tygodnia")));
    out(&period(label("This month", "Ten miesiąc"), &o.month, lang.t("vs last month", "wobec zeszłego miesiąca")));
    if o.burn.active {
        out(&format!(
            "{}{}",
            label("Burn rate", "Tempo"),
            tr!(
                lang,
                "{}/h, {} tokens/min over the last {} min",
                "{}/h, {} tokenów/min w ostatnich {} min",
                fmt::money(lang, o.burn.usd_per_hour),
                fmt::int(lang, o.burn.tokens_per_min.round() as i64),
                o.burn.window_minutes
            )
        ));
    }
    if let Some(b) = &o.active_block {
        let mut s = format!(
            "{}{}",
            label("Current block", "Bieżący blok"),
            tr!(lang, "{} until {}", "{} do {}", fmt::money(lang, b.usage.cost_usd), fmt::clock(&ctx, b.end))
        );
        if let Some(p) = b.projected_usd {
            s.push_str(&tr!(lang, ", about {} by the end", ", około {} na koniec", fmt::money(lang, p)));
        }
        out(&s);
    }
    if !o.active_sessions.is_empty() {
        out(&format!("{}{}", label("Active sessions", "Aktywne sesje"), o.active_sessions.len()));
    }
    let data = &o.data;
    let source = if data.statusline_connected {
        lang.t("limits come from Claude Code's status line", "limity pochodzą z paska stanu Claude Code").to_string()
    } else {
        lang.t(
            "no recent status line data, so limits may be missing; \"kanjo doctor\" explains why",
            "brak świeżych danych z paska stanu, więc limitów może brakować; \"kanjo doctor\" wyjaśni dlaczego",
        )
        .to_string()
    };
    out(&app.paint(
        &tr!(
            lang,
            "Costs are API list prices from transcripts (a lower bound); {source}.",
            "Koszty to ceny katalogowe API z transkryptów (dolna granica); {source}."
        ),
        "2",
    ));
    Ok(())
}

pub fn doctor(app: &mut App) -> CliResult<()> {
    let now = kanjo_core::time::now_ms();
    let exe = kanjo_setup::current_exe().ok();
    let mut database = None;
    let mut database_error = None;
    let mut last_statusline_at = None;
    let mut pricing = None;
    let mut quarantined = None;
    let mut cc_version = None;
    if app.paths.db().exists() {
        match app.store() {
            Ok(store) => {
                let ctx = app.ctx();
                database = Some(DatabaseInfo {
                    size_bytes: store.size_bytes() as i64,
                    encrypted: store.is_encrypted(),
                    key_source: app.key_source().unwrap_or(kanjo_core::api::KeySource::None),
                    schema_version: store.schema_version().unwrap_or_default(),
                });
                if let Ok(d) = kanjo_analytics::data_status(&store, &ctx) {
                    last_statusline_at = d.last_statusline_at;
                    quarantined = Some(d.quarantined_records);
                }
                pricing = kanjo_ops::pricing::info(&store).ok();
                cc_version = store.read(latest_cc_version).ok().flatten();
            }
            Err(Failure::Core(e)) => database_error = Some(e.to_string()),
            Err(Failure::Msg { message, .. }) => database_error = Some(message),
            Err(Failure::Silent(_)) => {}
        }
    }
    let service = kanjo_setup::service::status();
    let input = DoctorInput {
        exe: exe.as_deref(),
        database,
        database_error,
        last_statusline_at,
        pricing,
        quarantined_records: quarantined,
        claude_code_version: cc_version,
        service,
        ..DoctorInput::new(&app.paths, &app.config, now)
    };
    let checks = kanjo_setup::doctor::run(&input, app.lang.code());
    let failed = checks.iter().any(|c| c.status == CheckStatus::Fail);
    if app.json {
        app.print_json(&checks)?;
    } else {
        let lang = app.lang;
        for c in &checks {
            let (tag, code) = match c.status {
                CheckStatus::Ok => ("[ok]", "32"),
                CheckStatus::Warn => (lang.t("[warn]", "[uwaga]"), "33"),
                CheckStatus::Fail => (lang.t("[FAIL]", "[BŁĄD]"), "31;1"),
            };
            let width = if lang == Lang::Pl { 7 } else { 6 };
            let tag = app.paint(&format!("{tag:<width$}"), code);
            out(&format!("{tag} {}: {}", c.title, c.detail));
            if c.status != CheckStatus::Ok {
                if let Some(fix) = &c.fix {
                    out(&format!("{}-> {fix}", " ".repeat(width + 1)));
                }
            }
        }
        let fails = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
        let warns = checks.iter().filter(|c| c.status == CheckStatus::Warn).count();
        out("");
        out(&if fails == 0 && warns == 0 {
            lang.t("Everything looks fine.", "Wszystko wygląda dobrze.").to_string()
        } else {
            tr!(lang, "{fails} problem(s), {warns} warning(s).", "Problemy: {fails}, ostrzeżenia: {warns}.")
        });
    }
    if failed {
        return Err(Failure::Silent(1));
    }
    Ok(())
}

fn latest_cc_version(c: &rusqlite::Connection) -> kanjo_core::Result<Option<String>> {
    use rusqlite::OptionalExtension;
    let v: Option<Option<String>> = c
        .query_row(
            "SELECT v FROM (
               SELECT cc_version AS v, ts FROM session_status WHERE cc_version IS NOT NULL
               UNION ALL SELECT cc_version, ts FROM requests WHERE cc_version IS NOT NULL
             ) ORDER BY ts DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(v.flatten())
}

pub fn import(app: &mut App, rescan: bool) -> CliResult<()> {
    let lang = app.lang;
    let projects = app.paths.claude_projects();
    if !projects.is_dir() {
        return Err(fail(
            tr!(lang, "there are no Claude Code transcripts in {}.", "w katalogu {} nie ma transkryptów Claude Code.", projects.display()),
            tr!(
                lang,
                "use Claude Code at least once, or point Kanjo at its directory with --claude-dir DIR.",
                "użyj Claude Code przynajmniej raz albo wskaż jego katalog opcją --claude-dir KATALOG."
            ),
        ));
    }
    crate::app::init_logging(tracing_subscriber::filter::LevelFilter::WARN);
    let store = app.store_or_create()?;
    let ingestor = kanjo_ingest::Ingestor::new(store, app.paths.clone())?;
    if rescan {
        ingestor.rescan_from_scratch()?;
    }
    let before = ingestor.status().requests_stored;
    let tty = std::io::stderr().is_terminal();
    let progress = move |s: &kanjo_core::api::IngestStatus| {
        if !tty || s.files_total == 0 {
            return;
        }
        let mb = |b: i64| b as f64 / 1_048_576.0;
        let line = tr!(
            lang,
            "Reading transcripts: {}/{} files, {} of {} MB",
            "Czytanie transkryptów: {}/{} plików, {} z {} MB",
            s.files_done,
            s.files_total,
            fmt::decimal(lang, mb(s.bytes_done), 0),
            fmt::decimal(lang, mb(s.bytes_total), 0)
        );
        let mut err = std::io::stderr();
        let _ = write!(err, "\r{line}\x1b[K");
        let _ = err.flush();
    };
    let st = ingestor.backfill(&progress)?;
    if tty {
        eprintln!();
    }
    let spool = ingestor.drain_spool()?;
    let added = st.requests_stored - before;
    if app.json {
        return app.print_json(&json!({ "files": st.files_total, "requests_added": added, "requests_stored": st.requests_stored, "spool_statusline": spool.statusline, "spool_hooks": spool.hooks }));
    }
    out(&tr!(
        lang,
        "Done: {} files read, {} new requests, {} stored in total.",
        "Gotowe: przeczytano plików: {}, nowych zapytań: {}, wszystkich zapisanych: {}.",
        fmt::int(lang, st.files_total),
        fmt::int(lang, added),
        fmt::int(lang, st.requests_stored)
    ));
    Ok(())
}

pub fn backup(app: &mut App, action: Option<&BackupAction>) -> CliResult<()> {
    let lang = app.lang;
    match action.unwrap_or(&BackupAction::List) {
        BackupAction::Create => {
            let store = app.store()?;
            let info = kanjo_ops::backup::create(&store, &app.paths, kanjo_core::time::now_ms())?;
            app.audit("backup.create", &info.file, json!({ "size_bytes": info.size_bytes, "via": "cli" }));
            if app.json {
                return app.print_json(&info);
            }
            out(&tr!(
                lang,
                "Backup saved: {} ({}). Restore it with \"kanjo restore {}\".",
                "Kopia zapisana: {} ({}). Przywrócisz ją poleceniem \"kanjo restore {}\".",
                app.paths.backups_dir().join(&info.file).display(),
                size(lang, info.size_bytes),
                info.file
            ));
        }
        BackupAction::List => {
            let list = kanjo_ops::backup::list(&app.paths)?;
            if app.json {
                return app.print_json(&list);
            }
            if list.is_empty() {
                out(&tr!(
                    lang,
                    "No backups yet. Make one with \"kanjo backup create\".",
                    "Nie ma jeszcze kopii. Utwórz ją poleceniem \"kanjo backup create\"."
                ));
                return Ok(());
            }
            let ctx = app.ctx();
            let mut t = Table::new(&[
                (lang.t("File", "Plik"), Align::Left),
                (lang.t("Created", "Utworzono"), Align::Left),
                (lang.t("Size", "Rozmiar"), Align::Right),
            ]);
            for b in &list {
                t.row(vec![b.file.clone(), fmt::date_time(&ctx, b.created_at), size(lang, b.size_bytes)]);
            }
            out(t.render(app.color).trim_end());
            out(&tr!(lang, "Folder: {}", "Folder: {}", app.paths.backups_dir().display()));
        }
    }
    Ok(())
}

fn size(lang: Lang, bytes: i64) -> String {
    let b = bytes as f64;
    if b >= 1_048_576.0 {
        format!("{} MB", fmt::decimal(lang, b / 1_048_576.0, 1))
    } else {
        format!("{} kB", fmt::decimal(lang, (b / 1024.0).max(0.1), 1))
    }
}

/// How to stop whatever is serving the dashboard right now.
pub fn stop_hint(lang: Lang) -> String {
    let service = kanjo_setup::service::is_installed();
    match (service, std::env::consts::OS) {
        (true, "linux") => tr!(
            lang,
            "stop the background service with \"systemctl --user stop kanjo.service\" and run the command again; \"systemctl --user start kanjo.service\" starts it afterwards.",
            "zatrzymaj usługę w tle poleceniem \"systemctl --user stop kanjo.service\" i uruchom polecenie ponownie; potem włączysz ją przez \"systemctl --user start kanjo.service\"."
        ),
        (true, "macos") => tr!(
            lang,
            "stop the background service with \"launchctl bootout gui/$(id -u)/io.kyotu.kanjo\" and run the command again; \"kanjo service install\" starts it afterwards.",
            "zatrzymaj usługę w tle poleceniem \"launchctl bootout gui/$(id -u)/io.kyotu.kanjo\" i uruchom polecenie ponownie; potem włączysz ją przez \"kanjo service install\"."
        ),
        (true, _) => tr!(
            lang,
            "end the \"kanjo serve\" process in Task Manager and run the command again; it starts again at your next sign-in.",
            "zakończ proces \"kanjo serve\" w Menedżerze zadań i uruchom polecenie ponownie; uruchomi się znowu przy następnym logowaniu."
        ),
        (false, _) => tr!(
            lang,
            "stop \"kanjo serve\" (press Ctrl-C in its terminal) and run the command again.",
            "zatrzymaj \"kanjo serve\" (Ctrl-C w jego terminalu) i uruchom polecenie ponownie."
        ),
    }
}

pub fn restore(app: &mut App, file: &Path) -> CliResult<()> {
    let lang = app.lang;
    if crate::setup::server_running(&app.paths) {
        let message = match kanjo_setup::server_file::live(&app.paths) {
            Some(rec) => tr!(
                lang,
                "Kanjo is running at {} (process {}) and using the database, so it cannot be replaced now.",
                "Kanjo działa pod adresem {} (proces {}) i korzysta z bazy, więc nie można jej teraz podmienić.",
                rec.url,
                rec.pid
            ),
            None => lang
                .t(
                    "Kanjo is running and using the database, so it cannot be replaced now.",
                    "Kanjo działa i korzysta z bazy, więc nie można jej teraz podmienić.",
                )
                .to_string(),
        };
        return Err(fail(message, stop_hint(lang)));
    }
    app.close();
    kanjo_ops::backup::restore(&app.paths, &app.config, file)?;
    if app.store().is_ok() {
        app.audit("backup.restore", &file.display().to_string(), json!({ "via": "cli" }));
    }
    out(&tr!(
        lang,
        "Restored {}. The previous database was kept in {} as kanjo.db.pre-restore-<time>.",
        "Przywrócono {}. Poprzednia baza została zachowana w {} jako kanjo.db.pre-restore-<czas>.",
        file.display(),
        app.paths.home.display()
    ));
    Ok(())
}

pub fn pricing(app: &mut App, action: Option<&PricingAction>) -> CliResult<()> {
    let lang = app.lang;
    let store = app.store()?;
    let info = match action.unwrap_or(&PricingAction::Show) {
        PricingAction::Show => kanjo_ops::pricing::info(&store)?,
        PricingAction::Import { file, sig, pubkey, allow_unsigned } => {
            let data = std::fs::read(file).map_err(|e| {
                fail(
                    tr!(lang, "cannot read {} ({e}).", "nie można odczytać pliku {} ({e}).", file.display()),
                    tr!(lang, "check the path of the price file.", "sprawdź ścieżkę pliku z cenami."),
                )
            })?;
            let signature = match sig {
                Some(p) => Some(std::fs::read_to_string(p).map_err(|e| {
                    fail(
                        tr!(lang, "cannot read the signature {} ({e}).", "nie można odczytać podpisu {} ({e}).", p.display()),
                        tr!(lang, "check the path given with --sig.", "sprawdź ścieżkę podaną w --sig."),
                    )
                })?),
                None => None,
            };
            if signature.is_some() && pubkey.is_empty() {
                return Err(fail(
                    lang.t("a signature can only be checked against a public key.", "podpis można sprawdzić tylko kluczem publicznym.")
                        .into(),
                    lang.t(
                        "pass the publisher's minisign key with --pubkey KEY (the key itself or a .pub file).",
                        "podaj klucz minisign wydawcy opcją --pubkey KLUCZ (sam klucz albo plik .pub).",
                    )
                    .into(),
                ));
            }
            if signature.is_none() && !allow_unsigned {
                return Err(fail(
                    lang.t(
                        "this price file is not signed, so Kanjo cannot tell who made it.",
                        "ten plik z cenami nie jest podpisany, więc Kanjo nie wie, kto go przygotował.",
                    )
                    .into(),
                    lang.t(
                        "pass its signature with --sig FILE --pubkey KEY, or add --allow-unsigned if you trust the file.",
                        "podaj podpis opcjami --sig PLIK --pubkey KLUCZ albo dodaj --allow-unsigned, jeśli ufasz temu plikowi.",
                    )
                    .into(),
                ));
            }
            let keys: Vec<String> = pubkey
                .iter()
                .map(|k| if Path::new(k).is_file() { std::fs::read_to_string(k).unwrap_or_else(|_| k.clone()) } else { k.clone() })
                .collect();
            let info = kanjo_ops::pricing::import(&store, &data, signature.as_deref(), &keys, *allow_unsigned)?;
            app.audit(
                "pricing.import",
                &info.version,
                json!({ "file": file.display().to_string(), "signed": info.signed, "models": info.models, "via": "cli" }),
            );
            info
        }
        PricingAction::Reset => {
            let info = kanjo_ops::pricing::reset(&store)?;
            app.audit("pricing.reset", &info.version, json!({ "via": "cli" }));
            info
        }
    };
    if app.json {
        return app.print_json(&info);
    }
    let signed = if info.signed { lang.t("signed", "podpisany") } else { lang.t("not signed", "niepodpisany") };
    out(&tr!(
        lang,
        "Price table {} ({}), {} models, {signed}.",
        "Cennik {} ({}), modeli: {}, {signed}.",
        info.version,
        info.source,
        info.models
    ));
    if info.unpriced_models.is_empty() {
        out(lang.t("Every model in your data has a price.", "Każdy model w Twoich danych ma cenę."));
    } else {
        out(&tr!(
            lang,
            "Models without a price (not counted in costs): {}. Import a newer price table with \"kanjo pricing import FILE\".",
            "Modele bez ceny (nie wliczane do kosztów): {}. Zaimportuj nowszy cennik poleceniem \"kanjo pricing import PLIK\".",
            info.unpriced_models.join(", ")
        ));
    }
    Ok(())
}

fn report_line(lang: Lang, r: &PurgeReport) -> String {
    tr!(
        lang,
        "Deleted {} requests, {} sessions, {} hourly totals, {} limit readings, {} alerts, {} audit entries, {} quarantine rows.",
        "Usunięto: zapytania {}, sesje {}, sumy godzinowe {}, odczyty limitów {}, alerty {}, wpisy audytu {}, wpisy kwarantanny {}.",
        r.requests,
        r.sessions,
        r.aggregates,
        r.limit_samples,
        r.alerts,
        r.audit_entries,
        r.quarantine
    )
}

pub fn purge(app: &mut App, target: &PurgeTarget, yes: bool) -> CliResult<()> {
    let lang = app.lang;
    let store = app.store()?;
    let ctx = app.ctx();
    let mut req = PurgeRequest::default();
    let (what, audit_target, deleted) = if let Some(name) = &target.project {
        req.project = Some(reports::project_id(app, &ctx, name)?);
        (
            tr!(lang, "all data of project \"{name}\"", "wszystkie dane projektu \"{name}\""),
            format!("project:{}", req.project.unwrap_or_default()),
            name.clone(),
        )
    } else if let Some(id) = &target.session {
        req.session = Some(id.clone());
        (tr!(lang, "session {id}", "sesję {id}"), format!("session:{id}"), id.clone())
    } else if let Some(d) = &target.before {
        req.before = Some(reports::parse_date(lang, &ctx, d)?);
        let cutoff = req.before.unwrap_or_default();
        (tr!(lang, "all usage data before {d}", "wszystkie dane sprzed {d}"), format!("before:{cutoff}"), ctx.date_label(cutoff))
    } else {
        req.everything = true;
        (lang.t("all usage data", "wszystkie dane o użyciu").to_string(), "everything".to_string(), "everything".to_string())
    };
    if !yes {
        app.require_terminal(lang.t("add --yes to delete without being asked.", "dodaj --yes, żeby usunąć bez pytania."))?;
        out(&tr!(
            lang,
            "This deletes {what} from Kanjo's database. Settings, projects, budgets and the audit log stay. Claude Code's own files are not touched. A backup first is a good idea: \"kanjo backup create\".",
            "To usunie {what} z bazy Kanjo. Ustawienia, projekty, budżety i dziennik audytu zostaną. Pliki samego Claude Code pozostaną nietknięte. Warto najpierw zrobić kopię: \"kanjo backup create\"."
        ));
        let word = lang.t("delete", "usuń");
        let answer = crate::app::prompt(&tr!(lang, "Type \"{word}\" to confirm: ", "Wpisz \"{word}\", aby potwierdzić: "))?;
        let a = answer.trim().to_lowercase();
        let ok = a == word || (lang == Lang::Pl && a == "usun");
        if !ok {
            out(lang.t("Nothing was deleted.", "Nic nie zostało usunięte."));
            return Ok(());
        }
    }
    let report = kanjo_ops::retention::purge(&store, &req)?;
    app.audit("data.purge", &audit_target, json!({ "request": req, "deleted": deleted, "report": report, "via": "cli" }));
    if app.json {
        return app.print_json(&report);
    }
    out(&report_line(lang, &report));
    Ok(())
}

pub fn retention_apply(app: &mut App) -> CliResult<()> {
    let store = app.store()?;
    let settings = app.settings();
    let report = kanjo_ops::retention::apply(&store, &settings.retention, kanjo_core::time::now_ms())?;
    app.audit("retention.apply", "retention", json!({ "report": report, "settings": settings.retention, "via": "cli" }));
    if app.json {
        return app.print_json(&report);
    }
    out(&report_line(app.lang, &report));
    Ok(())
}

pub fn audit(app: &mut App, action: Option<&AuditAction>) -> CliResult<()> {
    let lang = app.lang;
    let store = app.store()?;
    match action.unwrap_or(&AuditAction::List { limit: 20 }) {
        AuditAction::List { limit } => {
            let page = kanjo_ops::audit::list(&store, &Query { limit: Some(*limit), ..Query::default() })?;
            if app.json {
                return app.print_json(&page);
            }
            if page.items.is_empty() {
                out(lang.t("The audit log is empty.", "Dziennik audytu jest pusty."));
                return Ok(());
            }
            let ctx = app.ctx();
            let mut t = Table::new(&[
                (lang.t("Time", "Czas"), Align::Left),
                (lang.t("Who", "Kto"), Align::Left),
                (lang.t("Action", "Akcja"), Align::Left),
                (lang.t("Target", "Cel"), Align::Left),
            ]);
            for e in page.items.iter().rev() {
                t.row(vec![fmt::date_time(&ctx, e.ts), e.actor.clone(), e.action.clone(), e.target.clone()]);
            }
            out(t.render(app.color).trim_end());
            let chain = if page.chain_ok {
                lang.t("the chain of entries is intact", "łańcuch wpisów jest nienaruszony")
            } else {
                lang.t(
                    "the chain of entries is BROKEN, run \"kanjo audit verify\"",
                    "łańcuch wpisów jest NARUSZONY, uruchom \"kanjo audit verify\"",
                )
            };
            out(&tr!(lang, "{} of {} entries; {chain}.", "{} z {} wpisów; {chain}.", page.items.len(), page.total));
        }
        AuditAction::Verify => {
            let ok = kanjo_ops::audit::verify(&store)?;
            if app.json {
                app.print_json(&json!({ "chain_ok": ok }))?;
            } else if ok {
                out(lang.t(
                    "The audit log is intact: no entry was changed or removed outside retention.",
                    "Dziennik audytu jest nienaruszony: żaden wpis nie został zmieniony ani usunięty poza retencją.",
                ));
            } else {
                return Err(fail(
                    lang.t(
                        "the audit log was changed: at least one entry does not match the chain.",
                        "dziennik audytu został zmieniony: co najmniej jeden wpis nie pasuje do łańcucha.",
                    )
                    .into(),
                    lang.t(
                        "compare with a backup (\"kanjo backup list\") to find out when it happened; Kanjo itself never edits entries.",
                        "porównaj z kopią (\"kanjo backup list\"), żeby ustalić, kiedy to się stało; Kanjo samo nigdy nie edytuje wpisów.",
                    )
                    .into(),
                ));
            }
        }
    }
    Ok(())
}

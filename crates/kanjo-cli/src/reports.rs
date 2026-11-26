use crate::app::{out, App};
use crate::cli::{ExportBy, ExportFormat, ExportGroup, ExportView, Filters, ReportArgs};
use crate::error::{fail, CliResult};
use crate::fmt::{self, Align, Table};
use crate::i18n::Lang;
use kanjo_analytics::export;
use kanjo_core::api::{Dimension, Granularity, Query, Usage};
use kanjo_core::time::DAY_MS;
use kanjo_core::Ctx;
use serde_json::json;

#[derive(Clone, Copy)]
pub enum Report {
    Daily,
    Weekly,
    Monthly,
    Session,
    Blocks,
}

/// Local midnight of a `YYYY-MM-DD` or `YYYYMMDD` date.
pub fn parse_date(lang: Lang, ctx: &Ctx, text: &str) -> CliResult<i64> {
    let t = text.trim();
    let parsed = chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d").or_else(|_| chrono::NaiveDate::parse_from_str(t, "%Y%m%d"));
    let date = parsed.map_err(|_| {
        fail(
            tr!(lang, "\"{t}\" is not a date Kanjo understands.", "\"{t}\" nie jest datą, którą Kanjo rozumie."),
            tr!(lang, "write dates as YYYY-MM-DD, for example 2026-11-01.", "podaj datę w formacie RRRR-MM-DD, na przykład 2026-11-01."),
        )
    })?;
    let utc_midnight = date.and_hms_opt(0, 0, 0).map(|d| d.and_utc().timestamp_millis()).unwrap_or_default();
    Ok(utc_midnight - ctx.tz_offset_min as i64 * 60_000)
}

pub fn build_query(app: &mut App, ctx: &Ctx, f: &Filters) -> CliResult<Query> {
    let lang = app.lang;
    let mut q = Query::default();
    if let Some(s) = &f.since {
        q.from = Some(parse_date(lang, ctx, s)?);
    }
    if let Some(u) = &f.until {
        q.to = Some(parse_date(lang, ctx, u)? + DAY_MS);
    }
    if let (Some(from), Some(to)) = (q.from, q.to) {
        if from >= to {
            return Err(fail(
                tr!(lang, "the start date is after the end date.", "data początkowa jest późniejsza niż końcowa."),
                tr!(lang, "swap --since and --until.", "zamień miejscami --since i --until."),
            ));
        }
    }
    q.model = f.model.clone();
    if let Some(name) = &f.project {
        q.project = Some(project_id(app, ctx, name)?);
    }
    Ok(q)
}

pub fn project_id(app: &mut App, ctx: &Ctx, name: &str) -> CliResult<i64> {
    let store = app.store()?;
    let projects = kanjo_analytics::projects(&store, ctx)?;
    let wanted = name.trim().to_lowercase();
    let exact = projects.iter().find(|p| p.name.to_lowercase() == wanted || p.id.to_string() == wanted);
    let found = exact.or_else(|| {
        let partial: Vec<_> = projects
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&wanted)
                    || p.path.as_deref().is_some_and(|path| path.to_lowercase().trim_end_matches('/').ends_with(&wanted))
            })
            .collect();
        (partial.len() == 1).then(|| partial[0])
    });
    match found {
        Some(p) => Ok(p.id),
        None => {
            let mut names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
            names.sort_unstable();
            let list = if names.is_empty() { "-".to_string() } else { names.iter().take(15).copied().collect::<Vec<_>>().join(", ") };
            Err(fail(
                tr!(app.lang, "there is no project called \"{name}\".", "nie ma projektu o nazwie \"{name}\"."),
                tr!(app.lang, "use one of these names: {list}.", "użyj jednej z tych nazw: {list}."),
            ))
        }
    }
}

fn period_query(report: Report, ctx: &Ctx, mut q: Query) -> Query {
    if q.from.is_none() {
        match report {
            Report::Weekly => q.from = Some(ctx.week_start_of(q.to.unwrap_or(ctx.now)) - 11 * 7 * DAY_MS),
            Report::Monthly => q.from = Some(ctx.month_shift(q.to.unwrap_or(ctx.now), -11)),
            _ => {}
        }
    }
    q.group = Some(match report {
        Report::Weekly => Granularity::Week,
        Report::Monthly => Granularity::Month,
        _ => Granularity::Day,
    });
    q
}

fn token_cells(lang: Lang, u: &Usage) -> Vec<String> {
    let t = &u.tokens;
    vec![
        fmt::int(lang, t.input),
        fmt::int(lang, t.output),
        fmt::int(lang, t.cache_write_5m + t.cache_write_1h),
        fmt::int(lang, t.cache_read),
        fmt::int(lang, t.total()),
        fmt::money(lang, u.cost_usd),
    ]
}

fn token_headers(lang: Lang) -> Vec<(&'static str, Align)> {
    vec![
        (lang.t("Input", "Wejście"), Align::Right),
        (lang.t("Output", "Wyjście"), Align::Right),
        (lang.t("Cache write", "Zapis cache"), Align::Right),
        (lang.t("Cache read", "Odczyt cache"), Align::Right),
        (lang.t("Total tokens", "Tokeny razem"), Align::Right),
        (lang.t("Cost", "Koszt"), Align::Right),
    ]
}

fn models_cell(models: &[String]) -> String {
    let mut short: Vec<&str> = models.iter().map(|m| fmt::short_model(m)).collect();
    short.dedup();
    short.join(", ")
}

fn note(app: &App, total: &Usage, empty: bool) {
    let lang = app.lang;
    if empty {
        out(&tr!(
            lang,
            "No usage in this period. Try a wider range with --since, or run \"kanjo import\" to read older transcripts.",
            "Brak użycia w tym okresie. Spróbuj szerszego zakresu (--since) albo uruchom \"kanjo import\", żeby wczytać starsze transkrypty."
        ));
        return;
    }
    out(&app.paint(
        &tr!(
            lang,
            "Costs are API list prices summed from Claude Code transcripts, a lower bound: the real figure is at least this.",
            "Koszty to ceny katalogowe API zsumowane z transkryptów Claude Code, czyli dolna granica: rzeczywista kwota jest co najmniej taka."
        ),
        "2",
    ));
    if total.unpriced_requests > 0 {
        let n = fmt::int(lang, total.unpriced_requests);
        out(&tr!(
            lang,
            "{n} requests use a model without a price and are not included; \"kanjo pricing show\" lists them.",
            "Nie wliczono {n} zapytań do modeli bez ceny; \"kanjo pricing show\" pokaże, których."
        ));
    }
}

pub fn run(app: &mut App, report: Report, args: &ReportArgs) -> CliResult<()> {
    let store = app.store()?;
    let ctx = app.ctx();
    let q = build_query(app, &ctx, &args.filters)?;
    let lang = app.lang;
    match report {
        Report::Daily | Report::Weekly | Report::Monthly => {
            if app.json {
                let v = match report {
                    Report::Daily => export::ccusage_daily(&store, &ctx, &q)?,
                    Report::Weekly => export::ccusage_weekly(&store, &ctx, &q)?,
                    _ => export::ccusage_monthly(&store, &ctx, &q)?,
                };
                return app.print_json(&v);
            }
            let q = period_query(report, &ctx, q);
            let r = kanjo_analytics::usage_report(&store, &ctx, &q)?;
            if args.csv {
                print_raw(&export::usage_csv(&r));
                return Ok(());
            }
            let first = match report {
                Report::Daily => lang.t("Date", "Dzień"),
                Report::Weekly => lang.t("Week", "Tydzień"),
                _ => lang.t("Month", "Miesiąc"),
            };
            let mut headers = vec![(first, Align::Left), (lang.t("Models", "Modele"), Align::Left)];
            headers.extend(token_headers(lang));
            let mut t = Table::new(&headers);
            for b in r.buckets.iter().filter(|b| b.usage.requests > 0 || b.usage.cost_usd > 0.0) {
                let models: Vec<String> = b.by_model.iter().map(|m| m.model.clone()).collect();
                let mut row = vec![b.label.clone(), models_cell(&models)];
                row.extend(token_cells(lang, &b.usage));
                t.row(row);
                if args.breakdown {
                    for m in &b.by_model {
                        t.row(vec![
                            String::new(),
                            format!("- {}", fmt::short_model(&m.model)),
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                            fmt::int(lang, m.tokens),
                            fmt::money(lang, m.cost_usd),
                        ]);
                    }
                }
            }
            let empty = t.is_empty();
            if !empty {
                t.rule();
                let mut row = vec![lang.t("Total", "Razem").to_string(), String::new()];
                row.extend(token_cells(lang, &r.total));
                t.row(row);
                out(t.render(app.color).trim_end());
            }
            note(app, &r.total, empty);
        }
        Report::Session => {
            if app.json {
                let v = export::ccusage_session(&store, &ctx, &q)?;
                return app.print_json(&v);
            }
            let mut q = q;
            q.sort = Some("recent".into());
            q.limit = Some(1000);
            let list = kanjo_analytics::sessions(&store, &ctx, &q)?;
            if args.csv {
                print_raw(&export::sessions_csv(&list));
                return Ok(());
            }
            let mut headers = vec![
                (lang.t("Session", "Sesja"), Align::Left),
                (lang.t("Project", "Projekt"), Align::Left),
                (lang.t("Last activity", "Ostatnia aktywność"), Align::Left),
                (lang.t("Models", "Modele"), Align::Left),
            ];
            headers.extend(token_headers(lang));
            let mut t = Table::new(&headers);
            let mut total = Usage::default();
            for s in list.items.iter().rev() {
                total += s.usage;
                let mut row =
                    vec![s.id.chars().take(8).collect(), s.project.clone(), fmt::date_time(&ctx, s.last_at), models_cell(&s.models)];
                row.extend(token_cells(lang, &s.usage));
                t.row(row);
                if args.breakdown {
                    if let Some(d) = kanjo_analytics::session_detail(&store, &ctx, &s.id)? {
                        for m in &d.by_model {
                            let mut row = vec![String::new(), String::new(), String::new(), format!("- {}", fmt::short_model(&m.label))];
                            row.extend(token_cells(lang, &m.usage));
                            t.row(row);
                        }
                    }
                }
            }
            let empty = t.is_empty();
            if !empty {
                t.rule();
                let mut row = vec![lang.t("Total", "Razem").to_string(), String::new(), String::new(), String::new()];
                row.extend(token_cells(lang, &total));
                t.row(row);
                out(t.render(app.color).trim_end());
                if list.total > list.items.len() as i64 {
                    out(&tr!(
                        lang,
                        "Showing the {} most recent of {} sessions; narrow the range with --since.",
                        "Pokazano {} najnowszych z {} sesji; zawęź zakres opcją --since.",
                        list.items.len(),
                        list.total
                    ));
                }
            }
            note(app, &total, empty);
        }
        Report::Blocks => {
            if app.json {
                let v = export::ccusage_blocks(&store, &ctx, &q)?;
                return app.print_json(&v);
            }
            let blocks = kanjo_analytics::blocks(&store, &ctx, &q)?;
            if args.csv {
                print_raw(&export::blocks_csv(&blocks));
                return Ok(());
            }
            let mut headers = vec![
                (lang.t("Start", "Początek"), Align::Left),
                (lang.t("Window", "Okno"), Align::Left),
                (lang.t("Peak", "Szczyt"), Align::Right),
                (lang.t("Models", "Modele"), Align::Left),
            ];
            headers.extend(token_headers(lang));
            headers.push((lang.t("Projected", "Prognoza"), Align::Right));
            let mut t = Table::new(&headers);
            let mut total = Usage::default();
            for b in blocks.iter().filter(|b| b.usage.requests > 0) {
                total += b.usage;
                let window = if b.is_active {
                    tr!(lang, "active until {}", "aktywne do {}", fmt::clock(&ctx, b.end))
                } else {
                    tr!(lang, "until {}", "do {}", fmt::clock(&ctx, b.end))
                };
                let window = if b.anchored { window } else { format!("{window} ({})", lang.t("inferred", "szacowane")) };
                let models: Vec<String> = b.by_model.iter().map(|m| m.key.clone()).collect();
                let mut row = vec![
                    fmt::date_time(&ctx, b.start),
                    window,
                    b.peak_pct.map(|p| fmt::pct(lang, p)).unwrap_or_default(),
                    models_cell(&models),
                ];
                row.extend(token_cells(lang, &b.usage));
                row.push(b.projected_usd.map(|p| fmt::money(lang, p)).unwrap_or_default());
                t.row(row);
                if args.breakdown {
                    for m in &b.by_model {
                        let mut row = vec![String::new(), String::new(), String::new(), format!("- {}", fmt::short_model(&m.label))];
                        row.extend(token_cells(lang, &m.usage));
                        t.row(row);
                    }
                }
            }
            let empty = t.is_empty();
            if !empty {
                t.rule();
                let mut row = vec![lang.t("Total", "Razem").to_string(), String::new(), String::new(), String::new()];
                row.extend(token_cells(lang, &total));
                t.row(row);
                out(t.render(app.color).trim_end());
            }
            note(app, &total, empty);
        }
    }
    Ok(())
}

pub fn export_view(
    app: &mut App,
    view: ExportView,
    format: ExportFormat,
    output: Option<&std::path::Path>,
    filters: &Filters,
    by: ExportBy,
    group: ExportGroup,
) -> CliResult<()> {
    let store = app.store()?;
    let ctx = app.ctx();
    let mut q = build_query(app, &ctx, filters)?;
    let csv = format == ExportFormat::Csv;
    let text = match view {
        ExportView::Usage => {
            q.group = Some(match group {
                ExportGroup::Hour => Granularity::Hour,
                ExportGroup::Day => Granularity::Day,
                ExportGroup::Week => Granularity::Week,
                ExportGroup::Month => Granularity::Month,
            });
            let r = kanjo_analytics::usage_report(&store, &ctx, &q)?;
            if csv {
                export::usage_csv(&r)
            } else {
                to_json(&r)?
            }
        }
        ExportView::Breakdown => {
            q.by = Some(match by {
                ExportBy::Project => Dimension::Project,
                ExportBy::Model => Dimension::Model,
                ExportBy::Agent => Dimension::Agent,
                ExportBy::Skill => Dimension::Skill,
                ExportBy::Mcp => Dimension::Mcp,
                ExportBy::Tool => Dimension::Tool,
                ExportBy::Branch => Dimension::Branch,
                ExportBy::Client => Dimension::Client,
                ExportBy::Session => Dimension::Session,
            });
            let r = kanjo_analytics::breakdown(&store, &ctx, &q)?;
            if csv {
                export::breakdown_csv(&r)
            } else {
                to_json(&r)?
            }
        }
        ExportView::Sessions => {
            q.limit = Some(1000);
            let r = kanjo_analytics::sessions(&store, &ctx, &q)?;
            if csv {
                export::sessions_csv(&r)
            } else {
                to_json(&r)?
            }
        }
        ExportView::Blocks => {
            let r = kanjo_analytics::blocks(&store, &ctx, &q)?;
            if csv {
                export::blocks_csv(&r)
            } else {
                to_json(&r)?
            }
        }
        ExportView::Turns => {
            q.limit = Some(1000);
            let r = kanjo_analytics::top_turns(&store, &ctx, &q)?;
            if csv {
                export::turns_csv(&r)
            } else {
                to_json(&r)?
            }
        }
        ExportView::Alerts => {
            q.limit = Some(1000);
            q.lang = Some(app.lang.code().into());
            let r = kanjo_alerts::list(&store, &ctx, &q)?;
            if csv {
                export::alerts_csv(&r)
            } else {
                to_json(&r)?
            }
        }
        ExportView::Audit => {
            q.limit = Some(1000);
            let r = kanjo_ops::audit::list(&store, &q)?;
            if csv {
                export::audit_csv(&r)
            } else {
                to_json(&r)?
            }
        }
    };
    let format_name = if csv { "csv" } else { "json" };
    match output {
        Some(path) => {
            std::fs::write(path, text.as_bytes()).map_err(|e| {
                fail(
                    tr!(app.lang, "cannot write {} ({e}).", "nie można zapisać pliku {} ({e}).", path.display()),
                    tr!(app.lang, "choose another file with -o.", "wybierz inny plik opcją -o."),
                )
            })?;
            eprintln!("{}", tr!(app.lang, "Saved to {}.", "Zapisano w pliku {}.", path.display()));
        }
        None => print_raw(&text),
    }
    let detail = json!({
        "format": format_name,
        "file": output.map(|p| p.display().to_string()),
        "from": q.from,
        "to": q.to,
        "project": q.project,
        "model": q.model,
        "bytes": text.len(),
        "via": "cli",
    });
    app.audit("export", view.name(), detail);
    Ok(())
}

/// Writes text as it is, ending with a newline, and ignores a closed pipe.
fn print_raw(text: &str) {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    if !text.ends_with('\n') {
        let _ = stdout.write_all(b"\n");
    }
}

fn to_json<T: serde::Serialize>(v: &T) -> CliResult<String> {
    Ok(serde_json::to_string_pretty(v).map_err(kanjo_core::Error::from)?)
}

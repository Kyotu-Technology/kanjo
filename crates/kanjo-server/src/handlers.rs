//! The `/api/v1` routes of docs/api.md.

use crate::error::{ApiErr, ApiJson, ApiPath, ApiQuery, MAX_LIMIT};
use crate::state::{AppState, Inner};
use crate::{assets, events, security};
use axum::extract::{ConnectInfo, DefaultBodyLimit, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use axum::{middleware, Json, Router};
use kanjo_core::api::*;
use kanjo_core::config::Config;
use kanjo_core::settings::Settings;
use kanjo_setup::service::ServiceState;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

const BODY_LIMIT: usize = 256 * 1024;
const MAX_ID_LEN: usize = 200;
const TRANSCRIPT_DEFAULT_MESSAGES: i64 = 500;
const QUARANTINE_ROWS: i64 = 200;
const EXPORT_VIEWS: [&str; 7] = ["usage", "breakdown", "sessions", "blocks", "turns", "alerts", "audit"];

type ApiResult<T> = Result<Json<T>, ApiErr>;

/// The complete application: `/api/v1` plus the embedded UI, behind the
/// security layers (Host, Origin, CSRF, authentication, headers, limits).
pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/overview", get(overview))
        .route("/usage", get(usage))
        .route("/breakdown", get(breakdown))
        .route("/sessions", get(sessions))
        .route("/sessions/{id}", get(session))
        .route("/sessions/{id}/transcript", get(transcript))
        .route("/turns", get(turns))
        .route("/projects", get(projects))
        .route("/projects/{id}", patch(patch_project))
        .route("/blocks", get(blocks))
        .route("/limits", get(limits))
        .route("/heatmap", get(heatmap))
        .route("/compare", get(compare))
        .route("/plan", get(plan))
        .route("/insights", get(insights))
        .route("/alerts", get(alerts))
        .route("/alerts/ack", post(ack))
        .route("/budgets", get(budgets).post(create_budget))
        .route("/budgets/{id}", put(update_budget).delete(delete_budget))
        .route("/settings", get(settings).put(put_settings))
        .route("/system", get(system))
        .route("/system/hooks", post(install_hooks).delete(remove_hooks))
        .route("/system/backups", get(backups))
        .route("/system/backup", post(backup))
        .route("/system/purge", post(purge))
        .route("/system/rescan", post(rescan))
        .route("/audit", get(audit_page))
        .route("/search", get(search))
        .route("/export/{view}", get(export))
        .route("/events", get(events::events))
        .fallback(|| async { ApiErr::not_found("no such API endpoint") });
    Router::new()
        .nest("/api/v1", api)
        .fallback(assets::ui)
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(middleware::from_fn(security::timeout))
        .layer(middleware::from_fn_with_state(state.clone(), security::guard))
        .layer(middleware::from_fn(security::api_errors))
        .layer(middleware::from_fn_with_state(state.clone(), security::headers))
        .layer(middleware::from_fn(security::access_log))
        .with_state(state)
}

/// The peer address of the request, when it came over a socket.
struct Caller(Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for Caller {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Infallible> {
        Ok(Caller(parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip())))
    }
}

/// Actor of an audit entry written for a request to the HTTP API (the dashboard or a script).
pub(crate) const ACTOR_LOCAL: &str = "local";
/// Actor of an audit entry the server writes on its own (daily retention).
pub(crate) const ACTOR_SERVICE: &str = "service";

/// Writes an audit entry with the actor `local`. The request's peer address
/// goes into the detail as `client`, the OS user running the server as `user`
/// (a fixed name in demo mode).
fn audit(i: &Inner, caller: &Caller, action: &str, target: &str, detail: Value) -> Result<(), ApiErr> {
    let mut detail = match detail {
        Value::Object(m) => m,
        other => {
            let mut m = serde_json::Map::new();
            m.insert("value".into(), other);
            m
        }
    };
    detail.insert("client".into(), caller.0.map_or(Value::Null, |ip| Value::String(ip.to_string())));
    let user = if i.demo { "demo".to_string() } else { kanjo_ops::audit::actor() };
    detail.insert("user".into(), Value::String(user));
    let detail = Value::Object(detail);
    let now = kanjo_core::time::now_ms();
    let result = i.store.write(|tx| kanjo_ops::audit::record_tx(tx, now, ACTOR_LOCAL, action, target, &detail));
    result.map_err(|e| {
        tracing::error!(error = %e, action, "audit entry could not be written");
        ApiErr::internal()
    })
}

fn check_id(id: &str) -> Result<(), ApiErr> {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return Err(ApiErr::invalid("invalid id"));
    }
    Ok(())
}

async fn health(State(s): State<AppState>) -> Json<Health> {
    Json(Health { status: "ok".into(), version: kanjo_core::VERSION.to_string(), demo: s.is_demo() })
}

macro_rules! read {
    ($name:ident, $ty:ty, $f:path) => {
        async fn $name(State(s): State<AppState>) -> ApiResult<$ty> {
            s.db(|i, ctx| Ok($f(&i.store, &ctx)?)).await.map(Json)
        }
    };
}

macro_rules! read_q {
    ($name:ident, $ty:ty, $f:path) => {
        async fn $name(State(s): State<AppState>, ApiQuery(q): ApiQuery) -> ApiResult<$ty> {
            s.db(move |i, ctx| Ok($f(&i.store, &ctx, &q)?)).await.map(Json)
        }
    };
}

read!(overview, Overview, kanjo_analytics::overview);
read!(projects, Vec<Project>, kanjo_analytics::projects);
read!(plan, PlanFit, kanjo_analytics::plan_fit);
read!(insights, Insights, kanjo_alerts::insights);
read!(budgets, Vec<Budget>, kanjo_alerts::budgets);
read_q!(usage, UsageReport, kanjo_analytics::usage_report);
read_q!(breakdown, Breakdown, kanjo_analytics::breakdown);
read_q!(sessions, SessionList, kanjo_analytics::sessions);
read_q!(turns, Vec<Turn>, kanjo_analytics::top_turns);
read_q!(blocks, Vec<Block>, kanjo_analytics::blocks);
read_q!(limits, LimitsDetail, kanjo_analytics::limits_detail);
read_q!(heatmap, Heatmap, kanjo_analytics::heatmap);
read_q!(compare, Comparison, kanjo_analytics::compare);
read_q!(search, Vec<SearchResult>, kanjo_analytics::search);
read_q!(alerts, AlertList, kanjo_alerts::list);

async fn audit_page(State(s): State<AppState>, ApiQuery(q): ApiQuery) -> ApiResult<AuditPage> {
    s.db(move |i, _| Ok(kanjo_ops::audit::list(&i.store, &q)?)).await.map(Json)
}

async fn session(State(s): State<AppState>, ApiPath(id): ApiPath<String>) -> ApiResult<SessionDetail> {
    check_id(&id)?;
    s.db(move |i, ctx| kanjo_analytics::session_detail(&i.store, &ctx, &id)?.ok_or_else(|| ApiErr::not_found("no such session")))
        .await
        .map(Json)
}

async fn transcript(
    State(s): State<AppState>,
    ApiPath(id): ApiPath<String>,
    ApiQuery(q): ApiQuery,
    caller: Caller,
) -> ApiResult<Transcript> {
    check_id(&id)?;
    s.db(move |i, _| {
        if !Settings::load(&i.store)?.privacy.conversation_viewer {
            return Err(ApiErr::disabled("the conversation viewer is off; enable it in Settings -> Privacy"));
        }
        if i.demo {
            return Err(ApiErr::not_found("demo sessions have no transcript"));
        }
        let limit = q.limit.unwrap_or(TRANSCRIPT_DEFAULT_MESSAGES) as usize;
        let t = kanjo_ingest::read_transcript(&i.store, &i.paths, &id, limit)?;
        audit(i, &caller, "transcript.view", &id, json!({ "messages": t.messages.len(), "truncated": t.truncated }))?;
        Ok(t)
    })
    .await
    .map(Json)
}

async fn patch_project(State(s): State<AppState>, ApiPath(id): ApiPath<i64>, ApiJson(p): ApiJson<ProjectPatch>) -> ApiResult<Project> {
    s.db(move |i, ctx| Ok(kanjo_analytics::update_project(&i.store, &ctx, id, &p)?)).await.map(Json)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AckRequest {
    id: Option<i64>,
}

#[derive(Debug, Serialize)]
struct AckResponse {
    acknowledged: i64,
}

async fn ack(State(s): State<AppState>, ApiJson(req): ApiJson<AckRequest>) -> ApiResult<AckResponse> {
    s.db(move |i, ctx| Ok(AckResponse { acknowledged: kanjo_alerts::acknowledge(&i.store, &ctx, req.id)? })).await.map(Json)
}

async fn create_budget(State(s): State<AppState>, ApiJson(b): ApiJson<BudgetInput>) -> ApiResult<Budget> {
    let budget = s.db(move |i, ctx| Ok(kanjo_alerts::create_budget(&i.store, &ctx, &b)?)).await?;
    s.inner.alert_kick.notify_one();
    Ok(Json(budget))
}

async fn update_budget(State(s): State<AppState>, ApiPath(id): ApiPath<i64>, ApiJson(b): ApiJson<BudgetInput>) -> ApiResult<Budget> {
    let budget = s.db(move |i, ctx| Ok(kanjo_alerts::update_budget(&i.store, &ctx, id, &b)?)).await?;
    s.inner.alert_kick.notify_one();
    Ok(Json(budget))
}

async fn delete_budget(State(s): State<AppState>, ApiPath(id): ApiPath<i64>) -> Result<StatusCode, ApiErr> {
    s.db(move |i, _| Ok(kanjo_alerts::delete_budget(&i.store, id)?)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn settings(State(s): State<AppState>) -> ApiResult<Settings> {
    s.db(|i, _| Ok(Settings::load(&i.store)?)).await.map(Json)
}

const MAX_USD_RATE: f64 = 1e6;
const MAX_RETENTION_DAYS: u32 = 36_500;

fn validate_settings(s: &Settings) -> Result<(), ApiErr> {
    let within = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
    if s.week_start > 6 {
        return Err(ApiErr::invalid("week_start must be 0 (Monday) to 6 (Sunday)"));
    }
    if !(s.currency.usd_rate.is_finite() && s.currency.usd_rate > 0.0 && s.currency.usd_rate <= MAX_USD_RATE) {
        return Err(ApiErr::invalid("currency.usd_rate must be a number above 0 and at most 1000000"));
    }
    if s.currency.code.len() != 3 || !s.currency.code.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(ApiErr::invalid("currency.code must be a three-letter currency code such as USD"));
    }
    if s.plan.monthly_usd.is_some_and(|v| !v.is_finite() || v < 0.0) {
        return Err(ApiErr::invalid("plan.monthly_usd must not be negative"));
    }
    let thresholds = &s.alerts.limit_thresholds;
    if thresholds.is_empty() || thresholds.len() > 10 || !thresholds.iter().all(|t| within(*t, 1.0, 100.0)) {
        return Err(ApiErr::invalid("alerts.limit_thresholds takes 1 to 10 percentages between 1 and 100"));
    }
    if !within(s.alerts.context_threshold, 10.0, 100.0) {
        return Err(ApiErr::invalid("alerts.context_threshold must be a percentage between 10 and 100"));
    }
    let r = &s.retention;
    let days = [r.requests_days, r.aggregates_days, r.limit_samples_days, r.alerts_days, r.audit_days, r.quarantine_days];
    if days.iter().any(|d| *d > MAX_RETENTION_DAYS) {
        return Err(ApiErr::invalid("retention days must be between 0 (keep forever) and 36500"));
    }
    Ok(())
}

fn changed_sections(old: &Settings, new: &Settings) -> Vec<String> {
    match (serde_json::to_value(old), serde_json::to_value(new)) {
        (Ok(Value::Object(a)), Ok(Value::Object(b))) => b.iter().filter(|(k, v)| a.get(*k) != Some(v)).map(|(k, _)| k.clone()).collect(),
        _ => Vec::new(),
    }
}

async fn put_settings(State(s): State<AppState>, caller: Caller, ApiJson(new): ApiJson<Settings>) -> ApiResult<Settings> {
    validate_settings(&new)?;
    s.db(move |i, _| {
        let old = Settings::load(&i.store)?;
        new.save(&i.store)?;
        i.apply_settings(&new)?;
        audit(i, &caller, "settings.update", "settings", json!({ "changed": changed_sections(&old, &new) }))?;
        Ok(Settings::load(&i.store)?)
    })
    .await
    .map(Json)
}

fn quarantine_rows(c: &rusqlite::Connection) -> kanjo_core::Result<Vec<QuarantineRow>> {
    let mut stmt =
        c.prepare("SELECT record_type, signature, reason, cc_version, count, last_seen FROM quarantine ORDER BY last_seen DESC LIMIT ?1")?;
    let rows = stmt.query_map([QUARANTINE_ROWS], |r| {
        Ok(QuarantineRow {
            record_type: r.get(0)?,
            signature: r.get(1)?,
            reason: r.get(2)?,
            cc_version: r.get(3)?,
            count: r.get(4)?,
            last_seen: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn claude_code_version(c: &rusqlite::Connection) -> kanjo_core::Result<Option<String>> {
    for sql in [
        "SELECT cc_version FROM session_status WHERE cc_version IS NOT NULL AND cc_version <> '' ORDER BY ts DESC LIMIT 1",
        "SELECT cc_version FROM requests WHERE cc_version IS NOT NULL AND cc_version <> '' ORDER BY ts DESC LIMIT 1",
    ] {
        if let Some(v) = c.query_row(sql, [], |r| r.get::<_, String>(0)).optional()? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

/// `HookStatus` with the last statusline time from the database. In demo mode
/// nothing outside the demo directory is inspected.
fn hook_status(i: &Inner, ctx: &kanjo_core::Ctx) -> Result<HookStatus, ApiErr> {
    let last = kanjo_analytics::data_status(&i.store, ctx)?.last_statusline_at;
    let mut status = if i.demo {
        HookStatus {
            statusline_installed: false,
            statusline_chain: None,
            session_hooks_installed: false,
            last_statusline_at: None,
            service_installed: false,
        }
    } else {
        kanjo_setup::hook_status(&i.paths, &i.config())
    };
    status.last_statusline_at = last;
    Ok(status)
}

async fn system(State(s): State<AppState>, ApiQuery(q): ApiQuery) -> ApiResult<SystemInfo> {
    let port = s.port();
    s.db(move |i, ctx| {
        let settings = Settings::load(&i.store)?;
        let config = i.config();
        let data = kanjo_analytics::data_status(&i.store, &ctx)?;
        let pricing = kanjo_ops::pricing::info(&i.store)?;
        let database = DatabaseInfo {
            size_bytes: i.store.size_bytes() as i64,
            encrypted: i.store.is_encrypted(),
            key_source: i.key_source,
            schema_version: i.store.schema_version()?,
        };
        let (quarantine, cc_version, stored) = i.store.read(|c| {
            let stored: (i64, Option<i64>) = c.query_row("SELECT count(*), MAX(ts) FROM requests", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok((quarantine_rows(c)?, claude_code_version(c)?, stored))
        })?;
        let mut ingest = i.ingest_status();
        ingest.requests_stored = stored.0;
        ingest.last_event_at = ingest.last_event_at.or(stored.1);
        let hooks = hook_status(i, &ctx)?;
        let service = if hooks.service_installed { ServiceState::Installed } else { ServiceState::NotInstalled };
        let exe = if i.demo { None } else { std::env::current_exe().ok() };
        let mut input = kanjo_setup::doctor::DoctorInput::new(&i.paths, &config, ctx.now);
        input.exe = exe.as_deref();
        input.database = Some(database.clone());
        input.last_statusline_at = data.last_statusline_at;
        input.pricing = Some(pricing.clone());
        input.quarantined_records = Some(data.quarantined_records);
        input.claude_code_version = cc_version.clone();
        input.service = service;
        input.demo = i.demo;
        let asked = q.lang.as_deref().map(|l| l.to_ascii_lowercase());
        let lang = match asked.as_deref().and_then(|l| l.get(..2)) {
            Some(l @ ("en" | "pl")) => l.to_string(),
            _ => kanjo_alerts::settings_lang(&settings).to_string(),
        };
        let checks = kanjo_setup::doctor::run(&input, &lang);
        Ok(SystemInfo {
            version: kanjo_core::VERSION.to_string(),
            demo: i.demo,
            bell_available: kanjo_alerts::notify::bell_available(),
            data_dir: i.paths.home.display().to_string(),
            claude_dir: i.paths.claude_dir.display().to_string(),
            claude_code_version: cc_version,
            database,
            ingest,
            hooks,
            pricing,
            server: ServerInfo { bind: config.server.bind.clone(), port, tls: i.tls, auth: i.token.is_some() },
            quarantine,
            retention: settings.retention,
            checks,
        })
    })
    .await
    .map(Json)
}

/// The installer saves the config it is given to config.toml, so it gets the
/// file's own content rather than the server's (which may carry overrides such
/// as port 0); the statusline section it changes is copied back.
fn with_disk_config<T>(i: &Inner, f: impl FnOnce(&mut Config) -> kanjo_core::Result<T>) -> Result<T, ApiErr> {
    let mut disk = Config::load(&i.paths)?;
    let out = f(&mut disk)?;
    i.config.write().unwrap_or_else(|e| e.into_inner()).statusline = disk.statusline;
    Ok(out)
}

async fn install_hooks(State(s): State<AppState>, caller: Caller, ApiJson(req): ApiJson<HookInstallRequest>) -> ApiResult<HookStatus> {
    if s.is_demo() {
        return Err(ApiErr::disabled("the hook installer is disabled in demo mode"));
    }
    s.db(move |i, ctx| {
        let exe = std::env::current_exe().map_err(kanjo_core::Error::from)?;
        let report = with_disk_config(i, |config| kanjo_setup::claude::install(&i.paths, config, &req, &exe))?;
        if req.service {
            kanjo_setup::service::install(&exe, &i.paths)?;
        }
        let detail = json!({
            "statusline": req.statusline,
            "session_hooks": req.session_hooks,
            "service": req.service,
            "changed": report.changed,
            "settings_backup": report.backup.as_ref().and_then(|b| b.file_name()).map(|n| n.to_string_lossy().into_owned()),
        });
        audit(i, &caller, "hooks.install", &report.settings_path.display().to_string(), detail)?;
        hook_status(i, &ctx)
    })
    .await
    .map(Json)
}

async fn remove_hooks(State(s): State<AppState>, caller: Caller) -> ApiResult<HookStatus> {
    if s.is_demo() {
        return Err(ApiErr::disabled("the hook installer is disabled in demo mode"));
    }
    s.db(move |i, ctx| {
        let exe = std::env::current_exe().map_err(kanjo_core::Error::from)?;
        let report = with_disk_config(i, |config| kanjo_setup::claude::uninstall(&i.paths, config, &exe))?;
        let detail = json!({ "changed": report.changed });
        audit(i, &caller, "hooks.uninstall", &report.settings_path.display().to_string(), detail)?;
        hook_status(i, &ctx)
    })
    .await
    .map(Json)
}

async fn backups(State(s): State<AppState>) -> ApiResult<Vec<BackupInfo>> {
    s.db(|i, _| Ok(kanjo_ops::backup::list(&i.paths)?)).await.map(Json)
}

async fn backup(State(s): State<AppState>, caller: Caller) -> ApiResult<BackupInfo> {
    s.db(move |i, ctx| {
        let info = kanjo_ops::backup::create(&i.store, &i.paths, ctx.now)?;
        audit(i, &caller, "backup.create", &info.file, json!({ "size_bytes": info.size_bytes }))?;
        Ok(info)
    })
    .await
    .map(Json)
}

/// The audit target of a purge and the name of what it deletes: the project's
/// display name (its alias under path redaction), the session id, the local
/// date of the cutoff, or "everything".
fn purge_target(i: &Inner, ctx: &kanjo_core::Ctx, req: &PurgeRequest) -> Result<(String, String), ApiErr> {
    Ok(match (req.project, &req.session, req.before, req.everything) {
        (Some(p), _, _, _) => {
            let redact = Settings::load(&i.store)?.privacy.redact_paths;
            let sql = "SELECT CASE WHEN ?2 THEN alias ELSE name END FROM projects WHERE id = ?1";
            let name = i.store.read(|c| Ok(c.query_row(sql, rusqlite::params![p, redact], |r| r.get::<_, String>(0)).optional()?))?;
            (format!("project:{p}"), name.unwrap_or_else(|| format!("project {p}")))
        }
        (_, Some(s), _, _) => (format!("session:{s}"), s.clone()),
        (_, _, Some(b), _) => (format!("before:{b}"), ctx.date_label(b)),
        (_, _, _, true) => ("everything".into(), "everything".into()),
        _ => ("none".into(), "nothing".into()),
    })
}

async fn purge(State(s): State<AppState>, caller: Caller, ApiJson(req): ApiJson<PurgeRequest>) -> ApiResult<PurgeReport> {
    let report = s
        .db(move |i, ctx| {
            let (target, name) = purge_target(i, &ctx, &req)?;
            let report = kanjo_ops::retention::purge(&i.store, &req)?;
            let detail = json!({ "request": req, "deleted": name, "report": report });
            audit(i, &caller, "data.purge", &target, detail)?;
            Ok(report)
        })
        .await?;
    s.publish(LiveEvent::Usage { requests: 0, cost_usd: 0.0, sessions: Vec::new() });
    Ok(Json(report))
}

/// Forgets the transcript cursors and imports all history again in the
/// background; stored requests deduplicate. Progress arrives as `Ingest` events.
async fn rescan(State(s): State<AppState>) -> ApiResult<IngestStatus> {
    crate::runtime::spawn_rescan(&s);
    Ok(Json(s.inner.ingest_status()))
}

fn render<T: Serialize>(csv: bool, data: &T, to_csv: impl FnOnce(&T) -> String) -> Result<String, ApiErr> {
    if csv {
        Ok(to_csv(data))
    } else {
        Ok(serde_json::to_string_pretty(data).map_err(kanjo_core::Error::from)?)
    }
}

/// `GET /api/v1/export/{view}`: CSV through `kanjo_analytics::export`, or the
/// view's documented API type as JSON. List views export up to 1000 rows per
/// request unless `limit` says otherwise; `offset` pages further.
async fn export(
    State(s): State<AppState>,
    ApiPath(view): ApiPath<String>,
    ApiQuery(q): ApiQuery,
    caller: Caller,
) -> Result<Response, ApiErr> {
    use kanjo_analytics::export as x;
    let csv = match q.format.as_deref().unwrap_or("csv") {
        "csv" => true,
        "json" => false,
        _ => return Err(ApiErr::invalid("format must be csv or json")),
    };
    if !EXPORT_VIEWS.contains(&view.as_str()) {
        return Err(ApiErr::not_found(format!("unknown export view; one of {}", EXPORT_VIEWS.join(", "))));
    }
    let (body, name) = s
        .db(move |i, ctx| {
            let rows = Query { limit: Some(q.limit.unwrap_or(MAX_LIMIT)), ..q.clone() };
            let st = &i.store;
            let body = match view.as_str() {
                "usage" => render(csv, &kanjo_analytics::usage_report(st, &ctx, &q)?, x::usage_csv)?,
                "breakdown" => render(csv, &kanjo_analytics::breakdown(st, &ctx, &q)?, x::breakdown_csv)?,
                "sessions" => render(csv, &kanjo_analytics::sessions(st, &ctx, &rows)?, x::sessions_csv)?,
                "blocks" => render(csv, &kanjo_analytics::blocks(st, &ctx, &q)?, |b: &Vec<Block>| x::blocks_csv(b))?,
                "turns" => render(csv, &kanjo_analytics::top_turns(st, &ctx, &rows)?, |t: &Vec<Turn>| x::turns_csv(t))?,
                "alerts" => render(csv, &kanjo_alerts::list(st, &ctx, &rows)?, x::alerts_csv)?,
                _ => render(csv, &kanjo_ops::audit::list(st, &rows)?, x::audit_csv)?,
            };
            let name = format!("kanjo-{view}-{}.{}", ctx.date_label(ctx.now), if csv { "csv" } else { "json" });
            let detail = json!({
                "format": if csv { "csv" } else { "json" },
                "from": q.from,
                "to": q.to,
                "project": q.project,
                "bytes": body.len(),
            });
            audit(i, &caller, "export", &view, detail)?;
            Ok((body, name))
        })
        .await?;
    let mut res = body.into_response();
    let h = res.headers_mut();
    let content_type = if csv { "text/csv; charset=utf-8" } else { "application/json" };
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

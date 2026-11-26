//! Export shaping: CSV for the report DTOs and ccusage-compatible JSON.
//!
//! CSV follows RFC 4180 (comma separator, CRLF line ends, fields with a comma, quote
//! or line break quoted, quotes doubled) with a fixed column order per view. A text
//! cell that starts with `=`, `+`, `-` or `@` gets a leading single quote so a
//! spreadsheet does not evaluate it; numeric cells are written as they are.
//! Timestamps are ISO 8601 UTC. Path redaction is applied by the functions that
//! produce the DTOs, so exports carry the same labels as the UI.

use crate::blocks::detailed;
use crate::common::{bucket_label, bucket_start, Env, Filter, Keys, Src, REPORT_DAYS, USAGE_REQUEST_COLS};
use kanjo_core::api::{AlertList, AuditPage, Block, Breakdown, Granularity, Query, SessionList, Turn, Usage, UsageReport};
use kanjo_core::time::{DAY_MS, MINUTE_MS};
use kanjo_core::{Ctx, Error, Result, Store};
use rusqlite::types::Value;
use serde_json::{json, Map, Value as Json};
use std::collections::BTreeMap;

enum Cell {
    Text(String),
    Num(String),
}

fn text(s: impl Into<String>) -> Cell {
    Cell::Text(s.into())
}

fn opt_text(s: &Option<String>) -> Cell {
    Cell::Text(s.clone().unwrap_or_default())
}

fn num(v: impl std::fmt::Display) -> Cell {
    Cell::Num(v.to_string())
}

fn opt_num<T: std::fmt::Display>(v: Option<T>) -> Cell {
    Cell::Num(v.map(|x| x.to_string()).unwrap_or_default())
}

fn ts(v: i64) -> Cell {
    Cell::Num(iso(v, false))
}

fn opt_ts(v: Option<i64>) -> Cell {
    Cell::Num(v.map(|x| iso(x, false)).unwrap_or_default())
}

fn iso(ms: i64, millis: bool) -> String {
    let dt = kanjo_core::time::utc_datetime(ms);
    if millis {
        dt.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    } else {
        dt.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
    }
}

fn enum_name<T: serde::Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(Json::String(s)) => s,
        _ => String::new(),
    }
}

fn field(cell: &Cell) -> String {
    let s = match cell {
        Cell::Text(s) if s.starts_with(['=', '+', '-', '@']) => format!("'{s}"),
        Cell::Text(s) | Cell::Num(s) => s.clone(),
    };
    if s.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn csv(header: &[&str], rows: impl IntoIterator<Item = Vec<Cell>>) -> String {
    let mut out = header.join(",");
    out.push_str("\r\n");
    for row in rows {
        out.push_str(&row.iter().map(field).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

const USAGE_HEADER: [&str; 9] = [
    "requests",
    "unpriced_requests",
    "input_tokens",
    "output_tokens",
    "cache_read_tokens",
    "cache_write_5m_tokens",
    "cache_write_1h_tokens",
    "total_tokens",
    "cost_usd",
];

fn header(before: &[&'static str], after: &[&'static str]) -> Vec<&'static str> {
    before.iter().chain(USAGE_HEADER.iter()).chain(after.iter()).copied().collect()
}

fn usage_cells(u: &Usage) -> Vec<Cell> {
    vec![
        num(u.requests),
        num(u.unpriced_requests),
        num(u.tokens.input),
        num(u.tokens.output),
        num(u.tokens.cache_read),
        num(u.tokens.cache_write_5m),
        num(u.tokens.cache_write_1h),
        num(u.tokens.total()),
        num(u.cost_usd),
    ]
}

fn row(mut before: Vec<Cell>, u: &Usage, after: Vec<Cell>) -> Vec<Cell> {
    before.extend(usage_cells(u));
    before.extend(after);
    before
}

/// Columns: start, label, usage columns, models.
pub fn usage_csv(r: &UsageReport) -> String {
    csv(
        &header(&["start", "label"], &["models"]),
        r.buckets.iter().map(|b| {
            let models = b.by_model.iter().map(|m| m.model.as_str()).collect::<Vec<_>>().join(";");
            row(vec![ts(b.start), text(&b.label)], &b.usage, vec![text(models)])
        }),
    )
}

/// Columns: dimension, key, label, usage columns, share.
pub fn breakdown_csv(b: &Breakdown) -> String {
    let dim = enum_name(&b.by);
    csv(
        &header(&["dimension", "key", "label"], &["share"]),
        b.rows.iter().map(|r| row(vec![text(&dim), text(&r.key), text(&r.label)], &r.usage, vec![num(r.share)])),
    )
}

/// Columns: id, project_id, project, title, git_branch, started_at, last_at, usage
/// columns, main_model, models, subagent_cost_usd, is_active, context_pct, cache_hit_ratio.
pub fn sessions_csv(l: &SessionList) -> String {
    csv(
        &header(
            &["id", "project_id", "project", "title", "git_branch", "started_at", "last_at"],
            &["main_model", "models", "subagent_cost_usd", "is_active", "context_pct", "cache_hit_ratio"],
        ),
        l.items.iter().map(|s| {
            row(
                vec![
                    text(&s.id),
                    num(s.project_id),
                    text(&s.project),
                    opt_text(&s.title),
                    opt_text(&s.git_branch),
                    ts(s.started_at),
                    ts(s.last_at),
                ],
                &s.usage,
                vec![
                    opt_text(&s.main_model),
                    text(s.models.join(";")),
                    num(s.subagent_cost_usd),
                    num(s.is_active),
                    opt_num(s.context_pct),
                    opt_num(s.cache_hit_ratio),
                ],
            )
        }),
    )
}

/// Columns: start, end, anchored, is_active, peak_pct, usage columns, models, burn_usd_per_hour, projected_usd.
pub fn blocks_csv(blocks: &[Block]) -> String {
    csv(
        &header(&["start", "end", "anchored", "is_active", "peak_pct"], &["models", "burn_usd_per_hour", "projected_usd"]),
        blocks.iter().map(|b| {
            let models = b.by_model.iter().map(|m| m.key.as_str()).collect::<Vec<_>>().join(";");
            row(
                vec![ts(b.start), ts(b.end), num(b.anchored), num(b.is_active), opt_num(b.peak_pct)],
                &b.usage,
                vec![text(models), opt_num(b.burn_usd_per_hour), opt_num(b.projected_usd)],
            )
        }),
    )
}

/// Columns: ts, message_id, session_id, project, model, agent_type, the token columns, total_tokens, cost_usd, stop_reason, tools.
pub fn turns_csv(turns: &[Turn]) -> String {
    let head = [
        "ts",
        "message_id",
        "session_id",
        "project",
        "model",
        "agent_type",
        "input_tokens",
        "output_tokens",
        "cache_read_tokens",
        "cache_write_5m_tokens",
        "cache_write_1h_tokens",
        "total_tokens",
        "cost_usd",
        "stop_reason",
        "tools",
    ];
    csv(
        &head,
        turns.iter().map(|t| {
            vec![
                ts(t.ts),
                text(&t.message_id),
                text(&t.session_id),
                text(&t.project),
                text(&t.model),
                opt_text(&t.agent_type),
                num(t.tokens.input),
                num(t.tokens.output),
                num(t.tokens.cache_read),
                num(t.tokens.cache_write_5m),
                num(t.tokens.cache_write_1h),
                num(t.tokens.total()),
                opt_num(t.cost_usd),
                opt_text(&t.stop_reason),
                text(t.tools.join(";")),
            ]
        }),
    )
}

/// Columns: id, ts, kind, severity, title, message, acknowledged_at, params (JSON).
pub fn alerts_csv(l: &AlertList) -> String {
    csv(
        &["id", "ts", "kind", "severity", "title", "message", "acknowledged_at", "params"],
        l.items.iter().map(|a| {
            vec![
                num(a.id),
                ts(a.ts),
                text(enum_name(&a.kind)),
                text(enum_name(&a.severity)),
                text(&a.title),
                text(&a.message),
                opt_ts(a.acknowledged_at),
                text(a.params.to_string()),
            ]
        }),
    )
}

/// Columns: id, ts, actor, action, target, detail (JSON), hash.
pub fn audit_csv(p: &AuditPage) -> String {
    csv(
        &["id", "ts", "actor", "action", "target", "detail", "hash"],
        p.items.iter().map(|e| {
            vec![num(e.id), ts(e.ts), text(&e.actor), text(&e.action), text(&e.target), text(e.detail.to_string()), text(&e.hash)]
        }),
    )
}

// ------------------------------------------------------------------- ccusage

#[derive(Default)]
struct Acc {
    usage: Usage,
    models: BTreeMap<String, Usage>,
}

fn cache_creation(u: &Usage) -> i64 {
    u.tokens.cache_write_5m + u.tokens.cache_write_1h
}

fn token_fields(m: &mut Map<String, Json>, u: &Usage) {
    m.insert("inputTokens".into(), json!(u.tokens.input));
    m.insert("outputTokens".into(), json!(u.tokens.output));
    m.insert("cacheCreationTokens".into(), json!(cache_creation(u)));
    m.insert("cacheReadTokens".into(), json!(u.tokens.cache_read));
}

/// `inputTokens`... `totalCost`, `modelsUsed` and `modelBreakdowns` of one row, after `first`.
fn ccusage_row(first: Vec<(&str, Json)>, acc: &Acc) -> Json {
    let mut m: Map<String, Json> = first.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    token_fields(&mut m, &acc.usage);
    m.insert("totalTokens".into(), json!(acc.usage.tokens.total()));
    m.insert("totalCost".into(), json!(acc.usage.cost_usd));
    let mut models: Vec<(&String, &Usage)> = acc.models.iter().collect();
    models.sort_by(|a, b| b.1.cost_usd.total_cmp(&a.1.cost_usd).then(a.0.cmp(b.0)));
    m.insert("modelsUsed".into(), json!(models.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>()));
    let breakdowns: Vec<Json> = models
        .iter()
        .map(|(name, u)| {
            let mut b = Map::new();
            b.insert("modelName".into(), json!(name));
            token_fields(&mut b, u);
            b.insert("cost".into(), json!(u.cost_usd));
            if u.unpriced_requests > 0 {
                b.insert("missingPricing".into(), json!(true));
            }
            Json::Object(b)
        })
        .collect();
    m.insert("modelBreakdowns".into(), Json::Array(breakdowns));
    Json::Object(m)
}

fn totals(rows: &[&Acc]) -> Json {
    let mut total = Usage::default();
    let mut unpriced: Vec<&String> = Vec::new();
    for acc in rows {
        total += acc.usage;
        unpriced.extend(acc.models.iter().filter(|(_, u)| u.unpriced_requests > 0).map(|(m, _)| m));
    }
    unpriced.sort();
    unpriced.dedup();
    let mut m = Map::new();
    token_fields(&mut m, &total);
    m.insert("totalTokens".into(), json!(total.tokens.total()));
    m.insert("totalCost".into(), json!(total.cost_usd));
    if !unpriced.is_empty() {
        m.insert("unpricedModels".into(), json!(unpriced));
    }
    Json::Object(m)
}

fn ccusage_periods(store: &Store, ctx: &Ctx, q: &Query, g: Granularity) -> Result<Json> {
    let to = q.to.unwrap_or(ctx.now);
    let from = q.from.unwrap_or_else(|| match g {
        Granularity::Week => ctx.week_start_of(to) - 11 * 7 * DAY_MS,
        Granularity::Month => ctx.month_shift(to, -11),
        _ => ctx.day_start(to) - (REPORT_DAYS - 1) * DAY_MS,
    });
    if from > to {
        return Err(Error::Invalid("from must not be after to".into()));
    }
    let f = Filter::from_query(q, &Env::load(store)?);
    let rows = store.read(|c| Src::new(c)?.fetch(from, to, &f, Keys { hour: true, model: true, ..Keys::NONE }))?;
    let mut buckets: BTreeMap<i64, Acc> = BTreeMap::new();
    for r in rows {
        let acc = buckets.entry(bucket_start(ctx, g, r.hour.max(from))).or_default();
        acc.usage += r.usage;
        *acc.models.entry(r.model).or_default() += r.usage;
    }
    let (key, field) = match g {
        Granularity::Week => ("weekly", "week"),
        Granularity::Month => ("monthly", "month"),
        _ => ("daily", "date"),
    };
    let items: Vec<Json> = buckets
        .iter()
        .map(|(start, acc)| {
            let label = if g == Granularity::Month { bucket_label(ctx, g, *start) } else { ctx.date_label(*start) };
            ccusage_row(vec![(field, json!(label))], acc)
        })
        .collect();
    Ok(json!({ key: items, "totals": totals(&buckets.values().collect::<Vec<_>>()) }))
}

/// ccusage `daily --json`: `{"daily": [{"date", token counts, "totalTokens", "totalCost",
/// "modelsUsed", "modelBreakdowns"}], "totals"}`, days with usage only, local dates.
/// Range defaults to 30 days; project / model / agent / client filters apply.
pub fn ccusage_daily(store: &Store, ctx: &Ctx, q: &Query) -> Result<Json> {
    ccusage_periods(store, ctx, q, Granularity::Day)
}

/// ccusage `weekly --json`: rows keyed by `week` (local date of the week start). Range defaults to 12 weeks.
pub fn ccusage_weekly(store: &Store, ctx: &Ctx, q: &Query) -> Result<Json> {
    ccusage_periods(store, ctx, q, Granularity::Week)
}

/// ccusage `monthly --json`: rows keyed by `month` (`YYYY-MM`). Range defaults to 12 months.
pub fn ccusage_monthly(store: &Store, ctx: &Ctx, q: &Query) -> Result<Json> {
    ccusage_periods(store, ctx, q, Granularity::Month)
}

/// ccusage `session --json`: `{"sessions": [{"sessionId", "projectPath", token counts,
/// "totalTokens", "totalCost", "firstActivity", "lastActivity", "modelsUsed",
/// "modelBreakdowns"}], "totals"}`, sessions overlapping the range (default 30 days),
/// oldest activity first. `projectPath` is the alias under path redaction.
pub fn ccusage_session(store: &Store, ctx: &Ctx, q: &Query) -> Result<Json> {
    let r = crate::common::range(ctx, q, REPORT_DAYS)?;
    let env = Env::load(store)?;
    store.read(|c| {
        let mut params = vec![Value::Integer(r.from), Value::Integer(r.to)];
        let mut cond = "s.last_at >= ? AND s.started_at < ?".to_string();
        if let Some(p) = q.project {
            cond.push_str(" AND s.project_id = ?");
            params.push(Value::Integer(p));
        }
        let sql = format!(
            "SELECT r.session_id, r.model, {USAGE_REQUEST_COLS} FROM requests r JOIN sessions s ON s.id = r.session_id WHERE {cond} GROUP BY 1, 2"
        );
        let mut stmt = c.prepare(&sql)?;
        let mut models: BTreeMap<String, BTreeMap<String, Usage>> = BTreeMap::new();
        for row in stmt.query_map(rusqlite::params_from_iter(params.clone()), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, crate::common::usage_at(r, 2)?))
        })? {
            let (sid, model, usage) = row?;
            models.entry(sid).or_default().insert(model, usage);
        }
        let sql = format!(
            "SELECT s.id, s.project_id, s.started_at, s.last_at, s.requests, s.input, s.output, s.cache_read, s.cache_write_5m,
                    s.cache_write_1h, s.cost_usd, s.unpriced_requests
             FROM sessions s WHERE {cond} ORDER BY s.last_at, s.id"
        );
        let mut stmt = c.prepare(&sql)?;
        let sessions = stmt
            .query_map(rusqlite::params_from_iter(params), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, crate::common::usage_at(r, 4)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let accs: Vec<(String, i64, i64, i64, Acc)> = sessions
            .into_iter()
            .map(|(id, project, first, last, usage)| {
                let m = models.remove(&id).unwrap_or_default();
                (id, project, first, last, Acc { usage, models: m })
            })
            .collect();
        let items: Vec<Json> = accs
            .iter()
            .map(|(id, project, first, last, acc)| {
                let path = env.project_path(*project).unwrap_or_else(|| env.project(*project));
                ccusage_row(
                    vec![
                        ("sessionId", json!(id)),
                        ("projectPath", json!(path)),
                        ("firstActivity", json!(iso(*first, true))),
                        ("lastActivity", json!(iso(*last, true))),
                    ],
                    acc,
                )
            })
            .collect();
        Ok(json!({ "sessions": items, "totals": totals(&accs.iter().map(|a| &a.4).collect::<Vec<_>>()) }))
    })
}

/// ccusage `blocks --json`: `{"blocks": [{"id", "startTime", "endTime", "actualEndTime",
/// "isActive", "isGap", "entries", "tokenCounts": {"inputTokens", "outputTokens",
/// "cacheCreationInputTokens", "cacheReadInputTokens"}, "totalTokens", "costUSD", "models",
/// "burnRate", "projection", "usageLimitResetTime" (anchored blocks only)}]}`. Range defaults
/// to 30 days; gap entries are not emitted.
pub fn ccusage_blocks(store: &Store, ctx: &Ctx, q: &Query) -> Result<Json> {
    let r = crate::common::range(ctx, q, REPORT_DAYS)?;
    let env = Env::load(store)?;
    let blocks = store.read(|c| detailed(&Src::new(c)?, ctx, &env, r.from, r.to))?;
    let items: Vec<Json> = blocks
        .iter()
        .map(|d| {
            let b = &d.block;
            let u = &b.usage;
            let (burn, projection) = if b.is_active {
                let minutes = ((ctx.now - d.first_ts.unwrap_or(b.start)).max(5 * MINUTE_MS)) as f64 / MINUTE_MS as f64;
                let remaining = (b.end - ctx.now).max(0) as f64 / MINUTE_MS as f64;
                let tpm = u.tokens.total() as f64 / minutes;
                (
                    json!({
                        "costPerHour": b.burn_usd_per_hour.unwrap_or(0.0),
                        "tokensPerMinute": tpm,
                        "tokensPerMinuteForIndicator": (u.tokens.input + u.tokens.output) as f64 / minutes,
                    }),
                    json!({
                        "totalTokens": (u.tokens.total() as f64 + tpm * remaining).round() as i64,
                        "totalCost": b.projected_usd.unwrap_or(u.cost_usd),
                        "remainingMinutes": remaining.round() as i64,
                    }),
                )
            } else {
                (Json::Null, Json::Null)
            };
            let mut item = json!({
                "id": iso(b.start, true),
                "startTime": iso(b.start, true),
                "endTime": iso(b.end, true),
                "actualEndTime": d.last_ts.map(|t| iso(t, true)),
                "isActive": b.is_active,
                "isGap": false,
                "entries": u.requests,
                "tokenCounts": {
                    "inputTokens": u.tokens.input,
                    "outputTokens": u.tokens.output,
                    "cacheCreationInputTokens": cache_creation(u),
                    "cacheReadInputTokens": u.tokens.cache_read,
                },
                "totalTokens": u.tokens.total(),
                "costUSD": u.cost_usd,
                "models": b.by_model.iter().map(|m| m.key.as_str()).collect::<Vec<_>>(),
                "burnRate": burn,
                "projection": projection,
            });
            if b.anchored {
                item["usageLimitResetTime"] = json!(iso(b.end, true));
            }
            item
        })
        .collect();
    Ok(json!({ "blocks": items }))
}

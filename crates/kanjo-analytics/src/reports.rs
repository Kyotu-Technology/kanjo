use crate::common::{
    agent_key, bucket_label, bucket_start, bucket_starts, dow_hour, range, share, Env, Filter, Keys, ModelAcc, Range, Row, Src,
    REPORT_DAYS, UNASSIGNED, USAGE_REQUEST_COLS,
};
use kanjo_core::api::{
    delta_pct, Breakdown, BreakdownRow, Comparison, ComparisonSide, Confidence, DayCell, Dimension, Driver, Granularity, HeatCell, Heatmap,
    ModelSlice, PeriodSummary, Query, Usage, UsageBucket, UsageReport,
};
use kanjo_core::time::DAY_MS;
use kanjo_core::{Ctx, Error, Result, Store};
use rusqlite::types::Value;
use std::collections::{BTreeMap, HashMap};

/// Heatmaps default to a year so the calendar view has something to show.
const HEATMAP_DAYS: i64 = 365;
/// Comparison drivers reported at most.
const MAX_DRIVERS: usize = 8;

/// Usage over time in local buckets (`group`, default day), honouring the
/// range (default 30 days) and the project / model / agent / client filters.
/// Empty buckets are included. Reads `usage_hourly`.
pub fn usage_report(store: &Store, ctx: &Ctx, q: &Query) -> Result<UsageReport> {
    let env = Env::load(store)?;
    store.read(|c| usage_report_in(&Src::new(c)?, ctx, &env, q))
}

fn usage_report_in(src: &Src, ctx: &Ctx, env: &Env, q: &Query) -> Result<UsageReport> {
    let r = range(ctx, q, REPORT_DAYS)?;
    let group = q.group.unwrap_or_default();
    let rows = src.fetch(r.from, r.to, &Filter::from_query(q, env), Keys { hour: true, model: true, ..Keys::NONE })?;
    let buckets = buckets(ctx, group, r, &rows)?;
    let total = buckets.iter().map(|b| b.usage).sum();
    Ok(UsageReport { group, from: r.from, to: r.to, buckets, total, confidence: Confidence::LowerBound })
}

/// Folds rows grouped by hour (and optionally model) into local buckets, empty ones included.
pub(crate) fn buckets(ctx: &Ctx, g: Granularity, r: Range, rows: &[Row]) -> Result<Vec<UsageBucket>> {
    let starts = bucket_starts(ctx, g, r.from, r.to)?;
    let index: HashMap<i64, usize> = starts.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    let mut acc: Vec<(Usage, ModelAcc)> = vec![Default::default(); starts.len()];
    for row in rows {
        let Some(&i) = index.get(&bucket_start(ctx, g, row.hour.max(r.from))) else { continue };
        acc[i].0 += row.usage;
        let m = acc[i].1.entry(row.model.clone()).or_default();
        m.0 += row.usage.cost_usd;
        m.1 += row.usage.tokens.total();
    }
    Ok(starts
        .into_iter()
        .zip(acc)
        .map(|(start, (usage, models))| UsageBucket { start, label: bucket_label(ctx, g, start), usage, by_model: model_slices(models) })
        .collect())
}

pub(crate) fn model_slices(models: ModelAcc) -> Vec<ModelSlice> {
    let mut v: Vec<ModelSlice> = models
        .into_iter()
        .filter(|(m, _)| !m.is_empty())
        .map(|(model, (cost_usd, tokens))| ModelSlice { model, cost_usd, tokens })
        .collect();
    v.sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd).then(b.tokens.cmp(&a.tokens)).then(a.model.cmp(&b.model)));
    v
}

/// Usage split by `q.by` (default project) over the range (default 30 days), with filters.
/// Project, model, agent and client come from `usage_hourly`; branch, session, tool,
/// skill and MCP from the request rows. Tool, skill and MCP rows attribute to each name
/// the usage of the distinct requests that invoked it, so their shares need not sum to 1.
pub fn breakdown(store: &Store, ctx: &Ctx, q: &Query) -> Result<Breakdown> {
    let env = Env::load(store)?;
    store.read(|c| breakdown_in(&Src::new(c)?, ctx, &env, q))
}

pub(crate) fn breakdown_in(src: &Src, ctx: &Ctx, env: &Env, q: &Query) -> Result<Breakdown> {
    let r = range(ctx, q, REPORT_DAYS)?;
    let by = q.by.unwrap_or_default();
    let f = Filter::from_query(q, env);
    let (mut rows, total, confidence) = match by {
        Dimension::Project | Dimension::Model | Dimension::Agent | Dimension::Client => {
            let keys = Keys { project: true, model: by == Dimension::Model, agent: by == Dimension::Agent, ..Keys::NONE };
            let fetched = src.fetch(r.from, r.to, &f, keys)?;
            let total: Usage = fetched.iter().map(|x| x.usage).sum();
            (group_rows(&fetched, by, env), total, Confidence::LowerBound)
        }
        Dimension::Branch | Dimension::Session => {
            let rows = request_groups(src, r, &f, by, env)?;
            let total = rows.iter().map(|x| x.usage).sum();
            (rows, total, Confidence::LowerBound)
        }
        Dimension::Skill | Dimension::Mcp | Dimension::Tool => {
            let kind = match by {
                Dimension::Skill => "skill",
                Dimension::Mcp => "mcp",
                _ => "tool",
            };
            let rows = tool_groups(src, r, &f, kind, None)?;
            let total = request_total(src, r, &f)?;
            (rows.into_iter().map(|(_, name, usage)| (name.clone(), name, usage)).map(row_of).collect(), total, Confidence::Estimate)
        }
    };
    finish(&mut rows, total.cost_usd);
    if let Some(n) = q.limit {
        rows.truncate(n.max(0) as usize);
    }
    Ok(Breakdown { by, from: r.from, to: r.to, rows, total, confidence })
}

fn row_of((key, label, usage): (String, String, Usage)) -> BreakdownRow {
    BreakdownRow { key, label, usage, share: 0.0 }
}

/// Sorts by cost (then tokens, then key) and fills in the shares.
pub(crate) fn finish(rows: &mut [BreakdownRow], total_cost: f64) {
    for r in rows.iter_mut() {
        r.share = share(r.usage.cost_usd, total_cost);
    }
    rows.sort_by(|a, b| {
        b.usage.cost_usd.total_cmp(&a.usage.cost_usd).then(b.usage.tokens.total().cmp(&a.usage.tokens.total())).then(a.key.cmp(&b.key))
    });
}

/// Groups usage rows by project, model, agent or client (other dimensions yield nothing).
pub(crate) fn group_rows(rows: &[Row], by: Dimension, env: &Env) -> Vec<BreakdownRow> {
    let mut acc: BTreeMap<String, (String, Usage)> = BTreeMap::new();
    for r in rows {
        let (key, label) = match by {
            Dimension::Project => (r.project.to_string(), env.project(r.project)),
            Dimension::Model => (r.model.clone(), r.model.clone()),
            Dimension::Agent => (agent_key(&r.agent), agent_key(&r.agent)),
            Dimension::Client => {
                let c = env.client(r.project).unwrap_or_else(|| UNASSIGNED.to_string());
                (c.clone(), c)
            }
            _ => continue,
        };
        acc.entry(key).or_insert_with(|| (label, Usage::default())).1 += r.usage;
    }
    let total: f64 = acc.values().map(|(_, u)| u.cost_usd).sum();
    let mut out: Vec<BreakdownRow> = acc.into_iter().map(|(key, (label, usage))| BreakdownRow { key, label, usage, share: 0.0 }).collect();
    finish(&mut out, total);
    out
}

fn request_total(src: &Src, r: Range, f: &Filter) -> Result<Usage> {
    let mut params = vec![Value::Integer(r.from), Value::Integer(r.to)];
    let filter = f.sql("r", false, &mut params);
    let sql = format!("SELECT {USAGE_REQUEST_COLS} FROM requests r WHERE r.ts >= ? AND r.ts < ?{filter}");
    Ok(src.c.query_row(&sql, rusqlite::params_from_iter(params), |row| crate::common::usage_at(row, 0))?)
}

fn request_groups(src: &Src, r: Range, f: &Filter, by: Dimension, env: &Env) -> Result<Vec<BreakdownRow>> {
    let key = if by == Dimension::Branch { "COALESCE(r.git_branch, '')" } else { "r.session_id" };
    let mut params = vec![Value::Integer(r.from), Value::Integer(r.to)];
    let filter = f.sql("r", false, &mut params);
    let sql =
        format!("SELECT {key}, MIN(r.project_id), {USAGE_REQUEST_COLS} FROM requests r WHERE r.ts >= ? AND r.ts < ?{filter} GROUP BY 1");
    let mut stmt = src.c.prepare(&sql)?;
    let raw = stmt
        .query_map(rusqlite::params_from_iter(params), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, crate::common::usage_at(row, 2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let titles: HashMap<String, Option<String>> = if by == Dimension::Session && !env.redact {
        let mut stmt = src.c.prepare("SELECT id, title FROM sessions WHERE last_at >= ?1")?;
        let m = stmt.query_map([r.from], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        m
    } else {
        HashMap::new()
    };
    raw.into_iter()
        .map(|(k, project, usage)| {
            let (key, label) = match by {
                Dimension::Branch if k.is_empty() => (String::new(), "(none)".to_string()),
                Dimension::Branch if env.redact => {
                    let p = kanjo_core::store::write::alias(src.c, "branch", &k)?;
                    (p.clone(), p)
                }
                Dimension::Branch => (k.clone(), k),
                _ => {
                    let short: String = k.chars().take(8).collect();
                    let title = titles.get(&k).cloned().flatten();
                    let label = match title {
                        Some(t) => format!("{t} ({})", env.project(project)),
                        None => format!("{short} ({})", env.project(project)),
                    };
                    (k, label)
                }
            };
            Ok(BreakdownRow { key, label, usage, share: 0.0 })
        })
        .collect()
}

/// Usage per (kind, name) of the distinct requests that invoked each name.
/// `kind` restricts to one kind; `session` to one session.
pub(crate) fn tool_groups(src: &Src, r: Range, f: &Filter, kind: &str, session: Option<&str>) -> Result<Vec<(String, String, Usage)>> {
    let mut params = vec![Value::Integer(r.from), Value::Integer(r.to)];
    let mut cond = f.sql("r", false, &mut params);
    if !kind.is_empty() {
        cond.push_str(" AND rt.kind = ?");
        params.push(Value::Text(kind.to_string()));
    }
    if let Some(s) = session {
        cond.push_str(" AND r.session_id = ?");
        params.push(Value::Text(s.to_string()));
    }
    let sql = format!(
        "SELECT x.kind, x.name, COUNT(*), SUM(x.input), SUM(x.output), SUM(x.cache_read), SUM(x.cache_write_5m), SUM(x.cache_write_1h),
                COALESCE(SUM(x.cost_usd), 0), SUM(x.cost_usd IS NULL)
         FROM (SELECT DISTINCT r.id, rt.kind, rt.name, r.input, r.output, r.cache_read, r.cache_write_5m, r.cache_write_1h, r.cost_usd
               FROM requests r JOIN request_tools rt ON rt.request_id = r.id
               WHERE r.ts >= ? AND r.ts < ?{cond}) x
         GROUP BY x.kind, x.name"
    );
    let mut stmt = src.c.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params), |row| Ok((row.get(0)?, row.get(1)?, crate::common::usage_at(row, 2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Cost, tokens and requests per local weekday and hour (all 168 cells) and per local
/// calendar day (empty days included). Range defaults to 365 days; filters apply.
pub fn heatmap(store: &Store, ctx: &Ctx, q: &Query) -> Result<Heatmap> {
    let r = range(ctx, q, HEATMAP_DAYS)?;
    let f = Filter::from_query(q, &Env::load(store)?);
    let rows = store.read(|c| Src::new(c)?.fetch(r.from, r.to, &f, Keys { hour: true, ..Keys::NONE }))?;
    let mut cells: Vec<HeatCell> =
        (0..7u8).flat_map(|dow| (0..24u8).map(move |hour| HeatCell { dow, hour, cost_usd: 0.0, tokens: 0, requests: 0 })).collect();
    let starts = bucket_starts(ctx, Granularity::Day, r.from, r.to)?;
    let index: HashMap<i64, usize> = starts.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    let mut days: Vec<DayCell> =
        starts.iter().map(|s| DayCell { date: ctx.date_label(*s), cost_usd: 0.0, tokens: 0, requests: 0 }).collect();
    for row in &rows {
        let (dow, hour) = dow_hour(ctx, row.hour.max(r.from));
        let cell = &mut cells[dow as usize * 24 + hour as usize];
        cell.cost_usd += row.usage.cost_usd;
        cell.tokens += row.usage.tokens.total();
        cell.requests += row.usage.requests;
        if let Some(&i) = index.get(&ctx.day_start(row.hour.max(r.from))) {
            days[i].cost_usd += row.usage.cost_usd;
            days[i].tokens += row.usage.tokens.total();
            days[i].requests += row.usage.requests;
        }
    }
    Ok(Heatmap { from: r.from, to: r.to, cells, days })
}

/// The current stretch of a period against the same stretch of the previous one.
pub(crate) fn period_summary(src: &Src, f: &Filter, cur: Range, prev: Range) -> Result<PeriodSummary> {
    let usage = src.total(cur.from, cur.to, f)?;
    let previous = src.total(prev.from, prev.to, f)?;
    Ok(PeriodSummary { from: cur.from, to: cur.to, usage, previous, delta_pct: delta_pct(usage.cost_usd, previous.cost_usd) })
}

/// Today, this week and this month up to now, each with the previous period up to the same elapsed span.
pub(crate) fn period_ranges(ctx: &Ctx) -> [(Range, Range); 3] {
    let now = ctx.now;
    let day = ctx.day_start(now);
    let week = ctx.week_start_of(now);
    let month = ctx.month_start(now);
    let prev_month = ctx.month_shift(now, -1);
    [
        (Range { from: day, to: now }, Range { from: day - DAY_MS, to: now - DAY_MS }),
        (Range { from: week, to: now }, Range { from: week - 7 * DAY_MS, to: now - 7 * DAY_MS }),
        (Range { from: month, to: now }, Range { from: prev_month, to: (prev_month + (now - month)).min(month) }),
    ]
}

/// Period-over-period comparison. `q.period` is `week` (default) or `month`;
/// `q.offset` counts periods back. The current period (offset 0) runs up to now and is
/// compared with the previous period up to the same elapsed span; earlier periods are
/// compared whole. Drivers are the largest absolute cost changes across projects,
/// models and agents. Filters apply.
pub fn compare(store: &Store, ctx: &Ctx, q: &Query) -> Result<Comparison> {
    let env = Env::load(store)?;
    store.read(|c| compare_in(&Src::new(c)?, ctx, &env, q))
}

fn compare_in(src: &Src, ctx: &Ctx, env: &Env, q: &Query) -> Result<Comparison> {
    let period = q.period.clone().unwrap_or_else(|| "week".into());
    let n = q.offset.unwrap_or(0).max(0);
    let now = ctx.now;
    let (g, cur, prev) = match period.as_str() {
        "week" => {
            let ws = ctx.week_start_of(now);
            if n == 0 {
                (Granularity::Week, Range { from: ws, to: now }, Range { from: ws - 7 * DAY_MS, to: now - 7 * DAY_MS })
            } else {
                let start = ws - n * 7 * DAY_MS;
                (Granularity::Week, Range { from: start, to: start + 7 * DAY_MS }, Range { from: start - 7 * DAY_MS, to: start })
            }
        }
        "month" => {
            let n = n as i32;
            if n == 0 {
                let ms = ctx.month_start(now);
                let pm = ctx.month_shift(now, -1);
                (Granularity::Month, Range { from: ms, to: now }, Range { from: pm, to: (pm + (now - ms)).min(ms) })
            } else {
                let cur = Range { from: ctx.month_shift(now, -n), to: ctx.month_shift(now, 1 - n) };
                (Granularity::Month, cur, Range { from: ctx.month_shift(now, -n - 1), to: cur.from })
            }
        }
        other => return Err(Error::Invalid(format!("unknown period {other:?}, expected week or month"))),
    };
    let f = Filter::from_query(q, env);
    let keys = Keys { project: true, model: true, agent: true, ..Keys::NONE };
    let cur_rows = src.fetch(cur.from, cur.to, &f, keys)?;
    let prev_rows = src.fetch(prev.from, prev.to, &f, keys)?;
    let side = |r: Range, rows: &[Row]| ComparisonSide {
        from: r.from,
        to: r.to,
        label: bucket_label(ctx, g, r.from),
        usage: rows.iter().map(|x| x.usage).sum(),
    };
    let current = side(cur, &cur_rows);
    let previous = side(prev, &prev_rows);
    let mut drivers = Vec::new();
    for dim in [Dimension::Project, Dimension::Model, Dimension::Agent] {
        let mut map: BTreeMap<String, (String, f64, f64)> = BTreeMap::new();
        for r in group_rows(&cur_rows, dim, env) {
            map.entry(r.key).or_insert((r.label, 0.0, 0.0)).1 += r.usage.cost_usd;
        }
        for r in group_rows(&prev_rows, dim, env) {
            map.entry(r.key).or_insert((r.label, 0.0, 0.0)).2 += r.usage.cost_usd;
        }
        for (key, (label, c, p)) in map {
            if (c - p).abs() > 1e-9 {
                drivers.push(Driver { dimension: dim, key, label, current_usd: c, previous_usd: p, delta_usd: c - p });
            }
        }
    }
    drivers.sort_by(|a, b| b.delta_usd.abs().total_cmp(&a.delta_usd.abs()).then(a.key.cmp(&b.key)));
    drivers.truncate(MAX_DRIVERS);
    Ok(Comparison {
        period,
        delta_usd: current.usage.cost_usd - previous.usage.cost_usd,
        delta_pct: delta_pct(current.usage.cost_usd, previous.usage.cost_usd),
        current,
        previous,
        drivers,
    })
}

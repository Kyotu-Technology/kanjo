use kanjo_core::api::{Granularity, Query, Tokens, Usage};
use kanjo_core::settings::Settings;
use kanjo_core::time::{hour_floor, DAY_MS, HOUR_MS, MINUTE_MS};
use kanjo_core::{Ctx, Error, Result, Store};
use rusqlite::types::Value;
use rusqlite::Connection;
use std::collections::HashMap;

/// Default length of a report range, in local days ending today.
pub(crate) const REPORT_DAYS: i64 = 30;
/// Key under which breakdowns report the main conversation.
pub(crate) const MAIN_AGENT: &str = "main";
/// Key under which breakdowns report projects without a client.
pub(crate) const UNASSIGNED: &str = "unassigned";
/// A session counts as active while its last request is at most this old.
pub(crate) const ACTIVE_SESSION_MS: i64 = kanjo_core::time::ACTIVE_SESSION_MS;
/// Burn rates look back this far.
pub(crate) const BURN_WINDOW_MIN: i64 = 60;

/// What every read model needs besides the connection: the redaction switch and the project labels.
pub(crate) struct Env {
    pub redact: bool,
    projects: HashMap<i64, ProjectLabel>,
}

pub(crate) struct ProjectLabel {
    pub name: String,
    pub alias: String,
    pub path: Option<String>,
    pub client: Option<String>,
}

impl Env {
    pub fn load(store: &Store) -> Result<Env> {
        let settings = Settings::load(store)?;
        let projects = store.read(load_projects)?;
        Ok(Env { redact: settings.privacy.redact_paths, projects })
    }

    /// Display label of a project: its alias when path redaction is on.
    pub fn project(&self, id: i64) -> String {
        match self.projects.get(&id) {
            Some(p) if self.redact => p.alias.clone(),
            Some(p) => p.name.clone(),
            None => format!("project-{id}"),
        }
    }

    pub fn project_path(&self, id: i64) -> Option<String> {
        if self.redact {
            return None;
        }
        self.projects.get(&id).and_then(|p| p.path.clone())
    }

    pub fn client(&self, id: i64) -> Option<String> {
        self.projects.get(&id).and_then(|p| p.client.clone())
    }

    pub fn hide(&self, value: Option<String>) -> Option<String> {
        if self.redact {
            None
        } else {
            value
        }
    }
}

fn load_projects(c: &Connection) -> Result<HashMap<i64, ProjectLabel>> {
    let mut stmt = c.prepare("SELECT id, name, alias, path, client FROM projects")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, ProjectLabel { name: r.get(1)?, alias: r.get(2)?, path: r.get(3)?, client: r.get(4)? })))?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}

/// Stable pseudonym for a free-text value (branch names under redaction).
/// Cost and tokens per model inside one bucket.
pub(crate) type ModelAcc = std::collections::BTreeMap<String, (f64, i64)>;

pub(crate) fn pseudonym(prefix: &str, value: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in value.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{prefix}-{:06x}", h & 0xff_ffff)
}

pub(crate) fn agent_key(agent_type: &str) -> String {
    if agent_type.is_empty() {
        MAIN_AGENT.to_string()
    } else {
        agent_type.to_string()
    }
}

/// A half-open time range `[from, to)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Range {
    pub from: i64,
    pub to: i64,
}

/// `q.from`/`q.to`, defaulting to the last `days` local days up to now.
pub(crate) fn range(ctx: &Ctx, q: &Query, days: i64) -> Result<Range> {
    let to = q.to.unwrap_or(ctx.now);
    let from = q.from.unwrap_or_else(|| ctx.day_start(to) - (days - 1) * DAY_MS);
    if from > to {
        return Err(Error::Invalid("from must not be after to".into()));
    }
    Ok(Range { from, to })
}

pub(crate) fn limit(q: &Query, default: i64, max: i64) -> i64 {
    q.limit.unwrap_or(default).clamp(1, max)
}

/// Filters shared by the usage queries. Agent `main` is the main conversation, client `unassigned` a project without client.
#[derive(Debug, Clone, Default)]
pub(crate) struct Filter {
    pub project: Option<i64>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub client: Option<String>,
}

impl Filter {
    pub fn from_query(q: &Query) -> Filter {
        let text = |v: &Option<String>| v.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Filter { project: q.project, model: text(&q.model), agent: text(&q.agent), client: text(&q.client) }
    }

    /// SQL conditions (each starting with ` AND `) for a table aliased `t`.
    pub fn sql(&self, t: &str, hourly: bool, params: &mut Vec<Value>) -> String {
        let mut s = String::new();
        if let Some(p) = self.project {
            s.push_str(&format!(" AND {t}.project_id = ?"));
            params.push(Value::Integer(p));
        }
        if let Some(m) = &self.model {
            s.push_str(&format!(" AND {t}.model = ?"));
            params.push(Value::Text(m.clone()));
        }
        if let Some(a) = &self.agent {
            let col = if hourly { format!("{t}.agent_type") } else { format!("COALESCE({t}.agent_type, '')") };
            s.push_str(&format!(" AND {col} = ?"));
            params.push(Value::Text(if a == MAIN_AGENT { String::new() } else { a.clone() }));
        }
        if let Some(cl) = &self.client {
            if cl == UNASSIGNED {
                s.push_str(&format!(" AND {t}.project_id IN (SELECT id FROM projects WHERE client IS NULL)"));
            } else {
                s.push_str(&format!(" AND {t}.project_id IN (SELECT id FROM projects WHERE client = ?)"));
                params.push(Value::Text(cl.clone()));
            }
        }
        s
    }
}

/// Which columns a usage fetch groups by.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Keys {
    pub hour: bool,
    pub project: bool,
    pub model: bool,
    pub agent: bool,
}

impl Keys {
    pub const NONE: Keys = Keys { hour: false, project: false, model: false, agent: false };
}

/// One aggregated usage row. Fields not grouped by hold their default.
#[derive(Debug, Clone, Default)]
pub(crate) struct Row {
    pub hour: i64,
    pub project: i64,
    pub model: String,
    pub agent: String,
    pub usage: Usage,
}

pub(crate) const USAGE_HOURLY_COLS: &str = "COALESCE(SUM(h.requests), 0), COALESCE(SUM(h.input), 0), COALESCE(SUM(h.output), 0),
     COALESCE(SUM(h.cache_read), 0), COALESCE(SUM(h.cache_write_5m), 0), COALESCE(SUM(h.cache_write_1h), 0),
     COALESCE(SUM(h.cost_usd), 0), COALESCE(SUM(h.unpriced_requests), 0)";

pub(crate) const USAGE_REQUEST_COLS: &str = "COUNT(r.id), COALESCE(SUM(r.input), 0), COALESCE(SUM(r.output), 0),
     COALESCE(SUM(r.cache_read), 0), COALESCE(SUM(r.cache_write_5m), 0), COALESCE(SUM(r.cache_write_1h), 0),
     COALESCE(SUM(r.cost_usd), 0), COALESCE(SUM(r.cost_usd IS NULL), 0)";

/// Reads the eight usage columns starting at `at`.
pub(crate) fn usage_at(r: &rusqlite::Row, at: usize) -> rusqlite::Result<Usage> {
    Ok(Usage {
        requests: r.get(at)?,
        tokens: Tokens {
            input: r.get(at + 1)?,
            output: r.get(at + 2)?,
            cache_read: r.get(at + 3)?,
            cache_write_5m: r.get(at + 4)?,
            cache_write_1h: r.get(at + 5)?,
        },
        cost_usd: r.get(at + 6)?,
        unpriced_requests: r.get(at + 7)?,
    })
}

/// Usage source: whole hours come from `usage_hourly` (which outlives the purge
/// of request rows), partial hours at the range edges from `requests`. An edge
/// hour whose request rows were already purged falls back to its whole hourly row.
pub(crate) struct Src<'a> {
    pub c: &'a Connection,
    oldest_request: Option<i64>,
}

impl<'a> Src<'a> {
    pub fn new(c: &'a Connection) -> Result<Src<'a>> {
        let oldest_request = c.query_row("SELECT MIN(ts) FROM requests", [], |r| r.get(0))?;
        Ok(Src { c, oldest_request })
    }

    fn requests_cover(&self, ts: i64) -> bool {
        self.oldest_request.is_some_and(|o| hour_floor(ts) >= hour_floor(o))
    }

    pub fn total(&self, from: i64, to: i64, f: &Filter) -> Result<Usage> {
        Ok(self.fetch(from, to, f, Keys::NONE)?.into_iter().map(|r| r.usage).sum())
    }

    pub fn fetch(&self, from: i64, to: i64, f: &Filter, keys: Keys) -> Result<Vec<Row>> {
        if from >= to {
            return Ok(Vec::new());
        }
        let hf = ceil_hour(from);
        let ht = hour_floor(to);
        let mut hourly: Vec<(i64, i64)> = Vec::new();
        let mut req: Vec<(i64, i64)> = Vec::new();
        if hf < ht {
            hourly.push((hf, ht));
        }
        let edges = if hf > ht { vec![(from, to)] } else { vec![(from, hf), (ht, to)] };
        for (a, b) in edges.into_iter().filter(|(a, b)| a < b) {
            if self.requests_cover(a) {
                req.push((a, b));
            } else {
                let h = hour_floor(a);
                hourly.push((h, h + HOUR_MS));
            }
        }
        let mut out = Vec::new();
        if !hourly.is_empty() {
            out.extend(self.query(true, &hourly, f, keys)?);
        }
        if !req.is_empty() {
            out.extend(self.query(false, &req, f, keys)?);
        }
        Ok(out)
    }

    fn query(&self, hourly: bool, ranges: &[(i64, i64)], f: &Filter, keys: Keys) -> Result<Vec<Row>> {
        let (t, ts, table, cols) =
            if hourly { ("h", "h.hour", "usage_hourly h", USAGE_HOURLY_COLS) } else { ("r", "r.ts", "requests r", USAGE_REQUEST_COLS) };
        let hour = if !keys.hour {
            "0".to_string()
        } else if hourly {
            "h.hour".to_string()
        } else {
            "(r.ts / 3600000) * 3600000".to_string()
        };
        let project = if keys.project { format!("{t}.project_id") } else { "0".into() };
        let model = if keys.model { format!("{t}.model") } else { "''".into() };
        let agent = match (keys.agent, hourly) {
            (false, _) => "''".to_string(),
            (true, true) => "h.agent_type".to_string(),
            (true, false) => "COALESCE(r.agent_type, '')".to_string(),
        };
        let mut params = Vec::new();
        let cond = ranges
            .iter()
            .map(|(a, b)| {
                params.push(Value::Integer(*a));
                params.push(Value::Integer(*b));
                format!("({ts} >= ? AND {ts} < ?)")
            })
            .collect::<Vec<_>>()
            .join(" OR ");
        let filter = f.sql(t, hourly, &mut params);
        let sql = format!("SELECT {hour}, {project}, {model}, {agent}, {cols} FROM {table} WHERE ({cond}){filter} GROUP BY 1, 2, 3, 4");
        let mut stmt = self.c.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params), |r| {
                Ok(Row { hour: r.get(0)?, project: r.get(1)?, model: r.get(2)?, agent: r.get(3)?, usage: usage_at(r, 4)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter(|r| r.usage.requests != 0 || r.usage.tokens.total() != 0 || r.usage.cost_usd != 0.0).collect())
    }
}

pub(crate) fn ceil_hour(ts: i64) -> i64 {
    let f = hour_floor(ts);
    if f == ts {
        f
    } else {
        f + HOUR_MS
    }
}

fn offset_ms(ctx: &Ctx) -> i64 {
    ctx.tz_offset_min as i64 * MINUTE_MS
}

fn local(ctx: &Ctx, ts: i64) -> jiff::civil::DateTime {
    kanjo_core::time::utc_datetime(ts + offset_ms(ctx))
}

/// Start of the local bucket of granularity `g` containing `ts`.
pub(crate) fn bucket_start(ctx: &Ctx, g: Granularity, ts: i64) -> i64 {
    match g {
        Granularity::Hour => (ts + offset_ms(ctx)).div_euclid(HOUR_MS) * HOUR_MS - offset_ms(ctx),
        Granularity::Day => ctx.day_start(ts),
        Granularity::Week => ctx.week_start_of(ts),
        Granularity::Month => ctx.month_start(ts),
    }
}

pub(crate) fn bucket_next(ctx: &Ctx, g: Granularity, start: i64) -> i64 {
    match g {
        Granularity::Hour => start + HOUR_MS,
        Granularity::Day => start + DAY_MS,
        Granularity::Week => start + 7 * DAY_MS,
        Granularity::Month => ctx.month_shift(start, 1),
    }
}

/// `YYYY-MM-DD HH:00`, `YYYY-MM-DD`, `YYYY-Www` (ISO week of the Monday inside the bucket) or `YYYY-MM`.
pub(crate) fn bucket_label(ctx: &Ctx, g: Granularity, start: i64) -> String {
    match g {
        Granularity::Hour => local(ctx, start).strftime("%Y-%m-%d %H:00").to_string(),
        Granularity::Day => ctx.date_label(start),
        Granularity::Week => {
            let monday = start + ((7 - ctx.week_start as i64) % 7) * DAY_MS;
            local(ctx, monday).strftime("%G-W%V").to_string()
        }
        Granularity::Month => local(ctx, start).strftime("%Y-%m").to_string(),
    }
}

pub(crate) fn minute_label(ctx: &Ctx, ts: i64) -> String {
    local(ctx, ts).strftime("%Y-%m-%d %H:%M").to_string()
}

/// Local weekday (0 = Monday) and hour of `ts`.
pub(crate) fn dow_hour(ctx: &Ctx, ts: i64) -> (u8, u8) {
    let l = ts + offset_ms(ctx);
    let days = l.div_euclid(DAY_MS);
    ((days + 3).rem_euclid(7) as u8, (l.rem_euclid(DAY_MS) / HOUR_MS) as u8)
}

/// Bucket starts covering `[from, to)`; an error when the range would produce an absurd number of buckets.
pub(crate) fn bucket_starts(ctx: &Ctx, g: Granularity, from: i64, to: i64) -> Result<Vec<i64>> {
    const MAX_BUCKETS: usize = 20_000;
    let mut out = Vec::new();
    let mut s = bucket_start(ctx, g, from);
    while s < to {
        out.push(s);
        if out.len() > MAX_BUCKETS {
            return Err(Error::Invalid("the range holds too many buckets for this grouping".into()));
        }
        s = bucket_next(ctx, g, s);
    }
    Ok(out)
}

/// Escapes `%`, `_` and the escape character for a `LIKE ... ESCAPE '\\'` pattern.
pub(crate) fn like_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub(crate) fn share(part: f64, total: f64) -> f64 {
    if total > 1e-12 {
        part / total
    } else {
        0.0
    }
}

/// Placeholders `?, ?, ...` for an `IN` list.
pub(crate) fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

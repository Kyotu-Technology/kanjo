use crate::common::{
    agent_key, bucket_label, bucket_next, bucket_start, like_escape, limit, minute_label, placeholders, range, usage_at, Env, Filter,
    ModelAcc, Range, Src, ACTIVE_SESSION_MS, BURN_WINDOW_MIN, REPORT_DAYS, USAGE_REQUEST_COLS,
};
use crate::reports::{finish, model_slices, tool_groups};
use kanjo_core::api::{
    BreakdownRow, BurnRate, CacheMiss, CacheStatus, ConfigChange, Granularity, Query, SearchResult, SessionDetail, SessionEvent,
    SessionList, SessionStatus, SessionSummary, Tokens, Turn, Usage, UsageBucket,
};
use kanjo_core::time::{DAY_MS, HOUR_MS, MINUTE_MS};
use kanjo_core::{Ctx, Error, Result, Store};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension};
use std::collections::{BTreeMap, HashMap};

/// Config changes this long before a cache miss are reported as correlated with it.
const CORRELATION_MS: i64 = 30 * MINUTE_MS;
const SEARCH_LIMIT: i64 = 10;

const SESSION_COLS: &str =
    "s.id, s.project_id, s.title, s.git_branch, s.started_at, s.last_at, s.requests, s.input, s.output, s.cache_read,
    s.cache_write_5m, s.cache_write_1h, s.cost_usd, s.unpriced_requests, s.sidechain_cost_usd, st.ctx_used_pct, st.cache_hit_ratio";

const SESSION_FROM: &str = "sessions s JOIN projects p ON p.id = s.project_id LEFT JOIN session_status st ON st.session_id = s.id";

/// Sessions overlapping the range (default 30 days), optionally of one project and
/// matching `q` (project name, title, branch or id prefix; only the alias and id under
/// path redaction). `sort`: `cost` (default), `recent`, `tokens`, `duration`.
/// `limit` defaults to 50 (at most 1000), `offset` to 0. Usage is the whole session.
pub fn sessions(store: &Store, ctx: &Ctx, q: &Query) -> Result<SessionList> {
    let env = Env::load(store)?;
    store.read(|c| sessions_in(c, ctx, &env, q))
}

fn sessions_in(c: &Connection, ctx: &Ctx, env: &Env, q: &Query) -> Result<SessionList> {
    let r = range(ctx, q, REPORT_DAYS)?;
    let order = match q.sort.as_deref().unwrap_or("cost") {
        "cost" => "s.cost_usd DESC, s.last_at DESC",
        "recent" => "s.last_at DESC",
        "tokens" => "(s.input + s.output + s.cache_read + s.cache_write_5m + s.cache_write_1h) DESC, s.last_at DESC",
        "duration" => "(s.last_at - s.started_at) DESC, s.last_at DESC",
        other => return Err(Error::Invalid(format!("unknown sort {other:?}, expected cost, recent, tokens or duration"))),
    };
    let mut params = vec![Value::Integer(r.from), Value::Integer(r.to)];
    let mut cond = "s.last_at >= ? AND s.started_at < ?".to_string();
    if let Some(p) = q.project {
        cond.push_str(" AND s.project_id = ?");
        params.push(Value::Integer(p));
    }
    if let Some(text) = q.q.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        cond.push_str(&text_match(env, text, &mut params));
    }
    let total: i64 =
        c.query_row(&format!("SELECT COUNT(*) FROM {SESSION_FROM} WHERE {cond}"), rusqlite::params_from_iter(params.clone()), |r| {
            r.get(0)
        })?;
    params.push(Value::Integer(limit(q, 50, 1000)));
    params.push(Value::Integer(q.offset.unwrap_or(0).max(0)));
    let items = load_sessions(c, ctx, env, &format!("{cond} ORDER BY {order}, s.id LIMIT ? OFFSET ?"), params)?;
    Ok(SessionList { items, total })
}

/// ` AND (...)` matching a session by project label, title, branch or id prefix.
fn text_match(env: &Env, text: &str, params: &mut Vec<Value>) -> String {
    let inner = format!("%{}%", like_escape(text));
    let prefix = format!("{}%", like_escape(text));
    if env.redact {
        params.extend([Value::Text(inner), Value::Text(prefix)]);
        " AND (p.alias LIKE ? ESCAPE '\\' OR s.id LIKE ? ESCAPE '\\')".to_string()
    } else {
        params.extend([Value::Text(inner.clone()), Value::Text(inner.clone()), Value::Text(inner), Value::Text(prefix)]);
        " AND (p.name LIKE ? ESCAPE '\\' OR s.title LIKE ? ESCAPE '\\' OR s.git_branch LIKE ? ESCAPE '\\' OR s.id LIKE ? ESCAPE '\\')"
            .to_string()
    }
}

/// Sessions selected by `tail` (a WHERE condition plus ordering), with their models.
pub(crate) fn load_sessions(c: &Connection, ctx: &Ctx, env: &Env, tail: &str, params: Vec<Value>) -> Result<Vec<SessionSummary>> {
    let sql = format!("SELECT {SESSION_COLS} FROM {SESSION_FROM} WHERE {tail}");
    let mut stmt = c.prepare(&sql)?;
    let mut items = stmt
        .query_map(rusqlite::params_from_iter(params), |r| {
            let usage = Usage {
                requests: r.get(6)?,
                tokens: Tokens {
                    input: r.get(7)?,
                    output: r.get(8)?,
                    cache_read: r.get(9)?,
                    cache_write_5m: r.get(10)?,
                    cache_write_1h: r.get(11)?,
                },
                cost_usd: r.get(12)?,
                unpriced_requests: r.get(13)?,
            };
            let last_at: i64 = r.get(5)?;
            let status_ratio: Option<f64> = r.get(16)?;
            Ok(SessionSummary {
                id: r.get(0)?,
                project_id: r.get(1)?,
                project: String::new(),
                title: env.hide(r.get(2)?),
                git_branch: env.hide(r.get(3)?),
                started_at: r.get(4)?,
                last_at,
                usage,
                main_model: None,
                models: Vec::new(),
                is_active: (ctx.now - last_at).abs() <= ACTIVE_SESSION_MS,
                subagent_cost_usd: r.get(14)?,
                context_pct: r.get(15)?,
                cache_hit_ratio: status_ratio.or_else(|| usage.tokens.cache_hit_ratio()),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if items.is_empty() {
        return Ok(items);
    }
    let ids: Vec<Value> = items.iter().map(|s| Value::Text(s.id.clone())).collect();
    let sql = format!(
        "SELECT session_id, model, COALESCE(SUM(cost_usd), 0), COUNT(*) FROM requests WHERE session_id IN ({}) GROUP BY 1, 2",
        placeholders(ids.len())
    );
    let mut stmt = c.prepare(&sql)?;
    let mut models: HashMap<String, Vec<(String, f64, i64)>> = HashMap::new();
    for row in stmt.query_map(rusqlite::params_from_iter(ids), |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))? {
        let (sid, model, cost, n) = row?;
        models.entry(sid).or_default().push((model, cost, n));
    }
    for s in &mut items {
        s.project = env.project(s.project_id);
        if let Some(mut m) = models.remove(&s.id) {
            m.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
            s.main_model = m.first().map(|x| x.0.clone());
            s.models = m.into_iter().map(|x| x.0).collect();
        }
    }
    Ok(items)
}

/// Everything known about one session; `None` for an unknown id.
pub fn session_detail(store: &Store, ctx: &Ctx, id: &str) -> Result<Option<SessionDetail>> {
    let env = Env::load(store)?;
    store.read(|c| session_detail_in(c, ctx, &env, id))
}

fn session_detail_in(c: &Connection, ctx: &Ctx, env: &Env, id: &str) -> Result<Option<SessionDetail>> {
    let Some(summary) = load_sessions(c, ctx, env, "s.id = ?", vec![Value::Text(id.to_string())])?.pop() else { return Ok(None) };
    let src = Src::new(c)?;
    let all = Range { from: i64::MIN / 2, to: i64::MAX / 2 };
    let f = Filter::default();
    let mut params = vec![Value::Text(id.to_string())];
    let per_model = grouped(c, "r.model", "r.session_id = ?", &mut params)?;
    let per_agent = grouped(c, "COALESCE(r.agent_type, '')", "r.session_id = ?", &mut vec![Value::Text(id.to_string())])?;
    let total: f64 = per_model.iter().map(|(_, u)| u.cost_usd).sum();
    let mut by_model: Vec<BreakdownRow> =
        per_model.into_iter().map(|(k, usage)| BreakdownRow { label: k.clone(), key: k, usage, share: 0.0 }).collect();
    finish(&mut by_model, total);
    let mut by_agent: Vec<BreakdownRow> =
        per_agent.into_iter().map(|(k, usage)| BreakdownRow { key: agent_key(&k), label: agent_key(&k), usage, share: 0.0 }).collect();
    finish(&mut by_agent, total);
    let mut tools: Vec<BreakdownRow> = tool_groups(&src, all, &f, "", Some(id))?
        .into_iter()
        .map(|(kind, name, usage)| BreakdownRow { key: format!("{kind}:{name}"), label: name, usage, share: 0.0 })
        .collect();
    finish(&mut tools, total);
    let top_turns = turns(c, env, "r.session_id = ?", vec![Value::Text(id.to_string())], 10)?;
    Ok(Some(SessionDetail {
        timeline: timeline(c, ctx, &summary)?,
        by_model,
        by_agent,
        tools,
        top_turns,
        status: status(c, id)?,
        cache_misses: cache_misses(c, env, Some(id), 50)?,
        events: events(c, id)?,
        burn: burn(c, ctx, Some(id))?,
        summary,
    }))
}

fn grouped(c: &Connection, key: &str, cond: &str, params: &mut Vec<Value>) -> Result<Vec<(String, Usage)>> {
    let sql = format!("SELECT {key}, {USAGE_REQUEST_COLS} FROM requests r WHERE {cond} GROUP BY 1");
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(std::mem::take(params)), |r| Ok((r.get(0)?, usage_at(r, 1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 5-minute buckets for sessions up to 2 hours, hourly up to 2 days, daily beyond.
fn timeline(c: &Connection, ctx: &Ctx, s: &SessionSummary) -> Result<Vec<UsageBucket>> {
    let span = s.last_at - s.started_at;
    let step = if span <= 2 * HOUR_MS {
        Some(5 * MINUTE_MS)
    } else if span <= 2 * DAY_MS {
        None
    } else {
        Some(DAY_MS)
    };
    let g = if step == Some(DAY_MS) { Granularity::Day } else { Granularity::Hour };
    let start_of = |ts: i64| match step {
        Some(m) if m < HOUR_MS => bucket_start(ctx, Granularity::Hour, ts) + (ts - bucket_start(ctx, Granularity::Hour, ts)) / m * m,
        _ => bucket_start(ctx, g, ts),
    };
    let next = |b: i64| match step {
        Some(m) if m < HOUR_MS => b + m,
        _ => bucket_next(ctx, g, b),
    };
    let mut stmt = c.prepare(
        "SELECT ts, model, 1, input, output, cache_read, cache_write_5m, cache_write_1h, COALESCE(cost_usd, 0), cost_usd IS NULL
         FROM requests r WHERE session_id = ?1 ORDER BY ts",
    )?;
    let rows = stmt
        .query_map([&s.id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, usage_at(r, 2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else { return Ok(Vec::new()) };
    let mut acc: BTreeMap<i64, (Usage, ModelAcc)> = BTreeMap::new();
    let mut b = start_of(first.0);
    while b <= last.0 {
        acc.insert(b, Default::default());
        b = next(b);
    }
    for (ts, model, usage) in rows {
        let e = acc.entry(start_of(ts)).or_default();
        e.0 += usage;
        let m = e.1.entry(model).or_default();
        m.0 += usage.cost_usd;
        m.1 += usage.tokens.total();
    }
    Ok(acc
        .into_iter()
        .map(|(start, (usage, models))| UsageBucket {
            start,
            label: if step.is_some_and(|m| m < HOUR_MS) { minute_label(ctx, start) } else { bucket_label(ctx, g, start) },
            usage,
            by_model: model_slices(models),
        })
        .collect())
}

/// Spend over the last hour up to `ctx.now`, of one session or of everything.
pub(crate) fn burn(c: &Connection, ctx: &Ctx, session: Option<&str>) -> Result<BurnRate> {
    let mut params = vec![Value::Integer(ctx.now - BURN_WINDOW_MIN * MINUTE_MS), Value::Integer(ctx.now)];
    let mut sql = format!("SELECT {USAGE_REQUEST_COLS} FROM requests r WHERE r.ts >= ? AND r.ts < ?");
    if let Some(s) = session {
        sql.push_str(" AND r.session_id = ?");
        params.push(Value::Text(s.to_string()));
    }
    let u = c.query_row(&sql, rusqlite::params_from_iter(params), |r| usage_at(r, 0))?;
    Ok(BurnRate {
        usd_per_hour: u.cost_usd * 60.0 / BURN_WINDOW_MIN as f64,
        tokens_per_min: u.tokens.total() as f64 / BURN_WINDOW_MIN as f64,
        window_minutes: BURN_WINDOW_MIN,
        active: u.requests > 0,
    })
}

fn status(c: &Connection, id: &str) -> Result<Option<SessionStatus>> {
    let row = c
        .query_row(
            "SELECT ts, COALESCE(model_name, model_id), cost_usd, duration_ms, api_duration_ms, lines_added, lines_removed, ctx_used_pct,
                    ctx_size, exceeds_200k, cache_warm, cache_ttl, cache_expires_at, cache_requests, cache_misses, cache_expected_rebuilds,
                    cache_hit_ratio, cache_write_tokens, cache_miss_recache_tokens, cache_last_miss_at, cache_miss_causes, cache_recache_if_cold
             FROM session_status WHERE session_id = ?1",
            [id],
            |r| {
                let causes: Option<String> = r.get(20)?;
                let cache = CacheStatus {
                    warm: r.get(10)?,
                    ttl: r.get(11)?,
                    expires_at: r.get(12)?,
                    requests: r.get(13)?,
                    misses: r.get(14)?,
                    expected_rebuilds: r.get(15)?,
                    hit_ratio: r.get(16)?,
                    write_tokens: r.get(17)?,
                    miss_recache_tokens: r.get(18)?,
                    last_miss_at: r.get(19)?,
                    miss_causes: causes.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default(),
                    recache_tokens_if_cold: r.get(21)?,
                };
                let empty = cache.warm.is_none()
                    && cache.ttl.is_none()
                    && cache.requests.is_none()
                    && cache.hit_ratio.is_none()
                    && cache.misses.is_none()
                    && cache.miss_causes.is_empty();
                Ok(SessionStatus {
                    ts: r.get(0)?,
                    model: r.get(1)?,
                    cost_usd: r.get(2)?,
                    duration_ms: r.get(3)?,
                    api_duration_ms: r.get(4)?,
                    lines_added: r.get(5)?,
                    lines_removed: r.get(6)?,
                    ctx_used_pct: r.get(7)?,
                    ctx_size: r.get(8)?,
                    exceeds_200k: r.get(9)?,
                    cache: (!empty).then_some(cache),
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// Recent cache misses, newest first, with the config changes shortly before each.
pub(crate) fn cache_misses(c: &Connection, env: &Env, session: Option<&str>, limit: i64) -> Result<Vec<CacheMiss>> {
    let mut stmt = c.prepare(
        "SELECT m.session_id, s.project_id, m.ts, m.causes, m.tools_added, m.tools_removed, m.system_char_delta
         FROM cache_misses m LEFT JOIN sessions s ON s.id = m.session_id
         WHERE ?1 IS NULL OR m.session_id = ?1 ORDER BY m.ts DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![session, limit], |r| {
            let causes: String = r.get(3)?;
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, i64>(2)?, causes, r.get(4)?, r.get(5)?, r.get(6)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut changes = c.prepare("SELECT ts, kind, target FROM config_changes WHERE ts >= ?1 AND ts <= ?2 ORDER BY ts")?;
    let mut out = Vec::with_capacity(rows.len());
    for (session_id, project, ts, causes, tools_added, tools_removed, system_char_delta) in rows {
        let correlated_changes = changes
            .query_map([ts - CORRELATION_MS, ts], |r| Ok(ConfigChange { ts: r.get(0)?, kind: r.get(1)?, target: r.get(2)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        out.push(CacheMiss {
            session_id,
            project: project.map(|p| env.project(p)).unwrap_or_default(),
            ts,
            causes: serde_json::from_str(&causes).unwrap_or_default(),
            tools_added,
            tools_removed,
            system_char_delta,
            correlated_changes,
        });
    }
    Ok(out)
}

fn events(c: &Connection, id: &str) -> Result<Vec<SessionEvent>> {
    let mut stmt = c.prepare("SELECT ts, event, source FROM session_events WHERE session_id = ?1 ORDER BY ts, id")?;
    let rows = stmt
        .query_map([id], |r| Ok(SessionEvent { ts: r.get(0)?, event: r.get(1)?, source: r.get(2)? }))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// The most expensive responses in the range (default 30 days), with the project /
/// model / agent / client filters; unpriced responses sort last. `limit` defaults to 20.
pub fn top_turns(store: &Store, ctx: &Ctx, q: &Query) -> Result<Vec<Turn>> {
    let env = Env::load(store)?;
    let r = range(ctx, q, REPORT_DAYS)?;
    let mut params = vec![Value::Integer(r.from), Value::Integer(r.to)];
    let filter = Filter::from_query(q).sql("r", false, &mut params);
    store.read(|c| turns(c, &env, &format!("r.ts >= ? AND r.ts < ?{filter}"), params, limit(q, 20, 1000)))
}

fn turns(c: &Connection, env: &Env, cond: &str, mut params: Vec<Value>, limit: i64) -> Result<Vec<Turn>> {
    params.push(Value::Integer(limit));
    let sql = format!(
        "SELECT r.id, r.message_id, r.session_id, r.project_id, r.ts, r.model, r.input, r.output, r.cache_read, r.cache_write_5m,
                r.cache_write_1h, r.cost_usd, r.agent_type, r.stop_reason
         FROM requests r WHERE {cond}
         ORDER BY r.cost_usd IS NULL, r.cost_usd DESC, (r.input + r.output + r.cache_read + r.cache_write_5m + r.cache_write_1h) DESC, r.ts DESC
         LIMIT ?"
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Turn {
                    message_id: r.get(1)?,
                    session_id: r.get(2)?,
                    project: env.project(r.get(3)?),
                    ts: r.get(4)?,
                    model: r.get(5)?,
                    tokens: Tokens {
                        input: r.get(6)?,
                        output: r.get(7)?,
                        cache_read: r.get(8)?,
                        cache_write_5m: r.get(9)?,
                        cache_write_1h: r.get(10)?,
                    },
                    cost_usd: r.get(11)?,
                    agent_type: r.get(12)?,
                    tools: Vec::new(),
                    stop_reason: r.get(13)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<Value> = rows.iter().map(|(id, _)| Value::Integer(*id)).collect();
    let sql = format!(
        "SELECT DISTINCT request_id, name FROM request_tools WHERE kind = 'tool' AND request_id IN ({}) ORDER BY request_id, name",
        placeholders(ids.len())
    );
    let mut stmt = c.prepare(&sql)?;
    let mut tools: HashMap<i64, Vec<String>> = HashMap::new();
    for row in stmt.query_map(rusqlite::params_from_iter(ids), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, name) = row?;
        tools.entry(id).or_default().push(name);
    }
    Ok(rows
        .into_iter()
        .map(|(id, mut t)| {
            t.tools = tools.remove(&id).unwrap_or_default();
            t
        })
        .collect())
}

/// Sessions, projects and models matching `q` (case-insensitive substring; session ids by
/// prefix). Under path redaction only aliases and ids are matched. Empty `q` yields nothing.
pub fn search(store: &Store, ctx: &Ctx, q: &Query) -> Result<Vec<SearchResult>> {
    let Some(text) = q.q.as_deref().map(str::trim).filter(|t| !t.is_empty()) else { return Ok(Vec::new()) };
    let env = Env::load(store)?;
    store.read(|c| {
        let mut params = Vec::new();
        let cond = format!("1 = 1{}", text_match(&env, text, &mut params));
        params.push(Value::Integer(SEARCH_LIMIT));
        let mut out: Vec<SearchResult> = load_sessions(c, ctx, &env, &format!("{cond} ORDER BY s.last_at DESC LIMIT ?"), params)?
            .into_iter()
            .map(|s| SearchResult {
                kind: "session".into(),
                label: s.title.clone().unwrap_or_else(|| s.id.chars().take(8).collect()),
                id: s.id,
                sublabel: Some(s.project),
            })
            .collect();
        let pattern = format!("%{}%", like_escape(text));
        let col = if env.redact { "alias" } else { "name" };
        let mut stmt =
            c.prepare(&format!("SELECT id FROM projects WHERE {col} LIKE ?1 ESCAPE '\\' ORDER BY last_seen DESC LIMIT {SEARCH_LIMIT}"))?;
        let ids = stmt.query_map([&pattern], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        out.extend(ids.into_iter().map(|id| SearchResult {
            kind: "project".into(),
            id: id.to_string(),
            label: env.project(id),
            sublabel: env.project_path(id),
        }));
        let mut stmt = c.prepare(&format!(
            "SELECT model FROM (SELECT DISTINCT model FROM usage_hourly) WHERE model LIKE ?1 ESCAPE '\\' ORDER BY model LIMIT {SEARCH_LIMIT}"
        ))?;
        let models = stmt.query_map([&pattern], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        out.extend(models.into_iter().map(|m| SearchResult { kind: "model".into(), id: m.clone(), label: m, sublabel: None }));
        Ok(out)
    })
}

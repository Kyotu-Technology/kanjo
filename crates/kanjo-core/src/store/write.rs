//! The only code that mutates `requests`, `sessions` and `usage_hourly`.
//! Keeping the three in step lives here so no other crate has to get it right.

use crate::model::{InsertOutcome, RequestRow, TokenCounts};
use crate::pricing::PriceTable;
use crate::time::hour_floor;
use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};

/// Returns the id of the project with this directory key, creating it on first sight.
pub fn upsert_project(tx: &Connection, key: &str, path: Option<&str>, ts: i64) -> Result<i64> {
    let existing: Option<(i64, Option<String>)> =
        tx.query_row("SELECT id, path FROM projects WHERE key = ?1", [key], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    if let Some((id, old_path)) = existing {
        tx.execute("UPDATE projects SET first_seen = MIN(first_seen, ?2), last_seen = MAX(last_seen, ?2) WHERE id = ?1", params![id, ts])?;
        if let (None, Some(p)) = (old_path, path) {
            // The display name is only derived from the path while it still has its default value.
            tx.execute(
                "UPDATE projects SET path = ?2, name = CASE WHEN name = key THEN ?3 ELSE name END WHERE id = ?1",
                params![id, p, display_name(key, Some(p))],
            )?;
        }
        return Ok(id);
    }
    tx.execute(
        "INSERT INTO projects (key, path, name, alias, first_seen, last_seen) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![key, path, display_name(key, path), project_alias(key), ts],
    )?;
    Ok(tx.last_insert_rowid())
}

fn display_name(key: &str, path: Option<&str>) -> String {
    path.and_then(|p| p.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| key.to_string())
}

/// Stable pseudonym for a project, used wherever path redaction is on.
pub fn project_alias(key: &str) -> String {
    // FNV-1a, fixed here so aliases never change between builds.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in key.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("project-{:06x}", h & 0xff_ffff)
}

fn exec(tx: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<usize> {
    Ok(tx.prepare_cached(sql)?.execute(params)?)
}

/// Sets or clears a session's title. Callers decide whether titles may be stored at all.
pub fn set_session_title(tx: &Connection, session_id: &str, title: Option<&str>) -> Result<()> {
    exec(tx, "UPDATE sessions SET title = ?2 WHERE id = ?1 AND title IS NOT ?2", params![session_id, title])?;
    Ok(())
}

pub fn set_session_account(tx: &Connection, session_id: &str, account: &str) -> Result<()> {
    exec(tx, "UPDATE sessions SET account = ?2 WHERE id = ?1 AND account IS NOT ?2", params![session_id, account])?;
    Ok(())
}

struct Existing {
    id: i64,
    session_id: String,
    project_id: i64,
    ts: i64,
    model: String,
    agent_type: String,
    is_sidechain: bool,
    tokens: TokenCounts,
    cost: Option<f64>,
}

struct Delta {
    requests: i64,
    tokens: TokenCounts,
    cost: f64,
    unpriced: i64,
}

/// Inserts one API response, deduplicated by `message_id`, and keeps the
/// session totals and the hourly aggregate consistent with it. A repeat of a
/// known message adds any tool uses not seen yet (one response is written as
/// several records, one per content block) and changes totals only when it
/// carries more tokens than before (a streaming record that was later completed).
pub fn insert_request(tx: &Connection, r: &RequestRow, prices: &PriceTable) -> Result<InsertOutcome> {
    let cost = prices.cost(&r.model, &r.tokens);
    let existing = tx
        .prepare_cached(
            "SELECT id, session_id, project_id, ts, model, COALESCE(agent_type, ''), is_sidechain,
                    input, output, cache_read, cache_write_5m, cache_write_1h, cost_usd
             FROM requests WHERE message_id = ?1",
        )?
        .query_row([&r.message_id], |row| {
            Ok(Existing {
                id: row.get(0)?,
                session_id: row.get(1)?,
                project_id: row.get(2)?,
                ts: row.get(3)?,
                model: row.get(4)?,
                agent_type: row.get(5)?,
                is_sidechain: row.get(6)?,
                tokens: TokenCounts {
                    input: row.get(7)?,
                    output: row.get(8)?,
                    cache_read: row.get(9)?,
                    cache_write_5m: row.get(10)?,
                    cache_write_1h: row.get(11)?,
                },
                cost: row.get(12)?,
            })
        })
        .optional()?;

    let Some(old) = existing else {
        exec(
            tx,
            "INSERT INTO requests (message_id, request_id, session_id, project_id, ts, model,
                input, output, cache_read, cache_write_5m, cache_write_1h, web_search, web_fetch,
                cost_usd, price_version, speed, service_tier, effort, is_sidechain, agent_id, agent_type,
                query_source, stop_reason, git_branch, cc_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
            params![
                r.message_id,
                r.request_id,
                r.session_id,
                r.project_id,
                r.ts,
                r.model,
                r.tokens.input,
                r.tokens.output,
                r.tokens.cache_read,
                r.tokens.cache_write_5m,
                r.tokens.cache_write_1h,
                r.web_search,
                r.web_fetch,
                cost,
                cost.map(|_| prices.version.as_str()),
                r.speed,
                r.service_tier,
                r.effort,
                r.is_sidechain,
                r.agent_id,
                r.agent_type,
                r.query_source,
                r.stop_reason,
                r.git_branch,
                r.cc_version,
            ],
        )?;
        add_tools(tx, tx.last_insert_rowid(), r)?;
        exec(
            tx,
            "INSERT INTO sessions (id, project_id, git_branch, cc_version, entrypoint, started_at, last_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT (id) DO UPDATE SET
               started_at = MIN(started_at, excluded.started_at),
               last_at = MAX(last_at, excluded.last_at),
               git_branch = COALESCE(excluded.git_branch, git_branch),
               cc_version = COALESCE(excluded.cc_version, cc_version),
               entrypoint = COALESCE(excluded.entrypoint, entrypoint)",
            params![r.session_id, r.project_id, r.git_branch, r.cc_version, r.entrypoint, r.ts],
        )?;
        exec(
            tx,
            "UPDATE projects SET first_seen = MIN(first_seen, ?2), last_seen = MAX(last_seen, ?2) WHERE id = ?1",
            params![r.project_id, r.ts],
        )?;
        let delta = Delta { requests: 1, tokens: r.tokens, cost: cost.unwrap_or(0.0), unpriced: cost.is_none() as i64 };
        apply(tx, &r.session_id, r.project_id, r.ts, &r.model, r.agent_type.as_deref().unwrap_or(""), r.is_sidechain, &delta)?;
        return Ok(InsertOutcome::Inserted);
    };

    add_tools(tx, old.id, r)?;
    if r.tokens.total() <= old.tokens.total() {
        return Ok(InsertOutcome::Duplicate);
    }
    // Re-price with the row's original model so the delta lands in the same aggregate bucket.
    let new_cost = prices.cost(&old.model, &r.tokens);
    exec(
        tx,
        "UPDATE requests SET input = ?2, output = ?3, cache_read = ?4, cache_write_5m = ?5, cache_write_1h = ?6,
                cost_usd = ?7, price_version = ?8, stop_reason = COALESCE(?9, stop_reason)
         WHERE id = ?1",
        params![
            old.id,
            r.tokens.input,
            r.tokens.output,
            r.tokens.cache_read,
            r.tokens.cache_write_5m,
            r.tokens.cache_write_1h,
            new_cost,
            new_cost.map(|_| prices.version.as_str()),
            r.stop_reason,
        ],
    )?;
    let delta = Delta {
        requests: 0,
        tokens: TokenCounts {
            input: r.tokens.input - old.tokens.input,
            output: r.tokens.output - old.tokens.output,
            cache_read: r.tokens.cache_read - old.tokens.cache_read,
            cache_write_5m: r.tokens.cache_write_5m - old.tokens.cache_write_5m,
            cache_write_1h: r.tokens.cache_write_1h - old.tokens.cache_write_1h,
        },
        cost: new_cost.unwrap_or(0.0) - old.cost.unwrap_or(0.0),
        unpriced: new_cost.is_none() as i64 - old.cost.is_none() as i64,
    };
    apply(tx, &old.session_id, old.project_id, old.ts, &old.model, &old.agent_type, old.is_sidechain, &delta)?;
    Ok(InsertOutcome::Updated)
}

fn add_tools(tx: &Connection, request_id: i64, r: &RequestRow) -> Result<()> {
    for tool in &r.tools {
        exec(
            tx,
            "INSERT OR IGNORE INTO request_tools (request_id, use_id, kind, name) VALUES (?1, ?2, ?3, ?4)",
            params![request_id, tool.use_id, tool.kind.as_str(), tool.name],
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply(
    tx: &Connection,
    session_id: &str,
    project_id: i64,
    ts: i64,
    model: &str,
    agent_type: &str,
    sidechain: bool,
    d: &Delta,
) -> Result<()> {
    exec(
        tx,
        "INSERT INTO usage_hourly (hour, project_id, model, agent_type, requests, input, output, cache_read,
                cache_write_5m, cache_write_1h, cost_usd, unpriced_requests)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT (hour, project_id, model, agent_type) DO UPDATE SET
           requests = requests + excluded.requests,
           input = input + excluded.input,
           output = output + excluded.output,
           cache_read = cache_read + excluded.cache_read,
           cache_write_5m = cache_write_5m + excluded.cache_write_5m,
           cache_write_1h = cache_write_1h + excluded.cache_write_1h,
           cost_usd = cost_usd + excluded.cost_usd,
           unpriced_requests = unpriced_requests + excluded.unpriced_requests",
        params![
            hour_floor(ts),
            project_id,
            model,
            agent_type,
            d.requests,
            d.tokens.input,
            d.tokens.output,
            d.tokens.cache_read,
            d.tokens.cache_write_5m,
            d.tokens.cache_write_1h,
            d.cost,
            d.unpriced,
        ],
    )?;
    exec(
        tx,
        "UPDATE sessions SET
           requests = requests + ?2, input = input + ?3, output = output + ?4, cache_read = cache_read + ?5,
           cache_write_5m = cache_write_5m + ?6, cache_write_1h = cache_write_1h + ?7,
           cost_usd = cost_usd + ?8, unpriced_requests = unpriced_requests + ?9,
           sidechain_requests = sidechain_requests + ?10, sidechain_cost_usd = sidechain_cost_usd + ?11
         WHERE id = ?1",
        params![
            session_id,
            d.requests,
            d.tokens.input,
            d.tokens.output,
            d.tokens.cache_read,
            d.tokens.cache_write_5m,
            d.tokens.cache_write_1h,
            d.cost,
            d.unpriced,
            if sidechain { d.requests } else { 0 },
            if sidechain { d.cost } else { 0.0 },
        ],
    )?;
    Ok(())
}

/// Counts a record the parser could not use. Stores its shape, never its content.
pub fn record_quarantine(
    tx: &Connection,
    record_type: &str,
    signature: &str,
    reason: &str,
    cc_version: Option<&str>,
    ts: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO quarantine (record_type, signature, reason, cc_version, count, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)
         ON CONFLICT (record_type, signature, reason, cc_version) DO UPDATE SET count = count + 1, last_seen = MAX(last_seen, excluded.last_seen)",
        params![record_type, signature, reason, cc_version.unwrap_or(""), ts],
    )?;
    Ok(())
}

const SNAPSHOT_SQL: &str = "
    CREATE TEMP TABLE IF NOT EXISTS reprice_hourly (hour INTEGER, project_id INTEGER, model TEXT, agent_type TEXT, cost REAL, unpriced INTEGER,
        PRIMARY KEY (hour, project_id, model, agent_type)) WITHOUT ROWID;
    CREATE TEMP TABLE IF NOT EXISTS reprice_sessions (id TEXT PRIMARY KEY, cost REAL, unpriced INTEGER, side REAL) WITHOUT ROWID;
    DELETE FROM reprice_hourly;
    DELETE FROM reprice_sessions;
    INSERT INTO reprice_hourly
      SELECT (ts / 3600000) * 3600000, project_id, model, COALESCE(agent_type, ''), -COALESCE(SUM(cost_usd), 0), -SUM(cost_usd IS NULL)
      FROM requests GROUP BY 1, 2, 3, 4;
    INSERT INTO reprice_sessions
      SELECT session_id, -COALESCE(SUM(cost_usd), 0), -SUM(cost_usd IS NULL), -COALESCE(SUM(CASE WHEN is_sidechain THEN cost_usd END), 0)
      FROM requests GROUP BY 1;";

const APPLY_SQL: &str = "
    INSERT INTO reprice_hourly
      SELECT (ts / 3600000) * 3600000, project_id, model, COALESCE(agent_type, ''), COALESCE(SUM(cost_usd), 0), SUM(cost_usd IS NULL)
      FROM requests WHERE true GROUP BY 1, 2, 3, 4
      ON CONFLICT DO UPDATE SET cost = cost + excluded.cost, unpriced = unpriced + excluded.unpriced;
    INSERT INTO reprice_sessions
      SELECT session_id, COALESCE(SUM(cost_usd), 0), SUM(cost_usd IS NULL), COALESCE(SUM(CASE WHEN is_sidechain THEN cost_usd END), 0)
      FROM requests WHERE true GROUP BY 1
      ON CONFLICT DO UPDATE SET cost = cost + excluded.cost, unpriced = unpriced + excluded.unpriced, side = side + excluded.side;
    UPDATE usage_hourly SET
        cost_usd = cost_usd + d.cost, unpriced_requests = unpriced_requests + d.unpriced
      FROM reprice_hourly d
      WHERE usage_hourly.hour = d.hour AND usage_hourly.project_id = d.project_id AND usage_hourly.model = d.model
        AND usage_hourly.agent_type = d.agent_type;
    UPDATE sessions SET
        cost_usd = cost_usd + d.cost, unpriced_requests = unpriced_requests + d.unpriced, sidechain_cost_usd = sidechain_cost_usd + d.side
      FROM reprice_sessions d WHERE sessions.id = d.id;
    DROP TABLE reprice_hourly;
    DROP TABLE reprice_sessions;";

/// Recomputes `cost_usd` of every stored request with `prices` and moves the
/// session totals and the hourly aggregate by the resulting difference.
/// Working with the difference matters once retention has removed old request
/// rows: their share of the totals is historical and stays as it was priced.
pub fn reprice_all(tx: &Connection, prices: &PriceTable) -> Result<i64> {
    tx.execute_batch(SNAPSHOT_SQL)?;
    let models: Vec<String> = {
        let mut stmt = tx.prepare("SELECT DISTINCT model FROM requests")?;
        let rows = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        rows
    };
    let mut changed = 0;
    for model in models {
        match prices.lookup(&model) {
            Some(p) => {
                let cr = p.cache_read_input_token_cost.unwrap_or(p.input_cost_per_token * 0.1);
                let w5 = p.cache_creation_input_token_cost.unwrap_or(p.input_cost_per_token * 1.25);
                let w1 = p.cache_creation_input_token_cost_above_1hr.unwrap_or(p.input_cost_per_token * 2.0);
                changed += tx.execute(
                    "UPDATE requests SET price_version = ?7,
                       cost_usd = input * ?2 + output * ?3 + cache_read * ?4 + cache_write_5m * ?5 + cache_write_1h * ?6
                     WHERE model = ?1",
                    params![model, p.input_cost_per_token, p.output_cost_per_token, cr, w5, w1, prices.version],
                )? as i64;
            }
            None => {
                changed += tx.execute("UPDATE requests SET cost_usd = NULL, price_version = NULL WHERE model = ?1", [&model])? as i64;
            }
        }
    }
    tx.execute_batch(APPLY_SQL)?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;

    fn totals(store: &crate::Store) -> (i64, i64, f64, i64, f64) {
        store
            .read(|c| {
                let (hr, ho, hc): (i64, i64, f64) =
                    c.query_row("SELECT SUM(requests), SUM(output), SUM(cost_usd) FROM usage_hourly", [], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })?;
                let (so, sc): (i64, f64) =
                    c.query_row("SELECT SUM(output), SUM(cost_usd) FROM sessions", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
                Ok((hr, ho, hc, so, sc))
            })
            .unwrap()
    }

    #[test]
    fn dedup_and_streaming_update_keep_aggregates_exact() {
        let t = testkit::TestStore::new();
        let prices = PriceTable::embedded();
        let mut r = testkit::request("msg_1", "s1", 1_795_000_000_000, "claude-opus-5-5", 100, 50);
        r.project_id = t.project("p1");
        assert_eq!(t.store.write(|tx| insert_request(tx, &r, prices)).unwrap(), InsertOutcome::Inserted);
        assert_eq!(t.store.write(|tx| insert_request(tx, &r, prices)).unwrap(), InsertOutcome::Duplicate);
        r.tokens.output = 400;
        assert_eq!(t.store.write(|tx| insert_request(tx, &r, prices)).unwrap(), InsertOutcome::Updated);
        let expected = prices.cost("claude-opus-5-5", &r.tokens).unwrap();
        let (hr, ho, hc, so, sc) = totals(&t.store);
        assert_eq!((hr, ho, so), (1, 400, 400));
        assert!((hc - expected).abs() < 1e-12 && (sc - expected).abs() < 1e-12);
    }

    #[test]
    fn tools_from_later_records_of_the_same_message_are_merged_once() {
        use crate::model::{ToolKind, ToolUse};
        let t = testkit::TestStore::new();
        let mut r = testkit::request("msg_1", "s1", 1_795_000_000_000, "claude-opus-5-5", 100, 50);
        r.project_id = t.project("p1");
        t.insert(&r);
        r.tools = vec![
            ToolUse { use_id: "toolu_1".into(), kind: ToolKind::Tool, name: "Skill".into() },
            ToolUse { use_id: "toolu_1".into(), kind: ToolKind::Skill, name: "commit".into() },
        ];
        t.insert(&r);
        t.insert(&r);
        r.tools = vec![ToolUse { use_id: "toolu_2".into(), kind: ToolKind::Mcp, name: "tracker".into() }];
        t.insert(&r);
        let (rows, requests): (i64, i64) = t
            .store
            .read(|c| {
                Ok(c.query_row("SELECT (SELECT count(*) FROM request_tools), (SELECT count(*) FROM requests)", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?)
            })
            .unwrap();
        assert_eq!((rows, requests), (3, 1));
    }

    #[test]
    fn unpriced_models_are_counted_not_costed() {
        let t = testkit::TestStore::new();
        let mut r = testkit::request("msg_1", "s1", 1_795_000_000_000, "mystery-model", 100, 50);
        r.project_id = t.project("p1");
        t.store.write(|tx| insert_request(tx, &r, PriceTable::embedded())).unwrap();
        let (cost, unpriced): (Option<f64>, i64) = t
            .store
            .read(|c| {
                Ok(c.query_row("SELECT r.cost_usd, s.unpriced_requests FROM requests r JOIN sessions s ON s.id = r.session_id", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?)
            })
            .unwrap();
        assert_eq!((cost, unpriced), (None, 1));
    }

    #[test]
    fn reprice_rebuilds_aggregates() {
        let t = testkit::TestStore::new();
        let p = t.project("p1");
        for i in 0..5 {
            let mut r = testkit::request(&format!("msg_{i}"), "s1", 1_795_000_000_000 + i * 1_000_000, "claude-opus-5-5", 1000, 500);
            r.project_id = p;
            r.is_sidechain = i == 0;
            t.store.write(|tx| insert_request(tx, &r, PriceTable::embedded())).unwrap();
        }
        let before = totals(&t.store);
        let mut doubled = PriceTable::embedded().clone();
        doubled.version = "test-x2".into();
        for price in doubled.models.values_mut() {
            price.input_cost_per_token *= 2.0;
            price.output_cost_per_token *= 2.0;
        }
        assert_eq!(t.store.write(|tx| reprice_all(tx, &doubled)).unwrap(), 5);
        let after = totals(&t.store);
        assert_eq!((before.0, before.1), (after.0, after.1));
        assert!((after.2 - before.2 * 2.0).abs() < 1e-9 && (after.4 - before.4 * 2.0).abs() < 1e-9);
    }

    #[test]
    fn reprice_keeps_the_share_of_requests_that_were_purged() {
        let t = testkit::TestStore::new();
        let p = t.project("p1");
        for i in 0..4 {
            let mut r = testkit::request(&format!("msg_{i}"), "s1", 1_795_000_000_000 + i * 60_000, "claude-opus-5-5", 1000, 500);
            r.project_id = p;
            t.insert(&r);
        }
        let one = PriceTable::embedded().cost("claude-opus-5-5", &testkit::request("x", "s", 0, "m", 1000, 500).tokens).unwrap();
        t.store.write(|tx| Ok(tx.execute("DELETE FROM requests WHERE message_id IN ('msg_0', 'msg_1')", [])?)).unwrap();
        let mut doubled = PriceTable::embedded().clone();
        doubled.version = "test-x2".into();
        for price in doubled.models.values_mut() {
            price.input_cost_per_token *= 2.0;
            price.output_cost_per_token *= 2.0;
        }
        t.store.write(|tx| reprice_all(tx, &doubled)).unwrap();
        let after = totals(&t.store);
        assert_eq!(after.0, 4);
        assert!((after.2 - one * 6.0).abs() < 1e-9, "hourly {}", after.2);
        assert!((after.4 - one * 6.0).abs() < 1e-9, "session {}", after.4);
    }

    #[test]
    fn project_alias_is_stable() {
        assert_eq!(project_alias("-home-dev-acme"), project_alias("-home-dev-acme"));
        assert_ne!(project_alias("-home-dev-acme"), project_alias("-home-dev-other"));
        assert!(project_alias("x").starts_with("project-"));
    }
}

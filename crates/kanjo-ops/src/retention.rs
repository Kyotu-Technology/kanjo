//! Retention by age and deletion on request.
//!
//! Both functions return what they removed; the caller writes the audit entry.
//! Neither touches `settings`, `projects`, `budgets`, `price_tables` or
//! `ingest_files`: the ingest cursors stay where they are, so purged history
//! is not imported again.

use kanjo_core::api::{PurgeReport, PurgeRequest};
use kanjo_core::settings::RetentionSettings;
use kanjo_core::time::{hour_floor, DAY_MS};
use kanjo_core::{Error, Result, Store};
use rusqlite::{params, params_from_iter, Connection, ToSql};

/// Deletes every data class older than its configured age (0 keeps it).
/// Removing request rows (their tool rows cascade) leaves session totals and
/// the hourly aggregate as they are; the request cutoff is floored to the hour.
pub fn apply(store: &Store, r: &RetentionSettings, now: i64) -> Result<PurgeReport> {
    store.write(|tx| apply_tx(tx, r, now))
}

fn cutoff(now: i64, days: u32) -> Option<i64> {
    (days > 0).then(|| now - days as i64 * DAY_MS)
}

fn apply_tx(tx: &Connection, r: &RetentionSettings, now: i64) -> Result<PurgeReport> {
    let mut rep = PurgeReport::default();
    if let Some(c) = cutoff(now, r.requests_days).map(hour_floor) {
        rep.requests = tx.execute("DELETE FROM requests WHERE ts < ?1", [c])? as i64;
        for table in ["session_events", "cache_misses", "config_changes", "session_status"] {
            tx.execute(&format!("DELETE FROM {table} WHERE ts < ?1"), [c])?;
        }
    }
    if let Some(c) = cutoff(now, r.aggregates_days).map(hour_floor) {
        rep.aggregates = tx.execute("DELETE FROM usage_hourly WHERE hour < ?1", [c])? as i64;
        let old = "SELECT id FROM sessions s WHERE s.last_at < ?1 AND NOT EXISTS (SELECT 1 FROM requests r WHERE r.session_id = s.id)";
        for table in ["session_events", "cache_misses", "session_status"] {
            tx.execute(&format!("DELETE FROM {table} WHERE session_id IN ({old})"), [c])?;
        }
        rep.sessions = tx.execute(&format!("DELETE FROM sessions WHERE id IN ({old})"), [c])? as i64;
    }
    if let Some(c) = cutoff(now, r.limit_samples_days) {
        rep.limit_samples = tx.execute("DELETE FROM limit_samples WHERE ts < ?1", [c])? as i64;
    }
    if let Some(c) = cutoff(now, r.alerts_days) {
        rep.alerts = tx.execute("DELETE FROM alerts WHERE ts < ?1", [c])? as i64;
    }
    if let Some(c) = cutoff(now, r.audit_days) {
        rep.audit_entries = crate::audit::prune_before(tx, c)?;
    }
    if let Some(c) = cutoff(now, r.quarantine_days) {
        rep.quarantine = tx.execute("DELETE FROM quarantine WHERE last_seen < ?1", [c])? as i64;
    }
    Ok(rep)
}

/// Deletes on request: one project, one session, everything before a
/// timestamp, or everything. Removes request rows, session rows, the hourly
/// aggregates of that scope, hook events, statusline snapshots, cache misses
/// and, where they can be attributed, alerts. Settings, projects, budgets and
/// the audit log survive.
pub fn purge(store: &Store, req: &PurgeRequest) -> Result<PurgeReport> {
    store.write(|tx| purge_tx(tx, req))
}

pub(crate) fn purge_tx(tx: &Connection, req: &PurgeRequest) -> Result<PurgeReport> {
    let selectors = req.project.is_some() as u8 + req.session.is_some() as u8 + req.before.is_some() as u8 + req.everything as u8;
    if selectors != 1 {
        return Err(Error::Invalid("a purge needs exactly one of project, session, before or everything".into()));
    }
    if req.everything {
        return purge_everything(tx);
    }
    if let Some(id) = &req.session {
        return purge_session(tx, id);
    }
    if let Some(p) = req.project {
        return purge_project(tx, p);
    }
    match req.before {
        Some(before) => purge_before(tx, before),
        None => Ok(PurgeReport::default()),
    }
}

fn purge_everything(tx: &Connection) -> Result<PurgeReport> {
    let mut rep = PurgeReport::default();
    let n = |table: &str| -> Result<i64> { Ok(tx.execute(&format!("DELETE FROM {table}"), [])? as i64) };
    rep.requests = n("requests")?;
    rep.sessions = n("sessions")?;
    rep.aggregates = n("usage_hourly")?;
    rep.limit_samples = n("limit_samples")?;
    rep.alerts = n("alerts")?;
    rep.quarantine = n("quarantine")?;
    for table in ["session_events", "session_status", "cache_misses", "config_changes"] {
        n(table)?;
    }
    Ok(rep)
}

const ALERT_SESSION: &str = "CASE WHEN json_valid(params) THEN json_extract(params, '$.session_id') END";
const ALERT_PROJECT: &str = "CASE WHEN json_valid(params) THEN CAST(json_extract(params, '$.project_id') AS INTEGER) END";

fn purge_session(tx: &Connection, id: &str) -> Result<PurgeReport> {
    let aggregates = subtract_hourly(tx, "session_id = ?1", &[&id])?;
    let requests = tx.execute("DELETE FROM requests WHERE session_id = ?1", [id])? as i64;
    let sessions = tx.execute("DELETE FROM sessions WHERE id = ?1", [id])? as i64;
    for table in ["session_events", "session_status", "cache_misses"] {
        tx.execute(&format!("DELETE FROM {table} WHERE session_id = ?1"), [id])?;
    }
    let alerts = tx.execute(&format!("DELETE FROM alerts WHERE {ALERT_SESSION} = ?1"), [id])? as i64;
    Ok(PurgeReport { requests, sessions, aggregates, alerts, ..Default::default() })
}

fn purge_project(tx: &Connection, project: i64) -> Result<PurgeReport> {
    let mut rep = PurgeReport::default();
    let sessions = "SELECT id FROM sessions WHERE project_id = ?1 UNION SELECT session_id FROM requests WHERE project_id = ?1";
    for table in ["session_events", "session_status", "cache_misses"] {
        tx.execute(&format!("DELETE FROM {table} WHERE session_id IN ({sessions})"), [project])?;
    }
    rep.alerts =
        tx.execute(&format!("DELETE FROM alerts WHERE {ALERT_PROJECT} = ?1 OR {ALERT_SESSION} IN ({sessions})"), [project])? as i64;
    rep.aggregates = tx.execute("DELETE FROM usage_hourly WHERE project_id = ?1", [project])? as i64;
    rep.requests = tx.execute("DELETE FROM requests WHERE project_id = ?1", [project])? as i64;
    rep.sessions = tx.execute("DELETE FROM sessions WHERE project_id = ?1", [project])? as i64;
    tx.execute("DELETE FROM config_changes WHERE project_id = ?1", [project])?;
    Ok(rep)
}

fn purge_before(tx: &Connection, before: i64) -> Result<PurgeReport> {
    let mut rep = PurgeReport::default();
    let boundary = hour_floor(before);
    rep.aggregates = subtract_hourly(tx, "ts >= ?1 AND ts < ?2", &[&boundary, &before])?;
    rep.aggregates += tx.execute("DELETE FROM usage_hourly WHERE hour < ?1", [boundary])? as i64;
    subtract_sessions(tx, before)?;
    rep.requests = tx.execute("DELETE FROM requests WHERE ts < ?1", [before])? as i64;
    let gone = "SELECT id FROM sessions WHERE last_at < ?1";
    for table in ["session_status", "cache_misses", "session_events"] {
        tx.execute(&format!("DELETE FROM {table} WHERE ts < ?1 OR session_id IN ({gone})"), [before])?;
    }
    rep.alerts = tx.execute(&format!("DELETE FROM alerts WHERE ts < ?1 OR {ALERT_SESSION} IN ({gone})"), [before])? as i64;
    rep.sessions = tx.execute("DELETE FROM sessions WHERE last_at < ?1", [before])? as i64;
    tx.execute("DELETE FROM config_changes WHERE ts < ?1", [before])?;
    rep.limit_samples = tx.execute("DELETE FROM limit_samples WHERE ts < ?1", [before])? as i64;
    rep.quarantine = tx.execute("DELETE FROM quarantine WHERE last_seen < ?1", [before])? as i64;
    Ok(rep)
}

/// Takes the contribution of the selected request rows out of `usage_hourly`
/// and drops the rows that become empty. Returns the number of rows dropped.
fn subtract_hourly(tx: &Connection, filter: &str, p: &[&dyn ToSql]) -> Result<i64> {
    let mut stmt = tx.prepare(&format!(
        "SELECT (ts / 3600000) * 3600000, project_id, model, COALESCE(agent_type, ''), COUNT(*), SUM(input), SUM(output),
                SUM(cache_read), SUM(cache_write_5m), SUM(cache_write_1h), COALESCE(SUM(cost_usd), 0), SUM(cost_usd IS NULL)
         FROM requests WHERE {filter} GROUP BY 1, 2, 3, 4"
    ))?;
    let groups = stmt
        .query_map(params_from_iter(p.iter()), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                [r.get::<_, i64>(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(11)?],
                r.get::<_, f64>(10)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut upd = tx.prepare(
        "UPDATE usage_hourly SET requests = requests - ?5, input = input - ?6, output = output - ?7, cache_read = cache_read - ?8,
                cache_write_5m = cache_write_5m - ?9, cache_write_1h = cache_write_1h - ?10, unpriced_requests = unpriced_requests - ?11,
                cost_usd = cost_usd - ?12
         WHERE hour = ?1 AND project_id = ?2 AND model = ?3 AND agent_type = ?4",
    )?;
    let mut hours = Vec::with_capacity(groups.len());
    for (hour, project, model, agent, n, cost) in &groups {
        upd.execute(params![hour, project, model, agent, n[0], n[1], n[2], n[3], n[4], n[5], n[6], cost])?;
        hours.push(*hour);
    }
    hours.sort_unstable();
    hours.dedup();
    let mut removed = 0;
    for hour in hours {
        removed += tx.execute("DELETE FROM usage_hourly WHERE hour = ?1 AND requests <= 0", [hour])? as i64;
    }
    Ok(removed)
}

/// For sessions that continue past `before`, takes the requests before it out
/// of the session totals and moves the start to the first remaining request.
fn subtract_sessions(tx: &Connection, before: i64) -> Result<()> {
    tx.execute(
        "UPDATE sessions SET (requests, input, output, cache_read, cache_write_5m, cache_write_1h, cost_usd, unpriced_requests,
                              sidechain_requests, sidechain_cost_usd) = (
            SELECT sessions.requests - COUNT(*), sessions.input - SUM(input), sessions.output - SUM(output),
                   sessions.cache_read - SUM(cache_read), sessions.cache_write_5m - SUM(cache_write_5m),
                   sessions.cache_write_1h - SUM(cache_write_1h), sessions.cost_usd - COALESCE(SUM(cost_usd), 0),
                   sessions.unpriced_requests - SUM(cost_usd IS NULL), sessions.sidechain_requests - SUM(is_sidechain != 0),
                   sessions.sidechain_cost_usd - COALESCE(SUM(CASE WHEN is_sidechain THEN cost_usd END), 0)
            FROM requests WHERE requests.session_id = sessions.id AND requests.ts < ?1)
         WHERE last_at >= ?1 AND id IN (SELECT session_id FROM requests WHERE ts < ?1)",
        [before],
    )?;
    tx.execute(
        "UPDATE sessions SET started_at = COALESCE((SELECT MIN(ts) FROM requests WHERE session_id = sessions.id AND ts >= ?1), ?1)
         WHERE last_at >= ?1 AND started_at < ?1",
        [before],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanjo_core::model::{ToolKind, ToolUse};
    use kanjo_core::pricing::PriceTable;
    use kanjo_core::store::write;
    use kanjo_core::testkit::{self, TestStore};
    use kanjo_core::time::HOUR_MS;

    const NOW: i64 = 1_795_648_000_000;

    fn count(t: &TestStore, sql: &str) -> i64 {
        t.store.read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap()
    }

    fn totals(t: &TestStore) -> (i64, i64, i64, i64, i64) {
        t.store
            .read(|c| {
                Ok(c.query_row(
                    "SELECT (SELECT COALESCE(SUM(requests), 0) FROM usage_hourly), (SELECT COALESCE(SUM(output), 0) FROM usage_hourly),
                            (SELECT COALESCE(SUM(requests), 0) FROM sessions), (SELECT COALESCE(SUM(output), 0) FROM sessions),
                            (SELECT COALESCE(SUM(CAST(ROUND(cost_usd * 1e9) AS INTEGER)), 0) FROM sessions)",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )?)
            })
            .unwrap()
    }

    /// Two projects, three sessions; session s2 straddles NOW - 10 days.
    fn fixture() -> (TestStore, i64, i64) {
        let t = TestStore::new();
        let (pa, pb) = (t.project("alpha"), t.project("beta"));
        let mut n = 0;
        for (session, project, start, count) in
            [("s1", pa, NOW - 30 * DAY_MS, 4), ("s2", pa, NOW - 10 * DAY_MS - 2 * HOUR_MS, 6), ("s3", pb, NOW - HOUR_MS * 5, 3)]
        {
            for i in 0..count {
                n += 1;
                let mut r = testkit::request(&format!("m{n}"), session, start + i * 30 * 60_000, "claude-opus-5-5", 100, 50 + i);
                r.project_id = project;
                r.is_sidechain = i % 3 == 2;
                r.agent_type = r.is_sidechain.then(|| "Explore".to_string());
                r.tools = vec![ToolUse { use_id: format!("t{n}"), kind: ToolKind::Tool, name: "Read".into() }];
                t.insert(&r);
            }
        }
        t.store
            .write(|tx| {
                for (i, s) in ["s1", "s2", "s3"].iter().enumerate() {
                    let ts = NOW - (30 - i as i64 * 12) * DAY_MS;
                    tx.execute("INSERT INTO session_events (ts, session_id, event) VALUES (?1, ?2, 'SessionStart')", params![ts, s])?;
                    tx.execute("INSERT INTO session_status (session_id, ts) VALUES (?1, ?2)", params![s, ts])?;
                    tx.execute("INSERT INTO cache_misses (session_id, ts, causes) VALUES (?1, ?2, '[\"ttl_expired_5m\"]')", params![s, ts])?;
                    tx.execute(
                        "INSERT INTO alerts (ts, kind, severity, dedup_key, params) VALUES (?1, 'runaway_session', 'warning', ?2, ?3)",
                        params![ts, format!("a{i}"), serde_json::json!({"session_id": s, "project_id": if i < 2 { pa } else { pb }}).to_string()],
                    )?;
                    kanjo_core::limits::insert_sample(
                        tx,
                        kanjo_core::limits::LimitKind::FiveHour,
                        &kanjo_core::limits::LimitSample {
                            ts,
                            used_pct: 10.0 * i as f64,
                            resets_at: ts + HOUR_MS,
                            used_usd: None,
                            limit_usd: None,
                            period: None,
                        },
                    )?;
                    write::record_quarantine(tx, "assistant", &format!("sig{i}"), "unknown_shape", None, ts)?;
                }
                tx.execute("INSERT INTO config_changes (ts, kind, target, project_id, fingerprint) VALUES (?1, 'claude_md', 'x', ?2, 'f')", params![NOW - 20 * DAY_MS, pa])?;
                tx.execute("INSERT INTO ingest_files (path, kind, size, offset, mtime, updated_at) VALUES ('/x/a.jsonl', 'transcript', 10, 10, 1, 1)", [])?;
                kanjo_core::settings::set(tx, "app", &serde_json::json!({"onboarding_done": true}))?;
                crate::audit::record_tx(tx, NOW - 40 * DAY_MS, "tester", "export", "usage", &serde_json::json!({}))?;
                crate::audit::record_tx(tx, NOW - DAY_MS, "tester", "export", "usage", &serde_json::json!({}))?;
                Ok(())
            })
            .unwrap();
        (t, pa, pb)
    }

    /// A reprice with the same table rebuilds totals from the remaining rows; they must not move.
    fn assert_consistent(t: &TestStore) {
        let before = totals(t);
        t.store.write(|tx| write::reprice_all(tx, PriceTable::embedded())).unwrap();
        assert_eq!(totals(t), before);
    }

    fn untouched(t: &TestStore) {
        assert_eq!(count(t, "SELECT count(*) FROM ingest_files WHERE offset = 10"), 1, "ingest cursors must stay");
        assert_eq!(count(t, "SELECT count(*) FROM settings WHERE key = 'app'"), 1);
        assert_eq!(count(t, "SELECT count(*) FROM projects"), 2);
        assert!(crate::audit::verify(&t.store).unwrap());
    }

    #[test]
    fn retention_per_class() {
        let (t, _, _) = fixture();
        let before = totals(&t);
        let keep = RetentionSettings {
            requests_days: 0,
            aggregates_days: 0,
            limit_samples_days: 0,
            alerts_days: 0,
            audit_days: 0,
            quarantine_days: 0,
        };
        assert_eq!(apply(&t.store, &keep, NOW).unwrap().requests, 0);

        let rep = apply(&t.store, &RetentionSettings { requests_days: 15, ..keep.clone() }, NOW).unwrap();
        assert_eq!((rep.requests, rep.sessions, rep.aggregates), (4, 0, 0));
        assert_eq!(count(&t, "SELECT count(*) FROM request_tools"), 9, "tool rows cascade");
        assert_eq!(totals(&t), before, "session totals and hourly aggregates stay");
        assert_eq!(count(&t, "SELECT count(*) FROM session_events"), 1);

        let rep = apply(
            &t.store,
            &RetentionSettings { limit_samples_days: 15, alerts_days: 15, quarantine_days: 15, audit_days: 15, ..keep.clone() },
            NOW,
        )
        .unwrap();
        assert_eq!((rep.limit_samples, rep.alerts, rep.quarantine, rep.audit_entries), (2, 2, 2, 1));
        assert!(crate::audit::verify(&t.store).unwrap());

        let rep = apply(&t.store, &RetentionSettings { aggregates_days: 15, ..keep }, NOW).unwrap();
        assert_eq!((rep.aggregates, rep.sessions), (3, 1));
        assert_eq!(count(&t, "SELECT count(*) FROM sessions WHERE id = 's1'"), 0);
        untouched(&t);
    }

    #[test]
    fn purge_session() {
        let (t, _, _) = fixture();
        let rep = purge(&t.store, &PurgeRequest { session: Some("s2".into()), ..Default::default() }).unwrap();
        assert_eq!((rep.requests, rep.sessions, rep.alerts), (6, 1, 1));
        assert_eq!(rep.aggregates, 5);
        for table in ["requests", "session_events", "session_status", "cache_misses"] {
            assert_eq!(count(&t, &format!("SELECT count(*) FROM {table} WHERE session_id = 's2'")), 0, "{table}");
        }
        assert_eq!(totals(&t).0, 7);
        assert_consistent(&t);
        untouched(&t);
    }

    #[test]
    fn purge_project() {
        let (t, pa, _) = fixture();
        let rep = purge(&t.store, &PurgeRequest { project: Some(pa), ..Default::default() }).unwrap();
        assert_eq!((rep.requests, rep.sessions, rep.alerts), (10, 2, 2));
        assert_eq!(count(&t, "SELECT count(*) FROM usage_hourly"), 2);
        assert_eq!(count(&t, "SELECT count(*) FROM config_changes"), 0);
        assert_eq!(count(&t, "SELECT count(*) FROM session_events"), 1);
        assert_consistent(&t);
        untouched(&t);
    }

    #[test]
    fn purge_before_splits_straddling_sessions() {
        let (t, _, _) = fixture();
        let cut = NOW - 10 * DAY_MS - 30 * 60_000;
        let rep = purge(&t.store, &PurgeRequest { before: Some(cut), ..Default::default() }).unwrap();
        assert_eq!((rep.requests, rep.sessions), (4 + 3, 1));
        let (requests, started): (i64, i64) = t
            .store
            .read(|c| Ok(c.query_row("SELECT requests, started_at FROM sessions WHERE id = 's2'", [], |r| Ok((r.get(0)?, r.get(1)?)))?))
            .unwrap();
        assert_eq!(requests, 3);
        assert!(started >= cut);
        assert_eq!(totals(&t).0, 6);
        assert_eq!(count(&t, "SELECT count(*) FROM limit_samples"), 1);
        assert_consistent(&t);
        untouched(&t);
    }

    #[test]
    fn purge_everything_keeps_settings_audit_and_cursors() {
        let (t, _, _) = fixture();
        let rep = purge(&t.store, &PurgeRequest { everything: true, ..Default::default() }).unwrap();
        assert_eq!((rep.requests, rep.sessions, rep.limit_samples, rep.alerts, rep.quarantine), (13, 3, 3, 3, 3));
        for table in
            ["requests", "sessions", "usage_hourly", "session_events", "session_status", "cache_misses", "config_changes", "request_tools"]
        {
            assert_eq!(count(&t, &format!("SELECT count(*) FROM {table}")), 0, "{table}");
        }
        assert_eq!(count(&t, "SELECT count(*) FROM audit_log"), 2);
        untouched(&t);
    }

    #[test]
    fn purge_needs_exactly_one_selector() {
        let (t, pa, _) = fixture();
        assert!(purge(&t.store, &PurgeRequest::default()).is_err());
        assert!(purge(&t.store, &PurgeRequest { project: Some(pa), everything: true, ..Default::default() }).is_err());
        assert_eq!(count(&t, "SELECT count(*) FROM requests"), 13);
    }
}

//! Detectors shared by the alert rules and the insights, plus the small
//! queries both need.

use crate::thresholds::*;
use kanjo_core::time::DAY_MS;
use kanjo_core::{Ctx, Result};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let mid = v.len() / 2;
    Some(if v.len().is_multiple_of(2) { (v[mid - 1] + v[mid]) / 2.0 } else { v[mid] })
}

/// Priced cost per local day in `[from, to)`, as (day start, cost), oldest first. Days without cost are absent.
pub(crate) fn daily_costs(conn: &Connection, ctx: &Ctx, from: i64, to: i64) -> Result<Vec<(i64, f64)>> {
    // Every UTC offset in use is a whole number of quarter hours, so quarter-hour sums cut local days exactly.
    let mut stmt = conn.prepare(
        "SELECT ts / 900000 * 900000 AS q, SUM(cost_usd) FROM requests
         WHERE ts >= ?1 AND ts < ?2 AND cost_usd IS NOT NULL GROUP BY q ORDER BY q",
    )?;
    let quarters = stmt.query_map(params![from, to], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?;
    let mut days: Vec<(i64, f64)> = Vec::new();
    for row in quarters {
        let (q, cost) = row?;
        let day = ctx.day_start(q);
        match days.last_mut() {
            Some((d, c)) if *d == day => *c += cost,
            _ => days.push((day, cost)),
        }
    }
    Ok(days.into_iter().filter(|(_, c)| *c > 0.0).collect())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Spike {
    pub cost: f64,
    pub median: f64,
    pub baseline_days: usize,
}

/// The day starting at `day` against the median of the active days before it.
pub(crate) fn cost_spike(daily: &[(i64, f64)], day: i64) -> Option<Spike> {
    let cost = daily.iter().find(|(d, _)| *d == day).map(|(_, c)| *c)?;
    // Day starts are local midnights; half a day of slack keeps a clock change in between from moving the edge.
    let earliest = day - SPIKE_LOOKBACK_DAYS * DAY_MS - DAY_MS / 2;
    let before: Vec<f64> = daily.iter().filter(|(d, _)| *d < day && *d >= earliest).map(|(_, c)| *c).collect();
    let baseline = &before[before.len().saturating_sub(SPIKE_BASELINE_DAYS)..];
    if baseline.len() < SPIKE_MIN_BASELINE_DAYS {
        return None;
    }
    let median = median(baseline)?;
    (cost > SPIKE_FACTOR * median && cost >= SPIKE_FLOOR_USD).then_some(Spike { cost, median, baseline_days: baseline.len() })
}

/// Cost of every session started at or after `from`, as (id, cost).
pub(crate) fn session_costs(conn: &Connection, from: i64) -> Result<Vec<(String, f64)>> {
    let mut stmt = conn.prepare("SELECT id, cost_usd FROM sessions WHERE started_at >= ?1 AND cost_usd > 0")?;
    let rows = stmt.query_map([from], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Runaway {
    pub cost: f64,
    pub median: f64,
    pub baseline_sessions: usize,
}

/// A session against the median of the other sessions in `baseline`.
pub(crate) fn runaway(session_id: &str, cost: f64, baseline: &[(String, f64)]) -> Option<Runaway> {
    let others: Vec<f64> = baseline.iter().filter(|(id, _)| id != session_id).map(|(_, c)| *c).collect();
    if others.len() < RUNAWAY_MIN_BASELINE_SESSIONS {
        return None;
    }
    let median = median(&others)?;
    (cost > RUNAWAY_FACTOR * median && cost >= RUNAWAY_FLOOR_USD).then_some(Runaway { cost, median, baseline_sessions: others.len() })
}

/// Many requests while the statusline reports almost no changed code.
pub(crate) fn is_agent_loop(requests: i64, lines_changed: i64) -> bool {
    requests >= LOOP_MIN_REQUESTS && lines_changed <= LOOP_MAX_CHANGED_LINES
}

/// Largest number of timestamps inside any window of `window` ms, and the last timestamp of that window.
/// `ts` must be sorted.
pub(crate) fn densest_window(ts: &[i64], window: i64) -> (i64, i64) {
    let mut best = (0, 0);
    let mut start = 0;
    for (end, t) in ts.iter().enumerate() {
        while *t - ts[start] >= window {
            start += 1;
        }
        let n = (end - start + 1) as i64;
        if n > best.0 {
            best = (n, *t);
        }
    }
    best
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveSession {
    pub id: String,
    pub project_id: i64,
    pub cost_usd: f64,
    /// Time of the session's last request.
    pub last_at: i64,
}

/// Sessions seen at or after `since` (request or statusline) that no `SessionEnd` marker closed afterwards.
pub(crate) fn active_sessions(conn: &Connection, since: i64) -> Result<Vec<ActiveSession>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.project_id, s.cost_usd, s.last_at
         FROM sessions s LEFT JOIN session_status st ON st.session_id = s.id
         WHERE MAX(s.last_at, COALESCE(st.ts, 0)) >= ?1
           AND NOT EXISTS (SELECT 1 FROM session_events e
                           WHERE e.session_id = s.id AND e.event = 'SessionEnd' AND e.ts >= MAX(s.last_at, COALESCE(st.ts, 0)))
         ORDER BY s.id",
    )?;
    let rows = stmt
        .query_map([since], |r| Ok(ActiveSession { id: r.get(0)?, project_id: r.get(1)?, cost_usd: r.get(2)?, last_at: r.get(3)? }))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Status {
    pub ts: i64,
    pub model_id: Option<String>,
    pub lines_added: Option<i64>,
    pub lines_removed: Option<i64>,
    pub ctx_used_pct: Option<f64>,
    pub ctx_size: Option<i64>,
    pub ctx_tokens: Option<i64>,
    pub exceeds_200k: Option<bool>,
    pub cache_warm: Option<bool>,
    pub cache_ttl: Option<String>,
    pub cache_expires_at: Option<i64>,
    pub cache_recache_if_cold: Option<i64>,
}

impl Status {
    /// Added plus removed lines, when the snapshot reports either.
    pub fn lines_changed(&self) -> Option<i64> {
        match (self.lines_added, self.lines_removed) {
            (None, None) => None,
            (a, r) => Some(a.unwrap_or(0) + r.unwrap_or(0)),
        }
    }
}

pub(crate) fn status(conn: &Connection, session_id: &str) -> Result<Option<Status>> {
    Ok(conn
        .query_row(
            "SELECT ts, model_id, lines_added, lines_removed, ctx_used_pct, ctx_size, ctx_tokens, exceeds_200k,
                    cache_warm, cache_ttl, cache_expires_at, cache_recache_if_cold
             FROM session_status WHERE session_id = ?1",
            [session_id],
            |r| {
                Ok(Status {
                    ts: r.get(0)?,
                    model_id: r.get(1)?,
                    lines_added: r.get(2)?,
                    lines_removed: r.get(3)?,
                    ctx_used_pct: r.get(4)?,
                    ctx_size: r.get(5)?,
                    ctx_tokens: r.get(6)?,
                    exceeds_200k: r.get(7)?,
                    cache_warm: r.get(8)?,
                    cache_ttl: r.get(9)?,
                    cache_expires_at: r.get(10)?,
                    cache_recache_if_cold: r.get(11)?,
                })
            },
        )
        .optional()?)
}

/// Display label of a project: its alias when path redaction is on, otherwise its name.
pub(crate) fn client_alias(conn: &Connection, name: &str) -> Result<String> {
    kanjo_core::store::write::alias(conn, "client", name)
}

pub(crate) fn project_label(conn: &Connection, project_id: i64, redact: bool) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT CASE WHEN ?2 THEN alias ELSE name END FROM projects WHERE id = ?1", params![project_id, redact], |r| r.get(0))
        .optional()?)
}

/// Requests of one session with `from < ts <= to`.
pub(crate) fn session_requests_between(conn: &Connection, session_id: &str, from: i64, to: i64) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM requests WHERE session_id = ?1 AND ts > ?2 AND ts <= ?3",
        params![session_id, from, to],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_and_window() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(median(&[]), None);
        assert_eq!(densest_window(&[0, 10, 20, 100, 105, 110, 115], 20), (4, 115));
    }

    #[test]
    fn spike_needs_a_baseline_and_a_floor() {
        let days: Vec<(i64, f64)> = (0..10).map(|i| (i * DAY_MS, 5.0)).chain([(10 * DAY_MS, 40.0)]).collect();
        let s = cost_spike(&days, 10 * DAY_MS).unwrap();
        assert_eq!((s.median, s.baseline_days), (5.0, 10));
        assert!(cost_spike(&days[6..], 10 * DAY_MS).is_none());
        let cheap: Vec<(i64, f64)> = (0..10).map(|i| (i * DAY_MS, 1.0)).chain([(10 * DAY_MS, 15.0)]).collect();
        assert!(cost_spike(&cheap, 10 * DAY_MS).is_none());
    }
}

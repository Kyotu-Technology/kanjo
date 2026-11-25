use crate::blocks;
use crate::common::{bucket_start, Env, Filter, Keys, Range, Src, ACTIVE_SESSION_MS, REPORT_DAYS};
use crate::limits::{limits_in, STALE_MS};
use crate::reports::{buckets, group_rows, period_ranges, period_summary};
use crate::sessions::{burn, load_sessions};
use kanjo_core::api::{Confidence, DataStatus, Dimension, Granularity, Overview};
use kanjo_core::time::{DAY_MS, HOUR_MS};
use kanjo_core::{Ctx, Result, Store};
use rusqlite::types::Value;
use rusqlite::Connection;

const TOP_ROWS: usize = 5;
const MAX_ACTIVE_SESSIONS: i64 = 20;

/// The dashboard landing page in one call.
pub fn overview(store: &Store, ctx: &Ctx) -> Result<Overview> {
    let env = Env::load(store)?;
    store.read(|c| {
        let src = Src::new(c)?;
        let none = Filter::default();
        let [today, week, month] = period_ranges(ctx);
        let daily_range = Range { from: ctx.day_start(ctx.now) - (REPORT_DAYS - 1) * DAY_MS, to: ctx.now };
        let rows = src.fetch(daily_range.from, daily_range.to, &none, Keys { hour: true, project: true, model: true, ..Keys::NONE })?;
        let hourly_range = Range { from: bucket_start(ctx, Granularity::Hour, ctx.now) - 23 * HOUR_MS, to: ctx.now };
        let recent: Vec<_> = rows.iter().filter(|r| r.hour >= hourly_range.from).cloned().collect();
        let mut top_projects = group_rows(&rows, Dimension::Project, &env);
        top_projects.truncate(TOP_ROWS);
        let mut top_models = group_rows(&rows, Dimension::Model, &env);
        top_models.truncate(TOP_ROWS);
        Ok(Overview {
            generated_at: ctx.now,
            today: period_summary(&src, &none, today.0, today.1)?,
            week: period_summary(&src, &none, week.0, week.1)?,
            month: period_summary(&src, &none, month.0, month.1)?,
            burn: burn(c, ctx, None)?,
            limits: limits_in(&src, ctx, &env)?,
            active_block: blocks::active(&src, ctx, &env)?.map(|d| d.block),
            active_sessions: load_sessions(
                c,
                ctx,
                &env,
                "s.last_at >= ? ORDER BY s.last_at DESC LIMIT ?",
                vec![Value::Integer(ctx.now - ACTIVE_SESSION_MS), Value::Integer(MAX_ACTIVE_SESSIONS)],
            )?,
            top_projects,
            top_models,
            hourly: buckets(ctx, Granularity::Hour, hourly_range, &recent)?,
            daily: buckets(ctx, Granularity::Day, daily_range, &rows)?,
            data: data_status_in(c, ctx)?,
        })
    })
}

/// Freshness of the inputs: statusline, transcripts, prices and quarantine.
pub fn data_status(store: &Store, ctx: &Ctx) -> Result<DataStatus> {
    store.read(|c| data_status_in(c, ctx))
}

fn data_status_in(c: &Connection, ctx: &Ctx) -> Result<DataStatus> {
    let last_statusline_at: Option<i64> = c.query_row(
        "SELECT MAX(t) FROM (
           SELECT MAX(ts) AS t FROM session_status
           UNION ALL SELECT MAX(ts) FROM limit_samples WHERE kind = 'five_hour'
           UNION ALL SELECT MAX(ts) FROM limit_samples WHERE kind = 'seven_day'
           UNION ALL SELECT MAX(ts) FROM limit_samples WHERE kind = 'spend')",
        [],
        |r| r.get(0),
    )?;
    let (last_req, first_req, last_hour, first_hour): (Option<i64>, Option<i64>, Option<i64>, Option<i64>) = c.query_row(
        "SELECT (SELECT MAX(ts) FROM requests), (SELECT MIN(ts) FROM requests), (SELECT MAX(hour) FROM usage_hourly), (SELECT MIN(hour) FROM usage_hourly)",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let last_request_at = last_req.or(last_hour);
    let first_request_at = match (first_req, first_hour) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let mut stmt = c.prepare("SELECT DISTINCT model FROM usage_hourly WHERE unpriced_requests > 0 ORDER BY model")?;
    let unpriced_models = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    let quarantined_records: i64 = c.query_row("SELECT COALESCE(SUM(count), 0) FROM quarantine", [], |r| r.get(0))?;
    Ok(DataStatus {
        statusline_connected: last_statusline_at.is_some_and(|t| ctx.now - t <= STALE_MS),
        last_statusline_at,
        last_request_at,
        first_request_at,
        unpriced_models,
        quarantined_records,
        cost_confidence: Confidence::LowerBound,
    })
}

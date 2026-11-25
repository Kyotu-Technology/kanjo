use crate::common::{range, Env, Filter, Keys, Src, REPORT_DAYS};
use crate::reports::group_rows;
use kanjo_core::api::{Block, Dimension, Query};
use kanjo_core::limits::SAME_WINDOW_TOLERANCE_MS;
use kanjo_core::time::{DAY_MS, FIVE_HOURS_MS, HOUR_MS, MINUTE_MS};
use kanjo_core::{Ctx, Result, Store};
use rusqlite::Connection;

/// How far before the requested range the block chain starts, so that block
/// boundaries do not depend on where the range begins (any idle gap of five hours
/// or an anchored window inside this stretch resets the chain).
const CHAIN_LOOKBACK_MS: i64 = 30 * DAY_MS;

/// A 5-hour window before its usage is filled in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Window {
    pub start: i64,
    pub end: i64,
    pub anchored: bool,
    pub peak_pct: Option<f64>,
}

/// A block plus the first and last request inside it.
pub(crate) struct Detailed {
    pub block: Block,
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
}

/// Five-hour windows overlapping `[from, to)`, sorted by start. A window is anchored
/// when `limit_samples` holds a five_hour reading with its `resets_at`
/// (`[resets_at - 5h, resets_at)`); activity outside anchored windows forms inferred
/// blocks that start at the hour floor of the first active hour after the previous
/// block and last five hours, cut short where an anchored window begins.
pub(crate) fn windows(c: &Connection, from: i64, to: i64) -> Result<Vec<Window>> {
    let lookback = from - CHAIN_LOOKBACK_MS;
    let anchored = anchored_windows(c, lookback, to)?;
    let mut out = anchored.clone();
    let mut stmt =
        c.prepare("SELECT hour FROM usage_hourly WHERE hour >= ?1 AND hour < ?2 GROUP BY hour HAVING SUM(requests) > 0 ORDER BY hour")?;
    let hours = stmt.query_map([lookback, to], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut prev_end = i64::MIN;
    for h in hours {
        let mut s = h.max(prev_end);
        while let Some(a) = anchored.iter().find(|a| a.start <= s && s < a.end) {
            s = a.end;
        }
        if s >= h + HOUR_MS {
            continue;
        }
        let next_anchor = anchored.iter().map(|a| a.start).filter(|st| *st > s).min().unwrap_or(i64::MAX);
        let seg_end = (h + HOUR_MS).min(next_anchor);
        if s > h || seg_end < h + HOUR_MS {
            let n: i64 = c.query_row("SELECT COUNT(*) FROM requests WHERE ts >= ?1 AND ts < ?2", [s, seg_end], |r| r.get(0))?;
            if n == 0 {
                continue;
            }
        }
        let end = (s + FIVE_HOURS_MS).min(next_anchor);
        out.push(Window { start: s, end, anchored: false, peak_pct: None });
        prev_end = end;
    }
    out.retain(|w| w.end > from && w.start < to);
    out.sort_by_key(|w| w.start);
    Ok(out)
}

/// Five-hour windows known from readings whose reset falls after `after`, readings
/// within the same-window tolerance merged, overlaps trimmed.
fn anchored_windows(c: &Connection, after: i64, to: i64) -> Result<Vec<Window>> {
    let mut stmt = c.prepare(
        "SELECT resets_at, MAX(used_pct) FROM limit_samples
         WHERE kind = 'five_hour' AND resets_at > ?1 AND resets_at - ?3 < ?2 GROUP BY resets_at ORDER BY resets_at",
    )?;
    let rows = stmt
        .query_map([after, to, FIVE_HOURS_MS], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut clusters: Vec<(i64, i64, f64)> = Vec::new();
    for (resets_at, pct) in rows {
        match clusters.last_mut() {
            Some((first, _, peak)) if resets_at - *first <= SAME_WINDOW_TOLERANCE_MS => *peak = peak.max(pct),
            _ => clusters.push((resets_at, resets_at, pct)),
        }
    }
    let mut out: Vec<Window> = Vec::with_capacity(clusters.len());
    for (_, resets_at, peak) in clusters {
        let start = out.last().map_or(resets_at - FIVE_HOURS_MS, |p| (resets_at - FIVE_HOURS_MS).max(p.end));
        if start < resets_at {
            out.push(Window { start, end: resets_at, anchored: true, peak_pct: Some(peak) });
        }
    }
    Ok(out)
}

/// Blocks overlapping the range (default 30 days), oldest first. Filters are ignored:
/// the windows are account-wide. Usage reads `usage_hourly` with request rows for
/// partial hours, so inferred blocks survive the purge of request rows.
pub fn blocks(store: &Store, ctx: &Ctx, q: &Query) -> Result<Vec<Block>> {
    let r = range(ctx, q, REPORT_DAYS)?;
    let env = Env::load(store)?;
    store.read(|c| Ok(detailed(&Src::new(c)?, ctx, &env, r.from, r.to)?.into_iter().map(|d| d.block).collect()))
}

pub(crate) fn detailed(src: &Src, ctx: &Ctx, env: &Env, from: i64, to: i64) -> Result<Vec<Detailed>> {
    windows(src.c, from, to)?.into_iter().map(|w| fill(src, ctx, env, w)).collect()
}

/// The block containing `ctx.now`, if any.
pub(crate) fn active(src: &Src, ctx: &Ctx, env: &Env) -> Result<Option<Detailed>> {
    let w = windows(src.c, ctx.now - FIVE_HOURS_MS, ctx.now + 1)?.into_iter().find(|w| w.start <= ctx.now && ctx.now < w.end);
    w.map(|w| fill(src, ctx, env, w)).transpose()
}

fn fill(src: &Src, ctx: &Ctx, env: &Env, w: Window) -> Result<Detailed> {
    let rows = src.fetch(w.start, w.end, &Filter::default(), Keys { project: true, model: true, ..Keys::NONE })?;
    let usage: kanjo_core::api::Usage = rows.iter().map(|r| r.usage).sum();
    let (first_ts, last_ts): (Option<i64>, Option<i64>) =
        src.c.query_row("SELECT MIN(ts), MAX(ts) FROM requests WHERE ts >= ?1 AND ts < ?2", [w.start, w.end], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let is_active = w.start <= ctx.now && ctx.now < w.end;
    let (burn_usd_per_hour, projected_usd) = if is_active {
        let (burn, projected) = burn(ctx, &w, first_ts, usage.cost_usd);
        (Some(burn), Some(projected))
    } else {
        (None, None)
    };
    Ok(Detailed {
        block: Block {
            start: w.start,
            end: w.end,
            is_active,
            anchored: w.anchored,
            peak_pct: w.peak_pct,
            usage,
            by_model: group_rows(&rows, Dimension::Model, env),
            by_project: group_rows(&rows, Dimension::Project, env),
            burn_usd_per_hour,
            projected_usd,
        },
        first_ts,
        last_ts,
    })
}

/// Hourly spend since the first request of the window (at least five minutes) and the
/// cost at the window end if it holds.
pub(crate) fn burn(ctx: &Ctx, w: &Window, first_ts: Option<i64>, cost: f64) -> (f64, f64) {
    let elapsed = (ctx.now - first_ts.unwrap_or(w.start)).max(5 * MINUTE_MS);
    let rate = cost / (elapsed as f64 / HOUR_MS as f64);
    (rate, cost + rate * ((w.end - ctx.now).max(0) as f64 / HOUR_MS as f64))
}

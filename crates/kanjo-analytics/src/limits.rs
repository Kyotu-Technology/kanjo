use crate::blocks::{self, windows};
use crate::common::{Env, Filter, Src};
use kanjo_core::api::{Confidence, LimitEstimate, LimitPoint, LimitWindow, Limits, LimitsDetail, Query, SpendLimit, UsdPerPctPoint};
use kanjo_core::limits::{self as lim, LimitKind, LimitSample, SAME_WINDOW_TOLERANCE_MS};
use kanjo_core::time::{DAY_MS, MINUTE_MS, SEVEN_DAYS_MS};
use kanjo_core::{Ctx, Result, Store};
use rusqlite::Connection;

/// A reading older than this is flagged stale.
pub(crate) const STALE_MS: i64 = 15 * MINUTE_MS;
/// History the block calibration looks at.
const ESTIMATE_LOOKBACK_MS: i64 = 60 * DAY_MS;
/// Finished inferred blocks needed before a calibration is offered.
const ESTIMATE_MIN_BLOCKS: usize = 5;
const HISTORY_5H_MS: i64 = 7 * DAY_MS;
const HISTORY_7D_MS: i64 = 35 * DAY_MS;
const WEEKLY_POINTS: usize = 12;
const DETAIL_BLOCKS_MS: i64 = 7 * DAY_MS;

/// Current subscription limits. A window whose reset has passed is omitted. Without a
/// live five_hour reading, and with enough inferred blocks in the last 60 days, an
/// estimate calibrated on the 90th percentile block cost is filled in.
pub fn limits(store: &Store, ctx: &Ctx) -> Result<Limits> {
    let env = Env::load(store)?;
    store.read(|c| limits_in(&Src::new(c)?, ctx, &env))
}

pub(crate) fn limits_in(src: &Src, ctx: &Ctx, env: &Env) -> Result<Limits> {
    let five_hour = window(src, ctx, LimitKind::FiveHour)?;
    let seven_day = window(src, ctx, LimitKind::SevenDay)?;
    let spend = spend(src.c, ctx)?;
    let estimate = if five_hour.is_none() { estimate(src, ctx, env)? } else { None };
    Ok(Limits { five_hour, seven_day, spend, estimate })
}

fn window(src: &Src, ctx: &Ctx, kind: LimitKind) -> Result<Option<LimitWindow>> {
    let Some(last) = lim::latest(src.c, kind)? else { return Ok(None) };
    if last.resets_at <= ctx.now {
        return Ok(None);
    }
    let samples = lim::window_samples(src.c, kind, last.resets_at)?;
    let f = lim::forecast(kind, &samples, ctx.now);
    let length = kind.window_ms().unwrap_or(SEVEN_DAYS_MS);
    let usd_in_window = src.total(last.resets_at - length, last.resets_at.min(ctx.now + 1), &Filter::default())?.cost_usd;
    let daily_budget_pct = (kind == LimitKind::SevenDay).then(|| {
        let days = (last.resets_at - ctx.now) as f64 / DAY_MS as f64;
        (100.0 - last.used_pct).max(0.0) / days
    });
    Ok(Some(LimitWindow {
        used_pct: last.used_pct,
        resets_at: last.resets_at,
        observed_at: last.ts,
        stale: ctx.now - last.ts > STALE_MS,
        rate_pct_per_hour: f.rate_pct_per_hour,
        exhausts_at: f.exhausts_at,
        will_exhaust: f.will_exhaust,
        usd_in_window,
        usd_per_pct: (last.used_pct >= 1.0).then(|| usd_in_window / last.used_pct),
        daily_budget_pct,
        confidence: Confidence::Official,
    }))
}

/// The spend limit; one without a known reset (`resets_at` 0) is kept, one whose reset passed is omitted.
fn spend(c: &Connection, ctx: &Ctx) -> Result<Option<SpendLimit>> {
    let Some(last) = lim::latest(c, LimitKind::Spend)? else { return Ok(None) };
    if last.resets_at > 0 && last.resets_at <= ctx.now {
        return Ok(None);
    }
    let samples = lim::window_samples(c, LimitKind::Spend, last.resets_at)?;
    let f = if last.resets_at > 0 { lim::forecast(LimitKind::Spend, &samples, ctx.now) } else { lim::Forecast::default() };
    Ok(Some(SpendLimit {
        used_pct: last.used_pct,
        resets_at: last.resets_at,
        observed_at: last.ts,
        used_usd: last.used_usd,
        limit_usd: last.limit_usd,
        period: last.period,
        exhausts_at: f.exhausts_at,
        will_exhaust: f.will_exhaust,
    }))
}

fn estimate(src: &Src, ctx: &Ctx, env: &Env) -> Result<Option<LimitEstimate>> {
    let from = ctx.now - ESTIMATE_LOOKBACK_MS;
    let mut costs: Vec<f64> = Vec::new();
    for w in windows(src.c, from, ctx.now + 1)? {
        if w.anchored || w.end > ctx.now || w.start < from {
            continue;
        }
        costs.push(src.total(w.start, w.end, &Filter::default())?.cost_usd);
    }
    if costs.len() < ESTIMATE_MIN_BLOCKS {
        return Ok(None);
    }
    let p90 = percentile(&mut costs, 0.9);
    if p90 <= 1e-9 {
        return Ok(None);
    }
    let current = blocks::active(src, ctx, env)?.map(|d| d.block.usage.cost_usd).unwrap_or(0.0);
    Ok(Some(LimitEstimate {
        p90_block_usd: p90,
        blocks_observed: costs.len() as i64,
        current_block_usd: current,
        current_block_pct: current / p90 * 100.0,
        confidence: Confidence::Estimate,
    }))
}

/// Nearest-rank percentile, `p` in 0..1.
pub(crate) fn percentile(values: &mut [f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let rank = (p * values.len() as f64).ceil() as usize;
    values[rank.clamp(1, values.len()) - 1]
}

/// Limits with their history: five-hour readings of the last 7 days and seven-day
/// readings of the last 35 days (`q.from`/`q.to` override both), the cost bought by one
/// weekly percentage point for each of the last 12 seven-day windows, and the blocks
/// of the last 7 days.
pub fn limits_detail(store: &Store, ctx: &Ctx, q: &Query) -> Result<LimitsDetail> {
    let env = Env::load(store)?;
    store.read(|c| {
        let src = Src::new(c)?;
        let to = q.to.unwrap_or(ctx.now + 1);
        let points = |kind: LimitKind, back: i64| -> Result<Vec<LimitPoint>> {
            Ok(lim::history(c, kind, q.from.unwrap_or(ctx.now - back), to)?
                .into_iter()
                .map(|s| LimitPoint { ts: s.ts, used_pct: s.used_pct, resets_at: s.resets_at })
                .collect())
        };
        Ok(LimitsDetail {
            current: limits_in(&src, ctx, &env)?,
            history_5h: points(LimitKind::FiveHour, HISTORY_5H_MS)?,
            history_7d: points(LimitKind::SevenDay, HISTORY_7D_MS)?,
            usd_per_pct_weekly: weekly(&src, ctx)?,
            blocks: blocks::detailed(&src, ctx, &env, ctx.now - DETAIL_BLOCKS_MS, ctx.now + 1)?.into_iter().map(|d| d.block).collect(),
        })
    })
}

/// Peak reading per distinct window of `kind` (resets within the tolerance merged), oldest first.
pub(crate) fn window_peaks(c: &Connection, kind: LimitKind, from: i64, to: i64) -> Result<Vec<(i64, f64)>> {
    let samples: Vec<LimitSample> = lim::history(c, kind, from, to)?;
    let mut by_reset: Vec<(i64, f64)> = samples.iter().map(|s| (s.resets_at, s.used_pct)).collect();
    by_reset.sort_by_key(|x| x.0);
    let mut out: Vec<(i64, f64)> = Vec::new();
    let mut first = i64::MIN;
    for (resets_at, pct) in by_reset {
        match out.last_mut() {
            Some(last) if resets_at - first <= SAME_WINDOW_TOLERANCE_MS => last.1 = last.1.max(pct),
            _ => {
                first = resets_at;
                out.push((resets_at, pct));
            }
        }
    }
    Ok(out)
}

fn weekly(src: &Src, ctx: &Ctx) -> Result<Vec<UsdPerPctPoint>> {
    let peaks = window_peaks(src.c, LimitKind::SevenDay, i64::MIN / 2, ctx.now + 1)?;
    let skip = peaks.len().saturating_sub(WEEKLY_POINTS);
    peaks
        .into_iter()
        .skip(skip)
        .map(|(resets_at, pct)| {
            let week_start = resets_at - SEVEN_DAYS_MS;
            let usd = src.total(week_start, resets_at.min(ctx.now + 1), &Filter::default())?.cost_usd;
            Ok(UsdPerPctPoint { week_start, usd, pct, usd_per_pct: (pct >= 1.0).then(|| usd / pct) })
        })
        .collect()
}

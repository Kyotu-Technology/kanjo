use crate::common::{bucket_label, Filter, Src};
use crate::limits::window_peaks;
use kanjo_core::api::{Confidence, Granularity, PlanAction, PlanFit, PlanMonth};
use kanjo_core::limits::LimitKind;
use kanjo_core::settings::{PlanKind, Settings};
use kanjo_core::time::DAY_MS;
use kanjo_core::{Ctx, Result, Store};

/// Thresholds behind the plan recommendation. Every figure looks at the last
/// `LOOKBACK_DAYS` days. The rules apply in this order:
/// no plan -> Unknown (`no_plan`); less than `MIN_HISTORY_DAYS` since the first
/// recorded usage -> Unknown (`no_history`); at least `UPGRADE_MIN_HOT_WINDOWS` 5-hour
/// windows peaking at `HOT_WINDOW_PCT` or more, or the weekly limit reaching
/// `UPGRADE_WEEKLY_PCT` -> Upgrade to the next tier (`frequent_limit_hits`; Keep on
/// the top tier or a plan without tiers); weekly peak under `DOWNGRADE_WEEKLY_PCT` and
/// 5-hour peak under `DOWNGRADE_FIVE_HOUR_PCT` on a plan with a cheaper tier ->
/// Downgrade (`low_utilisation`; on Ultra, which has no 5-hour limit once the account
/// is verified, a missing 5-hour peak counts as low); API-equivalent cost below the subscription price ->
/// Downgrade to API billing (`api_cheaper`); otherwise Keep (`within_plan`).
/// With less than `LOOKBACK_DAYS` of history the API-equivalent cost is scaled to a
/// `LOOKBACK_DAYS` pace for the `api_cheaper` test and for `roi`.
/// Tier order: Pro < Max < Ultra, and Team Standard < Team Premium for seats.
pub mod thresholds {
    pub const LOOKBACK_DAYS: i64 = 30;
    pub const MIN_HISTORY_DAYS: i64 = 7;
    pub const HOT_WINDOW_PCT: f64 = 90.0;
    pub const UPGRADE_MIN_HOT_WINDOWS: i64 = 3;
    pub const UPGRADE_WEEKLY_PCT: f64 = 95.0;
    pub const DOWNGRADE_WEEKLY_PCT: f64 = 25.0;
    pub const DOWNGRADE_FIVE_HOUR_PCT: f64 = 40.0;
    pub const MONTHS: i32 = 6;
}

use thresholds::*;

const LADDERS: [&[PlanKind]; 2] = [&[PlanKind::Pro, PlanKind::Max, PlanKind::Ultra], &[PlanKind::Team, PlanKind::TeamPremium]];

fn tier_shift(kind: PlanKind, delta: isize) -> Option<PlanKind> {
    let tiers = LADDERS.iter().find(|l| l.contains(&kind))?;
    let i = tiers.iter().position(|t| *t == kind)? as isize + delta;
    (0..tiers.len() as isize).contains(&i).then(|| tiers[i as usize])
}

/// How well the chosen plan fits the last 30 days of use, with the API-equivalent
/// cost of the last six calendar months.
pub fn plan_fit(store: &Store, ctx: &Ctx) -> Result<PlanFit> {
    let plan = Settings::load(store)?.plan;
    let subscription = plan.effective_monthly_usd();
    store.read(|c| {
        let src = Src::new(c)?;
        let f = Filter::default();
        let now = ctx.now;
        let mut months = Vec::new();
        for i in (0..MONTHS).rev() {
            let start = ctx.month_shift(now, -i);
            let end = ctx.month_shift(now, 1 - i).min(now);
            months.push(PlanMonth {
                month: bucket_label(ctx, Granularity::Month, start),
                api_equivalent_usd: src.total(start, end, &f)?.cost_usd,
                subscription_usd: subscription,
            });
        }
        let from = now - LOOKBACK_DAYS * DAY_MS;
        let api = src.total(from, now, &f)?.cost_usd;
        let five = window_peaks(c, LimitKind::FiveHour, from, now + 1)?;
        let seven = window_peaks(c, LimitKind::SevenDay, from, now + 1)?;
        let max_five_hour_pct = five.iter().map(|x| x.1).reduce(f64::max);
        let max_seven_day_pct = seven.iter().map(|x| x.1).reduce(f64::max);
        let blocks_near_limit = five.iter().filter(|x| x.1 >= HOT_WINDOW_PCT).count() as i64;
        let first: Option<i64> =
            c.query_row("SELECT MIN(t) FROM (SELECT MIN(hour) AS t FROM usage_hourly UNION ALL SELECT MIN(ts) FROM requests)", [], |r| {
                r.get(0)
            })?;
        let history_ok = first.is_some_and(|t| now - t >= MIN_HISTORY_DAYS * DAY_MS);
        let span = (now - first.unwrap_or(from).max(from)).max(DAY_MS);
        let api_pace = api * (LOOKBACK_DAYS * DAY_MS) as f64 / span as f64;
        let hot = blocks_near_limit >= UPGRADE_MIN_HOT_WINDOWS || max_seven_day_pct.is_some_and(|p| p >= UPGRADE_WEEKLY_PCT);
        let five_hour_low = match max_five_hour_pct {
            Some(p) => p < DOWNGRADE_FIVE_HOUR_PCT,
            None => plan.kind == PlanKind::Ultra,
        };
        let low = max_seven_day_pct.is_some_and(|p| p < DOWNGRADE_WEEKLY_PCT) && five_hour_low;
        let (action, target, reason) = if plan.kind == PlanKind::Unknown {
            (PlanAction::Unknown, None, "no_plan")
        } else if !history_ok {
            (PlanAction::Unknown, None, "no_history")
        } else if hot {
            match tier_shift(plan.kind, 1) {
                Some(next) => (PlanAction::Upgrade, Some(next), "frequent_limit_hits"),
                None => (PlanAction::Keep, None, "frequent_limit_hits"),
            }
        } else if let (true, Some(lower)) = (low, tier_shift(plan.kind, -1)) {
            (PlanAction::Downgrade, Some(lower), "low_utilisation")
        } else if plan.kind != PlanKind::Api && subscription.is_some_and(|s| api_pace < s) {
            (PlanAction::Downgrade, Some(PlanKind::Api), "api_cheaper")
        } else {
            (PlanAction::Keep, None, "within_plan")
        };
        Ok(PlanFit {
            months,
            api_equivalent_usd: api,
            subscription_usd: subscription,
            roi: subscription.filter(|s| *s > 0.0).map(|s| api_pace / s),
            max_five_hour_pct,
            max_seven_day_pct,
            blocks_near_limit,
            action,
            target,
            reason: reason.to_string(),
            confidence: Confidence::Estimate,
            plan: plan.clone(),
        })
    })
}

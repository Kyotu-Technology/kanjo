//! Every number the alert rules and insights decide on. Changing a value here
//! changes both the alerts and the matching insight, so they never disagree.

use kanjo_core::time::{DAY_MS, HOUR_MS, MINUTE_MS};

/// A session is active while its last request or statusline snapshot is at
/// most this old and no `SessionEnd` hook marker came after it.
pub const ACTIVE_SESSION_MS: i64 = 30 * MINUTE_MS;

/// A limit threshold at or above this percentage raises a critical alert.
pub const LIMIT_CRITICAL_PCT: f64 = 95.0;
/// A limit threshold at or above this percentage (and below critical) raises a warning; lower ones are informational.
pub const LIMIT_WARNING_PCT: f64 = 90.0;

/// Cost spike: the baseline is the median of this many most recent days with any cost, before the day examined.
pub const SPIKE_BASELINE_DAYS: usize = 14;
/// Cost spike: fewer active days than this in the baseline means there is no baseline yet.
pub const SPIKE_MIN_BASELINE_DAYS: usize = 5;
/// Cost spike: baseline days are looked for at most this many days back.
pub const SPIKE_LOOKBACK_DAYS: i64 = 90;
/// Cost spike: the day must cost more than this multiple of the baseline median...
pub const SPIKE_FACTOR: f64 = 3.0;
/// ...and at least this much in USD, so cheap days never alert.
pub const SPIKE_FLOOR_USD: f64 = 20.0;

/// Runaway session: the typical session is the median cost of the sessions started within this window.
pub const RUNAWAY_BASELINE_MS: i64 = 30 * DAY_MS;
/// Runaway session: fewer comparable sessions than this means there is no baseline yet.
pub const RUNAWAY_MIN_BASELINE_SESSIONS: usize = 5;
/// Runaway session: the session must cost more than this multiple of the typical session...
pub const RUNAWAY_FACTOR: f64 = 5.0;
/// ...and at least this much in USD.
pub const RUNAWAY_FLOOR_USD: f64 = 10.0;

/// Agent loop: requests are counted over this sliding window.
pub const LOOP_WINDOW_MS: i64 = 30 * MINUTE_MS;
/// Agent loop: at least this many requests (main conversation and subagents) inside the window.
pub const LOOP_MIN_REQUESTS: i64 = 60;
/// Agent loop: the statusline snapshot reports at most this many changed lines (added plus removed).
pub const LOOP_MAX_CHANGED_LINES: i64 = 10;

/// Cache cold: only worth an alert when at least this many tokens would have to be written to the cache again.
pub const CACHE_COLD_MIN_TOKENS: i64 = 50_000;
/// Cache cold: sessions idle for longer than this are left alone (a 1-hour cache can only expire after an hour of idling).
pub const CACHE_COLD_MAX_IDLE_MS: i64 = 3 * HOUR_MS;

/// Cache miss streak: misses are counted over this sliding window.
pub const MISS_STREAK_WINDOW_MS: i64 = 30 * MINUTE_MS;
/// Cache miss streak: at least this many misses of one session inside the window.
pub const MISS_STREAK_MIN: i64 = 3;

/// Insights look at this much history.
pub const INSIGHTS_WINDOW_MS: i64 = 30 * DAY_MS;
/// A configuration change is blamed for the cache misses that follow it within this time.
pub const BREAKER_WINDOW_MS: i64 = 30 * MINUTE_MS;
/// Number of most recent cache misses listed in the insights.
pub const RECENT_MISSES: usize = 20;
/// Number of cache breakers listed in the insights.
pub const MAX_BREAKERS: usize = 20;
/// Number of context warnings listed in the insights.
pub const MAX_CONTEXT_WARNINGS: usize = 20;

/// Model mix: a main-conversation response with at most this many output tokens counts as short.
pub const SHORT_RESPONSE_MAX_OUTPUT: i64 = 500;
/// Model mix: a suggestion is shown only when it saves at least this much over the insights window...
pub const MODEL_MIX_MIN_SAVING_USD: f64 = 5.0;
/// ...and at least this share of what the affected requests cost.
pub const MODEL_MIX_MIN_SAVING_SHARE: f64 = 0.10;

/// Budget projection: the pace is measured over at least this much of the period, so the first minutes do not extrapolate wildly.
pub const BUDGET_MIN_ELAPSED_MS: i64 = HOUR_MS;

/// Desktop notifications still being shown at once; further ones are dropped (and logged) instead of piling up threads.
pub const MAX_PENDING_NOTIFICATIONS: usize = 4;

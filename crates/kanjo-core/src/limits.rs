//! Subscription limit readings (statusline `rate_limits`) and the one shared
//! forecast. Dashboard, alerts and CLI all call this so their numbers agree.

use crate::time::{DAY_MS, FIVE_HOURS_MS, HOUR_MS, MINUTE_MS, SEVEN_DAYS_MS};
use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimitKind {
    FiveHour,
    SevenDay,
    Spend,
}

impl LimitKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            LimitKind::FiveHour => "five_hour",
            LimitKind::SevenDay => "seven_day",
            LimitKind::Spend => "spend",
        }
    }

    /// Length of the window, when it is fixed.
    pub fn window_ms(&self) -> Option<i64> {
        match self {
            LimitKind::FiveHour => Some(FIVE_HOURS_MS),
            LimitKind::SevenDay => Some(SEVEN_DAYS_MS),
            LimitKind::Spend => None,
        }
    }

    /// How far back the burn rate looks.
    pub fn lookback_ms(&self) -> i64 {
        match self {
            LimitKind::FiveHour => HOUR_MS,
            LimitKind::SevenDay | LimitKind::Spend => DAY_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LimitSample {
    pub ts: i64,
    pub used_pct: f64,
    pub resets_at: i64,
    pub used_usd: Option<f64>,
    pub limit_usd: Option<f64>,
    pub period: Option<String>,
}

/// Two readings belong to the same window when their reset times agree within this tolerance.
pub const SAME_WINDOW_TOLERANCE_MS: i64 = 2 * MINUTE_MS;

/// Stores a reading unless it repeats the previous one within ten minutes.
/// Returns whether a row was written.
pub fn insert_sample(tx: &Connection, kind: LimitKind, s: &LimitSample) -> Result<bool> {
    if let Some(last) = latest(tx, kind)? {
        let same_window = (last.resets_at - s.resets_at).abs() <= SAME_WINDOW_TOLERANCE_MS;
        let unchanged = same_window && (last.used_pct - s.used_pct).abs() < 1e-9 && last.used_usd == s.used_usd;
        if s.ts <= last.ts || (unchanged && s.ts - last.ts < 10 * MINUTE_MS) {
            return Ok(false);
        }
    }
    tx.execute(
        "INSERT OR IGNORE INTO limit_samples (kind, ts, used_pct, resets_at, used_usd, limit_usd, period) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![kind.as_str(), s.ts, s.used_pct, s.resets_at, s.used_usd, s.limit_usd, s.period],
    )?;
    Ok(true)
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<LimitSample> {
    Ok(LimitSample {
        ts: r.get(0)?,
        used_pct: r.get(1)?,
        resets_at: r.get(2)?,
        used_usd: r.get(3)?,
        limit_usd: r.get(4)?,
        period: r.get(5)?,
    })
}

const COLS: &str = "ts, used_pct, resets_at, used_usd, limit_usd, period";

pub fn latest(conn: &Connection, kind: LimitKind) -> Result<Option<LimitSample>> {
    Ok(conn
        .query_row(&format!("SELECT {COLS} FROM limit_samples WHERE kind = ?1 ORDER BY ts DESC LIMIT 1"), [kind.as_str()], row)
        .optional()?)
}

/// All readings of the window that resets at `resets_at`, oldest first.
pub fn window_samples(conn: &Connection, kind: LimitKind, resets_at: i64) -> Result<Vec<LimitSample>> {
    let mut stmt =
        conn.prepare(&format!("SELECT {COLS} FROM limit_samples WHERE kind = ?1 AND resets_at BETWEEN ?2 AND ?3 ORDER BY ts"))?;
    let rows = stmt
        .query_map(params![kind.as_str(), resets_at - SAME_WINDOW_TOLERANCE_MS, resets_at + SAME_WINDOW_TOLERANCE_MS], row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

pub fn history(conn: &Connection, kind: LimitKind, from: i64, to: i64) -> Result<Vec<LimitSample>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM limit_samples WHERE kind = ?1 AND ts >= ?2 AND ts < ?3 ORDER BY ts"))?;
    let rows = stmt.query_map(params![kind.as_str(), from, to], row)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Forecast {
    /// Percentage points per hour over the lookback. `None` without enough data.
    pub rate_pct_per_hour: Option<f64>,
    /// When the limit reaches 100% at that rate, if it does so before the reset.
    pub exhausts_at: Option<i64>,
    pub will_exhaust: bool,
    /// The window already reset; the reading is history, not state.
    pub expired: bool,
}

/// Linear forecast for the window the newest sample belongs to:
/// time to exhaustion = (100 - used%) / rate, where rate is the rise over the
/// lookback period up to `now` (so an idle stretch lowers it). When the
/// lookback holds too little history, the rate is taken from the window start.
/// `samples` must be one window's readings, oldest first.
pub fn forecast(kind: LimitKind, samples: &[LimitSample], now: i64) -> Forecast {
    let Some(last) = samples.last() else { return Forecast::default() };
    if now >= last.resets_at {
        return Forecast { expired: true, ..Forecast::default() };
    }
    let lookback = kind.lookback_ms();
    let from = now - lookback;
    let reference = samples.iter().find(|s| s.ts >= from).filter(|s| last.ts - s.ts >= lookback / 4);
    let (ref_ts, ref_pct) = match (reference, kind.window_ms()) {
        (Some(s), _) => (s.ts, s.used_pct),
        (None, Some(window)) => (last.resets_at - window, 0.0),
        (None, None) => (samples[0].ts, samples[0].used_pct),
    };
    let elapsed = now - ref_ts;
    if elapsed < 5 * MINUTE_MS {
        return Forecast::default();
    }
    let rate = (last.used_pct - ref_pct) / (elapsed as f64 / HOUR_MS as f64);
    if rate <= 1e-9 {
        return Forecast { rate_pct_per_hour: Some(rate.max(0.0)), ..Forecast::default() };
    }
    let remaining = (100.0 - last.used_pct).max(0.0);
    let at = now + (remaining / rate * HOUR_MS as f64) as i64;
    let will_exhaust = at < last.resets_at;
    Forecast { rate_pct_per_hour: Some(rate), exhausts_at: will_exhaust.then_some(at), will_exhaust, expired: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(ts: i64, pct: f64, resets_at: i64) -> LimitSample {
        LimitSample { ts, used_pct: pct, resets_at, used_usd: None, limit_usd: None, period: None }
    }

    #[test]
    fn forecast_uses_recent_rate() {
        let reset = 5 * HOUR_MS;
        // 20% -> 60% over the last hour, at t = 2h: 40 %/h, 40% left, exhausted at 3h.
        let samples = vec![s(HOUR_MS, 20.0, reset), s(2 * HOUR_MS, 60.0, reset)];
        let f = forecast(LimitKind::FiveHour, &samples, 2 * HOUR_MS);
        assert!((f.rate_pct_per_hour.unwrap() - 40.0).abs() < 1e-9);
        assert_eq!(f.exhausts_at, Some(3 * HOUR_MS));
        assert!(f.will_exhaust);
    }

    #[test]
    fn idle_time_lowers_the_rate_and_slow_use_never_exhausts() {
        let reset = 5 * HOUR_MS;
        let samples = vec![s(HOUR_MS, 10.0, reset), s(HOUR_MS + 30 * MINUTE_MS, 12.0, reset)];
        let f = forecast(LimitKind::FiveHour, &samples, 2 * HOUR_MS);
        assert!(f.rate_pct_per_hour.unwrap() < 3.0);
        assert!(!f.will_exhaust && f.exhausts_at.is_none());
    }

    #[test]
    fn single_sample_falls_back_to_window_start() {
        let reset = 5 * HOUR_MS;
        let f = forecast(LimitKind::FiveHour, &[s(2 * HOUR_MS, 80.0, reset)], 2 * HOUR_MS);
        assert!((f.rate_pct_per_hour.unwrap() - 40.0).abs() < 1e-9);
        assert_eq!(f.exhausts_at, Some(2 * HOUR_MS + 30 * MINUTE_MS));
    }

    #[test]
    fn expired_window_has_no_forecast() {
        let f = forecast(LimitKind::FiveHour, &[s(HOUR_MS, 50.0, 5 * HOUR_MS)], 6 * HOUR_MS);
        assert!(f.expired && f.rate_pct_per_hour.is_none());
    }

    #[test]
    fn samples_are_deduplicated() {
        let t = crate::testkit::TestStore::new();
        let reset = 5 * HOUR_MS;
        let n = t
            .store
            .write(|tx| {
                let mut n = 0;
                for (ts, pct) in [(1000, 10.0), (2000, 10.0), (3000, 11.0), (3000 + 11 * MINUTE_MS, 11.0)] {
                    n += insert_sample(tx, LimitKind::FiveHour, &s(ts, pct, reset))? as i32;
                }
                Ok(n)
            })
            .unwrap();
        assert_eq!(n, 3);
        let all = t.store.read(|c| window_samples(c, LimitKind::FiveHour, reset)).unwrap();
        assert_eq!(all.len(), 3);
    }
}

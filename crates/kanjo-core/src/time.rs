use chrono::{Datelike, Offset, TimeZone};

pub const MINUTE_MS: i64 = 60_000;
pub const HOUR_MS: i64 = 3_600_000;
pub const DAY_MS: i64 = 86_400_000;
pub const FIVE_HOURS_MS: i64 = 5 * HOUR_MS;
pub const SEVEN_DAYS_MS: i64 = 7 * DAY_MS;
/// A session counts as active while its last request is this recent. Shared by
/// the dashboard and the alert rules so they agree on what "running" means.
pub const ACTIVE_SESSION_MS: i64 = 15 * MINUTE_MS;

/// Evaluation context passed to every query: "now" and how to cut local days.
/// Tests construct it directly; production code uses `Ctx::system()`.
/// All timestamps in Kanjo are Unix epoch milliseconds, UTC.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub now: i64,
    /// Local offset from UTC in minutes (east positive). A single offset is used
    /// for the whole query, so buckets around a DST change shift by one hour.
    pub tz_offset_min: i32,
    /// 0 = Monday ... 6 = Sunday.
    pub week_start: u8,
}

impl Ctx {
    pub fn system() -> Ctx {
        let now = chrono::Local::now();
        Ctx { now: now.timestamp_millis(), tz_offset_min: now.offset().fix().local_minus_utc() / 60, week_start: 0 }
    }

    pub fn utc(now: i64) -> Ctx {
        Ctx { now, tz_offset_min: 0, week_start: 0 }
    }

    fn offset_ms(&self) -> i64 {
        self.tz_offset_min as i64 * MINUTE_MS
    }

    /// Start of the local day containing `ts`.
    pub fn day_start(&self, ts: i64) -> i64 {
        (ts + self.offset_ms()).div_euclid(DAY_MS) * DAY_MS - self.offset_ms()
    }

    /// Start of the local week containing `ts`, honouring `week_start`.
    pub fn week_start_of(&self, ts: i64) -> i64 {
        let day = self.day_start(ts);
        // 1970-01-01 was a Thursday (weekday index 3 with Monday = 0).
        let days = (day + self.offset_ms()).div_euclid(DAY_MS);
        let weekday = (days + 3).rem_euclid(7);
        let back = (weekday - self.week_start as i64).rem_euclid(7);
        day - back * DAY_MS
    }

    /// Start of the local calendar month containing `ts`.
    pub fn month_start(&self, ts: i64) -> i64 {
        let local = chrono::Utc.timestamp_millis_opt(ts + self.offset_ms()).single().unwrap_or_default();
        let first = chrono::Utc.with_ymd_and_hms(local.year(), local.month(), 1, 0, 0, 0).single().unwrap_or_default();
        first.timestamp_millis() - self.offset_ms()
    }

    /// Start of the month `delta` months away from the month containing `ts`.
    pub fn month_shift(&self, ts: i64, delta: i32) -> i64 {
        let local = chrono::Utc.timestamp_millis_opt(ts + self.offset_ms()).single().unwrap_or_default();
        let total = local.year() * 12 + local.month0() as i32 + delta;
        let first =
            chrono::Utc.with_ymd_and_hms(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1, 0, 0, 0).single().unwrap_or_default();
        first.timestamp_millis() - self.offset_ms()
    }

    /// Local calendar date of `ts` as `YYYY-MM-DD`.
    pub fn date_label(&self, ts: i64) -> String {
        let local = chrono::Utc.timestamp_millis_opt(ts + self.offset_ms()).single().unwrap_or_default();
        local.format("%Y-%m-%d").to_string()
    }
}

/// Floors an epoch-millisecond timestamp to the UTC hour.
pub fn hour_floor(ts: i64) -> i64 {
    ts.div_euclid(HOUR_MS) * HOUR_MS
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_day_and_week() {
        // 2026-11-25T22:30:00Z, Warsaw winter time (+60): local 23:30 on Wednesday.
        let ts = 1_795_645_800_000;
        let ctx = Ctx { now: ts, tz_offset_min: 60, week_start: 0 };
        assert_eq!(ctx.date_label(ts), "2026-11-25");
        assert_eq!(ctx.date_label(ctx.day_start(ts)), "2026-11-25");
        assert_eq!(ctx.date_label(ctx.week_start_of(ts)), "2026-11-23");
        assert_eq!(ctx.date_label(ctx.month_start(ts)), "2026-11-01");
        assert_eq!(ctx.date_label(ctx.month_shift(ts, -11)), "2025-12-01");
        assert_eq!(ctx.date_label(ctx.month_shift(ts, 2)), "2027-01-01");
        // 40 minutes later it is already Thursday locally.
        assert_eq!(ctx.date_label(ts + 40 * MINUTE_MS), "2026-11-26");
    }
}

use jiff::civil::{Date, DateTime};
use jiff::tz::{Offset, TimeZone};
use jiff::Timestamp;

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
///
/// With a `zone`, day, week and month boundaries and labels use the offset in
/// force at each timestamp, so ranges across a daylight saving change stay on
/// local midnights. Without one, `tz_offset_min` applies to every timestamp.
/// Reports built from `usage_hourly` assign whole UTC hours, so in a zone with
/// a fractional offset (e.g. UTC+5:30) a bucket edge is approximate to the hour.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub now: i64,
    /// Offset from UTC in minutes (east positive) at `now`; the offset of every
    /// timestamp when `zone` is `None`.
    pub tz_offset_min: i32,
    /// 0 = Monday ... 6 = Sunday.
    pub week_start: u8,
    pub zone: Option<TimeZone>,
}

impl Ctx {
    /// "Now" in the system time zone.
    pub fn system() -> Ctx {
        Ctx::with_zone(now_ms(), TimeZone::system(), 0)
    }

    pub fn utc(now: i64) -> Ctx {
        Ctx { now, tz_offset_min: 0, week_start: 0, zone: None }
    }

    pub fn with_zone(now: i64, zone: TimeZone, week_start: u8) -> Ctx {
        let tz_offset_min = Timestamp::from_millisecond(now).map(|t| zone.to_offset(t).seconds() / 60).unwrap_or(0);
        Ctx { now, tz_offset_min, week_start, zone: Some(zone) }
    }

    /// A context in the IANA time zone `name`, e.g. `Europe/Warsaw`.
    pub fn in_zone(now: i64, name: &str, week_start: u8) -> crate::Result<Ctx> {
        let zone = TimeZone::get(name).map_err(|e| crate::Error::Invalid(format!("unknown time zone {name}: {e}")))?;
        Ok(Ctx::with_zone(now, zone, week_start))
    }

    /// The same context at another "now".
    pub fn at(&self, now: i64) -> Ctx {
        match &self.zone {
            Some(z) => Ctx::with_zone(now, z.clone(), self.week_start),
            None => Ctx { now, ..self.clone() },
        }
    }

    /// Offset from UTC in force at `ts`, in milliseconds.
    pub fn offset_ms_at(&self, ts: i64) -> i64 {
        match &self.zone {
            Some(z) => match Timestamp::from_millisecond(ts) {
                Ok(t) => z.to_offset(t).seconds() as i64 * 1000,
                Err(_) => self.tz_offset_min as i64 * MINUTE_MS,
            },
            None => self.tz_offset_min as i64 * MINUTE_MS,
        }
    }

    /// Offset from UTC in force at `ts`, in minutes.
    pub fn offset_min_at(&self, ts: i64) -> i32 {
        (self.offset_ms_at(ts) / MINUTE_MS) as i32
    }

    /// Local civil date and time of `ts`.
    pub fn local(&self, ts: i64) -> DateTime {
        utc_datetime(ts + self.offset_ms_at(ts))
    }

    /// The instant a local civil date and time names. A time skipped by a
    /// clock change resolves to the moment after the gap, a repeated one to its
    /// first occurrence.
    pub fn from_local(&self, dt: DateTime) -> i64 {
        if let Some(z) = &self.zone {
            if let Ok(t) = z.to_timestamp(dt) {
                return t.as_millisecond();
            }
        }
        utc_millis(dt) - self.tz_offset_min as i64 * MINUTE_MS
    }

    /// Start of the local day containing `ts`.
    pub fn day_start(&self, ts: i64) -> i64 {
        match &self.zone {
            Some(_) => self.from_local(self.local(ts).date().at(0, 0, 0, 0)),
            None => {
                let off = self.tz_offset_min as i64 * MINUTE_MS;
                (ts + off).div_euclid(DAY_MS) * DAY_MS - off
            }
        }
    }

    /// Start of the local day `days` days away from the day containing `ts`.
    pub fn day_shift(&self, ts: i64, days: i64) -> i64 {
        match &self.zone {
            Some(_) => {
                let date = self.local(ts).date();
                match jiff::Span::new().try_days(days).and_then(|s| date.checked_add(s)) {
                    Ok(d) => self.from_local(d.at(0, 0, 0, 0)),
                    Err(_) => self.day_start(ts) + days * DAY_MS,
                }
            }
            None => self.day_start(ts) + days * DAY_MS,
        }
    }

    /// Start of the local hour containing `ts`.
    pub fn hour_start(&self, ts: i64) -> i64 {
        let off = self.offset_ms_at(ts);
        (ts + off).div_euclid(HOUR_MS) * HOUR_MS - off
    }

    /// Local weekday (0 = Monday) of `ts`.
    pub fn weekday(&self, ts: i64) -> u8 {
        let days = (ts + self.offset_ms_at(ts)).div_euclid(DAY_MS);
        // 1970-01-01 was a Thursday (weekday index 3 with Monday = 0).
        (days + 3).rem_euclid(7) as u8
    }

    /// Local weekday (0 = Monday) and hour of `ts`.
    pub fn weekday_hour(&self, ts: i64) -> (u8, u8) {
        let local = ts + self.offset_ms_at(ts);
        (self.weekday(ts), (local.rem_euclid(DAY_MS) / HOUR_MS) as u8)
    }

    /// Start of the local week containing `ts`, honouring `week_start`.
    pub fn week_start_of(&self, ts: i64) -> i64 {
        let back = (self.weekday(ts) as i64 - self.week_start as i64).rem_euclid(7);
        self.day_shift(ts, -back)
    }

    /// Start of the local calendar month containing `ts`.
    pub fn month_start(&self, ts: i64) -> i64 {
        self.month_shift(ts, 0)
    }

    /// Start of the month `delta` months away from the month containing `ts`.
    pub fn month_shift(&self, ts: i64, delta: i32) -> i64 {
        let local = self.local(ts);
        let total = local.year() as i64 * 12 + local.month() as i64 - 1 + delta as i64;
        match first_of_month(total.div_euclid(12), total.rem_euclid(12) + 1) {
            Some(dt) => self.from_local(dt),
            None => 0,
        }
    }

    /// Local calendar date of `ts` as `YYYY-MM-DD`.
    pub fn date_label(&self, ts: i64) -> String {
        self.local(ts).strftime("%Y-%m-%d").to_string()
    }
}

/// Floors an epoch-millisecond timestamp to the UTC hour.
pub fn hour_floor(ts: i64) -> i64 {
    ts.div_euclid(HOUR_MS) * HOUR_MS
}

pub fn now_ms() -> i64 {
    Timestamp::now().as_millisecond()
}

/// The civil date and time of `ts` read in UTC. Timestamps outside years
/// -9999..=9999 read as the epoch.
pub fn utc_datetime(ts: i64) -> DateTime {
    Offset::UTC.to_datetime(Timestamp::from_millisecond(ts).unwrap_or_default())
}

/// Epoch milliseconds of a civil date and time taken as UTC.
pub fn utc_millis(dt: DateTime) -> i64 {
    dt.duration_since(EPOCH).as_millis() as i64
}

/// Offset of the system time zone from UTC at `ts`, in minutes (east positive).
/// Falls back to 0 (UTC) when the time zone cannot be determined.
pub fn local_offset_min(ts: i64) -> i32 {
    Timestamp::from_millisecond(ts).map(|t| TimeZone::system().to_offset(t).seconds() / 60).unwrap_or(0)
}

const EPOCH: DateTime = jiff::civil::date(1970, 1, 1).at(0, 0, 0, 0);

fn first_of_month(year: i64, month: i64) -> Option<DateTime> {
    i16::try_from(year).ok().and_then(|y| Date::new(y, month as i8, 1).ok()).map(|d| d.at(0, 0, 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_day_and_week() {
        // 2026-11-25T22:30:00Z, Warsaw winter time (+60): local 23:30 on Wednesday.
        let ts = 1_795_645_800_000;
        let ctx = Ctx { now: ts, tz_offset_min: 60, week_start: 0, zone: None };
        assert_eq!(ctx.date_label(ts), "2026-11-25");
        assert_eq!(ctx.date_label(ctx.day_start(ts)), "2026-11-25");
        assert_eq!(ctx.date_label(ctx.week_start_of(ts)), "2026-11-23");
        assert_eq!(ctx.date_label(ctx.month_start(ts)), "2026-11-01");
        assert_eq!(ctx.date_label(ctx.month_shift(ts, -11)), "2025-12-01");
        assert_eq!(ctx.date_label(ctx.month_shift(ts, 2)), "2027-01-01");
        // 40 minutes later it is already Thursday locally.
        assert_eq!(ctx.date_label(ts + 40 * MINUTE_MS), "2026-11-26");
    }

    fn ms(dt: DateTime, offset_h: i64) -> i64 {
        utc_millis(dt) - offset_h * HOUR_MS
    }

    #[test]
    fn warsaw_days_follow_the_clock_changes() {
        // 2026: summer time from 29 March 02:00 (+1 -> +2) to 25 October 03:00 (+2 -> +1).
        let spring = ms(jiff::civil::date(2026, 3, 29).at(12, 0, 0, 0), 2);
        let ctx = crate::testkit::warsaw(spring);
        assert_eq!(ctx.tz_offset_min, 120);
        let day = ctx.day_start(spring);
        assert_eq!(day, ms(jiff::civil::date(2026, 3, 29).at(0, 0, 0, 0), 1));
        assert_eq!(ctx.day_shift(day, 1) - day, 23 * HOUR_MS);
        assert_eq!(ctx.date_label(ctx.day_shift(day, 1)), "2026-03-30");
        assert_eq!(ctx.day_shift(day, -1), ms(jiff::civil::date(2026, 3, 28).at(0, 0, 0, 0), 1));
        assert_eq!(ctx.day_shift(day, 1), ms(jiff::civil::date(2026, 3, 30).at(0, 0, 0, 0), 2));

        let autumn = ms(jiff::civil::date(2026, 10, 25).at(12, 0, 0, 0), 1);
        let a = ctx.at(autumn);
        assert_eq!(a.tz_offset_min, 60);
        let day = a.day_start(autumn);
        assert_eq!(day, ms(jiff::civil::date(2026, 10, 25).at(0, 0, 0, 0), 2));
        assert_eq!(a.day_shift(day, 1) - day, 25 * HOUR_MS);

        // A whole month back from November lands on local midnight in summer time.
        let nov = ms(jiff::civil::date(2026, 11, 3).at(9, 0, 0, 0), 1);
        let n = ctx.at(nov);
        assert_eq!(n.month_shift(nov, -1), ms(jiff::civil::date(2026, 10, 1).at(0, 0, 0, 0), 2));
        assert_eq!(n.month_start(nov), ms(jiff::civil::date(2026, 11, 1).at(0, 0, 0, 0), 1));
        assert_eq!(n.date_label(n.month_shift(nov, -1)), "2026-10-01");

        // Late evening in summer is still the same local day, though the fixed winter offset would say otherwise.
        let late = ms(jiff::civil::date(2026, 7, 14).at(23, 30, 0, 0), 2);
        assert_eq!(n.date_label(late), "2026-07-14");
        assert_eq!(n.weekday_hour(late), (1, 23));
        let fixed = Ctx { now: nov, tz_offset_min: 60, week_start: 0, zone: None };
        assert_eq!(fixed.date_label(late), "2026-07-14");
        assert_eq!(fixed.weekday_hour(late), (1, 22));

        // The week of the October change starts on Monday 19 October in summer time.
        assert_eq!(a.week_start_of(autumn), ms(jiff::civil::date(2026, 10, 19).at(0, 0, 0, 0), 2));
        assert_eq!(a.hour_start(autumn + 25 * MINUTE_MS), autumn);
    }
}

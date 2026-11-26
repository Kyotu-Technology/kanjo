#![allow(dead_code)]

use kanjo_core::api::{Alert, AlertKind};
use kanjo_core::model::RequestRow;
use kanjo_core::settings::Settings;
use kanjo_core::testkit::{self, TestStore};
use kanjo_core::time::{HOUR_MS, MINUTE_MS};
use kanjo_core::Ctx;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 2026-11-26T10:00:00Z, a Thursday; 11:00 in the test time zone (UTC+1).
pub const NOW: i64 = 1_795_687_200_000;
pub const MODEL: &str = "claude-opus-5-5";
/// USD per input token of `MODEL` in the embedded price table.
pub const INPUT_USD: f64 = 4.0e-6;

pub fn ctx() -> Ctx {
    at(NOW)
}

pub fn at(now: i64) -> Ctx {
    Ctx { now, tz_offset_min: 60, week_start: 0, zone: None }
}

pub fn store() -> TestStore {
    let t = TestStore::new();
    configure(&t, |s| s.language = kanjo_core::settings::Language::En);
    t
}

pub fn configure(t: &TestStore, f: impl FnOnce(&mut Settings)) {
    let mut s = Settings::load(&t.store).unwrap();
    f(&mut s);
    s.save(&t.store).unwrap();
}

fn next_id() -> String {
    static N: AtomicUsize = AtomicUsize::new(0);
    format!("msg_{}", N.fetch_add(1, Ordering::SeqCst))
}

pub fn row(pid: i64, session: &str, ts: i64, input: i64, output: i64) -> RequestRow {
    let mut r = testkit::request(&next_id(), session, ts, MODEL, input, output);
    r.project_id = pid;
    r
}

/// One main-conversation request on `MODEL` costing `usd`.
pub fn spend(t: &TestStore, pid: i64, session: &str, ts: i64, usd: f64) {
    t.insert(&row(pid, session, ts, (usd / INPUT_USD).round() as i64, 0));
}

pub fn sql(t: &TestStore, sql: &str) {
    t.store.write(|tx| Ok(tx.execute_batch(sql)?)).unwrap();
}

/// A statusline snapshot for `session` with the given extra columns, e.g. `("ctx_used_pct", "90")`.
pub fn status(t: &TestStore, session: &str, ts: i64, cols: &[(&str, &str)]) {
    let names: String = cols.iter().map(|(c, _)| format!(", {c}")).collect();
    let values: String = cols.iter().map(|(_, v)| format!(", {v}")).collect();
    sql(t, &format!("INSERT OR REPLACE INTO session_status (session_id, ts{names}) VALUES ('{session}', {ts}{values})"));
}

pub fn of(alerts: &[Alert], kind: AlertKind) -> Vec<&Alert> {
    alerts.iter().filter(|a| a.kind == kind).collect()
}

pub const MIN: i64 = MINUTE_MS;
pub const HOUR: i64 = HOUR_MS;

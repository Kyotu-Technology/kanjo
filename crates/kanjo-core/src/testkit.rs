//! Helpers for tests in this and downstream crates (feature `testkit`).

use crate::model::{RequestRow, TokenCounts};
use crate::pricing::PriceTable;
use crate::store::{write, Store};

/// A context in Europe/Warsaw (summer time from the last Sunday of March to
/// the last Sunday of October). Falls back to the equivalent POSIX rule when
/// the system has no time zone database.
pub fn warsaw(now: i64) -> crate::Ctx {
    let zone = jiff::tz::TimeZone::get("Europe/Warsaw")
        .or_else(|_| jiff::tz::TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3"))
        .expect("Warsaw time zone");
    crate::Ctx::with_zone(now, zone, 0)
}

pub struct TestStore {
    pub store: Store,
    pub dir: tempfile::TempDir,
}

impl Default for TestStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TestStore {
    /// A fresh unencrypted database in a temporary directory.
    pub fn new() -> TestStore {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(&dir.path().join("kanjo.db"), None, None).expect("open store");
        TestStore { store, dir }
    }

    pub fn project(&self, key: &str) -> i64 {
        self.store.write(|tx| write::upsert_project(tx, key, Some(&format!("/work/{key}")), 0)).expect("project")
    }

    /// Inserts a request priced with the embedded table.
    pub fn insert(&self, row: &RequestRow) {
        self.store.write(|tx| write::insert_request(tx, row, PriceTable::embedded())).expect("insert request");
    }
}

/// A minimal main-thread request; set further fields on the result as needed.
pub fn request(message_id: &str, session_id: &str, ts: i64, model: &str, input: i64, output: i64) -> RequestRow {
    RequestRow {
        message_id: message_id.to_string(),
        session_id: session_id.to_string(),
        project_id: 1,
        ts,
        model: model.to_string(),
        tokens: TokenCounts { input, output, ..Default::default() },
        ..Default::default()
    }
}

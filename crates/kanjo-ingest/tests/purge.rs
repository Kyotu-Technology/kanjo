#![allow(clippy::unwrap_used)]
//! Deleted data stays deleted: purge and retention followed by everything that
//! makes the collector read transcripts again.

use kanjo_core::api::PurgeRequest;
use kanjo_core::paths::Paths;
use kanjo_core::settings::RetentionSettings;
use kanjo_core::spool::{self, HookRecord, SpoolRecord};
use kanjo_core::statusline::StatuslineRecord;
use kanjo_core::testkit::TestStore;
use kanjo_core::time::{now_ms, utc_datetime, MINUTE_MS};
use kanjo_core::Store;
use kanjo_ingest::Ingestor;
use kanjo_ops::retention;
use rusqlite::types::FromSql;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const A: &str = "0b6a3c1e-5d2f-4a8b-9c7d-1e2f3a4b5c6d";
const B: &str = "7f1e2d3c-4b5a-4968-8776-655443322110";
const PROJECT: &str = "-work-acme-shop";

struct Env {
    dir: tempfile::TempDir,
    store: Arc<Store>,
    paths: Paths,
    ing: Ingestor,
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn env() -> Env {
    let TestStore { store, dir } = TestStore::new();
    let claude = dir.path().join("claude");
    copy_tree(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude"), &claude);
    let paths = Paths::at(dir.path().join("home"), &claude);
    let store = Arc::new(store);
    let ing = Ingestor::new(store.clone(), paths.clone()).unwrap();
    ing.backfill(&|_| {}).unwrap();
    Env { dir, store, paths, ing }
}

impl Env {
    fn q<T: FromSql>(&self, sql: &str) -> T {
        self.store.read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap()
    }

    fn file(&self, session: &str) -> PathBuf {
        self.paths.claude_projects().join(PROJECT).join(format!("{session}.jsonl"))
    }

    fn append(&self, file: &Path, text: &str) {
        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(file).unwrap().write_all(text.as_bytes()).unwrap();
    }

    /// Totals as the dashboard sees them: request rows, hourly aggregate, session totals.
    fn totals(&self) -> (i64, i64, i64, i64) {
        self.store
            .read(|c| {
                Ok(c.query_row(
                    "SELECT (SELECT count(*) FROM requests), (SELECT COALESCE(SUM(requests), 0) FROM usage_hourly),
                            (SELECT COALESCE(SUM(output), 0) FROM usage_hourly), (SELECT COALESCE(SUM(requests), 0) FROM sessions)",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )?)
            })
            .unwrap()
    }

    /// Every way the collector reads history again: a rescan from scratch, a
    /// transcript that shrank and grew back, and a fresh collector (restart).
    fn read_everything_again(&self) {
        self.ing.rescan_from_scratch().unwrap();
        self.ing.backfill(&|_| {}).unwrap();
        let file = self.file(A);
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, "").unwrap();
        self.ing.scan().unwrap();
        std::fs::write(&file, text).unwrap();
        self.ing.scan().unwrap();
        let fresh = Ingestor::new(self.store.clone(), self.paths.clone()).unwrap();
        fresh.rescan_from_scratch().unwrap();
        fresh.backfill(&|_| {}).unwrap();
    }
}

fn iso(ts: i64) -> String {
    utc_datetime(ts).strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn assistant(session: &str, id: &str, ts: i64) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"{session}","cwd":"/work/acme/shop","version":"2.1.300","timestamp":"{}","uuid":"u-{id}","message":{{"id":"{id}","model":"claude-opus-5-5","content":[],"usage":{{"input_tokens":1,"output_tokens":7}}}}}}
"#,
        iso(ts)
    )
}

#[test]
fn a_purged_session_stays_purged_and_its_later_activity_is_kept() {
    let e = env();
    let before = e.totals();
    let rep = retention::purge(&e.store, &PurgeRequest { session: Some(A.into()), ..Default::default() }).unwrap();
    assert_eq!(rep.requests, 4);
    let after_purge = e.totals();
    assert_eq!(after_purge, (before.0 - 4, before.1 - 4, after_purge.2, before.3 - 4));
    e.read_everything_again();
    assert_eq!(e.totals(), after_purge, "nothing of the purged session comes back");
    assert_eq!(e.q::<i64>(&format!("SELECT count(*) FROM sessions WHERE id = '{A}'")), 0);

    e.append(&e.file(A), &assistant(A, "msg_A_later", now_ms() + MINUTE_MS));
    e.ing.scan().unwrap();
    assert_eq!(e.q::<i64>(&format!("SELECT requests FROM sessions WHERE id = '{A}'")), 1, "the session resumed after the purge");
    e.read_everything_again();
    assert_eq!(e.totals().0, after_purge.0 + 1);
}

#[test]
fn a_purged_project_stays_purged() {
    let e = env();
    let project: i64 = e.q("SELECT id FROM projects");
    retention::purge(&e.store, &PurgeRequest { project: Some(project), ..Default::default() }).unwrap();
    assert_eq!(e.totals(), (0, 0, 0, 0));
    e.read_everything_again();
    assert_eq!(e.totals(), (0, 0, 0, 0));
    e.append(&e.file(B), &assistant(B, "msg_B_later", now_ms() + MINUTE_MS));
    e.ing.scan().unwrap();
    assert_eq!(e.totals(), (1, 1, 7, 1));
}

#[test]
fn a_resumed_session_does_not_get_back_the_title_of_the_purged_conversation() {
    let e = env();
    e.ing.set_store_titles(true);
    e.ing.rescan_from_scratch().unwrap();
    e.ing.backfill(&|_| {}).unwrap();
    let title = || e.q::<Option<String>>(&format!("SELECT title FROM sessions WHERE id = '{A}'"));
    assert_eq!(title().as_deref(), Some("Refactor the invented checkout"));
    retention::purge(&e.store, &PurgeRequest { session: Some(A.into()), ..Default::default() }).unwrap();
    e.ing.rescan_from_scratch().unwrap();
    e.ing.backfill(&|_| {}).unwrap();
    e.append(&e.file(A), &assistant(A, "msg_A_resumed", now_ms() + MINUTE_MS));
    e.ing.scan().unwrap();
    assert_eq!(title(), None);
    e.append(&e.file(A), &format!("{{\"type\":\"ai-title\",\"aiTitle\":\"Invented new title\",\"sessionId\":\"{A}\"}}\n"));
    e.ing.scan().unwrap();
    assert_eq!(title().as_deref(), Some("Invented new title"), "a title written after the purge is kept");
}

#[test]
fn turning_titles_on_does_not_bring_back_the_title_of_a_purged_conversation() {
    let e = env();
    retention::purge(&e.store, &PurgeRequest { session: Some(A.into()), ..Default::default() }).unwrap();
    e.append(&e.file(A), &assistant(A, "msg_A_resumed", now_ms() + MINUTE_MS));
    e.ing.scan().unwrap();
    e.ing.set_store_titles(true);
    e.ing.refresh_titles().unwrap();
    assert_eq!(e.q::<Option<String>>(&format!("SELECT title FROM sessions WHERE id = '{A}'")), None);
}

#[test]
fn a_title_after_a_purge_is_dated_by_the_record_before_it_whatever_its_type() {
    let e = env();
    retention::purge(&e.store, &PurgeRequest { session: Some(A.into()), ..Default::default() }).unwrap();
    let after = now_ms() + 2 * MINUTE_MS;
    e.append(
        &e.file(A),
        &format!(
            "{{\"type\":\"api-request\",\"id\":\"req_resumed\",\"querySource\":\"repl_main_thread\",\"timestamp\":\"{}\"}}\n{{\"type\":\"ai-title\",\"aiTitle\":\"Invented resumed title\",\"sessionId\":\"{A}\"}}\n{}",
            iso(after),
            assistant(A, "msg_A_after", after + MINUTE_MS)
        ),
    );
    e.ing.scan().unwrap();
    e.ing.set_store_titles(true);
    e.ing.refresh_titles().unwrap();
    assert_eq!(
        e.q::<Option<String>>(&format!("SELECT title FROM sessions WHERE id = '{A}'")).as_deref(),
        Some("Invented resumed title"),
        "a title after the purge is dated by the record before it, whatever its type"
    );
}

#[test]
fn a_title_before_any_dated_record_is_dated_by_the_first_response_of_its_file() {
    let e = env();
    let (kept, dropped) = ("1a2b3c4d-0000-4000-8000-00000000000a", "1a2b3c4d-0000-4000-8000-00000000000b");
    let title = |s: &str| format!("{{\"type\":\"ai-title\",\"aiTitle\":\"Invented title {s}\",\"sessionId\":\"{s}\"}}\n");
    std::fs::write(e.file(dropped), format!("{}{}", title(dropped), assistant(dropped, "msg_d_1", now_ms() - MINUTE_MS))).unwrap();
    e.ing.scan().unwrap();
    retention::purge(&e.store, &PurgeRequest { everything: true, ..Default::default() }).unwrap();
    e.append(&e.file(dropped), &assistant(dropped, "msg_d_2", now_ms() + MINUTE_MS));
    std::fs::write(e.file(kept), format!("{}{}", title(kept), assistant(kept, "msg_k_1", now_ms() + MINUTE_MS))).unwrap();
    e.ing.scan().unwrap();
    e.ing.set_store_titles(true);
    e.ing.refresh_titles().unwrap();
    let stored = |s: &str| e.q::<Option<String>>(&format!("SELECT title FROM sessions WHERE id = '{s}'"));
    assert_eq!(stored(kept), Some(format!("Invented title {kept}")), "a session started after purging everything keeps its title");
    assert_eq!(stored(dropped), None, "a resumed session's first title belongs to the purged conversation");
}

#[test]
fn purging_before_a_date_keeps_that_history_out() {
    let e = env();
    let cut = 1_795_219_200_000; // 2026-11-21T00:00:00Z
    let before = e.totals();
    let old: i64 = e.q(&format!("SELECT count(*) FROM requests WHERE ts < {cut}"));
    assert!(old > 0 && old < before.0, "the fixture straddles the cut");
    retention::purge(&e.store, &PurgeRequest { before: Some(cut), ..Default::default() }).unwrap();
    let after = e.totals();
    assert_eq!(after.0, before.0 - old);
    e.read_everything_again();
    assert_eq!(e.totals(), after);
    assert_eq!(e.q::<i64>(&format!("SELECT count(*) FROM requests WHERE ts < {cut}")), 0);
}

#[test]
fn purging_everything_keeps_history_out_and_new_activity_in() {
    let e = env();
    retention::purge(&e.store, &PurgeRequest { everything: true, ..Default::default() }).unwrap();
    e.read_everything_again();
    assert_eq!(e.totals(), (0, 0, 0, 0));
    e.append(&e.file(A), &assistant(A, "msg_A_new", now_ms() + MINUTE_MS));
    e.ing.scan().unwrap();
    assert_eq!(e.totals(), (1, 1, 7, 1));
}

#[test]
fn retention_then_rescan_counts_nothing_twice() {
    let e = env();
    let before = e.totals();
    let now = 1_795_305_600_000; // 2026-11-22T00:00:00Z
    let rep = retention::apply(&e.store, &RetentionSettings { requests_days: 1, ..Default::default() }, now).unwrap();
    assert!(rep.requests > 0);
    let kept = e.totals();
    assert_eq!((kept.1, kept.2, kept.3), (before.1, before.2, before.3), "aggregates survive retention");
    e.read_everything_again();
    assert_eq!(e.totals(), kept, "removed requests are neither restored nor counted again");
}

#[test]
fn undrained_spool_lines_of_a_purged_session_are_dropped() {
    let e = env();
    let ts = now_ms();
    let dir = e.paths.spool_dir();
    let status = StatuslineRecord { ts, session_id: A.into(), project_dir: Some("/work/acme/shop".into()), ..Default::default() };
    spool::append(&dir, ts, &SpoolRecord::Statusline(status)).unwrap();
    let hook = HookRecord { ts, event: "Stop".into(), session_id: A.into(), ..Default::default() };
    spool::append(&dir, ts, &SpoolRecord::Hook(hook)).unwrap();
    retention::purge_at(&e.store, &PurgeRequest { session: Some(A.into()), ..Default::default() }, ts).unwrap();
    let r = e.ing.drain_spool().unwrap();
    assert_eq!((r.statusline, r.hooks), (0, 0));
    assert_eq!(e.q::<i64>("SELECT (SELECT count(*) FROM session_status) + (SELECT count(*) FROM session_events)"), 0);
}

#[test]
fn turning_titles_off_clears_the_stored_ones() {
    let e = env();
    e.ing.set_store_titles(true);
    e.append(&e.file(A), &format!("{{\"type\":\"ai-title\",\"aiTitle\":\"Invented title\",\"sessionId\":\"{A}\"}}\n"));
    e.ing.scan().unwrap();
    let ts = now_ms();
    let status = StatuslineRecord { ts, session_id: A.into(), session_name: Some("Invented name".into()), ..Default::default() };
    spool::append(&e.paths.spool_dir(), ts, &SpoolRecord::Statusline(status)).unwrap();
    e.ing.drain_spool().unwrap();
    let stored = "SELECT (SELECT count(*) FROM sessions WHERE title IS NOT NULL) + (SELECT count(*) FROM session_status WHERE session_name IS NOT NULL)";
    assert_eq!(e.q::<i64>(stored), 2);
    e.ing.set_store_titles(false);
    assert_eq!(e.q::<i64>(stored), 0);
    let status = StatuslineRecord { ts: ts + 1, session_id: A.into(), session_name: Some("Invented name".into()), ..Default::default() };
    spool::append(&e.paths.spool_dir(), ts + 1, &SpoolRecord::Statusline(status)).unwrap();
    e.ing.drain_spool().unwrap();
    e.ing.rescan_from_scratch().unwrap();
    e.ing.backfill(&|_| {}).unwrap();
    assert_eq!(e.q::<i64>(stored), 0, "a spooled name or a re-read ai-title is ignored while titles are off");
    assert_eq!(e.q::<i64>("SELECT count(*) FROM sessions WHERE title_user != 0"), 0);
    assert_eq!(e.q::<i64>("SELECT count(*) FROM settings WHERE key = 'titles:rescanned'"), 0);
    e.ing.set_store_titles(true);
    assert!(e.ing.refresh_titles().unwrap().contains(&A.to_string()), "turning titles on again reads them again");
}

#[test]
fn starting_with_titles_off_forgets_the_title_rescan() {
    let e = env();
    e.store.write(|tx| kanjo_core::settings::set(tx, "titles:rescanned", &true)).unwrap();
    Ingestor::new(e.store.clone(), e.paths.clone()).unwrap();
    assert_eq!(e.q::<i64>("SELECT count(*) FROM settings WHERE key = 'titles:rescanned'"), 0);
}

#[test]
fn a_price_table_imported_by_another_process_applies_to_new_requests() {
    let e = env();
    let other = Store::open(&e.dir.path().join("kanjo.db"), None, None).unwrap();
    let mut table = kanjo_core::pricing::PriceTable::embedded().clone();
    table.version = "invented-x2".into();
    for p in table.models.values_mut() {
        p.input_cost_per_token *= 2.0;
        p.output_cost_per_token *= 2.0;
    }
    kanjo_ops::pricing::import(&other, serde_json::to_string(&table).unwrap().as_bytes(), None, &[], true).unwrap();
    e.append(&e.file(B), &assistant(B, "msg_B_after_import", now_ms()));
    e.ing.scan().unwrap();
    let (version, cost): (String, f64) = e
        .store
        .read(|c| {
            Ok(c.query_row("SELECT price_version, cost_usd FROM requests WHERE message_id = 'msg_B_after_import'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?)
        })
        .unwrap();
    assert_eq!(version, "invented-x2");
    let tokens = kanjo_core::model::TokenCounts { input: 1, output: 7, ..Default::default() };
    assert!((cost - table.cost("claude-opus-5-5", &tokens).unwrap()).abs() < 1e-12);
    assert_eq!(
        e.q::<i64>("SELECT count(DISTINCT price_version) FROM requests WHERE cost_usd IS NOT NULL"),
        1,
        "one table for old and new rows"
    );
}

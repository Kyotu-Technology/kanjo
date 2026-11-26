#![allow(clippy::unwrap_used)]
//! Transcript shapes of other Claude Code versions (synthetic). The main
//! fixture (`fixtures/claude`) carries 2.1.300 records; these cover an older
//! and a newer shape so a format change shows up as a failing test.

use kanjo_core::paths::Paths;
use kanjo_core::testkit::TestStore;
use kanjo_core::Store;
use kanjo_ingest::Ingestor;
use rusqlite::types::FromSql;
use std::path::Path;
use std::sync::Arc;

struct Env {
    _dir: tempfile::TempDir,
    store: Arc<Store>,
    ing: Ingestor,
}

impl Env {
    fn q<T: FromSql>(&self, sql: &str) -> T {
        self.store.read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap()
    }

    fn rows(&self) -> String {
        self.q(
            "SELECT group_concat(row, ' ') FROM (SELECT message_id || ':' || model || ':' || input || '/' || output || '/' || cache_read || '/'
               || cache_write_5m || '/' || cache_write_1h || ':' || COALESCE(agent_type, '-') || ':' || is_sidechain AS row
             FROM requests ORDER BY message_id)",
        )
    }
}

fn import(root: &str) -> Env {
    let TestStore { store, dir } = TestStore::new();
    let claude = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(root);
    let store = Arc::new(store);
    let ing = Ingestor::new(store.clone(), Paths::at(dir.path().join("home"), claude)).unwrap();
    let st = ing.backfill(&|_| {}).unwrap();
    assert_eq!(st.files_total, 1);
    Env { _dir: dir, store, ing }
}

fn rescan_changes_nothing(e: &Env) {
    let before = (e.rows(), e.q::<i64>("SELECT SUM(requests) FROM usage_hourly"), e.q::<f64>("SELECT SUM(cost_usd) FROM sessions"));
    e.ing.rescan_from_scratch().unwrap();
    e.ing.backfill(&|_| {}).unwrap();
    let after = (e.rows(), e.q::<i64>("SELECT SUM(requests) FROM usage_hourly"), e.q::<f64>("SELECT SUM(cost_usd) FROM sessions"));
    assert_eq!(before, after);
}

#[test]
fn claude_code_1_0_flat_cache_writes_and_sidechains_in_the_main_file() {
    let e = import("cc-1.0");
    assert_eq!(
        e.rows(),
        "msg_L1:claude-sonnet-4-5-20250929:4/120/0/2000/0:-:0 msg_L2:claude-haiku-4-5-20251001:30/80/1500/0/0:subagent:1",
        "flat cache_creation_input_tokens count as 5-minute writes; the completed streaming record wins"
    );
    assert_eq!(
        e.q::<String>("SELECT group_concat(kind || ':' || name, ',') FROM (SELECT * FROM request_tools ORDER BY kind)"),
        "agent:general-purpose,tool:Task"
    );
    assert_eq!(e.q::<i64>("SELECT unpriced_requests FROM sessions"), 0, "dated model names are priced");
    assert_eq!(e.q::<String>("SELECT git_branch || '|' || cc_version FROM sessions"), "develop|1.0.85");
    assert_eq!(e.q::<Option<String>>("SELECT query_source FROM requests WHERE message_id = 'msg_L1'"), None, "no api-request records yet");
    assert_eq!(e.q::<i64>("SELECT count(*) FROM quarantine"), 0, "costUSD, durationMs and summary records are tolerated");
    rescan_changes_nothing(&e);
}

#[test]
fn claude_code_2_4_new_fields_and_record_types() {
    let e = import("cc-2.4");
    assert_eq!(
        e.rows(),
        "msg_N1:claude-opus-5-5:12/40/9000/0/700:-:0 nu-2:claude-sonnet-5-5:5/9/0/0/0:invented-reviewer:1",
        "a response without a message id is keyed by its record uuid"
    );
    let meta: String = e
        .q("SELECT query_source || '|' || effort || '|' || speed || '|' || service_tier || '|' || web_fetch || '|' || git_branch
         FROM requests WHERE message_id = 'msg_N1'");
    assert_eq!(meta, "compact|max|fast|priority|2|feature/invented");
    assert_eq!(
        e.q::<String>("SELECT group_concat(kind || ':' || name, ',') FROM (SELECT * FROM request_tools ORDER BY kind)"),
        "mcp:plugin_invented_tracker"
    );
    assert_eq!(e.q::<String>("SELECT account || '|' || entrypoint FROM sessions"), "acct-0009|sdk-ts");
    assert_eq!(
        e.q::<String>("SELECT record_type || '|' || reason || '|' || cc_version FROM quarantine"),
        "plan-snapshot|unknown record type|2.4.12"
    );
    rescan_changes_nothing(&e);
}

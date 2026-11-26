#![allow(clippy::unwrap_used)]

use kanjo_core::paths::Paths;
use kanjo_core::spool::{self, HookRecord, SpoolRecord};
use kanjo_core::statusline::{CacheRecord, StatuslineRecord, WindowRecord};
use kanjo_core::store::write;
use kanjo_core::testkit::TestStore;
use kanjo_core::time::{now_ms, DAY_MS};
use kanjo_core::Store;
use kanjo_ingest::{read_transcript, Ingestor};
use rusqlite::types::FromSql;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const A: &str = "0b6a3c1e-5d2f-4a8b-9c7d-1e2f3a4b5c6d";
const B: &str = "7f1e2d3c-4b5a-4968-8776-655443322110";
const PROJECT: &str = "-work-acme-shop";

struct Env {
    _dir: tempfile::TempDir,
    store: Arc<Store>,
    paths: Paths,
    ing: Arc<Ingestor>,
}

impl Env {
    fn q<T: FromSql>(&self, sql: &str) -> T {
        self.store.read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap()
    }

    fn project_dir(&self) -> PathBuf {
        self.paths.claude_projects().join(PROJECT)
    }

    fn append(&self, file: &Path, text: &str) {
        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(file).unwrap().write_all(text.as_bytes()).unwrap();
    }
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

fn env_with(fixture: bool) -> Env {
    let TestStore { store, dir } = TestStore::new();
    let claude = dir.path().join("claude");
    std::fs::create_dir_all(claude.join("projects")).unwrap();
    if fixture {
        copy_tree(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude"), &claude);
    }
    let paths = Paths::at(dir.path().join("home"), &claude);
    let store = Arc::new(store);
    let ing = Arc::new(Ingestor::new(store.clone(), paths.clone()).unwrap());
    Env { _dir: dir, store, paths, ing }
}

fn env() -> Env {
    env_with(true)
}

fn assistant(session: &str, id: &str, ts: &str, output: i64) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"{session}","cwd":"/work/acme/shop","version":"2.1.300","timestamp":"{ts}","uuid":"u-{id}","message":{{"id":"{id}","model":"claude-opus-5-5","content":[{{"type":"text","text":"invented"}}],"usage":{{"input_tokens":1,"output_tokens":{output}}}}}}}"#
    )
}

#[test]
fn backfill_imports_the_fixture() {
    let e = env();
    let seen = std::sync::Mutex::new(Vec::new());
    let st = e.ing.backfill(&|s| seen.lock().unwrap().push(s.files_done)).unwrap();
    assert!(!st.backfill_running);
    assert_eq!((st.files_total, st.files_done), (4, 4), "journal.jsonl is not a transcript");
    assert_eq!(st.requests_stored, 5);
    assert!(!seen.lock().unwrap().is_empty());

    let ids: String = e.q("SELECT group_concat(message_id, ',') FROM (SELECT message_id FROM requests ORDER BY message_id)");
    assert_eq!(ids, "msg_A1,msg_A2,msg_B1,msg_S1,msg_S2");
    let tools: String = e.q("SELECT group_concat(kind || ':' || name, ',') FROM (SELECT t.kind, t.name FROM request_tools t
         JOIN requests r ON r.id = t.request_id WHERE r.message_id = 'msg_A1' ORDER BY t.use_id, t.kind)");
    assert_eq!(tools, "skill:commit,tool:Skill,mcp:tracker");
    let a1: (i64, i64, i64, i64, i64, i64) = e
        .store
        .read(|c| {
            Ok(c.query_row(
                "SELECT input, output, cache_read, cache_write_5m, cache_write_1h, web_search FROM requests WHERE message_id = 'msg_A1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )?)
        })
        .unwrap();
    assert_eq!(a1, (10, 200, 5000, 100, 200, 1));
    let meta: String = e
        .q("SELECT query_source || '|' || effort || '|' || service_tier || '|' || request_id || '|' || stop_reason || '|' || git_branch
         FROM requests WHERE message_id = 'msg_A1'");
    assert_eq!(meta, "repl_main_thread|high|standard|req_A1|tool_use|main");

    let a2: (i64, i64, String) = e
        .store
        .read(|c| {
            Ok(c.query_row("SELECT cache_write_5m, cache_write_1h, session_id FROM requests WHERE message_id = 'msg_A2'", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?)
        })
        .unwrap();
    assert_eq!(a2, (1000, 0, A.to_string()), "missing breakdown counts as 5-minute writes");
    let a2_tools: String = e.q("SELECT group_concat(kind || ':' || name, ',') FROM (SELECT t.kind, t.name FROM request_tools t
         JOIN requests r ON r.id = t.request_id WHERE r.message_id = 'msg_A2' ORDER BY t.kind)");
    assert_eq!(a2_tools, "agent:Explore,tool:Task");

    let subs: String = e
        .q("SELECT group_concat(message_id || ':' || agent_type || ':' || agent_id || ':' || is_sidechain || ':' || session_id, ',')
         FROM (SELECT * FROM requests WHERE message_id LIKE 'msg_S%' ORDER BY message_id)");
    assert_eq!(subs, format!("msg_S1:Explore:a1:1:{A},msg_S2:reviewer:b2:1:{A}"));

    let session: (i64, i64, Option<String>, Option<String>) = e
        .store
        .read(|c| {
            Ok(c.query_row(&format!("SELECT requests, sidechain_requests, account, title FROM sessions WHERE id = '{A}'"), [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?)
        })
        .unwrap();
    assert_eq!(session, (4, 2, Some("acct-0001".into()), None), "titles are off by default");
    assert_eq!(e.q::<i64>(&format!("SELECT requests FROM sessions WHERE id = '{B}'")), 1);
    assert_eq!(e.q::<i64>("SELECT SUM(requests) FROM usage_hourly"), 5);

    let project: String = e.q("SELECT key || '|' || path || '|' || name FROM projects");
    assert_eq!(project, "-work-acme-shop|/work/acme/shop|shop");

    let quarantine: String = e
        .q("SELECT group_concat(record_type || '|' || signature || '|' || reason || '|' || cc_version || '|' || count, ';')
         FROM (SELECT * FROM quarantine ORDER BY record_type)");
    assert_eq!(
        quarantine,
        "assistant|cwd,gitBranch,isSidechain,message,parentUuid,requestId,sessionId,timestamp,type,userType,uuid,version|usage does not deserialize|2.1.300|1;\
         future-thing|sessionId,type,widget|unknown record type||1"
    );

    let changes: String = e.q("SELECT group_concat(kind || ':' || target, ',') FROM config_changes");
    assert_eq!(changes, format!("tools:{A}"));
    let cursors: i64 = e.q("SELECT count(*) FROM ingest_files WHERE path LIKE '%journal%'");
    assert_eq!(cursors, 0);
}

#[test]
fn second_scan_reads_nothing_and_rescan_does_not_duplicate() {
    let e = env();
    e.ing.backfill(&|_| {}).unwrap();
    let again = e.ing.scan().unwrap();
    assert_eq!((again.files, again.inserted, again.updated), (0, 0, 0));
    e.ing.rescan_from_scratch().unwrap();
    let st = e.ing.backfill(&|_| {}).unwrap();
    assert_eq!(st.files_done, 4);
    assert_eq!(e.q::<i64>("SELECT count(*) FROM requests"), 5);
    assert_eq!(e.q::<i64>("SELECT SUM(requests) FROM usage_hourly"), 5);
    assert_eq!(e.q::<i64>("SELECT count(*) FROM config_changes"), 1, "tool changes are not replayed");
    assert_eq!(e.q::<i64>("SELECT count(*) FROM request_tools"), 6);
}

#[test]
fn partial_last_line_waits_for_completion() {
    let e = env_with(false);
    let dir = e.paths.claude_projects().join("-work-other");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("c0c0c0c0-0000-4000-8000-000000000001.jsonl");
    let second = assistant("c0", "msg_C2", "2026-11-22T08:00:01Z", 5);
    let (head, tail) = second.split_at(40);
    std::fs::write(&file, format!("{}\n{head}", assistant("c0", "msg_C1", "2026-11-22T08:00:00Z", 5))).unwrap();
    let r = e.ing.scan().unwrap();
    assert_eq!((r.files, r.inserted), (1, 1));
    assert_eq!(e.q::<i64>("SELECT count(*) FROM quarantine"), 0, "a partial line is not malformed");
    e.append(&file, &format!("{tail}\n"));
    let r = e.ing.scan().unwrap();
    assert_eq!((r.files, r.inserted), (1, 1));
    assert_eq!(r.sessions, vec!["c0".to_string()]);
    assert_eq!(e.q::<i64>("SELECT count(*) FROM requests"), 2);
    let size = std::fs::metadata(&file).unwrap().len() as i64;
    assert_eq!(e.q::<i64>("SELECT \"offset\" FROM ingest_files"), size);
}

#[test]
fn session_metadata_before_the_first_request_is_kept() {
    let e = env_with(false);
    e.ing.set_store_titles(true);
    let dir = e.paths.claude_projects().join("-work-other");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("e0.jsonl");
    std::fs::write(
        &file,
        "{\"type\":\"session-owner\",\"accountUuid\":\"acct-0002\",\"sessionId\":\"e0\"}\n{\"type\":\"ai-title\",\"aiTitle\":\"Invented\",\"sessionId\":\"e0\"}\n",
    )
    .unwrap();
    e.ing.scan().unwrap();
    assert_eq!(e.q::<i64>("SELECT count(*) FROM sessions"), 0);
    e.append(&file, &format!("{}\n", assistant("e0", "msg_E1", "2026-11-22T08:00:00Z", 5)));
    e.ing.scan().unwrap();
    assert_eq!(e.q::<String>("SELECT account || '|' || title FROM sessions WHERE id = 'e0'"), "acct-0002|Invented");
}

#[test]
fn shrunk_file_restarts_from_zero() {
    let e = env();
    e.ing.backfill(&|_| {}).unwrap();
    let file = e.project_dir().join(format!("{B}.jsonl"));
    std::fs::write(&file, format!("{}\n", assistant(B, "msg_B9", "2026-11-23T08:00:00Z", 3))).unwrap();
    let r = e.ing.scan().unwrap();
    assert_eq!((r.files, r.inserted), (1, 1));
    assert_eq!(e.q::<i64>("SELECT count(*) FROM requests"), 6);
}

#[test]
fn streamed_record_completed_later_updates_totals() {
    let e = env_with(false);
    let dir = e.paths.claude_projects().join("-work-other");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("d0.jsonl");
    std::fs::write(&file, format!("{}\n", assistant("d0", "msg_D1", "2026-11-22T08:00:00Z", 5))).unwrap();
    e.ing.ingest_file(&file).unwrap();
    e.append(&file, &format!("{}\n", assistant("d0", "msg_D1", "2026-11-22T08:00:02Z", 50)));
    let r = e.ing.ingest_file(&file).unwrap();
    assert_eq!((r.inserted, r.updated), (0, 1));
    assert_eq!(e.q::<i64>("SELECT output FROM sessions"), 50);
}

#[test]
fn ingest_file_refuses_paths_outside_projects() {
    let e = env();
    let creds = e.paths.claude_dir.join(".credentials.json");
    std::fs::write(&creds, "{}").unwrap();
    assert!(e.ing.ingest_file(&creds).is_err());
    let journal = e.project_dir().join(A).join("subagents/workflows/wf-1/journal.jsonl");
    assert!(e.ing.ingest_file(&journal).is_err());
    let sneaky = e.project_dir().join("..").join("..").join(".credentials.json");
    assert!(e.ing.ingest_file(&sneaky).is_err());
}

#[test]
fn titles_are_stored_only_when_enabled() {
    let e = env();
    e.ing.backfill(&|_| {}).unwrap();
    let title = || e.q::<Option<String>>(&format!("SELECT title FROM sessions WHERE id = '{A}'"));
    assert_eq!(title(), None);
    e.ing.set_store_titles(true);
    let file = e.project_dir().join(format!("{A}.jsonl"));
    e.append(&file, &format!("{{\"type\":\"ai-title\",\"aiTitle\":\"Invented title\",\"sessionId\":\"{A}\"}}\n"));
    e.ing.scan().unwrap();
    assert_eq!(title(), Some("Invented title".into()));
}

fn statusline(session: &str, ts: i64, name: &str, miss_at: i64) -> SpoolRecord {
    SpoolRecord::Statusline(StatuslineRecord {
        ts,
        session_id: session.into(),
        session_name: Some(name.into()),
        model_id: Some("claude-opus-5-5".into()),
        cost_usd: Some(1.5),
        ctx_used_pct: Some(12.0),
        five_hour: Some(WindowRecord { used_pct: 20.0, resets_at: ts + 3_600_000, ..Default::default() }),
        seven_day: Some(WindowRecord { used_pct: 40.0, resets_at: ts + DAY_MS, ..Default::default() }),
        cache: Some(CacheRecord {
            misses: Some(1),
            last_miss_at: Some(miss_at),
            last_miss_causes: vec!["tools_changed".into()],
            tools_added: Some(2),
            miss_causes: [("tools_changed".to_string(), 1)].into_iter().collect(),
            ..Default::default()
        }),
        ..Default::default()
    })
}

#[test]
fn spool_drain_applies_and_cleans_up() {
    let e = env();
    e.ing.backfill(&|_| {}).unwrap();
    let dir = e.paths.spool_dir();
    let now = now_ms();
    let old = now - 2 * DAY_MS;
    spool::append(&dir, old, &statusline(A, old, "Invented name", old - 1000)).unwrap();
    spool::append(&dir, now, &statusline(A, now, "Invented name", old - 1000)).unwrap();
    let hook =
        HookRecord { ts: now, event: "SessionStart".into(), session_id: A.into(), source: Some("startup".into()), ..Default::default() };
    spool::append(&dir, now, &SpoolRecord::Hook(hook)).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(spool::file_for(&dir, now))
        .map(|mut f| {
            use std::io::Write;
            f.write_all(b"{\"kind\":\"hook\",\"ts\":1").unwrap()
        })
        .unwrap();

    let r = e.ing.drain_spool().unwrap();
    assert_eq!((r.statusline, r.hooks, r.limits_changed), (2, 1, true));
    assert_eq!(r.sessions, vec![A.to_string()]);
    assert_eq!(e.q::<i64>("SELECT count(*) FROM limit_samples"), 4);
    assert_eq!(e.q::<i64>("SELECT count(*) FROM cache_misses"), 1, "the same miss is stored once");
    assert_eq!(e.q::<String>("SELECT causes FROM cache_misses"), r#"["tools_changed"]"#);
    let status: (i64, Option<String>, String) = e
        .store
        .read(|c| {
            Ok(c.query_row("SELECT ts, session_name, cache_miss_causes FROM session_status", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?)
        })
        .unwrap();
    assert_eq!(status, (now, None, r#"{"tools_changed":1}"#.to_string()), "session names need titles enabled");
    assert_eq!(e.q::<String>("SELECT event || ':' || source FROM session_events"), "SessionStart:startup");
    assert_eq!(e.q::<Option<String>>(&format!("SELECT title FROM sessions WHERE id = '{A}'")), None);
    assert!(!spool::file_for(&dir, old).exists(), "a consumed earlier day is deleted");
    assert!(spool::file_for(&dir, now).exists());

    let again = e.ing.drain_spool().unwrap();
    assert_eq!((again.statusline, again.hooks), (0, 0));
    assert_eq!(e.q::<i64>("SELECT count(*) FROM session_events"), 1);
}

#[test]
fn spool_titles_when_enabled() {
    let e = env();
    e.ing.backfill(&|_| {}).unwrap();
    e.ing.set_store_titles(true);
    let now = now_ms();
    spool::append(&e.paths.spool_dir(), now, &statusline(A, now, "Invented name", now - 5)).unwrap();
    let r = e.ing.drain_spool().unwrap();
    assert_eq!(r.statusline, 1);
    assert_eq!(e.q::<Option<String>>("SELECT session_name FROM session_status"), Some("Invented name".into()));
    assert_eq!(e.q::<Option<String>>(&format!("SELECT title FROM sessions WHERE id = '{A}'")), Some("Invented name".into()));
}

#[test]
fn config_fingerprints_baseline_then_changes() {
    let e = env_with(false);
    let claude = e.paths.claude_dir.clone();
    std::fs::write(claude.join("CLAUDE.md"), "# invented rules\n").unwrap();
    std::fs::write(claude.join("settings.json"), r#"{"theme":"dark","enabledPlugins":{"a@b":true}}"#).unwrap();
    let project = e._dir.path().join("work/acme");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("CLAUDE.md"), "project rules\n").unwrap();
    let project_id = e.store.write(|tx| write::upsert_project(tx, "-work-acme", project.to_str(), now_ms())).unwrap();
    e.store.write(|tx| write::upsert_project(tx, "-work-stale", Some("/nonexistent/stale"), now_ms() - 30 * DAY_MS)).unwrap();

    assert_eq!(e.ing.check_config_files().unwrap(), 0, "first sighting is a baseline");
    assert_eq!(e.ing.check_config_files().unwrap(), 0);
    let stored: String = e.q("SELECT group_concat(key, ',') FROM (SELECT key FROM settings WHERE key LIKE 'fingerprint:%' ORDER BY key)");
    let alias = e.store.read(|c| write::project_alias(c, "-work-acme")).unwrap();
    assert_eq!(
        stored,
        format!("fingerprint:claude_md:{alias},fingerprint:claude_md:user CLAUDE.md,fingerprint:plugins:enabledPlugins,fingerprint:settings:user settings.json")
    );
    let raw: String = e.q("SELECT group_concat(value, ' ') FROM settings");
    assert!(!raw.contains("invented rules") && !raw.contains("a@b"), "content is never stored");

    std::fs::write(claude.join("CLAUDE.md"), "# invented rules, edited\n").unwrap();
    assert_eq!(e.ing.check_config_files().unwrap(), 1);
    std::fs::write(claude.join("settings.json"), r#"{"enabledPlugins":{"a@b":true},"theme":"light"}"#).unwrap();
    assert_eq!(e.ing.check_config_files().unwrap(), 1, "settings changed, plugins did not");
    std::fs::write(claude.join("settings.json"), r#"{"enabledPlugins":{"a@b":false},"theme":"light"}"#).unwrap();
    assert_eq!(e.ing.check_config_files().unwrap(), 2);
    std::fs::write(project.join("CLAUDE.md"), "project rules v2, longer\n").unwrap();
    assert_eq!(e.ing.check_config_files().unwrap(), 1);
    std::fs::remove_file(claude.join("CLAUDE.md")).unwrap();
    assert_eq!(e.ing.check_config_files().unwrap(), 1);

    let rows: String = e.q(
        "SELECT group_concat(kind || ':' || target || ':' || COALESCE(project_id, 0) || ':' || length(fingerprint), ',') FROM (SELECT * FROM config_changes ORDER BY id)",
    );
    assert_eq!(
        rows,
        format!(
            "claude_md:user CLAUDE.md:0:64,settings:user settings.json:0:64,settings:user settings.json:0:64,plugins:enabledPlugins:0:64,claude_md:{alias}:{project_id}:64,claude_md:user CLAUDE.md:0:7"
        )
    );
}

#[test]
fn transcript_viewer_reads_text_on_demand() {
    let e = env();
    e.ing.backfill(&|_| {}).unwrap();
    let file = e.project_dir().join(format!("{A}.jsonl"));
    let long = "x".repeat(5000);
    e.append(
        &file,
        &format!(
            r#"{{"type":"user","sessionId":"{A}","timestamp":"2026-11-20T11:00:00Z","message":{{"role":"user","content":[{{"type":"text","text":"{long}"}}]}}}}"#
        ),
    );
    e.append(&file, "\n");
    let rows = "SELECT (SELECT count(*) FROM settings) + (SELECT count(*) FROM ingest_files) + (SELECT count(*) FROM quarantine)";
    let before: i64 = e.q(rows);
    let t = read_transcript(&e.store, &e.paths, A, 100).unwrap();
    assert!(!t.truncated);
    let kinds: Vec<String> = t.messages.iter().map(|m| format!("{}:{}:{}", m.role, m.kind, m.tool.clone().unwrap_or_default())).collect();
    assert_eq!(
        kinds,
        [
            "user:text:",
            "assistant:thinking:",
            "assistant:tool_use:Skill",
            "assistant:tool_use:mcp__tracker__create_issue",
            "assistant:tool_use:Task",
            "assistant:text:",
            "assistant:text:",
            "user:text:"
        ]
    );
    assert_eq!(t.messages[1].model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(t.messages[5].model, None, "synthetic placeholders carry no model");
    assert_eq!(t.messages.last().unwrap().text.chars().count(), 4000);
    let short = read_transcript(&e.store, &e.paths, A, 2).unwrap();
    assert!(short.truncated && short.messages.len() == 2);
    assert!(read_transcript(&e.store, &e.paths, "no-such-session", 10).is_err());
    assert_eq!(e.q::<i64>(rows), before, "the viewer stores nothing");
    for f in ["kanjo.db", "kanjo.db-wal"] {
        let bytes = std::fs::read(e._dir.path().join(f)).unwrap_or_default();
        assert!(!bytes.windows(64).any(|w| w.iter().all(|b| *b == b'x')), "no conversation text in {f}");
        assert!(!bytes.windows(15).any(|w| w == b"invented commit"), "no text in {f}");
    }
}

#[test]
fn watcher_picks_up_appends() {
    let e = env();
    e.ing.backfill(&|_| {}).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let handle = e
        .ing
        .watch(Box::new(move |r| {
            let _ = tx.lock().unwrap().send(r);
        }))
        .unwrap();
    assert!(e.ing.status().watcher_running);
    std::thread::sleep(Duration::from_millis(200));
    let file = e.project_dir().join(format!("{B}.jsonl"));
    e.append(&file, &format!("{}\n", assistant(B, "msg_B5", "2026-11-23T09:00:00Z", 9)));
    let report = rx.recv_timeout(Duration::from_secs(10)).expect("watcher reported the change");
    assert_eq!(report.inserted, 1);
    drop(handle);
    assert!(!e.ing.status().watcher_running);
}

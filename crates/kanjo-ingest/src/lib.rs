//! kanjo-ingest: turns Claude Code transcripts and the statusline/hook spool
//! into rows in the Kanjo store.
//!
//! Transcripts are read incrementally from `<claude>/projects` with a byte
//! cursor per file (`ingest_files`). Only usage numbers and metadata are
//! extracted; conversation text is read solely by [`read_transcript`], on
//! demand, and never stored.

mod config_files;
#[doc(hidden)]
pub mod diagnostics;
mod discover;
mod pipeline;
mod record;
mod spool_drain;
mod toplevel;
mod transcript;
mod watch;

pub use kanjo_core::api::{IngestStatus, Transcript, TranscriptMessage};
pub use transcript::{read_transcript, MAX_TEXT_CHARS};
pub use watch::WatchHandle;

use kanjo_core::paths::Paths;
use kanjo_core::pricing::{self, PriceTable};
use kanjo_core::settings::Settings;
use kanjo_core::{Error, Result, Store};
use pipeline::Job;
use record::QueryRefs;
use rusqlite::OptionalExtension;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

/// Outcome of one transcript pass.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanReport {
    /// Files that had new complete lines.
    pub files: i64,
    /// Responses stored for the first time.
    pub inserted: i64,
    /// Known responses whose usage grew (a streamed record completed later).
    pub updated: i64,
    /// List-price cost of the newly inserted responses; unpriced ones add nothing.
    pub cost_usd: f64,
    /// `config_changes` rows written because a session's tool set changed.
    pub config_changes: i64,
    /// Records counted into the quarantine.
    pub quarantined: i64,
    /// Sessions with new or updated requests, sorted.
    pub sessions: Vec<String>,
}

/// Outcome of one spool drain.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpoolReport {
    /// Statusline snapshots applied.
    pub statusline: i64,
    /// Hook markers stored.
    pub hooks: i64,
    /// At least one new limit sample was stored.
    pub limits_changed: bool,
    /// Sessions whose status snapshot or events changed, sorted.
    pub sessions: Vec<String>,
}

/// The collector. Share it behind an `Arc`; every method takes `&self` and
/// transcript passes are serialised internally.
pub struct Ingestor {
    store: Arc<Store>,
    paths: Paths,
    prices: RwLock<Arc<PriceTable>>,
    titles: AtomicBool,
    status: Mutex<IngestStatus>,
    refs: Mutex<HashMap<PathBuf, QueryRefs>>,
    pending_meta: Mutex<HashMap<String, pipeline::PendingMeta>>,
    pass: Mutex<()>,
    spool: Mutex<()>,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

const QUERY_REFS_KEPT: usize = 64;

impl Ingestor {
    /// Loads the active price table and the session-title setting from the store.
    pub fn new(store: Arc<Store>, paths: Paths) -> Result<Ingestor> {
        let prices = store.read(pricing::active)?;
        let titles = Settings::load(&store)?.privacy.session_titles;
        let stored: i64 = store.read(|c| Ok(c.query_row("SELECT count(*) FROM requests", [], |r| r.get(0))?))?;
        Ok(Ingestor {
            store,
            paths,
            prices: RwLock::new(Arc::new(prices)),
            titles: AtomicBool::new(titles),
            status: Mutex::new(IngestStatus { requests_stored: stored, ..IngestStatus::default() }),
            refs: Mutex::new(HashMap::new()),
            pending_meta: Mutex::new(HashMap::new()),
            pass: Mutex::new(()),
            spool: Mutex::new(()),
        })
    }

    /// Re-reads the active price table; new rows are priced with it. On failure the old table stays.
    pub fn reload_prices(&self) {
        match self.store.read(pricing::active) {
            Ok(table) => *self.prices.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(table),
            Err(e) => tracing::warn!(error = %e, "price table could not be reloaded"),
        }
    }

    /// Whether session titles (from `ai-title` records and the statusline) are stored. Off by default.
    pub fn set_store_titles(&self, on: bool) {
        self.titles.store(on, Ordering::Relaxed);
    }

    pub(crate) fn prices(&self) -> Arc<PriceTable> {
        self.prices.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Imports every transcript not read yet (all of them on first start), in
    /// parallel. `progress` is called after each committed batch.
    pub fn backfill(&self, progress: &(dyn Fn(&IngestStatus) + Sync)) -> Result<IngestStatus> {
        let _pass = lock(&self.pass);
        let jobs = self.plan_all()?;
        {
            let mut st = lock(&self.status);
            st.backfill_running = true;
            st.files_total = jobs.len() as i64;
            st.bytes_total = jobs.iter().map(|j| (j.size - j.start) as i64).sum();
            st.files_done = 0;
            st.bytes_done = 0;
        }
        progress(&self.status());
        let result = self.run(jobs, Some(progress));
        lock(&self.status).backfill_running = false;
        result?;
        let st = self.status();
        progress(&st);
        Ok(st)
    }

    /// Incremental pass: stats every transcript and reads only files whose
    /// size or mtime moved past their cursor.
    pub fn scan(&self) -> Result<ScanReport> {
        let _pass = lock(&self.pass);
        let jobs = self.plan_all()?;
        self.run(jobs, None)
    }

    fn plan_all(&self) -> Result<Vec<Job>> {
        let found = discover::walk(&self.paths.claude_projects());
        let cursors = self.store.read(|c| pipeline::cursors(c, &["transcript", "subagent"]))?;
        Ok(pipeline::plan(found, &cursors))
    }

    /// Reads one transcript from its cursor. The path must be a transcript
    /// inside `<claude>/projects`.
    pub fn ingest_file(&self, path: &Path) -> Result<ScanReport> {
        let projects = self.paths.claude_projects();
        let target = discover::classify(&projects, path)
            .filter(|_| discover::inside(&projects, path))
            .ok_or_else(|| Error::Invalid("not a transcript inside the Claude projects directory".into()))?;
        let meta = std::fs::symlink_metadata(path)?;
        if !meta.is_file() {
            return Err(Error::Invalid("not a regular file".into()));
        }
        let _pass = lock(&self.pass);
        let key = path.to_string_lossy().into_owned();
        let cursor = self.store.read(|c| {
            Ok(c.query_row("SELECT size, \"offset\", mtime FROM ingest_files WHERE path = ?1", [&key], |r| {
                Ok(pipeline::Cursor { size: r.get::<_, i64>(0)? as u64, offset: r.get::<_, i64>(1)? as u64, mtime: r.get(2)? })
            })
            .optional()?)
        })?;
        let (size, mtime) = (meta.len(), discover::mtime_ms(&meta));
        let start = pipeline::start_for(cursor.as_ref(), size, mtime, true).unwrap_or(0);
        self.run(vec![Job { target, start, size, mtime }], None)
    }

    pub fn status(&self) -> IngestStatus {
        lock(&self.status).clone()
    }

    pub(crate) fn set_watcher_running(&self, on: bool) {
        lock(&self.status).watcher_running = on;
    }

    pub(crate) fn note_written(&self, files: i64, bytes: i64, inserted: i64, backfill: bool) {
        let mut st = lock(&self.status);
        if backfill {
            st.files_done += files;
            st.bytes_done += bytes;
        }
        st.requests_stored += inserted;
        if inserted > 0 {
            st.last_event_at = Some(kanjo_core::time::now_ms());
        }
    }

    /// Forgets every transcript cursor, so the next backfill reads all history
    /// again. Stored requests stay; re-read records deduplicate.
    pub fn rescan_from_scratch(&self) -> Result<()> {
        let _pass = lock(&self.pass);
        self.store.write(|tx| Ok(tx.execute("DELETE FROM ingest_files WHERE kind IN ('transcript', 'subagent')", [])?))?;
        lock(&self.refs).clear();
        Ok(())
    }

    pub(crate) fn take_refs(&self, path: &Path, restart: bool) -> QueryRefs {
        let refs = lock(&self.refs).remove(path);
        if restart {
            QueryRefs::default()
        } else {
            refs.unwrap_or_default()
        }
    }

    /// Keeps the newest `api-request` ids of recently written files for the next pass.
    pub(crate) fn put_refs(&self, path: &Path, mut refs: QueryRefs, mtime: i64) {
        if kanjo_core::time::now_ms() - mtime < kanjo_core::time::DAY_MS {
            refs.trim(QUERY_REFS_KEPT);
            lock(&self.refs).insert(path.to_path_buf(), refs);
        }
    }
}

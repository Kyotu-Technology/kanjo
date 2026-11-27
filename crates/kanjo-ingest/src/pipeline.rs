//! The transcript pass: plan from cursors, parse on worker threads, write
//! through one writer in batched transactions.

use crate::discover::{self, FileKind, Found, Target};
use crate::record::{self, Chunk, FileCtx, QueryRefs};
use crate::{Ingestor, ScanReport};
use kanjo_core::model::InsertOutcome;
use kanjo_core::pricing::{self, PriceTable};
use kanjo_core::store::write;
use kanjo_core::{settings, Result, Store};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

const BATCH_ROWS: usize = 20_000;
const BATCH_FILES: usize = 256;
const READ_BUFFER: usize = 1 << 18;

#[derive(Debug, Clone)]
pub(crate) struct Job {
    pub target: Target,
    pub start: u64,
    pub size: u64,
    pub mtime: i64,
}

pub(crate) struct Parsed {
    pub target: Target,
    pub size: u64,
    pub mtime: i64,
    pub offset: u64,
    pub bytes: u64,
    pub chunk: Chunk,
}

pub(crate) struct Cursor {
    pub size: u64,
    pub offset: u64,
    pub mtime: i64,
}

pub(crate) fn cursors(conn: &Connection, kinds: &[&str]) -> Result<HashMap<String, Cursor>> {
    let mut stmt = conn.prepare("SELECT path, kind, size, \"offset\", mtime FROM ingest_files")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            Cursor { size: r.get::<_, i64>(2)? as u64, offset: r.get::<_, i64>(3)? as u64, mtime: r.get(4)? },
        ))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (path, kind, c) = row?;
        if kinds.contains(&kind.as_str()) {
            out.insert(path, c);
        }
    }
    Ok(out)
}

/// Start offset for a file given its cursor: `None` when nothing moved.
pub(crate) fn start_for(cursor: Option<&Cursor>, size: u64, mtime: i64, force: bool) -> Option<u64> {
    match cursor {
        None => Some(0),
        Some(c) if size < c.size || size < c.offset => Some(0),
        Some(c) if !force && c.size == size && c.mtime == mtime => None,
        Some(c) => Some(c.offset),
    }
}

pub(crate) fn plan(found: Vec<Found>, cursors: &HashMap<String, Cursor>) -> Vec<Job> {
    found
        .into_iter()
        .filter_map(|f| {
            let key = f.target.path.to_string_lossy();
            let start = start_for(cursors.get(key.as_ref()), f.size, f.mtime, false)?;
            Some(Job { target: f.target, start, size: f.size, mtime: f.mtime })
        })
        .collect()
}

pub(crate) type LineFn = fn(&[u8], &mut FileCtx, &mut Chunk);

pub(crate) fn parse_job(job: &Job, refs: &mut QueryRefs, titles: bool, line: LineFn) -> Result<Parsed> {
    let subagent = job.target.kind == FileKind::Subagent;
    let mut ctx = FileCtx {
        subagent,
        session_id: job.target.session_id.clone(),
        agent_id: job.target.agent_id.clone(),
        agent_type: if subagent { discover::meta_agent_type(&job.target.path) } else { None },
        titles,
        refs,
        offset: job.start,
        last_ts: 0,
    };
    let mut chunk = Chunk::default();
    let mut offset = job.start;
    if job.start < job.size {
        let mut file = File::open(&job.target.path)?;
        file.seek(SeekFrom::Start(job.start))?;
        let mut reader = BufReader::with_capacity(READ_BUFFER, file.take(job.size - job.start));
        let mut buf = Vec::with_capacity(1 << 16);
        loop {
            buf.clear();
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 || buf[n - 1] != b'\n' {
                break;
            }
            ctx.offset = offset;
            offset += n as u64;
            line(&buf[..n - 1], &mut ctx, &mut chunk);
            if buf.capacity() > 1 << 24 {
                buf = Vec::with_capacity(1 << 16);
            }
        }
    }
    Ok(Parsed { target: job.target.clone(), size: job.size, mtime: job.mtime, offset, bytes: offset - job.start, chunk })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ToolsState {
    fingerprint: String,
    /// Position of the last `tools-used` record applied, in the session's main transcript.
    offset: u64,
}

#[derive(Default)]
struct Written {
    files: i64,
    bytes: i64,
    inserted: i64,
}

struct Writer<'a> {
    store: &'a Store,
    stamp: Option<(String, i64)>,
    prices: Arc<PriceTable>,
    titles_flag: &'a AtomicBool,
    titles: bool,
    projects: HashMap<String, i64>,
    with_path: HashSet<String>,
    tools: HashMap<String, ToolsState>,
    report: ScanReport,
    sessions: BTreeSet<String>,
    pending: HashMap<String, PendingMeta>,
}

/// Session metadata read before the session's first request was stored.
#[derive(Debug, Clone, Default)]
pub(crate) struct PendingMeta {
    /// Title and whether the user chose it.
    pub(crate) title: Option<(String, bool)>,
    account: Option<String>,
}

pub(crate) const PENDING_META_CAP: usize = 10_000;

impl<'a> Writer<'a> {
    fn new(store: &'a Store, priced: crate::Priced, titles_flag: &'a AtomicBool, pending: HashMap<String, PendingMeta>) -> Writer<'a> {
        Writer {
            store,
            stamp: priced.0,
            prices: priced.1,
            titles_flag,
            titles: titles_flag.load(Ordering::SeqCst),
            projects: HashMap::new(),
            with_path: HashSet::new(),
            tools: HashMap::new(),
            report: ScanReport::default(),
            sessions: BTreeSet::new(),
            pending,
        }
    }

    fn write(&mut self, group: &mut [Parsed]) -> Result<Written> {
        let store = self.store;
        let saved = (self.report.clone(), self.sessions.clone(), self.pending.clone());
        let now = kanjo_core::time::now_ms();
        match store.write(|tx| self.apply(tx, group, now)) {
            Ok(w) => Ok(w),
            Err(e) => {
                self.projects.clear();
                self.with_path.clear();
                self.tools.clear();
                (self.report, self.sessions, self.pending) = saved;
                Err(e)
            }
        }
    }

    fn project(&mut self, tx: &Connection, p: &Parsed) -> Result<Option<i64>> {
        let key = &p.target.project_key;
        let cwd = if p.target.kind == FileKind::Transcript { p.chunk.cwd.as_deref() } else { None };
        if p.chunk.rows.is_empty() && cwd.is_none() {
            return Ok(self.projects.get(key).copied());
        }
        if let Some(&id) = self.projects.get(key) {
            if cwd.is_none() || self.with_path.contains(key) {
                return Ok(Some(id));
            }
        }
        let ts = p.chunk.rows.iter().map(|r| r.ts).min().unwrap_or(p.mtime);
        let id = write::upsert_project(tx, key, cwd, ts)?;
        self.projects.insert(key.clone(), id);
        if cwd.is_some() {
            self.with_path.insert(key.clone());
        }
        Ok(Some(id))
    }

    fn apply(&mut self, tx: &Connection, group: &mut [Parsed], now: i64) -> Result<Written> {
        // Both are read under the write lock, so a price import or a titles switch
        // committed by another process or thread applies from this batch on.
        let stamp = pricing::active_stamp(tx)?;
        if stamp != self.stamp {
            self.prices = Arc::new(pricing::active(tx)?);
            self.stamp = stamp;
        }
        self.titles = self.titles_flag.load(Ordering::SeqCst);
        let mut w = Written::default();
        let mut dirty_tools = BTreeSet::new();
        for p in group.iter_mut() {
            let project_id = self.project(tx, p)?;
            for row in p.chunk.rows.iter_mut() {
                row.project_id = project_id.unwrap_or_default();
                match write::insert_request(tx, row, &self.prices)? {
                    InsertOutcome::Inserted => {
                        self.report.inserted += 1;
                        w.inserted += 1;
                        self.report.cost_usd += self.prices.cost(&row.model, &row.tokens).unwrap_or(0.0);
                        self.sessions.insert(row.session_id.clone());
                    }
                    InsertOutcome::Updated => {
                        self.report.updated += 1;
                        self.sessions.insert(row.session_id.clone());
                    }
                    InsertOutcome::Duplicate | InsertOutcome::Purged => {}
                }
            }
            self.apply_meta(tx, p, project_id, now)?;
            for seen in &p.chunk.tools {
                let state = match self.tools.get(&seen.session_id) {
                    Some(s) => Some(s.clone()),
                    None => settings::get::<ToolsState>(tx, &tools_key(&seen.session_id))?,
                };
                match state {
                    Some(old) if seen.offset <= old.offset => continue,
                    Some(old) if old.fingerprint != seen.fingerprint => {
                        tx.prepare(
                            "INSERT INTO config_changes (ts, kind, target, project_id, fingerprint) VALUES (?1, 'tools', ?2, ?3, ?4)",
                        )?
                        .execute(params![
                            if seen.ts > 0 { seen.ts } else { p.mtime },
                            seen.session_id,
                            project_id,
                            seen.fingerprint
                        ])?;
                        self.report.config_changes += 1;
                    }
                    _ => {}
                }
                self.tools.insert(seen.session_id.clone(), ToolsState { fingerprint: seen.fingerprint.clone(), offset: seen.offset });
                dirty_tools.insert(seen.session_id.clone());
            }
            for q in &p.chunk.quarantine {
                write::record_quarantine(tx, &q.record_type, &q.signature, q.reason, q.version.as_deref(), now)?;
                self.report.quarantined += 1;
            }
            tx.prepare(
                "INSERT INTO ingest_files (path, kind, size, \"offset\", mtime, session_id, project_id, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (path) DO UPDATE SET kind = excluded.kind, size = excluded.size, \"offset\" = excluded.\"offset\",
                   mtime = excluded.mtime, session_id = excluded.session_id,
                   project_id = COALESCE(excluded.project_id, ingest_files.project_id), updated_at = excluded.updated_at",
            )?
            .execute(params![
                p.target.path.to_string_lossy(),
                p.target.kind.as_str(),
                p.size as i64,
                p.offset as i64,
                p.mtime,
                p.target.session_id,
                project_id,
                now
            ])?;
            w.files += 1;
            w.bytes += p.bytes as i64;
            if p.bytes > 0 {
                self.report.files += 1;
            }
        }
        for session in dirty_tools {
            if let Some(state) = self.tools.get(&session) {
                settings::set(tx, &tools_key(&session), state)?;
            }
        }
        Ok(w)
    }

    /// Title and account land once the session row exists; until then they wait in `pending`.
    /// A title written before a purge of its session is not kept, so a resumed
    /// session does not get back the title of the purged conversation.
    fn apply_meta(&mut self, tx: &Connection, p: &Parsed, project_id: Option<i64>, now: i64) -> Result<()> {
        if self.titles {
            for t in &p.chunk.titles {
                let session = &t.session_id;
                let at = match t.at {
                    0 => p.chunk.rows.iter().filter(|r| &r.session_id == session).map(|r| r.ts).min().unwrap_or(now),
                    at => at,
                };
                if write::is_purged(tx, session, project_id, at)? {
                    if let Some(meta) = self.pending.get_mut(session) {
                        meta.title = None;
                    }
                    continue;
                }
                let meta = self.pending.entry(session.clone()).or_default();
                if t.by_user || !meta.title.as_ref().is_some_and(|(_, user)| *user) {
                    meta.title = Some((t.title.clone(), t.by_user));
                }
            }
        }
        for (session, account) in &p.chunk.owners {
            self.pending.entry(session.clone()).or_default().account = Some(account.clone());
        }
        if self.pending.is_empty() {
            return Ok(());
        }
        let candidates: BTreeSet<&str> = p
            .chunk
            .titles
            .iter()
            .map(|t| t.session_id.as_str())
            .chain(p.chunk.owners.iter().map(|(s, _)| s.as_str()))
            .chain(p.chunk.rows.iter().map(|r| r.session_id.as_str()))
            .collect();
        for session in candidates {
            if !self.pending.contains_key(session) {
                continue;
            }
            let exists = tx.prepare("SELECT 1 FROM sessions WHERE id = ?1")?.exists([session])?;
            if !exists {
                continue;
            }
            let Some(meta) = self.pending.remove(session) else { continue };
            if let (Some((title, by_user)), true) = (meta.title, self.titles) {
                write::set_session_title(tx, session, &title, by_user)?;
            }
            if let Some(account) = meta.account {
                kanjo_core::store::write::set_session_account(tx, session, account.as_str())?;
            }
        }
        Ok(())
    }

    fn finish(mut self) -> (ScanReport, HashMap<String, PendingMeta>, crate::Priced) {
        self.report.sessions = self.sessions.into_iter().collect();
        if !self.titles_flag.load(Ordering::SeqCst) {
            self.pending.values_mut().for_each(|m| m.title = None);
        }
        (self.report, self.pending, (self.stamp, self.prices))
    }
}

fn tools_key(session: &str) -> String {
    format!("fingerprint:tools:{session}")
}

type Progress<'p> = Option<&'p (dyn Fn(&kanjo_core::api::IngestStatus) + Sync)>;

impl Ingestor {
    /// Parses `jobs` in parallel and writes them through one writer.
    pub(crate) fn run(&self, mut jobs: Vec<Job>, progress: Progress) -> Result<ScanReport> {
        jobs.sort_by_key(|j| std::cmp::Reverse(j.size.saturating_sub(j.start)));
        self.clear_titles_if_off();
        let priced = self.priced();
        let stamp = priced.0.clone();
        let titles = self.titles_on();
        let pending = std::mem::take(&mut *crate::lock(&self.pending_meta));
        let mut writer = Writer::new(&self.store, priced, &self.titles, pending);
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(jobs.len()).max(1);
        let next = AtomicUsize::new(0);
        let abort = AtomicBool::new(false);
        let outcome: Result<()> = std::thread::scope(|s| {
            let (tx, rx) = mpsc::sync_channel::<(Job, Result<Parsed>)>(workers * 2);
            for _ in 0..workers {
                let tx = tx.clone();
                let (jobs, next, abort) = (&jobs, &next, &abort);
                s.spawn(move || {
                    while !abort.load(Ordering::Relaxed) {
                        let Some(job) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) else { break };
                        let mut refs = self.take_refs(&job.target.path, job.start == 0);
                        let parsed = parse_job(job, &mut refs, titles, record::process_line);
                        self.put_refs(&job.target.path, refs, job.mtime);
                        if tx.send((job.clone(), parsed)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            let mut group: Vec<Parsed> = Vec::new();
            while let Ok(first) = rx.recv() {
                let take = |(job, parsed): (Job, Result<Parsed>), group: &mut Vec<Parsed>| match parsed {
                    Ok(p) => {
                        let n = p.chunk.rows.len();
                        group.push(p);
                        n
                    }
                    Err(e) => {
                        tracing::warn!(kind = job.target.kind.as_str(), error = %e, "transcript could not be read");
                        0
                    }
                };
                let mut rows = take(first, &mut group);
                while group.len() < BATCH_FILES {
                    match rx.try_recv() {
                        Ok(item) => rows += take(item, &mut group),
                        Err(_) => break,
                    }
                    if rows >= BATCH_ROWS {
                        break;
                    }
                }
                if group.is_empty() {
                    continue;
                }
                let written = match writer.write(&mut group) {
                    Ok(w) => w,
                    Err(e) => {
                        abort.store(true, Ordering::Relaxed);
                        return Err(e);
                    }
                };
                group.clear();
                self.note_written(written.files, written.bytes, written.inserted, progress.is_some());
                if let Some(cb) = progress {
                    cb(&self.status());
                }
            }
            Ok(())
        });
        let (report, mut pending, priced) = writer.finish();
        if priced.0 != stamp {
            self.set_prices(priced);
        }
        if pending.len() > PENDING_META_CAP {
            pending.clear();
        }
        let mut shared = crate::lock(&self.pending_meta);
        pending.extend(shared.drain());
        *shared = pending;
        drop(shared);
        self.rescan_titles_if_needed();
        self.clear_titles_if_off();
        outcome.map(|_| report)
    }
}

//! Applies the statusline / hook spool written by the short-lived
//! `kanjo statusline` and `kanjo hook` processes.

use crate::{lock, pipeline, Ingestor, SpoolReport};
use kanjo_core::limits::{self, LimitKind, LimitSample};
use kanjo_core::spool::{self, HookRecord, SpoolRecord, SPOOL_PREFIX, SPOOL_SUFFIX};
use kanjo_core::statusline::StatuslineRecord;
use kanjo_core::store::write;
use kanjo_core::time::{DAY_MS, MINUTE_MS};
use kanjo_core::Result;
#[cfg(test)]
use kanjo_core::{time::HOUR_MS, Store};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const RECORDS_PER_TX: usize = 2000;
/// An earlier day's file is only finished (partial line consumed, file deleted)
/// once it has not changed for this long: a writer that took its timestamp just
/// before midnight may still be appending to it.
const SETTLE_MS: i64 = 2 * MINUTE_MS;

fn spool_files(dir: &Path) -> Vec<(i64, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<(i64, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name();
            let day = name.to_str()?.strip_prefix(SPOOL_PREFIX)?.strip_suffix(SPOOL_SUFFIX)?.parse().ok()?;
            Some((day, e.path()))
        })
        .collect();
    out.sort();
    out
}

#[derive(Default)]
struct Applied {
    statusline: i64,
    hooks: i64,
    limits_changed: bool,
    sessions: BTreeSet<String>,
}

impl Ingestor {
    /// Applies new spool lines: limit samples, session status, cache misses,
    /// hook markers. Spool files of earlier days are deleted once consumed.
    pub fn drain_spool(&self) -> Result<SpoolReport> {
        self.drain_spool_at(kanjo_core::time::now_ms())
    }

    pub(crate) fn drain_spool_at(&self, now: i64) -> Result<SpoolReport> {
        let _guard = lock(&self.spool);
        self.clear_titles_if_off();
        let files = spool_files(&self.paths.spool_dir());
        if files.is_empty() {
            return Ok(SpoolReport::default());
        }
        let cursors = self.store.read(|c| pipeline::cursors(c, &["spool"]))?;
        let today = now.div_euclid(DAY_MS);
        let mut applied = Applied::default();
        for (day, path) in files {
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            if !meta.is_file() {
                continue;
            }
            let key = path.to_string_lossy().into_owned();
            let size = meta.len();
            let mtime = crate::discover::mtime_ms(&meta);
            let cursor = cursors.get(&key);
            let mut offset = match cursor {
                Some(c) if size >= c.offset => c.offset,
                _ => 0,
            };
            let old_day = day < today && (day < today - 1 || now - mtime >= SETTLE_MS);
            if offset < size || cursor.is_none() {
                let expected = cursor.map(|c| c.offset);
                offset = self.drain_file(&path, &key, expected, offset, size, mtime, old_day, now, &mut applied)?;
            }
            if old_day && offset >= size {
                if let Err(e) = std::fs::remove_file(&path) {
                    tracing::warn!(error = %e, "consumed spool file could not be deleted");
                    continue;
                }
                self.store.write(|tx| Ok(tx.execute("DELETE FROM ingest_files WHERE path = ?1", [&key])?))?;
            }
        }
        Ok(SpoolReport {
            statusline: applied.statusline,
            hooks: applied.hooks,
            limits_changed: applied.limits_changed,
            sessions: applied.sessions.into_iter().collect(),
        })
    }

    /// Returns the new offset. A trailing partial line of an earlier day will
    /// never be completed, so it counts as consumed. `expected` is the cursor
    /// this pass started from; when another collector (a second process on the
    /// same data directory) has moved it meanwhile, its lines are not applied twice.
    #[allow(clippy::too_many_arguments)]
    fn drain_file(
        &self,
        path: &Path,
        key: &str,
        mut expected: Option<u64>,
        start: u64,
        size: u64,
        mtime: i64,
        old_day: bool,
        now: i64,
        applied: &mut Applied,
    ) -> Result<u64> {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(start))?;
        let mut reader = BufReader::new(file.take(size - start));
        let mut buf = Vec::new();
        let mut records = Vec::new();
        let mut consumed = start;
        loop {
            buf.clear();
            let n = reader.read_until(b'\n', &mut buf)?;
            let complete = n > 0 && buf[n - 1] == b'\n';
            if complete {
                consumed += n as u64;
                match std::str::from_utf8(&buf[..n - 1]).ok().and_then(spool::parse_line) {
                    Some(r) => records.push(r),
                    None => tracing::debug!("unreadable spool line skipped"),
                }
            } else if n > 0 && old_day {
                consumed += n as u64;
            }
            if !complete || records.len() >= RECORDS_PER_TX {
                let at = consumed;
                let moved = self.store.write(|tx| {
                    let stored: Option<i64> =
                        tx.query_row("SELECT \"offset\" FROM ingest_files WHERE path = ?1", [key], |r| r.get(0)).optional()?;
                    let stored = stored.map(|o| o as u64);
                    if stored != expected {
                        return Ok(Some(stored.unwrap_or(0)));
                    }
                    let titles = self.titles_on();
                    for r in &records {
                        match r {
                            SpoolRecord::Statusline(s) => apply_statusline(tx, s, titles, now, applied)?,
                            SpoolRecord::Hook(h) => apply_hook(tx, h, applied)?,
                        }
                    }
                    tx.prepare(
                        "INSERT INTO ingest_files (path, kind, size, \"offset\", mtime, session_id, project_id, updated_at)
                         VALUES (?1, 'spool', ?2, ?3, ?4, NULL, NULL, ?5)
                         ON CONFLICT (path) DO UPDATE SET size = excluded.size, \"offset\" = excluded.\"offset\",
                           mtime = excluded.mtime, updated_at = excluded.updated_at",
                    )?
                    .execute(params![key, size as i64, at as i64, mtime, kanjo_core::time::now_ms()])?;
                    Ok(None)
                })?;
                if let Some(other) = moved {
                    tracing::debug!("spool file drained by another collector");
                    return Ok(other);
                }
                expected = Some(at);
                records.clear();
            }
            if !complete {
                return Ok(consumed);
            }
        }
    }
}

fn project_by_path(tx: &Connection, dir: Option<&str>) -> Result<Option<i64>> {
    let Some(dir) = dir else { return Ok(None) };
    Ok(tx.prepare_cached("SELECT id FROM projects WHERE path = ?1")?.query_row([dir], |r| r.get(0)).optional()?)
}

fn apply_statusline(tx: &Connection, r: &StatuslineRecord, titles: bool, now: i64, applied: &mut Applied) -> Result<()> {
    if write::is_purged(tx, "", None, r.ts)? {
        return Ok(());
    }
    for (kind, w) in [(LimitKind::FiveHour, &r.five_hour), (LimitKind::SevenDay, &r.seven_day), (LimitKind::Spend, &r.spend)] {
        let Some(w) = w else { continue };
        limits::discard_future(tx, kind, now)?;
        let sample = LimitSample {
            ts: r.ts,
            used_pct: w.used_pct,
            resets_at: w.resets_at,
            used_usd: w.used_usd,
            limit_usd: w.limit_usd,
            period: w.period.clone(),
        };
        if limits::insert_sample(tx, kind, &sample)? {
            applied.limits_changed = true;
        }
    }
    if write::is_purged(tx, &r.session_id, project_by_path(tx, r.project_dir.as_deref())?, r.ts)? {
        return Ok(());
    }
    applied.statusline += 1;
    applied.sessions.insert(r.session_id.clone());
    let session_name = if titles { r.session_name.as_deref().filter(|s| !s.is_empty()) } else { None };
    let c = r.cache.as_ref();
    let causes = match c {
        Some(c) if !c.miss_causes.is_empty() => Some(serde_json::to_string(&c.miss_causes)?),
        _ => None,
    };
    tx.prepare(
        "INSERT INTO session_status (session_id, ts, model_id, model_name, cc_version, project_dir, session_name, cost_usd,
            duration_ms, api_duration_ms, lines_added, lines_removed, ctx_used_pct, ctx_size, ctx_tokens, exceeds_200k, fast_mode,
            effort, cache_warm, cache_ttl, cache_expires_at, cache_requests, cache_misses, cache_expected_rebuilds, cache_hit_ratio,
            cache_write_tokens, cache_miss_recache_tokens, cache_last_miss_at, cache_recache_if_cold, cache_miss_causes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
            ?26, ?27, ?28, ?29, ?30)
         ON CONFLICT (session_id) DO UPDATE SET ts = excluded.ts, model_id = excluded.model_id, model_name = excluded.model_name,
            cc_version = excluded.cc_version, project_dir = excluded.project_dir, session_name = excluded.session_name,
            cost_usd = excluded.cost_usd, duration_ms = excluded.duration_ms, api_duration_ms = excluded.api_duration_ms,
            lines_added = excluded.lines_added, lines_removed = excluded.lines_removed, ctx_used_pct = excluded.ctx_used_pct,
            ctx_size = excluded.ctx_size, ctx_tokens = excluded.ctx_tokens, exceeds_200k = excluded.exceeds_200k,
            fast_mode = excluded.fast_mode, effort = excluded.effort, cache_warm = excluded.cache_warm, cache_ttl = excluded.cache_ttl,
            cache_expires_at = excluded.cache_expires_at, cache_requests = excluded.cache_requests, cache_misses = excluded.cache_misses,
            cache_expected_rebuilds = excluded.cache_expected_rebuilds, cache_hit_ratio = excluded.cache_hit_ratio,
            cache_write_tokens = excluded.cache_write_tokens, cache_miss_recache_tokens = excluded.cache_miss_recache_tokens,
            cache_last_miss_at = excluded.cache_last_miss_at, cache_recache_if_cold = excluded.cache_recache_if_cold,
            cache_miss_causes = excluded.cache_miss_causes
         WHERE excluded.ts >= session_status.ts OR session_status.ts > ?31",
    )?
    .execute(params![
        r.session_id,
        r.ts,
        r.model_id,
        r.model_name,
        r.cc_version,
        r.project_dir,
        session_name,
        r.cost_usd,
        r.duration_ms,
        r.api_duration_ms,
        r.lines_added,
        r.lines_removed,
        r.ctx_used_pct,
        r.ctx_size,
        r.ctx_tokens,
        r.exceeds_200k,
        r.fast_mode,
        r.effort,
        c.and_then(|c| c.warm),
        c.and_then(|c| c.ttl.clone()),
        c.and_then(|c| c.expires_at),
        c.and_then(|c| c.requests),
        c.and_then(|c| c.misses),
        c.and_then(|c| c.expected_rebuilds),
        c.and_then(|c| c.hit_ratio),
        c.and_then(|c| c.write_tokens),
        c.and_then(|c| c.miss_recache_tokens),
        c.and_then(|c| c.last_miss_at),
        c.and_then(|c| c.recache_tokens_if_cold),
        causes,
        now + limits::CLOCK_AHEAD_TOLERANCE_MS,
    ])?;
    if let Some((c, at)) = c.and_then(|c| c.last_miss_at.map(|at| (c, at))) {
        tx.prepare(
            "INSERT OR IGNORE INTO cache_misses (session_id, ts, causes, tools_added, tools_removed, system_char_delta)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?
        .execute(params![
            r.session_id,
            at,
            serde_json::to_string(&c.last_miss_causes)?,
            c.tools_added,
            c.tools_removed,
            c.system_char_delta
        ])?;
    }
    if let Some(name) = session_name {
        kanjo_core::store::write::set_session_title(tx, &r.session_id, Some(name))?;
    }
    Ok(())
}

fn apply_hook(tx: &Connection, h: &HookRecord, applied: &mut Applied) -> Result<()> {
    if h.session_id.is_empty() || h.event.is_empty() {
        return Ok(());
    }
    if write::is_purged(tx, &h.session_id, project_by_path(tx, h.cwd.as_deref())?, h.ts)? {
        return Ok(());
    }
    tx.prepare("INSERT INTO session_events (ts, session_id, event, source, cwd, model, agent_type) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?
        .execute(params![h.ts, h.session_id, h.event, h.source, h.cwd, h.model, h.agent_type])?;
    applied.hooks += 1;
    applied.sessions.insert(h.session_id.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanjo_core::paths::Paths;
    use kanjo_core::statusline::WindowRecord;
    use kanjo_core::testkit::TestStore;
    use std::sync::Arc;

    fn setup() -> (TestStore, Arc<Store>, Ingestor) {
        let t = TestStore::new();
        let store = Arc::new(Store::open(t.store.path(), None, None).unwrap());
        let paths = Paths::at(t.dir.path().join("home"), t.dir.path().join("claude"));
        let ing = Ingestor::new(store.clone(), paths).unwrap();
        (t, store, ing)
    }

    fn hook(ts: i64, event: &str) -> SpoolRecord {
        SpoolRecord::Hook(HookRecord { ts, event: event.into(), session_id: "s1".into(), ..Default::default() })
    }

    fn events(store: &Store) -> i64 {
        store.read(|c| Ok(c.query_row("SELECT count(*) FROM session_events", [], |r| r.get(0))?)).unwrap()
    }

    #[test]
    fn a_late_line_in_yesterdays_file_is_read_before_the_file_goes() {
        let (_t, store, ing) = setup();
        let dir = ing.paths.spool_dir();
        let now = kanjo_core::time::now_ms();
        let yesterday = now.div_euclid(DAY_MS) * DAY_MS - 1_000;
        spool::append(&dir, yesterday, &hook(yesterday, "Stop")).unwrap();
        ing.drain_spool_at(now).unwrap();
        assert!(spool::file_for(&dir, yesterday).exists(), "a file written to moments ago is kept");
        spool::append(&dir, yesterday, &hook(yesterday + 1, "SessionEnd")).unwrap();
        ing.drain_spool_at(now).unwrap();
        assert_eq!(events(&store), 2, "the late line is applied");
        ing.drain_spool_at(now + SETTLE_MS + 5_000).unwrap();
        assert!(!spool::file_for(&dir, yesterday).exists(), "deleted once it settled");
        assert_eq!(events(&store), 2);
    }

    #[test]
    fn lines_another_collector_applied_are_not_applied_again() {
        let (_t, store, ing) = setup();
        let dir = ing.paths.spool_dir();
        let now = kanjo_core::time::now_ms();
        for i in 0..3 {
            spool::append(&dir, now, &hook(now + i, "Stop")).unwrap();
        }
        let path = spool::file_for(&dir, now);
        let key = path.to_string_lossy().into_owned();
        let size = std::fs::metadata(&path).unwrap().len();
        let mut applied = Applied::default();
        assert_eq!(ing.drain_file(&path, &key, None, 0, size, 0, false, now, &mut applied).unwrap(), size);
        let mut stale = Applied::default();
        assert_eq!(
            ing.drain_file(&path, &key, None, 0, size, 0, false, now, &mut stale).unwrap(),
            size,
            "this pass read the cursor before the other one moved it"
        );
        assert_eq!((stale.hooks, events(&store)), (0, 3));
    }

    #[test]
    fn readings_from_a_clock_that_ran_ahead_do_not_block_later_ones() {
        let (_t, store, ing) = setup();
        let now = kanjo_core::time::now_ms();
        let status = |ts: i64, pct: f64| {
            SpoolRecord::Statusline(StatuslineRecord {
                ts,
                session_id: "s1".into(),
                five_hour: Some(WindowRecord { used_pct: pct, resets_at: ts + HOUR_MS, ..Default::default() }),
                ..Default::default()
            })
        };
        let dir = ing.paths.spool_dir();
        spool::append(&dir, now, &status(now + 3 * HOUR_MS, 50.0)).unwrap();
        ing.drain_spool_at(now).unwrap();
        spool::append(&dir, now, &status(now + MINUTE_MS, 20.0)).unwrap();
        let r = ing.drain_spool_at(now + MINUTE_MS).unwrap();
        assert!(r.limits_changed);
        let (pct, ts): (f64, i64) = store
            .read(|c| {
                Ok(c.query_row("SELECT s.used_pct, st.ts FROM limit_samples s, session_status st ORDER BY s.ts DESC LIMIT 1", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?)
            })
            .unwrap();
        assert_eq!((pct, ts), (20.0, now + MINUTE_MS));
        let n: i64 = store.read(|c| Ok(c.query_row("SELECT count(*) FROM limit_samples", [], |r| r.get(0))?)).unwrap();
        assert_eq!(n, 1, "the future reading is discarded");
    }
}

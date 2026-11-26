//! Applies the statusline / hook spool written by the short-lived
//! `kanjo statusline` and `kanjo hook` processes.

use crate::{lock, pipeline, Ingestor, SpoolReport};
use kanjo_core::limits::{self, LimitKind, LimitSample};
use kanjo_core::spool::{self, HookRecord, SpoolRecord, SPOOL_PREFIX, SPOOL_SUFFIX};
use kanjo_core::statusline::StatuslineRecord;
use kanjo_core::time::DAY_MS;
use kanjo_core::Result;
use rusqlite::{params, Connection};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

const RECORDS_PER_TX: usize = 2000;

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
        let files = spool_files(&self.paths.spool_dir());
        if files.is_empty() {
            return Ok(SpoolReport::default());
        }
        let cursors = self.store.read(|c| pipeline::cursors(c, &["spool"]))?;
        let titles = self.titles.load(Ordering::Relaxed);
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
            let old_day = day < today;
            if offset < size || cursor.is_none() {
                offset = self.drain_file(&path, &key, offset, size, mtime, old_day, titles, &mut applied)?;
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
    /// never be completed, so it counts as consumed.
    #[allow(clippy::too_many_arguments)]
    fn drain_file(
        &self,
        path: &Path,
        key: &str,
        start: u64,
        size: u64,
        mtime: i64,
        old_day: bool,
        titles: bool,
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
                self.store.write(|tx| {
                    for r in &records {
                        match r {
                            SpoolRecord::Statusline(s) => apply_statusline(tx, s, titles, applied)?,
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
                    Ok(())
                })?;
                records.clear();
            }
            if !complete {
                return Ok(consumed);
            }
        }
    }
}

fn apply_statusline(tx: &Connection, r: &StatuslineRecord, titles: bool, applied: &mut Applied) -> Result<()> {
    applied.statusline += 1;
    applied.sessions.insert(r.session_id.clone());
    for (kind, w) in [(LimitKind::FiveHour, &r.five_hour), (LimitKind::SevenDay, &r.seven_day), (LimitKind::Spend, &r.spend)] {
        let Some(w) = w else { continue };
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
         WHERE excluded.ts >= session_status.ts",
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
    tx.prepare("INSERT INTO session_events (ts, session_id, event, source, cwd, model, agent_type) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?
        .execute(params![h.ts, h.session_id, h.event, h.source, h.cwd, h.model, h.agent_type])?;
    applied.hooks += 1;
    applied.sessions.insert(h.session_id.clone());
    Ok(())
}

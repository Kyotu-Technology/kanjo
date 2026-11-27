//! Title-only rescan. Transcripts read while titles were off are not read
//! again by the incremental passes, so after titles are turned on (and once
//! after an upgrade that started reading `custom-title`) the transcripts
//! already imported are read up to their cursors for title records only.

use crate::discover;
use crate::pipeline::{self, Job};
use crate::record::{self, QueryRefs, TitleSeen};
use crate::{lock, Ingestor};
use kanjo_core::store::write;
use kanjo_core::{settings, Result};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Settings key present once the titles of imported transcripts were read.
pub(crate) const RESCANNED: &str = "titles:rescanned";

impl Ingestor {
    /// Reads the titles of transcripts imported while titles were off, when
    /// titles are on and that was not done since they were turned on. Returns
    /// the sessions whose title changed. Reads files and writes to the
    /// database, so call it off the async runtime.
    pub fn refresh_titles(&self) -> Result<Vec<String>> {
        let _pass = lock(&self.pass);
        self.rescan_titles()
    }

    /// The same, at the end of a pass; a failure is logged and retried on the next pass.
    pub(crate) fn rescan_titles_if_needed(&self) {
        if let Err(e) = self.rescan_titles() {
            tracing::warn!(error = %e, "session titles of imported transcripts could not be read; retrying on the next pass");
        }
    }

    fn rescan_titles(&self) -> Result<Vec<String>> {
        if !self.titles_on() || self.titles_checked.swap(true, Ordering::SeqCst) {
            return Ok(Vec::new());
        }
        let out = self.read_known_titles();
        if out.is_err() {
            self.titles_checked.store(false, Ordering::SeqCst);
        }
        out
    }

    fn read_known_titles(&self) -> Result<Vec<String>> {
        if self.store.read(|c| settings::get::<bool>(c, RESCANNED))?.unwrap_or(false) {
            return Ok(Vec::new());
        }
        let projects = self.paths.claude_projects();
        let known: Vec<(String, Option<i64>, i64)> = self.store.read(|c| {
            let mut stmt = c.prepare(
                "SELECT path, project_id, \"offset\" FROM ingest_files WHERE kind IN ('transcript', 'subagent') AND \"offset\" > 0",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        })?;
        let jobs: Vec<(Job, Option<i64>)> = known
            .into_iter()
            .filter_map(|(path, project_id, offset)| {
                let path = PathBuf::from(path);
                let target = discover::classify(&projects, &path).filter(|_| discover::inside(&projects, &path))?;
                let len = std::fs::symlink_metadata(&path).ok().filter(|m| m.is_file())?.len();
                Some((Job { target, start: 0, size: (offset as u64).min(len), mtime: 0 }, project_id))
            })
            .collect();
        let found = read_titles(&jobs);
        let changed = self.store.write(|tx| {
            // Read under the write lock: titles turned off meanwhile have been cleared, or are about to be.
            if !self.titles_on() {
                return Ok(BTreeSet::new());
            }
            let mut changed = BTreeSet::new();
            for (project_id, titles) in &found {
                for t in titles {
                    if !write::is_purged(tx, &t.session_id, *project_id, t.at)?
                        && write::set_session_title(tx, &t.session_id, &t.title, t.by_user)?
                    {
                        changed.insert(t.session_id.clone());
                    }
                }
            }
            settings::set(tx, RESCANNED, &true)?;
            Ok(changed)
        })?;
        if !changed.is_empty() {
            tracing::info!(sessions = changed.len(), "read session titles of imported transcripts");
        }
        Ok(changed.into_iter().collect())
    }
}

/// Title records of each file, in file order, in parallel.
fn read_titles(jobs: &[(Job, Option<i64>)]) -> Vec<(Option<i64>, Vec<TitleSeen>)> {
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(jobs.len()).max(1);
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    while let Some((job, project_id)) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
                        match pipeline::parse_job(job, &mut QueryRefs::default(), true, record::title_line) {
                            Ok(p) if !p.chunk.titles.is_empty() => {
                                let first = p.chunk.first_ts;
                                let titles =
                                    p.chunk.titles.into_iter().map(|t| TitleSeen { at: if t.at == 0 { first } else { t.at }, ..t });
                                out.push((*project_id, titles.collect()));
                            }
                            Ok(_) => {}
                            Err(e) => tracing::warn!(error = %e, "transcript could not be read for its title"),
                        }
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

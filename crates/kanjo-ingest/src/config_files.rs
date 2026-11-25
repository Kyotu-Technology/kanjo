//! Fingerprints of the files that shape the prompt prefix. Content is hashed
//! and dropped; only size, mtime and the SHA-256 are kept.

use crate::record::sha256_hex;
use crate::Ingestor;
use kanjo_core::time::DAY_MS;
use kanjo_core::{settings, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_FILE: u64 = 16 << 20;
pub(crate) const MISSING: &str = "missing";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Fingerprint {
    size: Option<u64>,
    mtime: Option<i64>,
    sha256: String,
}

impl Fingerprint {
    fn missing() -> Fingerprint {
        Fingerprint { size: None, mtime: None, sha256: MISSING.into() }
    }
}

enum Source {
    File(PathBuf),
    Plugins(PathBuf),
}

struct Check {
    kind: &'static str,
    target: String,
    project_id: Option<i64>,
    source: Source,
}

fn key(kind: &str, target: &str) -> String {
    format!("fingerprint:{kind}:{target}")
}

fn stat(path: &Path) -> Option<(u64, i64)> {
    let meta = std::fs::metadata(path).ok()?;
    meta.is_file().then(|| (meta.len(), crate::discover::mtime_ms(&meta)))
}

fn read_small(path: &Path, size: u64) -> Option<Vec<u8>> {
    if size > MAX_FILE {
        return None;
    }
    std::fs::read(path).ok()
}

/// `None` when the file cannot be judged right now (unreadable or too large); the old fingerprint stays.
fn fingerprint(source: &Source, old: Option<&Fingerprint>) -> Option<Fingerprint> {
    let path = match source {
        Source::File(p) | Source::Plugins(p) => p,
    };
    let Some((size, mtime)) = stat(path) else { return Some(Fingerprint::missing()) };
    if let Some(o) = old.filter(|o| o.size == Some(size) && o.mtime == Some(mtime)) {
        return Some(o.clone());
    }
    let bytes = read_small(path, size)?;
    let sha256 = match source {
        Source::File(_) => sha256_hex(&bytes),
        Source::Plugins(_) => {
            let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
            match value.get("enabledPlugins") {
                Some(plugins) => sha256_hex(plugins.to_string().as_bytes()),
                None => MISSING.into(),
            }
        }
    };
    Some(Fingerprint { size: Some(size), mtime: Some(mtime), sha256 })
}

impl Ingestor {
    /// Fingerprints the user CLAUDE.md, settings.json, its enabledPlugins and
    /// the CLAUDE.md of projects active in the last 7 days. A first sighting
    /// is the baseline; a later difference writes a `config_changes` row.
    /// Returns the number of rows written.
    pub fn check_config_files(&self) -> Result<usize> {
        self.check_config_files_at(kanjo_core::time::now_ms())
    }

    pub(crate) fn check_config_files_at(&self, now: i64) -> Result<usize> {
        let claude = &self.paths.claude_dir;
        let settings_path = self.paths.claude_settings();
        let mut checks = vec![
            Check { kind: "claude_md", target: "user CLAUDE.md".into(), project_id: None, source: Source::File(claude.join("CLAUDE.md")) },
            Check { kind: "settings", target: "user settings.json".into(), project_id: None, source: Source::File(settings_path.clone()) },
            Check { kind: "plugins", target: "enabledPlugins".into(), project_id: None, source: Source::Plugins(settings_path) },
        ];
        let projects: Vec<(i64, String, String)> = self.store.read(|c| {
            let mut stmt = c.prepare("SELECT id, path, alias FROM projects WHERE last_seen >= ?1 AND path IS NOT NULL ORDER BY id")?;
            let rows = stmt.query_map([now - 7 * DAY_MS], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        })?;
        for (id, path, alias) in projects {
            checks.push(Check {
                kind: "claude_md",
                target: alias,
                project_id: Some(id),
                source: Source::File(Path::new(&path).join("CLAUDE.md")),
            });
        }
        let olds: Vec<Option<Fingerprint>> =
            self.store.read(|c| checks.iter().map(|ch| settings::get::<Fingerprint>(c, &key(ch.kind, &ch.target))).collect())?;
        let news: Vec<Option<Fingerprint>> = checks.iter().zip(&olds).map(|(ch, old)| fingerprint(&ch.source, old.as_ref())).collect();
        self.store.write(|tx| {
            let mut changes = 0;
            for ((ch, old), new) in checks.iter().zip(&olds).zip(news) {
                let Some(new) = new else { continue };
                if old.as_ref() == Some(&new) {
                    continue;
                }
                if old.as_ref().is_some_and(|o| o.sha256 != new.sha256) {
                    tx.execute(
                        "INSERT INTO config_changes (ts, kind, target, project_id, fingerprint) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![now, ch.kind, ch.target, ch.project_id, new.sha256],
                    )?;
                    changes += 1;
                }
                settings::set(tx, &key(ch.kind, &ch.target), &new)?;
            }
            Ok(changes)
        })
    }
}

//! Append-only hand-off between the short-lived `kanjo statusline` / `kanjo hook`
//! processes and the long-running agent. The fast path never opens the
//! database: it appends one line here and exits.

use crate::statusline::StatuslineRecord;
use crate::time::DAY_MS;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum SpoolRecord {
    Statusline(StatuslineRecord),
    Hook(HookRecord),
}

/// A SessionStart / Stop / SessionEnd marker. No prompt or response text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HookRecord {
    pub ts: i64,
    pub event: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub source: Option<String>,
    pub model: Option<String>,
    pub agent_type: Option<String>,
}

pub const SPOOL_PREFIX: &str = "spool-";
pub const SPOOL_SUFFIX: &str = ".ndjson";

/// One file per UTC day, so the agent can delete fully consumed days.
pub fn file_for(spool_dir: &Path, ts: i64) -> PathBuf {
    spool_dir.join(format!("{SPOOL_PREFIX}{}{SPOOL_SUFFIX}", ts.div_euclid(DAY_MS)))
}

/// Appends one record as a single line with a single write.
pub fn append(spool_dir: &Path, ts: i64, record: &SpoolRecord) -> Result<()> {
    if !spool_dir.is_dir() {
        crate::paths::create_private_dir(spool_dir)?;
    }
    let mut line = serde_json::to_vec(record)?;
    line.push(b'\n');
    let path = file_for(spool_dir, ts);
    let mut options = std::fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(&line)?;
    Ok(())
}

/// Parses one spool line; `None` for a line that is not a known record.
pub fn parse_line(line: &str) -> Option<SpoolRecord> {
    serde_json::from_str(line).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let spool = dir.path().join("spool");
        let a = SpoolRecord::Statusline(StatuslineRecord { ts: 5, session_id: "s".into(), ..Default::default() });
        let b = SpoolRecord::Hook(HookRecord { ts: 6, event: "SessionStart".into(), session_id: "s".into(), ..Default::default() });
        append(&spool, 5, &a).unwrap();
        append(&spool, 6, &b).unwrap();
        let text = std::fs::read_to_string(file_for(&spool, 5)).unwrap();
        let back: Vec<_> = text.lines().filter_map(parse_line).collect();
        assert_eq!(back, vec![a, b]);
        assert!(parse_line("garbage").is_none());
    }
}

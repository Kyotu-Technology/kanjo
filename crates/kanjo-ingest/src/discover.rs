//! Which files under `<claude>/projects` are transcripts, and how they map to
//! projects and sessions. Nothing outside that directory is ever opened, and
//! symbolic links are not followed.

use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    Transcript,
    Subagent,
}

impl FileKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileKind::Transcript => "transcript",
            FileKind::Subagent => "subagent",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Target {
    pub path: PathBuf,
    pub kind: FileKind,
    pub project_key: String,
    pub session_id: String,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Found {
    pub target: Target,
    pub size: u64,
    pub mtime: i64,
}

/// Maps a path to a transcript target, or `None` when it is not one (the
/// workflow journal, meta files, anything outside `projects`).
pub(crate) fn classify(projects: &Path, path: &Path) -> Option<Target> {
    let rel = path.strip_prefix(projects).ok()?;
    let mut parts = Vec::new();
    for c in rel.components() {
        match c {
            Component::Normal(p) => parts.push(p.to_str()?),
            _ => return None,
        }
    }
    let file = *parts.last()?;
    let stem = file.strip_suffix(".jsonl").filter(|s| !s.is_empty())?;
    let project_key = parts[0].to_string();
    match parts.len() {
        2 => {
            Some(Target { path: path.to_path_buf(), kind: FileKind::Transcript, project_key, session_id: stem.to_string(), agent_id: None })
        }
        n if n >= 4 && parts[2] == "subagents" => {
            let agent = stem.strip_prefix("agent-").filter(|s| !s.is_empty())?;
            Some(Target {
                path: path.to_path_buf(),
                kind: FileKind::Subagent,
                project_key,
                session_id: parts[1].to_string(),
                agent_id: Some(agent.to_string()),
            })
        }
        _ => None,
    }
}

/// True when `path` resolves (following links) to a place inside `projects`.
pub(crate) fn inside(projects: &Path, path: &Path) -> bool {
    match (std::fs::canonicalize(projects), std::fs::canonicalize(path)) {
        (Ok(root), Ok(p)) => p.starts_with(root),
        _ => false,
    }
}

pub(crate) fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Lists every transcript with its size and modification time.
pub(crate) fn walk(projects: &Path) -> Vec<Found> {
    let mut out = Vec::new();
    for project in dirs(projects) {
        let Ok(entries) = std::fs::read_dir(&project) else { continue };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            if ft.is_file() {
                push(projects, &path, &mut out);
            } else if ft.is_dir() {
                let sub = path.join("subagents");
                if std::fs::symlink_metadata(&sub).map(|m| m.is_dir()).unwrap_or(false) {
                    walk_subagents(projects, &sub, &mut out, 0);
                }
            }
        }
    }
    out
}

fn dirs(parent: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };
    entries.flatten().filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false)).map(|e| e.path()).collect()
}

fn walk_subagents(projects: &Path, dir: &Path, out: &mut Vec<Found>, depth: usize) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        if ft.is_file() {
            push(projects, &path, out);
        } else if ft.is_dir() {
            walk_subagents(projects, &path, out, depth + 1);
        }
    }
}

fn push(projects: &Path, path: &Path, out: &mut Vec<Found>) {
    let Some(target) = classify(projects, path) else { return };
    let Ok(meta) = std::fs::symlink_metadata(path) else { return };
    if meta.is_file() {
        out.push(Found { target, size: meta.len(), mtime: mtime_ms(&meta) });
    }
}

/// `agentType` from the `agent-<id>.meta.json` next to a subagent transcript.
pub(crate) fn meta_agent_type(transcript: &Path) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Meta<'a> {
        #[serde(rename = "agentType", borrow, default)]
        agent_type: crate::record::Str<'a>,
    }
    let name = transcript.file_name()?.to_str()?.strip_suffix(".jsonl")?;
    let path = transcript.with_file_name(format!("{name}.meta.json"));
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > 1 << 20 {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    serde_json::from_slice::<Meta>(&bytes).ok()?.agent_type.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts() {
        let root = Path::new("/c/projects");
        let t = classify(root, Path::new("/c/projects/-work-app/0000-1111.jsonl")).unwrap();
        assert_eq!((t.kind, t.project_key.as_str(), t.session_id.as_str()), (FileKind::Transcript, "-work-app", "0000-1111"));
        let s = classify(root, Path::new("/c/projects/-work-app/0000-1111/subagents/agent-a1.jsonl")).unwrap();
        assert_eq!((s.kind, s.session_id.as_str(), s.agent_id.as_deref()), (FileKind::Subagent, "0000-1111", Some("a1")));
        let w = classify(root, Path::new("/c/projects/-work-app/0000-1111/subagents/workflows/wf1/agent-b2.jsonl")).unwrap();
        assert_eq!((w.kind, w.agent_id.as_deref()), (FileKind::Subagent, Some("b2")));
        for skip in [
            "/c/projects/-work-app/0000-1111/subagents/workflows/wf1/journal.jsonl",
            "/c/projects/-work-app/0000-1111/subagents/agent-a1.meta.json",
            "/c/projects/-work-app/0000-1111/tool-results/x.jsonl",
            "/c/projects/x.jsonl",
            "/c/projects/-work-app/../../.credentials.json",
            "/c/settings.json",
        ] {
            assert!(classify(root, Path::new(skip)).is_none(), "{skip}");
        }
    }
}

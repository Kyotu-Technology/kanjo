//! The transcript viewer's reader: the only code that touches conversation
//! text. It reads the file on demand and stores nothing.

use crate::discover::{self, FileKind};
use crate::record::{self, parse_ts, Kind};
use kanjo_core::api::{Transcript, TranscriptMessage};
use kanjo_core::paths::Paths;
use kanjo_core::{Error, Result, Store};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

/// Longest text kept per message, in characters.
pub const MAX_TEXT_CHARS: usize = 4000;

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    model: Option<String>,
    content: Option<Value>,
}

fn clip(s: &str) -> String {
    match s.char_indices().nth(MAX_TEXT_CHARS) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

fn messages(line: Line, out: &mut Vec<TranscriptMessage>) {
    let Some(role) = line.kind else { return };
    let Some(msg) = line.message else { return };
    let ts = line.timestamp.as_deref().and_then(parse_ts);
    let model = if role == "assistant" { msg.model.filter(|m| m != "<synthetic>") } else { None };
    let make = |kind: &str, text: &str, tool: Option<String>| TranscriptMessage {
        ts,
        role: role.clone(),
        kind: kind.into(),
        text: clip(text),
        model: model.clone(),
        tool,
    };
    match msg.content {
        Some(Value::String(s)) if !s.is_empty() => out.push(make("text", &s, None)),
        Some(Value::Array(blocks)) => {
            for b in blocks {
                let field = |k: &str| b.get(k).and_then(Value::as_str);
                match field("type") {
                    Some("text") => {
                        if let Some(t) = field("text").filter(|t| !t.is_empty()) {
                            out.push(make("text", t, None));
                        }
                    }
                    Some("thinking") => {
                        if let Some(t) = field("thinking").filter(|t| !t.is_empty()) {
                            out.push(make("thinking", t, None));
                        }
                    }
                    Some("tool_use") => out.push(make("tool_use", "", field("name").map(str::to_string))),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

/// Reads up to `limit` messages (user and assistant text, thinking, tool
/// names) from the main transcript of `session_id`, oldest first. Each text
/// is cut to [`MAX_TEXT_CHARS`]; `truncated` says the list was cut. The
/// caller is responsible for the viewer setting and the audit entry.
pub fn read_transcript(store: &Store, paths: &Paths, session_id: &str, limit: usize) -> Result<Transcript> {
    let stored: Option<String> = store.read(|c| {
        Ok(c.query_row(
            "SELECT path FROM ingest_files WHERE session_id = ?1 AND kind = 'transcript' ORDER BY updated_at DESC LIMIT 1",
            [session_id],
            |r| r.get(0),
        )
        .optional()?)
    })?;
    let path = PathBuf::from(stored.ok_or_else(|| Error::NotFound(format!("transcript of session {session_id}")))?);
    let projects = paths.claude_projects();
    let is_transcript = discover::classify(&projects, &path).is_some_and(|t| t.kind == FileKind::Transcript);
    if !is_transcript || !discover::inside(&projects, &path) {
        return Err(Error::Invalid("the stored transcript path is outside the Claude projects directory".into()));
    }
    let mut reader = BufReader::new(std::fs::File::open(&path)?);
    let mut buf = Vec::new();
    let mut out = Vec::new();
    let mut truncated = false;
    let mut pending = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        let line = buf.strip_suffix(b"\n").unwrap_or(&buf);
        if !matches!(record::kind_of(line), Kind::Known("user" | "assistant")) {
            continue;
        }
        let Ok(parsed) = serde_json::from_slice::<Line>(line) else { continue };
        messages(parsed, &mut pending);
        for m in pending.drain(..) {
            if out.len() >= limit {
                truncated = true;
                break;
            }
            out.push(m);
        }
        if truncated {
            break;
        }
    }
    Ok(Transcript { session_id: session_id.to_string(), messages: out, truncated })
}

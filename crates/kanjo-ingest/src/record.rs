//! One transcript line -> usage rows and metadata. Only the fields listed
//! here are ever decoded; text blocks and tool inputs (apart from the skill
//! name and subagent type) are skipped without being copied.

use crate::toplevel;
use kanjo_core::model::{RequestRow, TokenCounts, ToolKind, ToolUse};
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;

/// Record types that carry nothing Kanjo uses.
pub(crate) const NOISE: &[&str] = &[
    "user",
    "system",
    "attachment",
    "summary",
    "file-history-snapshot",
    "file-history-delta",
    "queue-operation",
    "permission-mode",
    "mode",
    "last-prompt",
    "api-request-blob",
    "atis-latch",
    "repl-mode",
    "data-flow-records",
];

/// Further types seen on Claude Code 2.1.3xx data; their key signatures carry no usage.
pub(crate) const OBSERVED_NOISE: &[&str] = &[
    "api-request-shape",
    "bridge-session",
    "cost-state",
    "agent-name",
    "pr-link",
    "derived-from",
    "relocated",
    "frame-link",
    "artifact-autoreact-ledger",
    "artifact-comment-monitor",
    "data-flow-incomplete",
    "continued-in",
    "fork-context-ref",
];

pub(crate) const HANDLED: &[&str] = &["assistant", "ai-title", "custom-title", "session-owner", "api-request", "tools-used"];

/// Record types that carry session metadata, handled by `meta`.
pub(crate) const META: &[&str] = &["ai-title", "custom-title", "session-owner", "api-request", "tools-used"];

pub(crate) fn known(kind: &[u8]) -> Option<&'static str> {
    NOISE.iter().chain(OBSERVED_NOISE).chain(HANDLED).copied().find(|k| k.as_bytes() == kind)
}

pub(crate) enum Kind {
    Known(&'static str),
    Unknown { kind: String, signature: String, version: Option<String> },
    Invalid,
}

/// Classifies a line by its top-level `type`, reading as little of it as possible.
pub(crate) fn kind_of(line: &[u8]) -> Kind {
    if let Some(shape) = toplevel::scan(line, |t| known(t).is_some()) {
        if let Some(k) = shape.kind.and_then(known) {
            return Kind::Known(k);
        }
        return Kind::Unknown {
            kind: toplevel::clean_type(shape.kind),
            signature: toplevel::signature(shape.keys.into_iter()),
            version: toplevel::clean_version(shape.version),
        };
    }
    match toplevel::probe(line) {
        Some(shape) => match shape.kind.as_deref().map(str::as_bytes).and_then(known) {
            Some(k) => Kind::Known(k),
            None => Kind::Unknown {
                kind: toplevel::clean_type(shape.kind.as_deref().map(str::as_bytes)),
                signature: toplevel::signature(shape.keys.iter().map(|k| k.as_bytes())),
                version: toplevel::clean_version(shape.version.as_deref().map(str::as_bytes)),
            },
        },
        None => Kind::Invalid,
    }
}

fn full_shape(line: &[u8]) -> (String, Option<String>) {
    if let Some(s) = toplevel::scan(line, |_| false) {
        return (toplevel::signature(s.keys.into_iter()), toplevel::clean_version(s.version));
    }
    match toplevel::probe(line) {
        Some(s) => {
            (toplevel::signature(s.keys.iter().map(|k| k.as_bytes())), toplevel::clean_version(s.version.as_deref().map(str::as_bytes)))
        }
        None => (String::new(), None),
    }
}

macro_rules! lenient_scalars {
    () => {
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(Default::default())
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(Default::default())
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(Default::default())
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(Default::default())
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(Default::default())
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(Default::default())
        }
    };
}

macro_rules! lenient_str {
    () => {
        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok(Default::default())
        }
    };
}

macro_rules! lenient_seq {
    () => {
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            Ok(Default::default())
        }
    };
}

macro_rules! lenient_map {
    () => {
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok(Default::default())
        }
    };
}

/// A string if the value is one, otherwise nothing. Borrows when it can.
#[derive(Debug, Default, Clone)]
pub(crate) struct Str<'a>(pub Option<Cow<'a, str>>);

impl Str<'_> {
    pub fn get(&self) -> Option<&str> {
        self.0.as_deref().filter(|s| !s.is_empty())
    }

    pub fn into_owned(self) -> Option<String> {
        self.0.filter(|s| !s.is_empty()).map(Cow::into_owned)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Str<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<'a>(PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for V<'a> {
            type Value = Str<'a>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any value")
            }
            fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Str<'a>, E> {
                Ok(Str(Some(Cow::Borrowed(v))))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Str<'a>, E> {
                Ok(Str(Some(Cow::Owned(v.to_owned()))))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Str<'a>, E> {
                Ok(Str(Some(Cow::Owned(v))))
            }
            lenient_scalars!();
            lenient_seq!();
            lenient_map!();
        }
        d.deserialize_any(V(PhantomData))
    }
}

#[derive(Debug, Default)]
struct Flag(bool);

impl<'de> Deserialize<'de> for Flag {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Flag;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any value")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Flag, E> {
                Ok(Flag(v))
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> Result<Flag, E> {
                Ok(Flag(false))
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> Result<Flag, E> {
                Ok(Flag(false))
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Flag, E> {
                Ok(Flag(false))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Flag, E> {
                Ok(Flag(false))
            }
            lenient_str!();
            lenient_seq!();
            lenient_map!();
        }
        d.deserialize_any(V)
    }
}

/// A list of strings; anything else in or instead of the list is dropped.
#[derive(Debug, Default)]
struct StrList<'a>(Option<Vec<Str<'a>>>);

impl<'de: 'a, 'a> Deserialize<'de> for StrList<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<'a>(PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for V<'a> {
            type Value = StrList<'a>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any value")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<StrList<'a>, A::Error> {
                let mut out = Vec::new();
                while let Some(s) = seq.next_element::<Str<'a>>()? {
                    if s.get().is_some() {
                        out.push(s);
                    }
                }
                Ok(StrList(Some(out)))
            }
            lenient_scalars!();
            lenient_str!();
            lenient_map!();
        }
        d.deserialize_any(V(PhantomData))
    }
}

/// The two tool input fields Kanjo reads. Everything else in `input` is skipped.
#[derive(Debug, Default)]
struct ToolInput<'a> {
    skill: Str<'a>,
    subagent_type: Str<'a>,
}

impl<'de: 'a, 'a> Deserialize<'de> for ToolInput<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<'a>(PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for V<'a> {
            type Value = ToolInput<'a>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any value")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ToolInput<'a>, A::Error> {
                let mut out = ToolInput::default();
                while let Some(key) = map.next_key::<Str<'a>>()? {
                    match key.0.as_deref() {
                        Some("skill") => out.skill = map.next_value()?,
                        Some("subagent_type") => out.subagent_type = map.next_value()?,
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(out)
            }
            lenient_scalars!();
            lenient_str!();
            lenient_seq!();
        }
        d.deserialize_any(V(PhantomData))
    }
}

#[derive(Debug, Default)]
struct Block<'a> {
    kind: Str<'a>,
    id: Str<'a>,
    name: Str<'a>,
    input: ToolInput<'a>,
}

impl<'de: 'a, 'a> Deserialize<'de> for Block<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<'a>(PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for V<'a> {
            type Value = Block<'a>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any value")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Block<'a>, A::Error> {
                let mut out = Block::default();
                while let Some(key) = map.next_key::<Str<'a>>()? {
                    match key.0.as_deref() {
                        Some("type") => out.kind = map.next_value()?,
                        Some("id") => out.id = map.next_value()?,
                        Some("name") => out.name = map.next_value()?,
                        Some("input") => out.input = map.next_value()?,
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(out)
            }
            lenient_scalars!();
            lenient_str!();
            lenient_seq!();
        }
        d.deserialize_any(V(PhantomData))
    }
}

/// `message.content`: the blocks of an array; a plain string carries no tools.
#[derive(Debug, Default)]
struct Content<'a>(Vec<Block<'a>>);

impl<'de: 'a, 'a> Deserialize<'de> for Content<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<'a>(PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for V<'a> {
            type Value = Content<'a>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any value")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Content<'a>, A::Error> {
                let mut out = Vec::new();
                while let Some(b) = seq.next_element::<Block<'a>>()? {
                    if b.kind.get() == Some("tool_use") {
                        out.push(b);
                    }
                }
                Ok(Content(out))
            }
            lenient_scalars!();
            lenient_str!();
            lenient_map!();
        }
        d.deserialize_any(V(PhantomData))
    }
}

#[derive(Deserialize)]
struct Message<'a> {
    #[serde(borrow, default)]
    id: Str<'a>,
    #[serde(borrow, default)]
    model: Str<'a>,
    #[serde(borrow, default)]
    stop_reason: Str<'a>,
    #[serde(borrow, default)]
    usage: Option<&'a RawValue>,
    #[serde(borrow, default)]
    content: Content<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssistantRecord<'a> {
    #[serde(rename = "type", borrow, default)]
    kind: Str<'a>,
    #[serde(borrow, default)]
    message: Option<Message<'a>>,
    #[serde(borrow, default)]
    request_id: Str<'a>,
    #[serde(borrow, default)]
    session_id: Str<'a>,
    #[serde(borrow, default)]
    timestamp: Str<'a>,
    #[serde(borrow, default)]
    cwd: Str<'a>,
    #[serde(borrow, default)]
    git_branch: Str<'a>,
    #[serde(borrow, default)]
    version: Str<'a>,
    #[serde(borrow, default)]
    entrypoint: Str<'a>,
    #[serde(default)]
    is_sidechain: Flag,
    #[serde(borrow, default)]
    agent_id: Str<'a>,
    #[serde(borrow, default)]
    attribution_agent: Str<'a>,
    #[serde(borrow, default)]
    effort: Str<'a>,
    #[serde(borrow, default)]
    request_ref: Str<'a>,
    #[serde(borrow, default)]
    uuid: Str<'a>,
}

/// Token counts are strict: a usage object whose numbers do not decode is
/// quarantined rather than guessed.
#[derive(Deserialize)]
struct Usage<'a> {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cache_read_input_tokens: Option<i64>,
    cache_creation_input_tokens: Option<i64>,
    cache_creation: Option<CacheCreation>,
    server_tool_use: Option<ServerToolUse>,
    #[serde(borrow, default)]
    service_tier: Str<'a>,
    #[serde(borrow, default)]
    speed: Str<'a>,
}

#[derive(Deserialize)]
struct CacheCreation {
    ephemeral_5m_input_tokens: Option<i64>,
    ephemeral_1h_input_tokens: Option<i64>,
}

#[derive(Deserialize)]
struct ServerToolUse {
    web_search_requests: Option<i64>,
    web_fetch_requests: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetaRecord<'a> {
    #[serde(rename = "type", borrow, default)]
    kind: Str<'a>,
    #[serde(borrow, default)]
    session_id: Str<'a>,
    #[serde(borrow, default)]
    ai_title: Str<'a>,
    #[serde(borrow, default)]
    custom_title: Str<'a>,
    #[serde(borrow, default)]
    account_uuid: Str<'a>,
    #[serde(borrow, default)]
    id: Str<'a>,
    #[serde(borrow, default)]
    query_source: Str<'a>,
    #[serde(borrow, default)]
    timestamp: Str<'a>,
    #[serde(borrow, default)]
    tools: StrList<'a>,
}

/// `api-request` ids seen recently in one file, so a later assistant record
/// can resolve its `requestRef` to a query source.
#[derive(Debug, Default)]
pub(crate) struct QueryRefs {
    order: VecDeque<String>,
    map: HashMap<String, String>,
}

pub(crate) const QUERY_REFS_WHILE_READING: usize = 4096;

impl QueryRefs {
    fn insert(&mut self, id: String, source: String) {
        if self.map.insert(id.clone(), source).is_none() {
            self.order.push_back(id);
        }
        self.trim(QUERY_REFS_WHILE_READING);
    }

    fn get(&self, id: &str) -> Option<&String> {
        self.map.get(id)
    }

    /// Keeps only the newest `cap` entries.
    pub fn trim(&mut self, cap: usize) {
        while self.order.len() > cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }
}

pub(crate) struct FileCtx<'r> {
    pub subagent: bool,
    /// Session id taken from the file path; records normally carry the same one.
    pub session_id: String,
    pub agent_id: Option<String>,
    /// `agentType` from the subagent's meta.json.
    pub agent_type: Option<String>,
    pub titles: bool,
    pub refs: &'r mut QueryRefs,
    /// Byte offset of the line being processed.
    pub offset: u64,
    /// Newest record timestamp seen so far in this pass, 0 before the first.
    pub last_ts: i64,
}

/// An `ai-title` or `custom-title` record.
#[derive(Debug, Clone)]
pub(crate) struct TitleSeen {
    pub session_id: String,
    pub title: String,
    /// Time of the last timestamped record before it, 0 when none.
    pub at: i64,
    /// Chosen by the user with `/rename` (`custom-title`), not generated.
    pub by_user: bool,
}

/// A `tools-used` record. These carry no timestamp, so they are ordered by
/// their position in the session's main transcript.
pub(crate) struct ToolsSeen {
    pub session_id: String,
    pub offset: u64,
    pub ts: i64,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Quarantined {
    pub record_type: String,
    pub signature: String,
    pub reason: &'static str,
    pub version: Option<String>,
}

/// What one pass over (part of) a file produced.
#[derive(Default)]
pub(crate) struct Chunk {
    pub rows: Vec<RequestRow>,
    index: HashMap<String, usize>,
    pub cwd: Option<String>,
    pub titles: Vec<TitleSeen>,
    /// Earliest assistant timestamp seen by `title_line` (0 when none).
    pub first_ts: i64,
    pub owners: Vec<(String, String)>,
    pub tools: Vec<ToolsSeen>,
    pub quarantine: Vec<Quarantined>,
    pub counts: Counts,
}

/// Assistant records by outcome, for diagnostics.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Counts {
    pub usage_records: u64,
    pub no_message: u64,
    pub no_usage: u64,
    pub synthetic: u64,
    pub synthetic_with_tokens: u64,
}

impl Chunk {
    /// Records of one response share a message id: keep one row with the
    /// largest usage and every tool use, as `insert_request` would end up with.
    fn push(&mut self, row: RequestRow) {
        let Some(&i) = self.index.get(&row.message_id) else {
            self.index.insert(row.message_id.clone(), self.rows.len());
            self.rows.push(row);
            return;
        };
        let have = &mut self.rows[i];
        for t in row.tools {
            if !have.tools.iter().any(|h| h.use_id == t.use_id && h.kind == t.kind) {
                have.tools.push(t);
            }
        }
        if row.tokens.total() > have.tokens.total() {
            have.tokens = row.tokens;
            have.web_search = row.web_search;
            have.web_fetch = row.web_fetch;
            if row.stop_reason.is_some() {
                have.stop_reason = row.stop_reason;
            }
        }
        if have.query_source.is_none() {
            have.query_source = row.query_source;
        }
    }

    fn quarantine(&mut self, record_type: &str, line: &[u8], reason: &'static str) {
        let (signature, version) = full_shape(line);
        self.quarantine.push(Quarantined { record_type: record_type.to_string(), signature, reason, version });
    }
}

pub(crate) fn process_line(line: &[u8], ctx: &mut FileCtx, out: &mut Chunk) {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.iter().all(u8::is_ascii_whitespace) {
        return;
    }
    match kind_of(line) {
        Kind::Known("assistant") => assistant(line, ctx, out),
        Kind::Known(k) if META.contains(&k) => meta(k, line, ctx, out),
        Kind::Known(_) => {}
        Kind::Unknown { kind, signature, version } => {
            let reason = if kind == "<none>" { "record without type" } else { "unknown record type" };
            out.quarantine.push(Quarantined { record_type: kind, signature, reason, version });
        }
        Kind::Invalid => out.quarantine.push(Quarantined {
            record_type: "<invalid>".into(),
            signature: String::new(),
            reason: "malformed line",
            version: None,
        }),
    }
}

#[derive(Deserialize)]
struct Stamp<'a> {
    #[serde(borrow, default)]
    timestamp: Str<'a>,
    #[serde(borrow, default)]
    message: Option<StampMessage<'a>>,
}

#[derive(Deserialize)]
struct StampMessage<'a> {
    #[serde(borrow, default)]
    model: Str<'a>,
    #[serde(default)]
    usage: Option<IgnoredAny>,
}

/// Title-only reading of a line: title records, and the timestamps of the
/// records `process_line` takes them from, so each title gets the same time.
pub(crate) fn title_line(line: &[u8], ctx: &mut FileCtx, out: &mut Chunk) {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    match kind_of(line) {
        Kind::Known(k @ ("ai-title" | "custom-title")) => meta(k, line, ctx, out),
        Kind::Known("assistant") => {
            let usage_ts = serde_json::from_slice::<Stamp>(line).ok().and_then(|r| {
                let m = r.message?;
                m.usage?;
                (m.model.get()? != "<synthetic>").then_some(())?;
                r.timestamp.get().and_then(parse_ts)
            });
            if let Some(ts) = usage_ts {
                ctx.last_ts = ctx.last_ts.max(ts);
                if out.first_ts == 0 || ts < out.first_ts {
                    out.first_ts = ts;
                }
            }
        }
        Kind::Known(k) if META.contains(&k) => {
            if let Some(ts) = serde_json::from_slice::<Stamp>(line).ok().and_then(|r| r.timestamp.get().and_then(parse_ts)) {
                ctx.last_ts = ctx.last_ts.max(ts);
            }
        }
        _ => {}
    }
}

fn assistant(line: &[u8], ctx: &mut FileCtx, out: &mut Chunk) {
    let Ok(rec) = serde_json::from_slice::<AssistantRecord>(line) else {
        return out.quarantine("assistant", line, "assistant record does not deserialize");
    };
    if rec.kind.get() != Some("assistant") {
        return out.quarantine("assistant", line, "top-level type differs after parse");
    }
    let Some(msg) = rec.message else {
        out.counts.no_message += 1;
        return;
    };
    let Some(raw_usage) = msg.usage else {
        out.counts.no_usage += 1;
        return;
    };
    let Some(model) = msg.model.get() else {
        return out.quarantine("assistant", line, "usage without model");
    };
    if model == "<synthetic>" {
        out.counts.synthetic += 1;
        if serde_json::from_str::<Usage>(raw_usage.get()).is_ok_and(|u| {
            [u.input_tokens, u.output_tokens, u.cache_read_input_tokens, u.cache_creation_input_tokens].iter().flatten().any(|n| *n != 0)
        }) {
            out.counts.synthetic_with_tokens += 1;
        }
        return;
    }
    let Ok(usage) = serde_json::from_str::<Usage>(raw_usage.get()) else {
        return out.quarantine("assistant", line, "usage does not deserialize");
    };
    let Some(ts) = rec.timestamp.get().and_then(parse_ts) else {
        return out.quarantine("assistant", line, "missing or invalid timestamp");
    };
    ctx.last_ts = ctx.last_ts.max(ts);
    let Some(message_id) = msg.id.get().or(rec.uuid.get()).map(str::to_string) else {
        return out.quarantine("assistant", line, "missing message id");
    };
    let creation = usage.cache_creation_input_tokens.unwrap_or(0);
    let (write_5m, write_1h) = match &usage.cache_creation {
        Some(c) if c.ephemeral_5m_input_tokens.is_some() || c.ephemeral_1h_input_tokens.is_some() => {
            let (a, b) = (c.ephemeral_5m_input_tokens.unwrap_or(0), c.ephemeral_1h_input_tokens.unwrap_or(0));
            if a + b == 0 && creation > 0 {
                (creation, 0)
            } else {
                (a, b)
            }
        }
        _ => (creation, 0),
    };
    let sidechain = ctx.subagent || rec.is_sidechain.0;
    let agent_type = if ctx.subagent {
        Some(ctx.agent_type.clone().or_else(|| rec.attribution_agent.clone().into_owned()).unwrap_or_else(|| "subagent".into()))
    } else if sidechain {
        Some(rec.attribution_agent.clone().into_owned().unwrap_or_else(|| "subagent".into()))
    } else {
        None
    };
    if out.cwd.is_none() {
        out.cwd = rec.cwd.get().map(str::to_string);
    }
    let query_source = rec.request_ref.get().and_then(|r| ctx.refs.get(r)).cloned();
    let row = RequestRow {
        message_id,
        request_id: rec.request_id.into_owned(),
        session_id: rec.session_id.into_owned().unwrap_or_else(|| ctx.session_id.clone()),
        project_id: 0,
        ts,
        model: model.to_string(),
        tokens: TokenCounts {
            input: usage.input_tokens.unwrap_or(0),
            output: usage.output_tokens.unwrap_or(0),
            cache_read: usage.cache_read_input_tokens.unwrap_or(0),
            cache_write_5m: write_5m,
            cache_write_1h: write_1h,
        },
        web_search: usage.server_tool_use.as_ref().and_then(|s| s.web_search_requests).unwrap_or(0),
        web_fetch: usage.server_tool_use.as_ref().and_then(|s| s.web_fetch_requests).unwrap_or(0),
        speed: usage.speed.into_owned(),
        service_tier: usage.service_tier.into_owned(),
        effort: rec.effort.into_owned(),
        is_sidechain: sidechain,
        agent_id: rec.agent_id.into_owned().or_else(|| if ctx.subagent { ctx.agent_id.clone() } else { None }),
        agent_type,
        query_source,
        stop_reason: msg.stop_reason.into_owned(),
        git_branch: rec.git_branch.into_owned(),
        cc_version: rec.version.into_owned(),
        entrypoint: rec.entrypoint.into_owned(),
        tools: tools(msg.content.0),
    };
    out.counts.usage_records += 1;
    out.push(row);
}

fn tools(blocks: Vec<Block>) -> Vec<ToolUse> {
    let mut out = Vec::new();
    for b in blocks {
        let (Some(id), Some(name)) = (b.id.get(), b.name.get()) else { continue };
        let tool = |kind, name: &str| ToolUse { use_id: id.to_string(), kind, name: name.to_string() };
        if let Some(rest) = name.strip_prefix("mcp__") {
            match rest.split("__").next().filter(|s| !s.is_empty()) {
                Some(server) => out.push(tool(ToolKind::Mcp, server)),
                None => out.push(tool(ToolKind::Tool, name)),
            }
            continue;
        }
        out.push(tool(ToolKind::Tool, name));
        match name {
            "Skill" => {
                if let Some(skill) = b.input.skill.get() {
                    out.push(tool(ToolKind::Skill, skill));
                }
            }
            "Agent" | "Task" => {
                if let Some(agent) = b.input.subagent_type.get() {
                    out.push(tool(ToolKind::Agent, agent));
                }
            }
            _ => {}
        }
    }
    out
}

fn meta(kind: &'static str, line: &[u8], ctx: &mut FileCtx, out: &mut Chunk) {
    let Ok(rec) = serde_json::from_slice::<MetaRecord>(line) else {
        return out.quarantine(kind, line, "record does not deserialize");
    };
    if rec.kind.get() != Some(kind) {
        return out.quarantine(kind, line, "top-level type differs after parse");
    }
    if let Some(ts) = rec.timestamp.get().and_then(parse_ts) {
        ctx.last_ts = ctx.last_ts.max(ts);
    }
    let session = || rec.session_id.get().map(str::to_string).unwrap_or_else(|| ctx.session_id.clone());
    match kind {
        "ai-title" | "custom-title" => {
            let by_user = kind == "custom-title";
            let title = if by_user { rec.custom_title.get() } else { rec.ai_title.get() };
            if let (true, Some(title)) = (ctx.titles, title.map(str::trim).filter(|t| !t.is_empty())) {
                out.titles.push(TitleSeen { session_id: session(), title: title.to_string(), at: ctx.last_ts, by_user });
            }
        }
        "session-owner" => {
            if let Some(account) = rec.account_uuid.get() {
                out.owners.push((session(), account.to_string()));
            }
        }
        "api-request" => {
            if let (Some(id), Some(source)) = (rec.id.get(), rec.query_source.get()) {
                let (id, source) = (id.to_string(), source.to_string());
                ctx.refs.insert(id, source);
            }
        }
        "tools-used" => {
            let (Some(names), false) = (rec.tools.0.as_ref(), ctx.subagent) else { return };
            let mut names: Vec<&str> = names.iter().filter_map(Str::get).collect();
            names.sort_unstable();
            names.dedup();
            out.tools.push(ToolsSeen {
                session_id: ctx.session_id.clone(),
                offset: ctx.offset,
                ts: ctx.last_ts,
                fingerprint: sha256_hex(names.join("\n").as_bytes()),
            });
        }
        _ => {}
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 15) as usize] as char);
    }
    s
}

fn digits(b: &[u8], from: usize, len: usize) -> Option<i64> {
    let part = b.get(from..from + len)?;
    part.iter().try_fold(0i64, |acc, c| c.is_ascii_digit().then(|| acc * 10 + (c - b'0') as i64))
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// ISO 8601 / RFC 3339 (`2026-11-26T10:20:30.123Z` or with an offset) to epoch milliseconds.
pub(crate) fn parse_ts(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let (y, mo, d) = (digits(b, 0, 4)?, digits(b, 5, 2)?, digits(b, 8, 2)?);
    let (h, mi, sec) = (digits(b, 11, 2)?, digits(b, 14, 2)?, digits(b, 17, 2)?);
    if b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_days = match mo {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=12).contains(&mo) || d < 1 || d > month_days || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let mut i = 19;
    let mut ms = 0;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            if i - start < 3 {
                ms = ms * 10 + (b[i] - b'0') as i64;
            }
            i += 1;
        }
        if i == start {
            return None;
        }
        for _ in (i - start)..3 {
            ms *= 10;
        }
    }
    let offset_min = match b.get(i)? {
        b'Z' | b'z' if i + 1 == b.len() => 0,
        sign @ (b'+' | b'-') if i + 6 == b.len() && b[i + 3] == b':' => {
            let m = digits(b, i + 1, 2)? * 60 + digits(b, i + 4, 2)?;
            if *sign == b'+' {
                m
            } else {
                -m
            }
        }
        _ => return None,
    };
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec - offset_min * 60;
    Some(secs * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps() {
        assert_eq!(parse_ts("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ts("2026-11-25T22:30:00.000Z"), Some(1_795_645_800_000));
        assert_eq!(parse_ts("2026-11-25T22:30:00.5Z"), Some(1_795_645_800_500));
        assert_eq!(parse_ts("2026-11-25T23:30:00.123456+01:00"), Some(1_795_645_800_123));
        assert_eq!(parse_ts("2026-11-25"), None);
        assert_eq!(parse_ts("2026-13-25T22:30:00Z"), None);
        assert_eq!(parse_ts("2026-02-31T22:30:00Z"), None);
        assert!(parse_ts("2028-02-29T00:00:00Z").is_some());
        assert_eq!(parse_ts("2026-11-25T22:30:00"), None);
    }

    #[test]
    fn tool_mapping() {
        let line = br#"{"type":"assistant","timestamp":"2026-11-25T22:30:00Z","sessionId":"s","message":{"id":"msg_1","model":"m",
            "usage":{"input_tokens":1},"content":[
            {"type":"tool_use","id":"t1","name":"mcp__plugin_tracker_tracker__create","input":{"title":"secret"}},
            {"type":"tool_use","id":"t2","name":"Skill","input":{"skill":"commit","args":"secret"}},
            {"type":"tool_use","id":"t3","name":"Task","input":{"subagent_type":"Explore","prompt":"secret"}},
            {"type":"tool_use","id":"t4","name":"Bash","input":"odd"},
            {"type":"text","text":"secret"}, "odd", 5]}}"#;
        let mut refs = QueryRefs::default();
        let mut ctx = FileCtx {
            subagent: false,
            session_id: "s".into(),
            agent_id: None,
            agent_type: None,
            titles: false,
            refs: &mut refs,
            offset: 0,
            last_ts: 0,
        };
        let mut chunk = Chunk::default();
        process_line(line, &mut ctx, &mut chunk);
        assert!(chunk.quarantine.is_empty(), "{:?}", chunk.quarantine);
        let got: Vec<(String, &str, String)> =
            chunk.rows[0].tools.iter().map(|t| (t.use_id.clone(), t.kind.as_str(), t.name.clone())).collect();
        let want = [
            ("t1", "mcp", "plugin_tracker_tracker"),
            ("t2", "tool", "Skill"),
            ("t2", "skill", "commit"),
            ("t3", "tool", "Task"),
            ("t3", "agent", "Explore"),
            ("t4", "tool", "Bash"),
        ];
        assert_eq!(got, want.map(|(a, b, c)| (a.to_string(), b, c.to_string())));
    }
}

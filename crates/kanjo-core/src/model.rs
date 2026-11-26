use serde::{Deserialize, Serialize};

/// Token counts of one API response, exactly as reported in the transcript.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCounts {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write_5m: i64,
    pub cache_write_1h: i64,
}

impl TokenCounts {
    pub fn total(&self) -> i64 {
        self.input + self.output + self.cache_read + self.cache_write_5m + self.cache_write_1h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// Built-in or custom tool, by name.
    Tool,
    /// MCP server name (the `server` part of `mcp__server__tool`).
    Mcp,
    /// Skill name passed to the Skill tool.
    Skill,
    /// Subagent type passed to the Agent/Task tool.
    Agent,
}

impl ToolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolKind::Tool => "tool",
            ToolKind::Mcp => "mcp",
            ToolKind::Skill => "skill",
            ToolKind::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolUse {
    /// Id of the `tool_use` content block (`toolu_...`).
    pub use_id: String,
    pub kind: ToolKind,
    pub name: String,
}

/// One billable API response. This is the only shape that enters `requests`;
/// it carries usage and metadata, never prompt or response text.
#[derive(Debug, Clone, Default)]
pub struct RequestRow {
    /// `message.id` (`msg_...`); the global dedup key. Falls back to the record uuid.
    pub message_id: String,
    pub request_id: Option<String>,
    pub session_id: String,
    pub project_id: i64,
    pub ts: i64,
    pub model: String,
    pub tokens: TokenCounts,
    pub web_search: i64,
    pub web_fetch: i64,
    pub speed: Option<String>,
    pub service_tier: Option<String>,
    pub effort: Option<String>,
    pub is_sidechain: bool,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub query_source: Option<String>,
    pub stop_reason: Option<String>,
    pub git_branch: Option<String>,
    pub cc_version: Option<String>,
    pub entrypoint: Option<String>,
    pub tools: Vec<ToolUse>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    /// The same message was seen again with higher token counts (streaming); totals were adjusted.
    Updated,
    Duplicate,
    /// The record lies inside a purge mark (deleted on request or by retention) and was not stored.
    Purged,
}

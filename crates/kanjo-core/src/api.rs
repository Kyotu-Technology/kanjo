//! The API contract: every JSON shape exchanged between the engine, the HTTP
//! server, the CLI (`--json`) and the web UI. TypeScript bindings are
//! generated from these types (`cargo test -p kanjo-core export_bindings`), so
//! a change here is a change of the public interface.
//!
//! Conventions: timestamps are epoch milliseconds; money is USD (`*_usd`);
//! ratios are 0..1 (`*_ratio`, `share`); percentages are 0..100 (`*_pct`).

use crate::model::TokenCounts;
use crate::settings::{PlanKind, PlanSettings, RetentionSettings};
use std::collections::BTreeMap;

macro_rules! dto {
    ($($item:item)*) => {
        $(
            #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
            #[ts(export)]
            $item
        )*
    };
}

pub type Ts = i64;

dto! {
    /// How far a number can be trusted. Shown next to every figure in the UI.
    #[derive(Copy, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum Confidence {
        /// Reported by Claude Code itself (statusline limits, session cost).
        Official,
        /// Derived by Kanjo: forecasts, calibrations, attribution, suggestions.
        Estimate,
        /// Summed from transcripts, which can miss requests: the real figure is at least this.
        LowerBound,
    }

    #[derive(Copy, Default, PartialEq, Eq)]
    pub struct Tokens {
        pub input: i64,
        pub output: i64,
        pub cache_read: i64,
        pub cache_write_5m: i64,
        pub cache_write_1h: i64,
    }

    /// The unit every report is built from.
    #[derive(Copy, Default, PartialEq)]
    pub struct Usage {
        pub tokens: Tokens,
        /// API-equivalent list price of the priced requests.
        pub cost_usd: f64,
        pub requests: i64,
        /// Requests whose model has no price; their cost is missing from `cost_usd`.
        pub unpriced_requests: i64,
    }

    #[derive(Copy, Default, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum Granularity {
        Hour,
        #[default]
        Day,
        Week,
        Month,
    }

    #[derive(Copy, Default, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum Dimension {
        #[default]
        Project,
        Model,
        /// Subagent type; the main conversation is reported as `main`.
        Agent,
        Skill,
        Mcp,
        Tool,
        Branch,
        Client,
        Session,
    }

    /// Query parameters shared by the read endpoints. Each endpoint documents
    /// which fields it honours; the rest are ignored.
    #[derive(Default)]
    #[serde(default)]
    pub struct Query {
        pub from: Option<Ts>,
        pub to: Option<Ts>,
        pub project: Option<i64>,
        pub model: Option<String>,
        pub agent: Option<String>,
        pub client: Option<String>,
        pub group: Option<Granularity>,
        pub by: Option<Dimension>,
        /// Free-text filter (sessions, search).
        pub q: Option<String>,
        /// `cost` (default), `recent`, `tokens`, `duration`.
        pub sort: Option<String>,
        pub limit: Option<i64>,
        pub offset: Option<i64>,
        /// `week` or `month` (comparison).
        pub period: Option<String>,
        /// `en` or `pl`; language of rendered alert texts.
        pub lang: Option<String>,
        /// `json` or `csv` (export).
        pub format: Option<String>,
    }

    // ---------------------------------------------------------------- overview

    pub struct PeriodSummary {
        pub from: Ts,
        pub to: Ts,
        pub usage: Usage,
        /// The same stretch of the previous period (e.g. last week up to the same weekday and hour).
        pub previous: Usage,
        /// Cost change against `previous` in percent; `None` when there is no baseline.
        pub delta_pct: Option<f64>,
    }

    pub struct BurnRate {
        pub usd_per_hour: f64,
        pub tokens_per_min: f64,
        /// Minutes of recent activity the rate is computed over.
        pub window_minutes: i64,
        /// False when nothing happened in the window.
        pub active: bool,
    }

    pub struct DataStatus {
        /// A statusline payload arrived recently, so limit figures are official.
        pub statusline_connected: bool,
        pub last_statusline_at: Option<Ts>,
        pub last_request_at: Option<Ts>,
        pub first_request_at: Option<Ts>,
        pub unpriced_models: Vec<String>,
        pub quarantined_records: i64,
        pub cost_confidence: Confidence,
    }

    pub struct Overview {
        pub generated_at: Ts,
        pub today: PeriodSummary,
        pub week: PeriodSummary,
        pub month: PeriodSummary,
        pub burn: BurnRate,
        pub limits: Limits,
        pub active_block: Option<Block>,
        pub active_sessions: Vec<SessionSummary>,
        pub top_projects: Vec<BreakdownRow>,
        pub top_models: Vec<BreakdownRow>,
        /// Last 24 hours, hourly.
        pub hourly: Vec<UsageBucket>,
        /// Last 30 days, daily.
        pub daily: Vec<UsageBucket>,
        pub data: DataStatus,
    }

    // ------------------------------------------------------------------ limits

    pub struct Limits {
        pub five_hour: Option<LimitWindow>,
        pub seven_day: Option<LimitWindow>,
        pub spend: Option<SpendLimit>,
        /// Present when there is no live reading and history allows a calibration.
        pub estimate: Option<LimitEstimate>,
    }

    pub struct LimitWindow {
        pub used_pct: f64,
        pub resets_at: Ts,
        pub observed_at: Ts,
        /// The reading is older than 15 minutes; usage may have moved since.
        pub stale: bool,
        pub rate_pct_per_hour: Option<f64>,
        /// Forecast moment of exhaustion, when it falls before the reset.
        pub exhausts_at: Option<Ts>,
        pub will_exhaust: bool,
        /// API-equivalent cost accumulated inside this window.
        pub usd_in_window: f64,
        pub usd_per_pct: Option<f64>,
        /// Percentage points per day that can still be spent evenly until the reset (7-day window).
        pub daily_budget_pct: Option<f64>,
        pub confidence: Confidence,
    }

    pub struct SpendLimit {
        pub used_pct: f64,
        pub resets_at: Ts,
        pub observed_at: Ts,
        pub used_usd: Option<f64>,
        pub limit_usd: Option<f64>,
        pub period: Option<String>,
        pub exhausts_at: Option<Ts>,
        pub will_exhaust: bool,
    }

    /// Calibration from history for accounts without a statusline reading.
    pub struct LimitEstimate {
        /// 90th percentile cost of past 5-hour blocks, used as the assumed ceiling.
        pub p90_block_usd: f64,
        pub blocks_observed: i64,
        pub current_block_usd: f64,
        pub current_block_pct: f64,
        pub confidence: Confidence,
    }

    pub struct LimitPoint {
        pub ts: Ts,
        pub used_pct: f64,
        pub resets_at: Ts,
    }

    pub struct UsdPerPctPoint {
        pub week_start: Ts,
        pub usd: f64,
        /// Peak 7-day percentage reached in that window.
        pub pct: f64,
        pub usd_per_pct: Option<f64>,
    }

    pub struct LimitsDetail {
        pub current: Limits,
        pub history_5h: Vec<LimitPoint>,
        pub history_7d: Vec<LimitPoint>,
        /// Cost bought by one percentage point of the weekly limit, week by week.
        pub usd_per_pct_weekly: Vec<UsdPerPctPoint>,
        pub blocks: Vec<Block>,
    }

    /// A 5-hour usage window.
    pub struct Block {
        pub start: Ts,
        pub end: Ts,
        pub is_active: bool,
        /// True when the window is anchored to a real `resets_at`; false when inferred from activity.
        pub anchored: bool,
        pub peak_pct: Option<f64>,
        pub usage: Usage,
        pub by_model: Vec<BreakdownRow>,
        pub by_project: Vec<BreakdownRow>,
        pub burn_usd_per_hour: Option<f64>,
        /// Cost at the end of the block if the current burn rate holds (active block only).
        pub projected_usd: Option<f64>,
    }

    // ----------------------------------------------------------------- reports

    pub struct ModelSlice {
        pub model: String,
        pub cost_usd: f64,
        pub tokens: i64,
    }

    pub struct UsageBucket {
        pub start: Ts,
        /// `YYYY-MM-DD`, `YYYY-MM-DD HH:00`, `YYYY-Www` or `YYYY-MM`, in local time.
        pub label: String,
        pub usage: Usage,
        pub by_model: Vec<ModelSlice>,
    }

    pub struct UsageReport {
        pub group: Granularity,
        pub from: Ts,
        pub to: Ts,
        pub buckets: Vec<UsageBucket>,
        pub total: Usage,
        pub confidence: Confidence,
    }

    pub struct BreakdownRow {
        /// Stable identifier within the dimension (project id, model name, ...).
        pub key: String,
        pub label: String,
        pub usage: Usage,
        /// Share of the total cost, 0..1.
        pub share: f64,
    }

    pub struct Breakdown {
        pub by: Dimension,
        pub from: Ts,
        pub to: Ts,
        pub rows: Vec<BreakdownRow>,
        pub total: Usage,
        /// `Estimate` for skill / MCP / tool, where cost is attributed by the requests that invoked them.
        pub confidence: Confidence,
    }

    pub struct HeatCell {
        /// 0 = Monday ... 6 = Sunday, local time.
        pub dow: u8,
        pub hour: u8,
        pub cost_usd: f64,
        pub tokens: i64,
        pub requests: i64,
    }

    pub struct DayCell {
        pub date: String,
        pub cost_usd: f64,
        pub tokens: i64,
        pub requests: i64,
    }

    pub struct Heatmap {
        pub from: Ts,
        pub to: Ts,
        pub cells: Vec<HeatCell>,
        /// One entry per local calendar day in range, including empty days.
        pub days: Vec<DayCell>,
    }

    pub struct Driver {
        pub dimension: Dimension,
        pub key: String,
        pub label: String,
        pub current_usd: f64,
        pub previous_usd: f64,
        pub delta_usd: f64,
    }

    pub struct ComparisonSide {
        pub from: Ts,
        pub to: Ts,
        pub label: String,
        pub usage: Usage,
    }

    /// Period-over-period diff with the projects, models and agents that explain it.
    pub struct Comparison {
        pub period: String,
        pub current: ComparisonSide,
        pub previous: ComparisonSide,
        pub delta_usd: f64,
        pub delta_pct: Option<f64>,
        /// Largest absolute changes first.
        pub drivers: Vec<Driver>,
    }

    // ---------------------------------------------------------------- sessions

    pub struct SessionSummary {
        pub id: String,
        pub project_id: i64,
        pub project: String,
        pub title: Option<String>,
        pub git_branch: Option<String>,
        pub started_at: Ts,
        pub last_at: Ts,
        pub usage: Usage,
        pub main_model: Option<String>,
        pub models: Vec<String>,
        pub is_active: bool,
        pub subagent_cost_usd: f64,
        pub context_pct: Option<f64>,
        pub cache_hit_ratio: Option<f64>,
    }

    pub struct SessionList {
        pub items: Vec<SessionSummary>,
        pub total: i64,
    }

    /// One API response. Metadata only.
    pub struct Turn {
        pub message_id: String,
        pub session_id: String,
        pub project: String,
        pub ts: Ts,
        pub model: String,
        pub tokens: Tokens,
        pub cost_usd: Option<f64>,
        pub agent_type: Option<String>,
        pub tools: Vec<String>,
        pub stop_reason: Option<String>,
    }

    pub struct CacheStatus {
        pub warm: Option<bool>,
        pub ttl: Option<String>,
        pub expires_at: Option<Ts>,
        pub requests: Option<i64>,
        pub misses: Option<i64>,
        pub expected_rebuilds: Option<i64>,
        pub hit_ratio: Option<f64>,
        pub write_tokens: Option<i64>,
        pub miss_recache_tokens: Option<i64>,
        pub last_miss_at: Option<Ts>,
        pub miss_causes: BTreeMap<String, i64>,
        pub recache_tokens_if_cold: Option<i64>,
    }

    /// Latest statusline snapshot of a session (official figures).
    pub struct SessionStatus {
        pub ts: Ts,
        pub model: Option<String>,
        pub cost_usd: Option<f64>,
        pub duration_ms: Option<i64>,
        pub api_duration_ms: Option<i64>,
        pub lines_added: Option<i64>,
        pub lines_removed: Option<i64>,
        pub ctx_used_pct: Option<f64>,
        pub ctx_size: Option<i64>,
        pub exceeds_200k: Option<bool>,
        pub cache: Option<CacheStatus>,
    }

    pub struct ConfigChange {
        pub ts: Ts,
        /// `claude_md`, `settings`, `plugins` or `tools`.
        pub kind: String,
        pub target: String,
    }

    pub struct CacheMiss {
        pub session_id: String,
        pub project: String,
        pub ts: Ts,
        pub causes: Vec<String>,
        pub tools_added: Option<i64>,
        pub tools_removed: Option<i64>,
        pub system_char_delta: Option<i64>,
        /// Prompt-shaping files that changed shortly before the miss.
        pub correlated_changes: Vec<ConfigChange>,
    }

    pub struct SessionEvent {
        pub ts: Ts,
        pub event: String,
        pub source: Option<String>,
    }

    pub struct SessionDetail {
        pub summary: SessionSummary,
        pub timeline: Vec<UsageBucket>,
        pub by_model: Vec<BreakdownRow>,
        pub by_agent: Vec<BreakdownRow>,
        pub tools: Vec<BreakdownRow>,
        pub top_turns: Vec<Turn>,
        pub status: Option<SessionStatus>,
        pub cache_misses: Vec<CacheMiss>,
        pub events: Vec<SessionEvent>,
        pub burn: BurnRate,
    }

    pub struct TranscriptMessage {
        pub ts: Option<Ts>,
        /// `user`, `assistant` or `system`.
        pub role: String,
        /// `text`, `thinking`, `tool_use` or `tool_result`.
        pub kind: String,
        pub text: String,
        pub model: Option<String>,
        pub tool: Option<String>,
    }

    /// Read on demand from the transcript file; never stored by Kanjo.
    pub struct Transcript {
        pub session_id: String,
        pub messages: Vec<TranscriptMessage>,
        pub truncated: bool,
    }

    pub struct Project {
        pub id: i64,
        pub name: String,
        /// `None` when path redaction is on.
        pub path: Option<String>,
        pub client: Option<String>,
        pub first_seen: Ts,
        pub last_seen: Ts,
        pub sessions: i64,
        pub usage_30d: Usage,
        pub usage_total: Usage,
    }

    #[derive(Default)]
    #[serde(default)]
    pub struct ProjectPatch {
        pub name: Option<String>,
        /// `Some("")` clears the client.
        pub client: Option<String>,
    }

    // ------------------------------------------------------------ plan, insights

    #[derive(Copy, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum PlanAction {
        Keep,
        Upgrade,
        Downgrade,
        /// Not enough history or no plan chosen.
        Unknown,
    }

    pub struct PlanMonth {
        /// `YYYY-MM`.
        pub month: String,
        pub api_equivalent_usd: f64,
        pub subscription_usd: Option<f64>,
    }

    pub struct PlanFit {
        pub plan: PlanSettings,
        pub months: Vec<PlanMonth>,
        /// Last 30 days.
        pub api_equivalent_usd: f64,
        pub subscription_usd: Option<f64>,
        /// API-equivalent value per subscription dollar.
        pub roi: Option<f64>,
        pub max_five_hour_pct: Option<f64>,
        pub max_seven_day_pct: Option<f64>,
        /// 5-hour windows in the last 30 days that peaked at 90% or more.
        pub blocks_near_limit: i64,
        pub action: PlanAction,
        pub target: Option<PlanKind>,
        /// Machine-readable reason, e.g. `frequent_limit_hits`, `low_utilisation`, `api_cheaper`, `no_plan`, `no_history`.
        pub reason: String,
        pub confidence: Confidence,
    }

    pub struct CacheRow {
        pub key: String,
        pub label: String,
        pub hit_ratio: Option<f64>,
        pub cache_read: i64,
        pub cache_write: i64,
        pub input: i64,
    }

    pub struct CauseCount {
        pub cause: String,
        pub count: i64,
    }

    pub struct CacheBreaker {
        pub change: ConfigChange,
        pub misses_after: i64,
        pub sessions_affected: i64,
    }

    pub struct CacheInsights {
        /// cache read / (cache read + cache write + uncached input), last 30 days.
        pub hit_ratio: Option<f64>,
        pub by_project: Vec<CacheRow>,
        pub miss_causes: Vec<CauseCount>,
        pub recent_misses: Vec<CacheMiss>,
        pub breakers: Vec<CacheBreaker>,
    }

    pub struct ModelMixSuggestion {
        /// `subagents_to_haiku`, `short_turns_to_sonnet`, ...; the UI owns the wording.
        pub code: String,
        pub from_model: String,
        pub to_model: String,
        pub scope: String,
        pub affected_requests: i64,
        pub current_usd: f64,
        pub projected_usd: f64,
        pub saving_usd: f64,
        pub confidence: Confidence,
    }

    pub struct ContextWarning {
        pub session_id: String,
        pub project: String,
        pub ts: Ts,
        pub ctx_used_pct: f64,
        pub ctx_size: Option<i64>,
        pub exceeds_200k: bool,
    }

    #[derive(Copy, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum AnomalyKind {
        /// A day far above the trailing median.
        CostSpike,
        /// A session far more expensive than the typical one and still growing.
        RunawaySession,
        /// Many requests with almost no code changes.
        AgentLoop,
    }

    pub struct Anomaly {
        pub kind: AnomalyKind,
        pub ts: Ts,
        pub session_id: Option<String>,
        pub project: Option<String>,
        pub value: f64,
        pub baseline: f64,
    }

    pub struct Insights {
        pub generated_at: Ts,
        pub cache: CacheInsights,
        pub model_mix: Vec<ModelMixSuggestion>,
        pub context_warnings: Vec<ContextWarning>,
        pub anomalies: Vec<Anomaly>,
    }

    // ------------------------------------------------------- alerts and budgets

    #[derive(Copy, PartialEq, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum AlertKind {
        LimitThreshold,
        LimitForecast,
        BudgetWarning,
        BudgetExceeded,
        CostSpike,
        RunawaySession,
        AgentLoop,
        CacheCold,
        CacheMissStreak,
        ContextNearLimit,
    }

    #[derive(Copy, PartialEq, Eq, PartialOrd, Ord)]
    #[serde(rename_all = "snake_case")]
    pub enum Severity {
        Info,
        Warning,
        Critical,
    }

    pub struct Alert {
        pub id: i64,
        pub ts: Ts,
        pub kind: AlertKind,
        pub severity: Severity,
        /// Raw values behind the alert (window, percentages, ids), for the UI to link and format.
        pub params: serde_json::Value,
        /// Rendered in the requested language.
        pub title: String,
        pub message: String,
        pub acknowledged_at: Option<Ts>,
    }

    pub struct AlertList {
        pub items: Vec<Alert>,
        pub total: i64,
        pub unacknowledged: i64,
    }

    #[derive(Copy, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum BudgetScope {
        Global,
        Project,
        Client,
        Account,
    }

    #[derive(Copy, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum BudgetPeriod {
        Day,
        Week,
        Month,
    }

    #[derive(Copy, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum BudgetState {
        Ok,
        Warning,
        Exceeded,
    }

    pub struct BudgetInput {
        pub name: String,
        pub scope: BudgetScope,
        /// Project id, client name or account id, depending on `scope`.
        pub scope_value: Option<String>,
        pub period: BudgetPeriod,
        pub amount_usd: f64,
        pub warn_pct: f64,
        pub enabled: bool,
    }

    pub struct Budget {
        pub id: i64,
        pub name: String,
        pub scope: BudgetScope,
        pub scope_value: Option<String>,
        pub scope_label: Option<String>,
        pub period: BudgetPeriod,
        pub amount_usd: f64,
        pub warn_pct: f64,
        pub enabled: bool,
        pub period_start: Ts,
        pub period_end: Ts,
        pub spent_usd: f64,
        pub spent_pct: f64,
        /// Spend at the end of the period if the pace so far holds.
        pub projected_usd: f64,
        pub state: BudgetState,
    }

    // ------------------------------------------------------- system and privacy

    #[derive(Copy, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum CheckStatus {
        Ok,
        Warn,
        Fail,
    }

    /// One line of `kanjo doctor`.
    pub struct DoctorCheck {
        pub id: String,
        pub status: CheckStatus,
        pub title: String,
        pub detail: String,
        /// What the user can do about it, when something can be done.
        pub fix: Option<String>,
    }

    #[derive(Copy, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum KeySource {
        Keyring,
        Keyfile,
        /// The database is not encrypted.
        None,
    }

    pub struct DatabaseInfo {
        pub size_bytes: i64,
        pub encrypted: bool,
        pub key_source: KeySource,
        pub schema_version: i64,
    }

    #[derive(Default)]
    pub struct IngestStatus {
        pub backfill_running: bool,
        pub watcher_running: bool,
        pub files_total: i64,
        pub files_done: i64,
        pub bytes_total: i64,
        pub bytes_done: i64,
        pub requests_stored: i64,
        pub last_event_at: Option<Ts>,
    }

    pub struct HookStatus {
        pub statusline_installed: bool,
        /// The user's own statusLine command that Kanjo forwards to, if any.
        pub statusline_chain: Option<String>,
        pub session_hooks_installed: bool,
        pub last_statusline_at: Option<Ts>,
        pub service_installed: bool,
    }

    pub struct PricingInfo {
        pub version: String,
        pub source: String,
        pub models: i64,
        /// True for an imported table whose signature was verified.
        pub signed: bool,
        pub unpriced_models: Vec<String>,
    }

    pub struct QuarantineRow {
        pub record_type: String,
        pub signature: String,
        pub reason: String,
        pub cc_version: String,
        pub count: i64,
        pub last_seen: Ts,
    }

    pub struct ServerInfo {
        pub bind: String,
        pub port: u16,
        pub tls: bool,
        pub auth: bool,
    }

    pub struct SystemInfo {
        pub version: String,
        pub data_dir: String,
        pub claude_dir: String,
        pub claude_code_version: Option<String>,
        pub database: DatabaseInfo,
        pub ingest: IngestStatus,
        pub hooks: HookStatus,
        pub pricing: PricingInfo,
        pub server: ServerInfo,
        pub quarantine: Vec<QuarantineRow>,
        pub retention: RetentionSettings,
        pub checks: Vec<DoctorCheck>,
    }

    pub struct AuditEntry {
        pub id: i64,
        pub ts: Ts,
        pub actor: String,
        /// e.g. `export`, `transcript.view`, `settings.update`, `data.purge`, `backup.create`, `pricing.import`, `hooks.install`.
        pub action: String,
        pub target: String,
        pub detail: serde_json::Value,
        pub hash: String,
    }

    pub struct AuditPage {
        pub items: Vec<AuditEntry>,
        pub total: i64,
        /// Result of verifying the hash chain over the whole log.
        pub chain_ok: bool,
    }

    #[derive(Default)]
    pub struct PurgeReport {
        pub requests: i64,
        pub sessions: i64,
        pub aggregates: i64,
        pub limit_samples: i64,
        pub alerts: i64,
        pub audit_entries: i64,
        pub quarantine: i64,
    }

    /// Deletion on request. Exactly one selector, or `everything`.
    #[derive(Default)]
    #[serde(default)]
    pub struct PurgeRequest {
        pub project: Option<i64>,
        pub session: Option<String>,
        pub before: Option<Ts>,
        pub everything: bool,
    }

    pub struct BackupInfo {
        pub file: String,
        pub created_at: Ts,
        pub size_bytes: i64,
    }

    pub struct SearchResult {
        /// `session`, `project`, `model`, `page` or `action`.
        pub kind: String,
        pub id: String,
        pub label: String,
        pub sublabel: Option<String>,
    }

    /// Request body of the hook installer.
    #[derive(Default)]
    #[serde(default)]
    pub struct HookInstallRequest {
        pub statusline: bool,
        pub session_hooks: bool,
        pub service: bool,
    }

    /// Server-sent events on `/api/v1/events`.
    #[allow(clippy::large_enum_variant)]
    #[serde(tag = "type", content = "data", rename_all = "snake_case")]
    pub enum LiveEvent {
        /// New requests were stored; clients refetch what they show.
        Usage { requests: i64, cost_usd: f64, sessions: Vec<String> },
        Limits(Limits),
        Alert(Alert),
        Ingest(IngestStatus),
        /// A statusline snapshot changed for this session.
        Status { session_id: String },
    }

    pub struct ApiError {
        /// Machine-readable code, e.g. `not_found`, `invalid`, `unauthorized`, `disabled`.
        pub code: String,
        pub message: String,
    }
}

impl From<TokenCounts> for Tokens {
    fn from(t: TokenCounts) -> Tokens {
        Tokens {
            input: t.input,
            output: t.output,
            cache_read: t.cache_read,
            cache_write_5m: t.cache_write_5m,
            cache_write_1h: t.cache_write_1h,
        }
    }
}

impl Tokens {
    pub fn total(&self) -> i64 {
        self.input + self.output + self.cache_read + self.cache_write_5m + self.cache_write_1h
    }

    /// cache read / (cache read + cache writes + uncached input); `None` when there was no input at all.
    pub fn cache_hit_ratio(&self) -> Option<f64> {
        let denom = self.cache_read + self.cache_write_5m + self.cache_write_1h + self.input;
        (denom > 0).then(|| self.cache_read as f64 / denom as f64)
    }
}

impl std::ops::AddAssign for Usage {
    fn add_assign(&mut self, o: Usage) {
        self.tokens.input += o.tokens.input;
        self.tokens.output += o.tokens.output;
        self.tokens.cache_read += o.tokens.cache_read;
        self.tokens.cache_write_5m += o.tokens.cache_write_5m;
        self.tokens.cache_write_1h += o.tokens.cache_write_1h;
        self.cost_usd += o.cost_usd;
        self.requests += o.requests;
        self.unpriced_requests += o.unpriced_requests;
    }
}

impl std::iter::Sum for Usage {
    fn sum<I: Iterator<Item = Usage>>(iter: I) -> Usage {
        let mut total = Usage::default();
        for u in iter {
            total += u;
        }
        total
    }
}

/// Percent change from `previous` to `current`; `None` without a baseline.
pub fn delta_pct(current: f64, previous: f64) -> Option<f64> {
    (previous.abs() > 1e-9).then(|| (current - previous) / previous * 100.0)
}

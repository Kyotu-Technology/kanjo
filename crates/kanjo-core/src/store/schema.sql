-- Kanjo schema v1. All timestamps are Unix epoch milliseconds (UTC).
-- No table stores prompt or response text.

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE projects (
  id INTEGER PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,          -- directory name under <claude>/projects
  path TEXT,                         -- working directory seen in records
  name TEXT NOT NULL,                -- display name, user-editable
  alias TEXT NOT NULL,               -- stable pseudonym shown when path redaction is on
  client TEXT,                       -- user-assigned client / cost centre
  first_seen INTEGER NOT NULL,
  last_seen INTEGER NOT NULL
);

CREATE TABLE sessions (
  id TEXT PRIMARY KEY,
  project_id INTEGER NOT NULL REFERENCES projects(id),
  title TEXT,                        -- only filled when privacy.session_titles is on
  git_branch TEXT,
  cc_version TEXT,
  entrypoint TEXT,
  account TEXT,                      -- opaque account id from the transcript, if present
  started_at INTEGER NOT NULL,
  last_at INTEGER NOT NULL,
  requests INTEGER NOT NULL DEFAULT 0,
  input INTEGER NOT NULL DEFAULT 0,
  output INTEGER NOT NULL DEFAULT 0,
  cache_read INTEGER NOT NULL DEFAULT 0,
  cache_write_5m INTEGER NOT NULL DEFAULT 0,
  cache_write_1h INTEGER NOT NULL DEFAULT 0,
  cost_usd REAL NOT NULL DEFAULT 0,
  unpriced_requests INTEGER NOT NULL DEFAULT 0,
  sidechain_requests INTEGER NOT NULL DEFAULT 0,
  sidechain_cost_usd REAL NOT NULL DEFAULT 0
);
CREATE INDEX sessions_last_at ON sessions(last_at);
CREATE INDEX sessions_project ON sessions(project_id, last_at);

CREATE TABLE requests (
  id INTEGER PRIMARY KEY,
  message_id TEXT NOT NULL UNIQUE,
  request_id TEXT,
  session_id TEXT NOT NULL,
  project_id INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  model TEXT NOT NULL,
  input INTEGER NOT NULL DEFAULT 0,
  output INTEGER NOT NULL DEFAULT 0,
  cache_read INTEGER NOT NULL DEFAULT 0,
  cache_write_5m INTEGER NOT NULL DEFAULT 0,
  cache_write_1h INTEGER NOT NULL DEFAULT 0,
  web_search INTEGER NOT NULL DEFAULT 0,
  web_fetch INTEGER NOT NULL DEFAULT 0,
  cost_usd REAL,                     -- NULL: model missing from the price table
  price_version TEXT,
  speed TEXT,
  service_tier TEXT,
  effort TEXT,
  is_sidechain INTEGER NOT NULL DEFAULT 0,
  agent_id TEXT,
  agent_type TEXT,
  query_source TEXT,
  stop_reason TEXT,
  git_branch TEXT,
  cc_version TEXT
);
CREATE INDEX requests_ts ON requests(ts);
CREATE INDEX requests_session ON requests(session_id, ts);
CREATE INDEX requests_project ON requests(project_id, ts);

-- One row per tool_use block and kind. A response is written to the transcript
-- as several records, so tool rows arrive piecemeal; the key makes re-reading idempotent.
CREATE TABLE request_tools (
  request_id INTEGER NOT NULL REFERENCES requests(id) ON DELETE CASCADE,
  use_id TEXT NOT NULL,              -- tool_use block id
  kind TEXT NOT NULL,                -- tool | mcp | skill | agent
  name TEXT NOT NULL,
  PRIMARY KEY (request_id, use_id, kind)
) WITHOUT ROWID;
CREATE INDEX request_tools_name ON request_tools(kind, name);

-- Long-lived aggregate. Survives the purge of old request rows.
CREATE TABLE usage_hourly (
  hour INTEGER NOT NULL,             -- epoch ms floored to the UTC hour
  project_id INTEGER NOT NULL,
  model TEXT NOT NULL,
  agent_type TEXT NOT NULL DEFAULT '',  -- '' = main conversation
  requests INTEGER NOT NULL DEFAULT 0,
  input INTEGER NOT NULL DEFAULT 0,
  output INTEGER NOT NULL DEFAULT 0,
  cache_read INTEGER NOT NULL DEFAULT 0,
  cache_write_5m INTEGER NOT NULL DEFAULT 0,
  cache_write_1h INTEGER NOT NULL DEFAULT 0,
  cost_usd REAL NOT NULL DEFAULT 0,
  unpriced_requests INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (hour, project_id, model, agent_type)
) WITHOUT ROWID;

-- Official subscription limit readings from the statusline payload.
CREATE TABLE limit_samples (
  kind TEXT NOT NULL,                -- five_hour | seven_day | spend
  ts INTEGER NOT NULL,
  used_pct REAL NOT NULL,
  resets_at INTEGER NOT NULL,
  used_usd REAL,
  limit_usd REAL,
  period TEXT,
  PRIMARY KEY (kind, ts)
) WITHOUT ROWID;

-- Latest statusline snapshot per session.
CREATE TABLE session_status (
  session_id TEXT PRIMARY KEY,
  ts INTEGER NOT NULL,
  model_id TEXT,
  model_name TEXT,
  cc_version TEXT,
  project_dir TEXT,
  session_name TEXT,                 -- only when privacy.session_titles is on
  cost_usd REAL,
  duration_ms INTEGER,
  api_duration_ms INTEGER,
  lines_added INTEGER,
  lines_removed INTEGER,
  ctx_used_pct REAL,
  ctx_size INTEGER,
  ctx_tokens INTEGER,
  exceeds_200k INTEGER,
  fast_mode INTEGER,
  effort TEXT,
  cache_warm INTEGER,
  cache_ttl TEXT,
  cache_expires_at INTEGER,
  cache_requests INTEGER,
  cache_misses INTEGER,
  cache_expected_rebuilds INTEGER,
  cache_hit_ratio REAL,
  cache_write_tokens INTEGER,
  cache_miss_recache_tokens INTEGER,
  cache_last_miss_at INTEGER,
  cache_recache_if_cold INTEGER,
  cache_miss_causes TEXT             -- JSON object: cause -> count
);

CREATE TABLE cache_misses (
  session_id TEXT NOT NULL,
  ts INTEGER NOT NULL,
  causes TEXT NOT NULL,              -- JSON array of cause names
  tools_added INTEGER,
  tools_removed INTEGER,
  system_char_delta INTEGER,
  PRIMARY KEY (session_id, ts)
) WITHOUT ROWID;
CREATE INDEX cache_misses_ts ON cache_misses(ts);

-- SessionStart / Stop / SessionEnd hook markers.
CREATE TABLE session_events (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  session_id TEXT NOT NULL,
  event TEXT NOT NULL,
  source TEXT,
  cwd TEXT,
  model TEXT,
  agent_type TEXT
);
CREATE INDEX session_events_session ON session_events(session_id, ts);

-- Changes to files that shape the prompt prefix (CLAUDE.md, settings, plugins).
-- Only a fingerprint is kept, never content.
CREATE TABLE config_changes (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  kind TEXT NOT NULL,                -- claude_md | settings | plugins | tools
  target TEXT NOT NULL,              -- e.g. "user CLAUDE.md", project alias
  project_id INTEGER,
  fingerprint TEXT NOT NULL
);
CREATE INDEX config_changes_ts ON config_changes(ts);

CREATE TABLE ingest_files (
  path TEXT PRIMARY KEY,
  kind TEXT NOT NULL,                -- transcript | subagent | spool
  size INTEGER NOT NULL,
  offset INTEGER NOT NULL,
  mtime INTEGER NOT NULL,
  session_id TEXT,
  project_id INTEGER,
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;

-- Records the parser could not understand. Shape only: type, sorted key
-- signature and a counter. Never the record itself.
CREATE TABLE quarantine (
  record_type TEXT NOT NULL,
  signature TEXT NOT NULL,
  reason TEXT NOT NULL,
  cc_version TEXT NOT NULL DEFAULT '',
  count INTEGER NOT NULL DEFAULT 0,
  first_seen INTEGER NOT NULL,
  last_seen INTEGER NOT NULL,
  PRIMARY KEY (record_type, signature, reason, cc_version)
) WITHOUT ROWID;

CREATE TABLE budgets (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  scope TEXT NOT NULL,               -- global | project | client | account
  scope_value TEXT,
  period TEXT NOT NULL,              -- day | week | month
  amount_usd REAL NOT NULL,
  warn_pct REAL NOT NULL DEFAULT 80,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL
);

CREATE TABLE alerts (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  kind TEXT NOT NULL,
  severity TEXT NOT NULL,            -- info | warning | critical
  dedup_key TEXT NOT NULL UNIQUE,    -- one alert per rule per window
  params TEXT NOT NULL,              -- JSON, rendered per language at read time
  acknowledged_at INTEGER,
  delivered TEXT
);
CREATE INDEX alerts_ts ON alerts(ts);

-- Append-only, hash-chained: hash = SHA-256(prev_hash || ts || actor || action || target || detail).
CREATE TABLE audit_log (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  target TEXT NOT NULL,
  detail TEXT NOT NULL,
  prev_hash TEXT NOT NULL,
  hash TEXT NOT NULL
);

CREATE TABLE price_tables (
  version TEXT PRIMARY KEY,
  source TEXT NOT NULL,
  imported_at INTEGER NOT NULL,
  signed_by TEXT,
  data TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;

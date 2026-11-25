//! kanjo-analytics: every read model behind the dashboard, the reports and the CLI,
//! plus CSV and ccusage-compatible JSON export shaping.
//!
//! Every function takes `(&Store, &Ctx, ...)`, never reads the system clock and loads
//! the settings it needs (path redaction) from the store. Time-bucketed reports and the
//! project / model / agent / client breakdowns read `usage_hourly`, which outlives the
//! purge of request rows; session, branch, tool-level and turn views read `sessions`,
//! `requests` and `request_tools`.

mod blocks;
mod common;
pub mod export;
mod limits;
mod overview;
pub mod plan;
mod projects;
mod reports;
mod sessions;

pub use blocks::blocks;
pub use limits::{limits, limits_detail};
pub use overview::{data_status, overview};
pub use plan::plan_fit;
pub use projects::{projects, update_project};
pub use reports::{breakdown, compare, heatmap, usage_report};
pub use sessions::{search, session_detail, sessions, top_turns};

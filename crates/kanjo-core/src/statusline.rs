//! The JSON Claude Code pipes to a statusLine command, reduced to the fields
//! Kanjo keeps. Everything is optional: the payload differs by plan, version
//! and session state, and unknown fields are ignored.

use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct StatuslineInput {
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub cwd: Option<String>,
    pub version: Option<String>,
    pub model: Option<ModelRef>,
    pub workspace: Option<Workspace>,
    pub cost: Option<Cost>,
    pub context_window: Option<ContextWindow>,
    pub exceeds_200k_tokens: Option<bool>,
    pub prompt_cache: Option<PromptCache>,
    pub fast_mode: Option<bool>,
    pub effort: Option<Effort>,
    pub rate_limits: Option<RateLimits>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ModelRef {
    pub id: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub current_dir: Option<String>,
    pub project_dir: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Cost {
    pub total_cost_usd: Option<f64>,
    pub total_duration_ms: Option<f64>,
    pub total_api_duration_ms: Option<f64>,
    pub total_lines_added: Option<f64>,
    pub total_lines_removed: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ContextWindow {
    pub total_input_tokens: Option<f64>,
    pub context_window_size: Option<f64>,
    pub used_percentage: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Effort {
    pub level: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RateLimits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub spend_limit: Option<Window>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Window {
    pub used_percentage: Option<f64>,
    /// Unix epoch seconds.
    pub resets_at: Option<f64>,
    pub used_usd: Option<f64>,
    pub limit_usd: Option<f64>,
    pub period: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PromptCache {
    pub warm: Option<bool>,
    pub ttl: Option<String>,
    pub expires_at: Option<f64>,
    pub requests: Option<f64>,
    pub misses: Option<f64>,
    pub expected_rebuilds: Option<f64>,
    pub hit_ratio: Option<f64>,
    pub cache_write_tokens: Option<f64>,
    pub miss_recache_tokens: Option<f64>,
    pub last_miss_at: Option<f64>,
    pub last_miss_cause: Option<MissCause>,
    pub miss_causes: Option<BTreeMap<String, f64>>,
    pub recache_tokens_if_cold: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct MissCause {
    pub causes: Vec<String>,
    pub tools_added: Option<f64>,
    pub tools_removed: Option<f64>,
    pub system_char_delta: Option<f64>,
}

/// What the statusline fast path writes to the spool and the agent later
/// stores. Times are epoch milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatuslineRecord {
    pub ts: i64,
    pub session_id: String,
    pub session_name: Option<String>,
    pub project_dir: Option<String>,
    pub cc_version: Option<String>,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
    pub api_duration_ms: Option<i64>,
    pub lines_added: Option<i64>,
    pub lines_removed: Option<i64>,
    pub ctx_used_pct: Option<f64>,
    pub ctx_size: Option<i64>,
    pub ctx_tokens: Option<i64>,
    pub exceeds_200k: Option<bool>,
    pub fast_mode: Option<bool>,
    pub effort: Option<String>,
    pub five_hour: Option<WindowRecord>,
    pub seven_day: Option<WindowRecord>,
    pub spend: Option<WindowRecord>,
    pub cache: Option<CacheRecord>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowRecord {
    pub used_pct: f64,
    pub resets_at: i64,
    pub used_usd: Option<f64>,
    pub limit_usd: Option<f64>,
    pub period: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheRecord {
    pub warm: Option<bool>,
    pub ttl: Option<String>,
    pub expires_at: Option<i64>,
    pub requests: Option<i64>,
    pub misses: Option<i64>,
    pub expected_rebuilds: Option<i64>,
    pub hit_ratio: Option<f64>,
    pub write_tokens: Option<i64>,
    pub miss_recache_tokens: Option<i64>,
    pub last_miss_at: Option<i64>,
    pub last_miss_causes: Vec<String>,
    pub tools_added: Option<i64>,
    pub tools_removed: Option<i64>,
    pub system_char_delta: Option<i64>,
    pub miss_causes: BTreeMap<String, i64>,
    pub recache_tokens_if_cold: Option<i64>,
}

fn int(v: Option<f64>) -> Option<i64> {
    v.filter(|x| x.is_finite()).map(|x| x as i64)
}

fn secs_to_ms(v: Option<f64>) -> Option<i64> {
    v.filter(|x| x.is_finite() && *x > 0.0).map(|x| (x * 1000.0) as i64)
}

impl StatuslineInput {
    pub fn parse(bytes: &[u8]) -> Result<StatuslineInput> {
        Ok(serde_json::from_slice(bytes)?)
    }

    /// `None` when the payload has no session id, since nothing can be attributed.
    pub fn to_record(&self, now_ms: i64) -> Option<StatuslineRecord> {
        let session_id = self.session_id.clone().filter(|s| !s.is_empty())?;
        let window = |w: &Option<Window>| {
            let w = w.as_ref()?;
            Some(WindowRecord {
                used_pct: w.used_percentage.filter(|p| p.is_finite())?,
                resets_at: secs_to_ms(w.resets_at)?,
                used_usd: w.used_usd,
                limit_usd: w.limit_usd,
                period: w.period.clone(),
            })
        };
        let limits = self.rate_limits.as_ref();
        let cost = self.cost.as_ref();
        let ctx = self.context_window.as_ref();
        Some(StatuslineRecord {
            ts: now_ms,
            session_id,
            session_name: self.session_name.clone(),
            project_dir: self.workspace.as_ref().and_then(|w| w.project_dir.clone()).or_else(|| self.cwd.clone()),
            cc_version: self.version.clone(),
            model_id: self.model.as_ref().and_then(|m| m.id.clone()),
            model_name: self.model.as_ref().and_then(|m| m.display_name.clone()),
            cost_usd: cost.and_then(|c| c.total_cost_usd),
            duration_ms: int(cost.and_then(|c| c.total_duration_ms)),
            api_duration_ms: int(cost.and_then(|c| c.total_api_duration_ms)),
            lines_added: int(cost.and_then(|c| c.total_lines_added)),
            lines_removed: int(cost.and_then(|c| c.total_lines_removed)),
            ctx_used_pct: ctx.and_then(|c| c.used_percentage),
            ctx_size: int(ctx.and_then(|c| c.context_window_size)),
            ctx_tokens: int(ctx.and_then(|c| c.total_input_tokens)),
            exceeds_200k: self.exceeds_200k_tokens,
            fast_mode: self.fast_mode,
            effort: self.effort.as_ref().and_then(|e| e.level.clone()),
            five_hour: limits.and_then(|l| window(&l.five_hour)),
            seven_day: limits.and_then(|l| window(&l.seven_day)),
            spend: limits.and_then(|l| window(&l.spend_limit)),
            cache: self.prompt_cache.as_ref().map(|c| {
                let cause = c.last_miss_cause.as_ref();
                CacheRecord {
                    warm: c.warm,
                    ttl: c.ttl.clone(),
                    expires_at: secs_to_ms(c.expires_at),
                    requests: int(c.requests),
                    misses: int(c.misses),
                    expected_rebuilds: int(c.expected_rebuilds),
                    hit_ratio: c.hit_ratio,
                    write_tokens: int(c.cache_write_tokens),
                    miss_recache_tokens: int(c.miss_recache_tokens),
                    last_miss_at: secs_to_ms(c.last_miss_at),
                    last_miss_causes: cause.map(|m| m.causes.clone()).unwrap_or_default(),
                    tools_added: int(cause.and_then(|m| m.tools_added)),
                    tools_removed: int(cause.and_then(|m| m.tools_removed)),
                    system_char_delta: int(cause.and_then(|m| m.system_char_delta)),
                    miss_causes: c
                        .miss_causes
                        .as_ref()
                        .map(|m| m.iter().map(|(k, v)| (k.clone(), *v as i64)).collect())
                        .unwrap_or_default(),
                    recache_tokens_if_cold: int(c.recache_tokens_if_cold),
                }
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{
      "cwd": "/work/app", "session_id": "abc", "session_name": "refactor", "transcript_path": "/x/y.jsonl",
      "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
      "workspace": {"current_dir": "/work/app/sub", "project_dir": "/work/app", "added_dirs": []},
      "version": "2.1.332",
      "cost": {"total_cost_usd": 1.25, "total_duration_ms": 45000, "total_api_duration_ms": 2300, "total_lines_added": 156, "total_lines_removed": 23},
      "context_window": {"total_input_tokens": 15500, "total_output_tokens": 1200, "context_window_size": 200000, "used_percentage": 8, "current_usage": null},
      "exceeds_200k_tokens": false,
      "prompt_cache": {"warm": true, "caching_observed": true, "ttl": "1h", "expires_at": 1738429200, "requests": 14, "misses": 2,
        "expected_rebuilds": 1, "hit_ratio": 0.91, "cache_write_tokens": 352000, "miss_recache_tokens": 310200, "last_miss_at": 1738425230,
        "last_miss_cause": {"causes": ["tools_changed"], "tools_added": 2, "tools_removed": 0}, "miss_causes": {"tools_changed": 2},
        "recache_tokens_if_cold": 45000},
      "fast_mode": false, "effort": {"level": "high"}, "thinking": {"enabled": true},
      "rate_limits": {"five_hour": {"used_percentage": 23.5, "resets_at": 1738425600}, "seven_day": {"used_percentage": 41.2, "resets_at": 1738857600},
        "spend_limit": {"used_percentage": 12, "resets_at": 1738857600, "used_usd": 6.0, "limit_usd": 50.0, "period": "weekly"}},
      "some_future_field": {"x": 1}
    }"#;

    #[test]
    fn parses_the_documented_payload() {
        let rec = StatuslineInput::parse(FULL.as_bytes()).unwrap().to_record(42).unwrap();
        assert_eq!(rec.session_id, "abc");
        assert_eq!(rec.project_dir.as_deref(), Some("/work/app"));
        let five = rec.five_hour.unwrap();
        assert_eq!((five.used_pct, five.resets_at), (23.5, 1_738_425_600_000));
        assert_eq!(rec.spend.unwrap().limit_usd, Some(50.0));
        let cache = rec.cache.unwrap();
        assert_eq!(cache.last_miss_causes, vec!["tools_changed"]);
        assert_eq!(cache.miss_causes.get("tools_changed"), Some(&2));
        assert_eq!(cache.last_miss_at, Some(1_738_425_230_000));
    }

    #[test]
    fn tolerates_minimal_and_odd_payloads() {
        assert!(StatuslineInput::parse(b"{}").unwrap().to_record(1).is_none());
        let rec = StatuslineInput::parse(
            br#"{"session_id":"s","rate_limits":{"five_hour":{"used_percentage":5}},"context_window":{"used_percentage":null}}"#,
        )
        .unwrap()
        .to_record(1)
        .unwrap();
        assert!(rec.five_hour.is_none() && rec.cache.is_none() && rec.ctx_used_pct.is_none());
        assert!(StatuslineInput::parse(b"not json").is_err());
    }
}

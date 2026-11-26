//! Cache, model-mix, context and anomaly insights over the last 30 days.

use crate::detect;
use crate::thresholds::*;
use kanjo_core::api::{
    Anomaly, AnomalyKind, CacheBreaker, CacheInsights, CacheMiss, CacheRow, CauseCount, Confidence, ConfigChange, ContextWarning, Insights,
    ModelMixSuggestion, Tokens,
};
use kanjo_core::model::TokenCounts;
use kanjo_core::pricing::{self, PriceTable};
use kanjo_core::settings::Settings;
use kanjo_core::time::DAY_MS;
use kanjo_core::{Ctx, Result, Store};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;

/// Cache efficiency and breakers, model-mix savings, long contexts and anomalies.
pub fn insights(store: &Store, ctx: &Ctx) -> Result<Insights> {
    let settings = Settings::load(store)?;
    store.read(|c| build(c, ctx, &settings))
}

fn build(conn: &Connection, ctx: &Ctx, settings: &Settings) -> Result<Insights> {
    let redact = settings.privacy.redact_paths;
    let from = ctx.now - INSIGHTS_WINDOW_MS;
    Ok(Insights {
        generated_at: ctx.now,
        cache: cache(conn, ctx, from, redact)?,
        model_mix: model_mix(conn, ctx, from)?,
        context_warnings: context_warnings(conn, from, settings.alerts.context_threshold, redact)?,
        anomalies: anomalies(conn, ctx, from, redact)?,
    })
}

fn label_sql(redact: bool) -> &'static str {
    if redact {
        "p.alias"
    } else {
        "p.name"
    }
}

fn cache(conn: &Connection, ctx: &Ctx, from: i64, redact: bool) -> Result<CacheInsights> {
    let mut stmt = conn.prepare(&format!(
        "SELECT u.project_id, COALESCE({}, ''), SUM(u.input), SUM(u.output), SUM(u.cache_read), SUM(u.cache_write_5m), SUM(u.cache_write_1h)
         FROM usage_hourly u LEFT JOIN projects p ON p.id = u.project_id
         WHERE u.hour >= ?1 AND u.hour <= ?2 GROUP BY u.project_id",
        label_sql(redact)
    ))?;
    let projects = stmt
        .query_map(params![kanjo_core::time::hour_floor(from), ctx.now], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                Tokens { input: r.get(2)?, output: r.get(3)?, cache_read: r.get(4)?, cache_write_5m: r.get(5)?, cache_write_1h: r.get(6)? },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut total = Tokens::default();
    let mut by_project: Vec<CacheRow> = Vec::new();
    for (id, label, t) in projects {
        total.input += t.input;
        total.output += t.output;
        total.cache_read += t.cache_read;
        total.cache_write_5m += t.cache_write_5m;
        total.cache_write_1h += t.cache_write_1h;
        by_project.push(CacheRow {
            key: id.to_string(),
            label,
            hit_ratio: t.cache_hit_ratio(),
            cache_read: t.cache_read,
            cache_write: t.cache_write_5m + t.cache_write_1h,
            input: t.input,
        });
    }
    by_project
        .sort_by(|a, b| (b.cache_read + b.cache_write + b.input).cmp(&(a.cache_read + a.cache_write + a.input)).then(a.key.cmp(&b.key)));

    let mut stmt = conn.prepare(&format!(
        "SELECT m.session_id, s.project_id, COALESCE({}, ''), m.ts, m.causes, m.tools_added, m.tools_removed, m.system_char_delta
         FROM cache_misses m LEFT JOIN sessions s ON s.id = m.session_id LEFT JOIN projects p ON p.id = s.project_id
         WHERE m.ts >= ?1 AND m.ts <= ?2 ORDER BY m.ts DESC",
        label_sql(redact)
    ))?;
    let misses = stmt
        .query_map(params![from, ctx.now], |r| {
            Ok(MissRow {
                session_id: r.get(0)?,
                project_id: r.get(1)?,
                project: r.get(2)?,
                ts: r.get(3)?,
                causes: r.get(4)?,
                tools_added: r.get(5)?,
                tools_removed: r.get(6)?,
                system_char_delta: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let miss_causes = crate::rules::count_causes(misses.iter().map(|m| m.causes.as_str()))
        .into_iter()
        .map(|(cause, count)| CauseCount { cause, count })
        .collect();

    let changes = config_changes(conn, from - BREAKER_WINDOW_MS, ctx.now)?;
    let mut recent_misses = Vec::new();
    for m in misses.iter().take(RECENT_MISSES) {
        recent_misses.push(CacheMiss {
            session_id: m.session_id.clone(),
            project: m.project.clone(),
            ts: m.ts,
            causes: serde_json::from_str(&m.causes).unwrap_or_default(),
            tools_added: m.tools_added,
            tools_removed: m.tools_removed,
            system_char_delta: m.system_char_delta,
            correlated_changes: changes.iter().filter(|c| c.explains(m)).map(|c| c.change.clone()).collect(),
        });
    }

    let mut breakers: Vec<CacheBreaker> = changes
        .iter()
        .filter(|c| c.change.ts >= from)
        .filter_map(|c| {
            let after: Vec<&MissRow> = misses.iter().filter(|m| c.explains(m)).collect();
            if after.is_empty() {
                return None;
            }
            let mut sessions: Vec<&str> = after.iter().map(|m| m.session_id.as_str()).collect();
            sessions.sort_unstable();
            sessions.dedup();
            Some(CacheBreaker { change: c.change.clone(), misses_after: after.len() as i64, sessions_affected: sessions.len() as i64 })
        })
        .collect();
    breakers.sort_by(|a, b| b.misses_after.cmp(&a.misses_after).then(b.change.ts.cmp(&a.change.ts)));
    breakers.truncate(MAX_BREAKERS);

    Ok(CacheInsights { hit_ratio: total.cache_hit_ratio(), by_project, miss_causes, recent_misses, breakers })
}

struct MissRow {
    session_id: String,
    project_id: Option<i64>,
    project: String,
    ts: i64,
    causes: String,
    tools_added: Option<i64>,
    tools_removed: Option<i64>,
    system_char_delta: Option<i64>,
}

struct Change {
    change: ConfigChange,
    /// `None` for user-level files, which shape every project's prompt.
    project_id: Option<i64>,
}

impl Change {
    /// The change happened at most `BREAKER_WINDOW_MS` before the miss, in the same project or at user level;
    /// a tool set change only explains misses of its own session.
    fn explains(&self, m: &MissRow) -> bool {
        let close = m.ts >= self.change.ts && m.ts - self.change.ts <= BREAKER_WINDOW_MS;
        let own_session = self.change.kind != "tools" || self.change.target == m.session_id;
        close && own_session && (self.project_id.is_none() || self.project_id == m.project_id)
    }
}

fn config_changes(conn: &Connection, from: i64, to: i64) -> Result<Vec<Change>> {
    let mut stmt = conn.prepare("SELECT ts, kind, target, project_id FROM config_changes WHERE ts >= ?1 AND ts <= ?2 ORDER BY ts, id")?;
    let rows = stmt
        .query_map(params![from, to], |r| {
            Ok(Change { change: ConfigChange { ts: r.get(0)?, kind: r.get(1)?, target: r.get(2)? }, project_id: r.get(3)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

// ------------------------------------------------------------------ model mix

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelName {
    family: String,
    version: Vec<u32>,
    tagged: bool,
    dated: bool,
}

/// Family and version from a model name: `claude-opus-4-6-20260205` -> opus [4, 6], dated.
fn parse_model(name: &str) -> Option<ModelName> {
    let base = name.split('[').next().unwrap_or(name).to_ascii_lowercase();
    let mut family: Option<String> = None;
    let mut version = Vec::new();
    let (mut tagged, mut dated) = (false, false);
    for part in base.split('-').filter(|p| !p.is_empty()) {
        if part == "claude" && family.is_none() {
            continue;
        }
        if part.len() == 8 && part.bytes().all(|b| b.is_ascii_digit()) {
            dated = true;
        } else if let Ok(n) = part.parse::<u32>() {
            version.push(n);
        } else if family.is_none() && part.bytes().all(|b| b.is_ascii_alphabetic()) {
            family = Some(part.to_string());
        } else {
            tagged = true;
        }
    }
    family.map(|family| ModelName { family, version, tagged, dated })
}

/// Model families of the price table ordered by price, cheapest first; families priced alike share a tier.
/// Each tier names its current model: the newest, untagged, undated name of its first family.
struct Tiers {
    tiers: Vec<(Vec<String>, String)>,
}

impl Tiers {
    fn from_prices(prices: &PriceTable) -> Tiers {
        let mut current: BTreeMap<String, (ModelName, String)> = BTreeMap::new();
        for name in prices.models.keys() {
            let Some(parsed) = parse_model(name) else { continue };
            let better = match current.get(&parsed.family) {
                None => true,
                Some((best, best_name)) => {
                    let key = |m: &ModelName, n: &str| (!m.tagged, m.version.clone(), !m.dated, std::cmp::Reverse(n.len()));
                    key(&parsed, name) > key(best, best_name)
                }
            };
            if better {
                current.insert(parsed.family.clone(), (parsed, name.clone()));
            }
        }
        let mut priced: Vec<(f64, String, String)> = current
            .into_iter()
            .filter_map(|(family, (_, model))| {
                let p = prices.lookup(&model)?;
                Some((p.input_cost_per_token + p.output_cost_per_token, family, model))
            })
            .collect();
        priced.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut tiers: Vec<(f64, Vec<String>, String)> = Vec::new();
        for (price, family, model) in priced {
            match tiers.last_mut() {
                Some((p, families, _)) if (price - *p).abs() <= 1e-9 * price.max(*p) => families.push(family),
                _ => tiers.push((price, vec![family], model)),
            }
        }
        Tiers { tiers: tiers.into_iter().map(|(_, f, m)| (f, m)).collect() }
    }

    fn tier_of(&self, model: &str) -> Option<usize> {
        let family = parse_model(model)?.family;
        self.tiers.iter().position(|(families, _)| families.contains(&family))
    }

    /// Top tier: the upper half of the tiers, never the cheapest one.
    fn is_top(&self, tier: usize) -> bool {
        let n = self.tiers.len();
        n >= 2 && tier > 0 && tier >= n.div_ceil(2)
    }

    fn current(&self, tier: usize) -> Option<&str> {
        self.tiers.get(tier).map(|(_, m)| m.as_str())
    }
}

struct Group {
    model: String,
    scope: String,
    requests: i64,
    tokens: TokenCounts,
    cost: f64,
}

fn groups(conn: &Connection, sql_filter: &str, scope_sql: &str, from: i64, to: i64) -> Result<Vec<Group>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT model, {scope_sql}, COUNT(*), SUM(input), SUM(output), SUM(cache_read), SUM(cache_write_5m), SUM(cache_write_1h), SUM(cost_usd)
         FROM requests WHERE ts >= ?1 AND ts <= ?2 AND cost_usd IS NOT NULL AND {sql_filter}
         GROUP BY 1, 2 ORDER BY 1, 2"
    ))?;
    let rows = stmt
        .query_map(params![from, to], |r| {
            Ok(Group {
                model: r.get(0)?,
                scope: r.get(1)?,
                requests: r.get(2)?,
                tokens: TokenCounts {
                    input: r.get(3)?,
                    output: r.get(4)?,
                    cache_read: r.get(5)?,
                    cache_write_5m: r.get(6)?,
                    cache_write_1h: r.get(7)?,
                },
                cost: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

const SUBAGENT: &str = "(is_sidechain = 1 OR COALESCE(agent_type, '') <> '')";

fn model_mix(conn: &Connection, ctx: &Ctx, from: i64) -> Result<Vec<ModelMixSuggestion>> {
    let prices = pricing::active(conn)?;
    let tiers = Tiers::from_prices(&prices);
    let mut out = Vec::new();
    let mut suggest = |g: &Group, code: &str, target_tier: usize| {
        let Some(target) = tiers.current(target_tier) else { return };
        let Some(family) = parse_model(target).map(|m| m.family) else { return };
        if target == g.model {
            return;
        }
        let Some(projected) = prices.cost(target, &g.tokens) else { return };
        let saving = g.cost - projected;
        if saving >= MODEL_MIX_MIN_SAVING_USD && saving >= MODEL_MIX_MIN_SAVING_SHARE * g.cost {
            out.push(ModelMixSuggestion {
                code: format!("{code}_to_{family}"),
                from_model: g.model.clone(),
                to_model: target.to_string(),
                scope: g.scope.clone(),
                affected_requests: g.requests,
                current_usd: g.cost,
                projected_usd: projected,
                saving_usd: saving,
                confidence: Confidence::Estimate,
            });
        }
    };
    let subagent_scope = "COALESCE(NULLIF(agent_type, ''), 'subagent')";
    for g in groups(conn, SUBAGENT, subagent_scope, from, ctx.now)? {
        if tiers.tier_of(&g.model).is_some_and(|t| tiers.is_top(t)) {
            suggest(&g, "subagents", 0);
        }
    }
    let short_main = format!("NOT {SUBAGENT} AND output <= {SHORT_RESPONSE_MAX_OUTPUT}");
    for g in groups(conn, &short_main, "'main'", from, ctx.now)? {
        if let Some(t) = tiers.tier_of(&g.model).filter(|t| tiers.is_top(*t)) {
            suggest(&g, "short_turns", t - 1);
        }
    }
    out.sort_by(|a, b| b.saving_usd.total_cmp(&a.saving_usd));
    Ok(out)
}

// ------------------------------------------------------- context, anomalies

fn context_warnings(conn: &Connection, from: i64, threshold: f64, redact: bool) -> Result<Vec<ContextWarning>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT st.session_id, COALESCE({}, ''), st.ts, st.ctx_used_pct, st.ctx_size, COALESCE(st.exceeds_200k, 0)
         FROM session_status st LEFT JOIN sessions s ON s.id = st.session_id LEFT JOIN projects p ON p.id = s.project_id
         WHERE st.ts >= ?1 AND st.ctx_used_pct IS NOT NULL AND (st.ctx_used_pct >= ?2 OR st.exceeds_200k = 1)
         ORDER BY st.ts DESC LIMIT ?3",
        label_sql(redact)
    ))?;
    let rows = stmt
        .query_map(params![from, threshold, MAX_CONTEXT_WARNINGS as i64], |r| {
            Ok(ContextWarning {
                session_id: r.get(0)?,
                project: r.get(1)?,
                ts: r.get(2)?,
                ctx_used_pct: r.get(3)?,
                ctx_size: r.get(4)?,
                exceeds_200k: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn anomalies(conn: &Connection, ctx: &Ctx, from: i64, redact: bool) -> Result<Vec<Anomaly>> {
    let mut out = Vec::new();
    let first_day = ctx.day_start(from);
    let today = ctx.day_start(ctx.now);
    let daily = detect::daily_costs(conn, ctx, first_day - SPIKE_LOOKBACK_DAYS * DAY_MS, today + DAY_MS)?;
    for (day, _) in daily.iter().filter(|(d, _)| *d >= first_day) {
        if let Some(s) = detect::cost_spike(&daily, *day) {
            out.push(Anomaly {
                kind: AnomalyKind::CostSpike,
                ts: *day,
                session_id: None,
                project: None,
                value: s.cost,
                baseline: s.median,
            });
        }
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT s.id, COALESCE({}, ''), s.cost_usd, s.last_at FROM sessions s LEFT JOIN projects p ON p.id = s.project_id
         WHERE s.last_at >= ?1 ORDER BY s.last_at",
        label_sql(redact)
    ))?;
    let sessions = stmt
        .query_map([from], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?, r.get::<_, i64>(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let baseline = detect::session_costs(conn, ctx.now - RUNAWAY_BASELINE_MS)?;
    for (id, project, cost, last_at) in &sessions {
        if let Some(r) = detect::runaway(id, *cost, &baseline) {
            out.push(Anomaly {
                kind: AnomalyKind::RunawaySession,
                ts: *last_at,
                session_id: Some(id.clone()),
                project: Some(project.clone()),
                value: r.cost,
                baseline: r.median,
            });
        }
    }

    let mut ts_stmt = conn.prepare("SELECT ts FROM requests WHERE session_id = ?1 AND ts >= ?2 AND ts <= ?3 ORDER BY ts")?;
    for (id, project, _, _) in &sessions {
        let Some(lines) = detect::status(conn, id)?.and_then(|st| st.lines_changed()) else { continue };
        if lines > LOOP_MAX_CHANGED_LINES {
            continue;
        }
        let ts: Vec<i64> = ts_stmt.query_map(params![id, from, ctx.now], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let (requests, at) = detect::densest_window(&ts, LOOP_WINDOW_MS);
        if detect::is_agent_loop(requests, lines) {
            out.push(Anomaly {
                kind: AnomalyKind::AgentLoop,
                ts: at,
                session_id: Some(id.clone()),
                project: Some(project.clone()),
                value: requests as f64,
                baseline: LOOP_MIN_REQUESTS as f64,
            });
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.ts));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_names_parse_into_family_and_version() {
        let m = parse_model("claude-opus-4-6-20260205").unwrap();
        assert_eq!((m.family.as_str(), m.version.as_slice(), m.dated, m.tagged), ("opus", &[4, 6][..], true, false));
        assert_eq!(parse_model("claude-3-5-sonnet-20241022").unwrap().family, "sonnet");
        assert!(parse_model("claude-mythos-preview").unwrap().tagged);
        assert_eq!(parse_model("claude-opus-5-5[1m]").unwrap().version, vec![5, 5]);
    }

    #[test]
    fn tiers_follow_the_embedded_prices() {
        let tiers = Tiers::from_prices(PriceTable::embedded());
        let cheapest = tiers.current(0).unwrap();
        assert!(cheapest.contains("haiku"), "{cheapest}");
        let opus = tiers.tier_of("claude-opus-4-6").unwrap();
        assert!(tiers.is_top(opus));
        assert!(tiers.current(opus - 1).unwrap().contains("sonnet"));
        assert!(!tiers.is_top(tiers.tier_of("claude-sonnet-4-5").unwrap()));
    }
}

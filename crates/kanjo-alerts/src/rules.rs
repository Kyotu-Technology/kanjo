//! The alert rules. Each one turns the current state into candidates with a
//! dedup key naming the window it may fire in once.

use crate::detect::{self, ActiveSession};
use crate::stored::{self, Candidate};
use crate::thresholds::*;
use kanjo_core::api::{AlertKind, BudgetState, Severity};
use kanjo_core::limits::{self, LimitKind};
use kanjo_core::model::TokenCounts;
use kanjo_core::settings::{AlertSettings, Settings};
use kanjo_core::time::{hour_floor, DAY_MS, MINUTE_MS};
use kanjo_core::{pricing, Ctx, Result};
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(crate) fn candidates(conn: &Connection, ctx: &Ctx, settings: &Settings) -> Result<Vec<Candidate>> {
    let a = &settings.alerts;
    let redact = settings.privacy.redact_paths;
    let mut out = Vec::new();
    limit_rules(conn, ctx, a, &mut out)?;
    if a.budgets {
        budget_rules(conn, ctx, &mut out)?;
    }
    let active = detect::active_sessions(conn, ctx.now - ACTIVE_SESSION_MS)?;
    if a.anomalies {
        cost_spike_rule(conn, ctx, &mut out)?;
        runaway_rule(conn, ctx, &active, redact, &mut out)?;
        agent_loop_rule(conn, ctx, &active, redact, &mut out)?;
    }
    if a.cache {
        cache_cold_rule(conn, ctx, redact, &mut out)?;
        miss_streak_rule(conn, ctx, redact, &mut out)?;
    }
    if a.context {
        context_rule(conn, a, &active, redact, &mut out)?;
    }
    Ok(out)
}

/// Severity of a limit threshold.
pub(crate) fn threshold_severity(threshold: f64) -> Severity {
    if threshold >= LIMIT_CRITICAL_PCT {
        Severity::Critical
    } else if threshold >= LIMIT_WARNING_PCT {
        Severity::Warning
    } else {
        Severity::Info
    }
}

fn limit_rules(conn: &Connection, ctx: &Ctx, a: &AlertSettings, out: &mut Vec<Candidate>) -> Result<()> {
    let thresholds: Vec<f64> = a.limit_thresholds.iter().copied().filter(|t| t.is_finite() && *t > 0.0).collect();
    let highest = thresholds.iter().copied().fold(f64::NAN, f64::max);
    for kind in [LimitKind::FiveHour, LimitKind::SevenDay] {
        let Some(last) = limits::latest(conn, kind)? else { continue };
        if ctx.now >= last.resets_at {
            continue;
        }
        let samples = limits::window_samples(conn, kind, last.resets_at)?;
        // The first reading names the window, so jitter in later reset times cannot open a new one.
        let window_id = samples.first().map(|s| s.resets_at).unwrap_or(last.resets_at);
        let base = json!({
            "window": kind.as_str(),
            "used_pct": last.used_pct,
            "resets_at": last.resets_at,
            "observed_at": last.ts,
            "evaluated_at": ctx.now,
            "tz_offset_min": ctx.tz_offset_min,
        });
        if let Some(th) = thresholds.iter().copied().filter(|t| last.used_pct >= *t).reduce(f64::max) {
            let mut params = base.clone();
            params["threshold"] = json!(th);
            out.push(Candidate {
                kind: AlertKind::LimitThreshold,
                severity: threshold_severity(th),
                dedup_key: format!("limit_threshold:{}:{window_id}:{th}", kind.as_str()),
                params,
            });
        }
        let ceiling = if highest.is_nan() { 100.0 } else { highest.min(100.0) };
        if a.forecast && last.used_pct < ceiling {
            let f = limits::forecast(kind, &samples, ctx.now);
            if let (true, Some(at)) = (f.will_exhaust, f.exhausts_at) {
                let mut params = base;
                params["exhausts_at"] = json!(at);
                params["rate_pct_per_hour"] = json!(f.rate_pct_per_hour);
                out.push(Candidate {
                    kind: AlertKind::LimitForecast,
                    severity: Severity::Warning,
                    dedup_key: format!("limit_forecast:{}:{window_id}", kind.as_str()),
                    params,
                });
            }
        }
    }
    Ok(())
}

/// Parameters keep the real scope labels; redaction is applied when the alert is read.
fn budget_rules(conn: &Connection, ctx: &Ctx, out: &mut Vec<Candidate>) -> Result<()> {
    // Budget ids can be reused after a delete, so the creation time is part of the key.
    let created: BTreeMap<i64, i64> = {
        let mut stmt = conn.prepare("SELECT id, created_at FROM budgets")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        rows
    };
    for b in crate::budgets::all(conn, ctx, false)? {
        let created_at = created.get(&b.id).copied().unwrap_or(0);
        if !b.enabled {
            continue;
        }
        let (kind, severity, tag) = match b.state {
            BudgetState::Ok => continue,
            BudgetState::Warning => (AlertKind::BudgetWarning, Severity::Warning, "budget_warning"),
            BudgetState::Exceeded => (AlertKind::BudgetExceeded, Severity::Critical, "budget_exceeded"),
        };
        out.push(Candidate {
            kind,
            severity,
            dedup_key: format!("{tag}:{}:{created_at}:{}:{}", b.id, crate::budgets::period_str(b.period), ctx.date_label(b.period_start)),
            params: json!({
                "budget_id": b.id,
                "name": b.name,
                "scope": crate::budgets::scope_str(b.scope),
                "scope_value": b.scope_value,
                "scope_label": b.scope_label,
                "period": crate::budgets::period_str(b.period),
                "period_start": b.period_start,
                "period_end": b.period_end,
                "amount_usd": b.amount_usd,
                "spent_usd": b.spent_usd,
                "spent_pct": b.spent_pct,
                "warn_pct": b.warn_pct,
                "projected_usd": b.projected_usd,
            }),
        });
    }
    Ok(())
}

fn cost_spike_rule(conn: &Connection, ctx: &Ctx, out: &mut Vec<Candidate>) -> Result<()> {
    let today = ctx.day_start(ctx.now);
    let daily = detect::daily_costs(conn, ctx, today - SPIKE_LOOKBACK_DAYS * DAY_MS, today + DAY_MS)?;
    if let Some(s) = detect::cost_spike(&daily, today) {
        let date = ctx.date_label(today);
        out.push(Candidate {
            kind: AlertKind::CostSpike,
            severity: Severity::Warning,
            dedup_key: format!("cost_spike:{date}"),
            params: json!({
                "date": date,
                "day_start": today,
                "cost_usd": s.cost,
                "median_usd": s.median,
                "factor": s.cost / s.median,
                "baseline_days": s.baseline_days,
            }),
        });
    }
    Ok(())
}

/// `extra` with the session id, project id and project label added.
fn session_params(conn: &Connection, session_id: &str, project_id: Option<i64>, redact: bool, extra: Value) -> Result<Value> {
    let mut m = Map::new();
    m.insert("session_id".into(), json!(session_id));
    if let Some(pid) = project_id {
        m.insert("project_id".into(), json!(pid));
        m.insert("project".into(), json!(detect::project_label(conn, pid, redact)?));
    }
    if let Value::Object(e) = extra {
        m.extend(e);
    }
    Ok(Value::Object(m))
}

fn runaway_rule(conn: &Connection, ctx: &Ctx, active: &[ActiveSession], redact: bool, out: &mut Vec<Candidate>) -> Result<()> {
    if active.is_empty() {
        return Ok(());
    }
    let baseline = detect::session_costs(conn, ctx.now - RUNAWAY_BASELINE_MS)?;
    for s in active {
        if let Some(r) = detect::runaway(&s.id, s.cost_usd, &baseline) {
            out.push(Candidate {
                kind: AlertKind::RunawaySession,
                severity: Severity::Warning,
                dedup_key: format!("runaway_session:{}", s.id),
                params: session_params(
                    conn,
                    &s.id,
                    Some(s.project_id),
                    redact,
                    json!({
                        "cost_usd": r.cost,
                        "median_usd": r.median,
                        "factor": r.cost / r.median,
                        "baseline_sessions": r.baseline_sessions,
                    }),
                )?,
            });
        }
    }
    Ok(())
}

fn agent_loop_rule(conn: &Connection, ctx: &Ctx, active: &[ActiveSession], redact: bool, out: &mut Vec<Candidate>) -> Result<()> {
    for s in active {
        let Some(lines) = detect::status(conn, &s.id)?.and_then(|st| st.lines_changed()) else { continue };
        let requests = detect::session_requests_between(conn, &s.id, ctx.now - LOOP_WINDOW_MS, ctx.now)?;
        if detect::is_agent_loop(requests, lines) {
            out.push(Candidate {
                kind: AlertKind::AgentLoop,
                severity: Severity::Warning,
                dedup_key: format!("agent_loop:{}:{}", s.id, hour_floor(ctx.now)),
                params: session_params(
                    conn,
                    &s.id,
                    Some(s.project_id),
                    redact,
                    json!({ "requests": requests, "window_minutes": LOOP_WINDOW_MS / MINUTE_MS, "lines_changed": lines }),
                )?,
            });
        }
    }
    Ok(())
}

fn cache_cold_rule(conn: &Connection, ctx: &Ctx, redact: bool, out: &mut Vec<Candidate>) -> Result<()> {
    let sessions = detect::active_sessions(conn, ctx.now - CACHE_COLD_MAX_IDLE_MS)?;
    if sessions.is_empty() {
        return Ok(());
    }
    let prices = pricing::active(conn)?;
    for s in &sessions {
        let Some(st) = detect::status(conn, &s.id)? else { continue };
        let expired = st.cache_expires_at.is_some_and(|e| e <= ctx.now) || st.cache_warm == Some(false);
        if !expired {
            continue;
        }
        let Some(tokens) = st.cache_recache_if_cold.or(st.ctx_tokens).filter(|t| *t >= CACHE_COLD_MIN_TOKENS) else { continue };
        let expired_at = st.cache_expires_at.filter(|e| *e <= ctx.now).unwrap_or(st.ts);
        // Snapshots are replaced on every statusline refresh; only new requests start a new idle stretch.
        let anchor = st.cache_expires_at.unwrap_or(s.last_at);
        let one_hour = st.cache_ttl.as_deref().is_some_and(|t| t.starts_with("1h") || t == "60m");
        let write = if one_hour {
            TokenCounts { cache_write_1h: tokens, ..Default::default() }
        } else {
            TokenCounts { cache_write_5m: tokens, ..Default::default() }
        };
        let recache_usd = st.model_id.as_deref().and_then(|m| prices.cost(m, &write));
        out.push(Candidate {
            kind: AlertKind::CacheCold,
            severity: Severity::Info,
            dedup_key: format!("cache_cold:{}:{anchor}", s.id),
            params: session_params(
                conn,
                &s.id,
                Some(s.project_id),
                redact,
                json!({
                    "model": st.model_id,
                    "ttl": st.cache_ttl,
                    "expired_at": expired_at,
                    "recache_tokens": tokens,
                    "recache_usd": recache_usd,
                    "evaluated_at": ctx.now,
                    "tz_offset_min": ctx.tz_offset_min,
                }),
            )?,
        });
    }
    Ok(())
}

/// Cause counts over cache-miss rows (each row holds a JSON array of cause names), most frequent first.
pub(crate) fn count_causes<'a>(rows: impl IntoIterator<Item = &'a str>) -> Vec<(String, i64)> {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for raw in rows {
        match serde_json::from_str::<Vec<String>>(raw) {
            Ok(causes) => {
                for c in causes {
                    *counts.entry(c).or_default() += 1;
                }
            }
            Err(e) => tracing::debug!(error = %e, "unreadable cache miss causes"),
        }
    }
    let mut v: Vec<(String, i64)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

/// Session id, timestamp, causes (JSON array) and project of one cache miss.
type MissRow = (String, i64, String, Option<i64>);

fn miss_streak_rule(conn: &Connection, ctx: &Ctx, redact: bool, out: &mut Vec<Candidate>) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT m.session_id, m.ts, m.causes, s.project_id FROM cache_misses m LEFT JOIN sessions s ON s.id = m.session_id
         WHERE m.ts > ?1 AND m.ts <= ?2 ORDER BY m.session_id, m.ts",
    )?;
    let rows = stmt
        .query_map(params![ctx.now - MISS_STREAK_WINDOW_MS, ctx.now], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<i64>>(3)?))
        })?
        .collect::<rusqlite::Result<Vec<MissRow>>>()?;
    let mut by_session: BTreeMap<&str, Vec<&MissRow>> = BTreeMap::new();
    for row in &rows {
        by_session.entry(row.0.as_str()).or_default().push(row);
    }
    for (sid, misses) in by_session {
        if (misses.len() as i64) < MISS_STREAK_MIN {
            continue;
        }
        let first = misses[0].1;
        let last = misses[misses.len() - 1].1;
        // A streak that keeps going is still the streak already reported.
        if stored::exists_since(conn, AlertKind::CacheMissStreak, &format!("cache_miss_streak:{sid}:"), "last_ts", first)? {
            continue;
        }
        let causes = count_causes(misses.iter().map(|m| m.2.as_str()));
        let top = causes.first().map(|c| c.0.clone());
        out.push(Candidate {
            kind: AlertKind::CacheMissStreak,
            severity: Severity::Warning,
            dedup_key: format!("cache_miss_streak:{sid}:{first}"),
            params: session_params(
                conn,
                sid,
                misses[0].3,
                redact,
                json!({
                    "misses": misses.len(),
                    "window_minutes": MISS_STREAK_WINDOW_MS / MINUTE_MS,
                    "first_ts": first,
                    "last_ts": last,
                    "causes": causes.iter().map(|(c, n)| json!({"cause": c, "count": n})).collect::<Vec<_>>(),
                    "top_cause": top,
                }),
            )?,
        });
    }
    Ok(())
}

fn context_rule(conn: &Connection, a: &AlertSettings, active: &[ActiveSession], redact: bool, out: &mut Vec<Candidate>) -> Result<()> {
    for s in active {
        let Some(st) = detect::status(conn, &s.id)? else { continue };
        let Some(pct) = st.ctx_used_pct.filter(|p| *p >= a.context_threshold) else { continue };
        out.push(Candidate {
            kind: AlertKind::ContextNearLimit,
            severity: Severity::Warning,
            dedup_key: format!("context_near_limit:{}", s.id),
            params: session_params(
                conn,
                &s.id,
                Some(s.project_id),
                redact,
                json!({
                    "ctx_used_pct": pct,
                    "ctx_size": st.ctx_size,
                    "exceeds_200k": st.exceeds_200k.unwrap_or(false),
                    "threshold": a.context_threshold,
                }),
            )?,
        });
    }
    Ok(())
}

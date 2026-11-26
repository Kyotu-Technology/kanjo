//! The `alerts` table: insertion with deduplication, listing and acknowledgement.

use crate::render::{render_styled, Style};
use kanjo_core::api::{Alert, AlertKind, Query, Severity, Ts};
use kanjo_core::Result;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashMap;

/// An alert a rule wants to raise. It is stored only if its dedup key is new.
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub kind: AlertKind,
    pub severity: Severity,
    pub dedup_key: String,
    pub params: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredAlert {
    pub id: i64,
    pub ts: Ts,
    pub kind: AlertKind,
    pub severity: Severity,
    pub params: Value,
    pub acknowledged_at: Option<Ts>,
}

impl StoredAlert {
    pub fn render(self, style: &Style) -> Alert {
        let (title, message) = render_styled(self.kind, &self.params, style);
        Alert {
            id: self.id,
            ts: self.ts,
            kind: self.kind,
            severity: self.severity,
            params: self.params,
            title,
            message,
            acknowledged_at: self.acknowledged_at,
        }
    }
}

const KINDS: [(AlertKind, &str); 10] = [
    (AlertKind::LimitThreshold, "limit_threshold"),
    (AlertKind::LimitForecast, "limit_forecast"),
    (AlertKind::BudgetWarning, "budget_warning"),
    (AlertKind::BudgetExceeded, "budget_exceeded"),
    (AlertKind::CostSpike, "cost_spike"),
    (AlertKind::RunawaySession, "runaway_session"),
    (AlertKind::AgentLoop, "agent_loop"),
    (AlertKind::CacheCold, "cache_cold"),
    (AlertKind::CacheMissStreak, "cache_miss_streak"),
    (AlertKind::ContextNearLimit, "context_near_limit"),
];

/// The stored name of an alert kind; the same string as its JSON form.
pub(crate) fn kind_str(kind: AlertKind) -> &'static str {
    KINDS.iter().find(|(k, _)| *k == kind).map(|(_, s)| *s).unwrap_or("unknown")
}

fn parse_kind(s: &str) -> Option<AlertKind> {
    KINDS.iter().find(|(_, name)| *name == s).map(|(k, _)| *k)
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Critical => "critical",
    }
}

fn parse_severity(s: &str) -> Severity {
    match s {
        "critical" => Severity::Critical,
        "warning" => Severity::Warning,
        _ => Severity::Info,
    }
}

/// Stores the candidates whose dedup key is new and returns them.
pub(crate) fn insert(tx: &Connection, now: Ts, candidates: &[Candidate]) -> Result<Vec<StoredAlert>> {
    let mut out = Vec::new();
    for c in candidates {
        let n = tx.execute(
            "INSERT OR IGNORE INTO alerts (ts, kind, severity, dedup_key, params) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now, kind_str(c.kind), severity_str(c.severity), c.dedup_key, serde_json::to_string(&c.params)?],
        )?;
        if n == 1 {
            out.push(StoredAlert {
                id: tx.last_insert_rowid(),
                ts: now,
                kind: c.kind,
                severity: c.severity,
                params: c.params.clone(),
                acknowledged_at: None,
            });
        }
    }
    Ok(out)
}

pub(crate) struct Page {
    pub items: Vec<StoredAlert>,
    pub total: i64,
    pub unacknowledged: i64,
}

/// Newest first. `q.q == "open"` keeps unacknowledged alerts only. Labels in the parameters (project names, and
/// client names of client budgets) are refreshed so renames and redaction apply to old alerts.
pub(crate) fn page(conn: &Connection, q: &Query, redact: bool) -> Result<Page> {
    let open = q.q.as_deref().map(str::trim) == Some("open");
    let filter = if open { "WHERE acknowledged_at IS NULL" } else { "" };
    let limit = q.limit.unwrap_or(50).clamp(1, 1000);
    let offset = q.offset.unwrap_or(0).max(0);
    let mut stmt = conn.prepare(&format!(
        "SELECT id, ts, kind, severity, params, acknowledged_at FROM alerts {filter} ORDER BY ts DESC, id DESC LIMIT ?1 OFFSET ?2"
    ))?;
    let rows = stmt
        .query_map(params![limit, offset], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut labels: HashMap<i64, Option<String>> = HashMap::new();
    let mut items = Vec::with_capacity(rows.len());
    for (id, ts, kind, severity, params, acknowledged_at) in rows {
        let Some(kind) = parse_kind(&kind) else {
            tracing::warn!(id, kind, "skipping alert of unknown kind");
            continue;
        };
        let mut params: Value = match serde_json::from_str(&params) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(id, error = %e, "skipping alert with unreadable parameters");
                continue;
            }
        };
        refresh_labels(conn, &mut params, redact, &mut labels)?;
        items.push(StoredAlert { id, ts, kind, severity: parse_severity(&severity), params, acknowledged_at });
    }
    let (total, unacknowledged): (i64, i64) =
        conn.query_row("SELECT COUNT(*), COUNT(*) FILTER (WHERE acknowledged_at IS NULL) FROM alerts", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(Page { items, total: if open { unacknowledged } else { total }, unacknowledged })
}

pub(crate) fn refresh_labels(conn: &Connection, params: &mut Value, redact: bool, labels: &mut HashMap<i64, Option<String>>) -> Result<()> {
    let scope = params.get("scope").and_then(Value::as_str).unwrap_or("");
    let scope_value = params.get("scope_value").and_then(Value::as_str).map(str::to_string);
    let budget_project = (scope == "project").then(|| scope_value.as_deref().and_then(|v| v.parse::<i64>().ok())).flatten();
    if scope == "client" {
        if let (Some(client), Some(obj)) = (scope_value, params.as_object_mut()) {
            let label = if redact { crate::detect::client_alias(conn, &client)? } else { client };
            obj.insert("scope_value".into(), Value::String(label.clone()));
            obj.insert("scope_label".into(), Value::String(label));
        }
    }
    for (field, pid) in [("project", params.get("project_id").and_then(Value::as_i64)), ("scope_label", budget_project)] {
        let Some(pid) = pid else { continue };
        let label = match labels.get(&pid) {
            Some(l) => l.clone(),
            None => {
                let l = crate::detect::project_label(conn, pid, redact)?;
                labels.insert(pid, l.clone());
                l
            }
        };
        if let (Some(label), Some(obj)) = (label, params.as_object_mut()) {
            obj.insert(field.into(), Value::String(label));
        }
    }
    Ok(())
}

/// Acknowledges one alert, or every open one with `None`. Returns how many changed.
pub(crate) fn acknowledge(tx: &Connection, now: Ts, id: Option<i64>) -> Result<i64> {
    match id {
        Some(id) => {
            let exists: i64 = tx.query_row("SELECT COUNT(*) FROM alerts WHERE id = ?1", [id], |r| r.get(0))?;
            if exists == 0 {
                return Err(kanjo_core::Error::NotFound(format!("alert {id}")));
            }
            Ok(tx.execute("UPDATE alerts SET acknowledged_at = ?2 WHERE id = ?1 AND acknowledged_at IS NULL", params![id, now])? as i64)
        }
        None => Ok(tx.execute("UPDATE alerts SET acknowledged_at = ?1 WHERE acknowledged_at IS NULL", [now])? as i64),
    }
}

/// Whether an alert of `kind` with a dedup key starting with `prefix` has a parameter `field` at or after `ts`.
pub(crate) fn exists_since(conn: &Connection, kind: AlertKind, prefix: &str, field: &str, ts: Ts) -> Result<bool> {
    let pattern = format!("{}%", prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
    let path = format!("$.{field}");
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM alerts WHERE kind = ?1 AND dedup_key LIKE ?2 ESCAPE '\\' AND json_extract(params, ?3) >= ?4",
        params![kind_str(kind), pattern, path, ts],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

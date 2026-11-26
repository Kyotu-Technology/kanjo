//! Spending budgets: CRUD and the current period's status.

use crate::thresholds::BUDGET_MIN_ELAPSED_MS;
use kanjo_core::api::{Budget, BudgetInput, BudgetPeriod, BudgetScope, BudgetState};
use kanjo_core::settings::Settings;
use kanjo_core::{Ctx, Error, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};

/// Every budget with its spend in the current period, in creation order.
pub fn budgets(store: &Store, ctx: &Ctx) -> Result<Vec<Budget>> {
    let redact = Settings::load(store)?.privacy.redact_paths;
    store.read(|c| all(c, ctx, redact))
}

/// Validates and stores a new budget; returns it with its current status.
pub fn create_budget(store: &Store, ctx: &Ctx, input: &BudgetInput) -> Result<Budget> {
    let redact = Settings::load(store)?.privacy.redact_paths;
    let id = store.write(|tx| {
        let v = validate(tx, input, redact)?;
        tx.execute(
            "INSERT INTO budgets (name, scope, scope_value, period, amount_usd, warn_pct, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                v.name,
                scope_str(input.scope),
                v.scope_value,
                period_str(input.period),
                input.amount_usd,
                input.warn_pct,
                input.enabled,
                ctx.now
            ],
        )?;
        Ok(tx.last_insert_rowid())
    })?;
    store.read(|c| one(c, ctx, id, redact))
}

/// Replaces a budget's definition; `NotFound` when there is no such budget.
pub fn update_budget(store: &Store, ctx: &Ctx, id: i64, input: &BudgetInput) -> Result<Budget> {
    let redact = Settings::load(store)?.privacy.redact_paths;
    store.write(|tx| {
        let v = validate(tx, input, redact)?;
        let n = tx.execute(
            "UPDATE budgets SET name = ?2, scope = ?3, scope_value = ?4, period = ?5, amount_usd = ?6, warn_pct = ?7, enabled = ?8
             WHERE id = ?1",
            params![
                id,
                v.name,
                scope_str(input.scope),
                v.scope_value,
                period_str(input.period),
                input.amount_usd,
                input.warn_pct,
                input.enabled
            ],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("budget {id}")));
        }
        Ok(())
    })?;
    store.read(|c| one(c, ctx, id, redact))
}

/// Deletes a budget. Alerts it raised stay in the history.
pub fn delete_budget(store: &Store, id: i64) -> Result<()> {
    store.write(|tx| {
        if tx.execute("DELETE FROM budgets WHERE id = ?1", [id])? == 0 {
            return Err(Error::NotFound(format!("budget {id}")));
        }
        Ok(())
    })
}

struct Valid {
    name: String,
    scope_value: Option<String>,
}

fn validate(conn: &Connection, input: &BudgetInput, redact: bool) -> Result<Valid> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(Error::Invalid("the budget name must have 1 to 100 characters".into()));
    }
    if !input.amount_usd.is_finite() || input.amount_usd <= 0.0 {
        return Err(Error::Invalid("the budget amount must be a positive number".into()));
    }
    if !input.warn_pct.is_finite() || input.warn_pct <= 0.0 || input.warn_pct > 100.0 {
        return Err(Error::Invalid("the warning level must be between 0 and 100 percent".into()));
    }
    let value = input.scope_value.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let scope_value = match input.scope {
        BudgetScope::Global => None,
        BudgetScope::Project => {
            let id: i64 = value
                .as_deref()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| Error::Invalid("a project budget needs a project id".into()))?;
            let exists: Option<i64> = conn.query_row("SELECT id FROM projects WHERE id = ?1", [id], |r| r.get(0)).optional()?;
            if exists.is_none() {
                return Err(Error::Invalid(format!("project {id} does not exist")));
            }
            Some(id.to_string())
        }
        BudgetScope::Client | BudgetScope::Account => {
            let value = value.ok_or_else(|| Error::Invalid("a client or account budget needs a scope value".into()))?;
            if input.scope == BudgetScope::Client && redact {
                Some(real_client(conn, &value)?)
            } else {
                Some(value)
            }
        }
    };
    Ok(Valid { name: name.to_string(), scope_value })
}

fn real_client(conn: &Connection, value: &str) -> Result<String> {
    let mut stmt = conn.prepare(
        "SELECT client FROM projects WHERE client IS NOT NULL UNION SELECT scope_value FROM budgets WHERE scope = 'client' AND scope_value IS NOT NULL",
    )?;
    let clients = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for c in clients {
        if crate::detect::client_alias(conn, &c)? == value {
            return Ok(c);
        }
    }
    Ok(value.to_string())
}

pub(crate) fn scope_str(s: BudgetScope) -> &'static str {
    match s {
        BudgetScope::Global => "global",
        BudgetScope::Project => "project",
        BudgetScope::Client => "client",
        BudgetScope::Account => "account",
    }
}

fn parse_scope(s: &str) -> BudgetScope {
    match s {
        "project" => BudgetScope::Project,
        "client" => BudgetScope::Client,
        "account" => BudgetScope::Account,
        _ => BudgetScope::Global,
    }
}

pub(crate) fn period_str(p: BudgetPeriod) -> &'static str {
    match p {
        BudgetPeriod::Day => "day",
        BudgetPeriod::Week => "week",
        BudgetPeriod::Month => "month",
    }
}

fn parse_period(s: &str) -> BudgetPeriod {
    match s {
        "day" => BudgetPeriod::Day,
        "week" => BudgetPeriod::Week,
        _ => BudgetPeriod::Month,
    }
}

/// The local period containing `ctx.now`, as `[start, end)`.
pub(crate) fn period_bounds(ctx: &Ctx, period: BudgetPeriod) -> (i64, i64) {
    match period {
        BudgetPeriod::Day => {
            let start = ctx.day_start(ctx.now);
            (start, ctx.day_shift(start, 1))
        }
        BudgetPeriod::Week => {
            let start = ctx.week_start_of(ctx.now);
            (start, ctx.day_shift(start, 7))
        }
        BudgetPeriod::Month => (ctx.month_start(ctx.now), ctx.month_shift(ctx.now, 1)),
    }
}

const COLS: &str = "id, name, scope, scope_value, period, amount_usd, warn_pct, enabled";

type Row = (i64, String, String, Option<String>, String, f64, f64, bool);

fn read_row(r: &rusqlite::Row) -> rusqlite::Result<Row> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
}

pub(crate) fn all(conn: &Connection, ctx: &Ctx, redact: bool) -> Result<Vec<Budget>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM budgets ORDER BY id"))?;
    let rows = stmt.query_map([], read_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|r| status(conn, ctx, r, redact)).collect()
}

fn one(conn: &Connection, ctx: &Ctx, id: i64, redact: bool) -> Result<Budget> {
    let row = conn
        .query_row(&format!("SELECT {COLS} FROM budgets WHERE id = ?1"), [id], read_row)
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("budget {id}")))?;
    status(conn, ctx, row, redact)
}

fn status(conn: &Connection, ctx: &Ctx, row: Row, redact: bool) -> Result<Budget> {
    let (id, name, scope, scope_value, period, amount_usd, warn_pct, enabled) = row;
    let scope = parse_scope(&scope);
    let period = parse_period(&period);
    let (start, end) = period_bounds(ctx, period);
    let spent_usd = spend(conn, scope, scope_value.as_deref(), start, end)?;
    let scope_label = match (scope, scope_value.as_deref()) {
        (BudgetScope::Project, Some(v)) => match v.parse::<i64>() {
            Ok(pid) => crate::detect::project_label(conn, pid, redact)?,
            Err(_) => None,
        },
        (BudgetScope::Global, _) => None,
        (BudgetScope::Client, Some(v)) if redact => Some(crate::detect::client_alias(conn, v)?),
        (_, v) => v.map(str::to_string),
    };
    let shown_value = match scope {
        BudgetScope::Client if redact => scope_label.clone(),
        _ => scope_value,
    };
    let spent_pct = if amount_usd > 0.0 { spent_usd / amount_usd * 100.0 } else { 0.0 };
    let length = (end - start).max(1);
    let elapsed = (ctx.now - start).clamp(BUDGET_MIN_ELAPSED_MS.min(length), length);
    let projected_usd = spent_usd * length as f64 / elapsed as f64;
    let state = if spent_usd >= amount_usd {
        BudgetState::Exceeded
    } else if spent_pct >= warn_pct {
        BudgetState::Warning
    } else {
        BudgetState::Ok
    };
    Ok(Budget {
        id,
        name,
        scope,
        scope_value: shown_value,
        scope_label,
        period,
        amount_usd,
        warn_pct,
        enabled,
        period_start: start,
        period_end: end,
        spent_usd,
        spent_pct,
        projected_usd,
        state,
    })
}

/// Priced spend of a scope in `[from, to)`.
fn spend(conn: &Connection, scope: BudgetScope, value: Option<&str>, from: i64, to: i64) -> Result<f64> {
    let base = "SELECT COALESCE(SUM(cost_usd), 0) FROM requests WHERE ts >= ?1 AND ts < ?2";
    let v = match scope {
        BudgetScope::Global => conn.query_row(base, params![from, to], |r| r.get(0))?,
        BudgetScope::Project => {
            let Some(pid) = value.and_then(|v| v.parse::<i64>().ok()) else { return Ok(0.0) };
            conn.query_row(&format!("{base} AND project_id = ?3"), params![from, to, pid], |r| r.get(0))?
        }
        BudgetScope::Client => conn.query_row(
            &format!("{base} AND project_id IN (SELECT id FROM projects WHERE client = ?3)"),
            params![from, to, value],
            |r| r.get(0),
        )?,
        BudgetScope::Account => conn.query_row(
            &format!("{base} AND session_id IN (SELECT id FROM sessions WHERE account = ?3)"),
            params![from, to, value],
            |r| r.get(0),
        )?,
    };
    Ok(v)
}

use crate::common::{Env, Filter, Keys, Src, REPORT_DAYS};
use kanjo_core::api::{Project, ProjectPatch, Usage};
use kanjo_core::time::DAY_MS;
use kanjo_core::{Ctx, Error, Result, Store};
use rusqlite::Connection;
use std::collections::HashMap;

const MAX_NAME_CHARS: usize = 200;

/// All projects, most recently active first, with usage over the last 30 local days and in total.
pub fn projects(store: &Store, ctx: &Ctx) -> Result<Vec<Project>> {
    let env = Env::load(store)?;
    store.read(|c| load(c, ctx, &env, None))
}

/// Renames a project or changes its client (`Some("")` clears the client).
pub fn update_project(store: &Store, ctx: &Ctx, id: i64, patch: &ProjectPatch) -> Result<Project> {
    let name = match patch.name.as_deref().map(str::trim) {
        Some("") => return Err(Error::Invalid("project name must not be empty".into())),
        Some(n) if n.chars().count() > MAX_NAME_CHARS => return Err(Error::Invalid("project name is too long".into())),
        other => other.map(str::to_string),
    };
    let client = match patch.client.as_deref().map(str::trim) {
        Some(c) if c.chars().count() > MAX_NAME_CHARS => return Err(Error::Invalid("client name is too long".into())),
        Some("") => Some(None),
        Some(c) => Some(Some(c.to_string())),
        None => None,
    };
    store.write(|tx| {
        let exists: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM projects WHERE id = ?1)", [id], |r| r.get(0))?;
        if !exists {
            return Err(Error::NotFound(format!("project {id}")));
        }
        if let Some(n) = &name {
            tx.execute("UPDATE projects SET name = ?2 WHERE id = ?1", rusqlite::params![id, n])?;
        }
        if let Some(c) = &client {
            tx.execute("UPDATE projects SET client = ?2 WHERE id = ?1", rusqlite::params![id, c])?;
        }
        Ok(())
    })?;
    let env = Env::load(store)?;
    store.read(|c| load(c, ctx, &env, Some(id)))?.pop().ok_or_else(|| Error::NotFound(format!("project {id}")))
}

fn load(c: &Connection, ctx: &Ctx, env: &Env, only: Option<i64>) -> Result<Vec<Project>> {
    let src = Src::new(c)?;
    let f = Filter { project: only, ..Filter::default() };
    let from = ctx.day_start(ctx.now) - (REPORT_DAYS - 1) * DAY_MS;
    let recent: HashMap<i64, Usage> =
        src.fetch(from, ctx.now, &f, Keys { project: true, ..Keys::NONE })?.into_iter().map(|r| (r.project, r.usage)).collect();
    let mut stmt = c.prepare(&format!(
        "SELECT h.project_id, {} FROM usage_hourly h WHERE ?1 IS NULL OR h.project_id = ?1 GROUP BY 1",
        crate::common::USAGE_HOURLY_COLS
    ))?;
    let total: HashMap<i64, Usage> =
        stmt.query_map([only], |r| Ok((r.get(0)?, crate::common::usage_at(r, 1)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut stmt = c.prepare(
        "SELECT p.id, p.client, p.first_seen, p.last_seen, (SELECT COUNT(*) FROM sessions s WHERE s.project_id = p.id)
         FROM projects p WHERE ?1 IS NULL OR p.id = ?1 ORDER BY p.last_seen DESC, p.id",
    )?;
    let rows = stmt
        .query_map([only], |r| {
            let id: i64 = r.get(0)?;
            Ok(Project {
                id,
                name: env.project(id),
                path: env.project_path(id),
                client: r.get(1)?,
                first_seen: r.get(2)?,
                last_seen: r.get(3)?,
                sessions: r.get(4)?,
                usage_30d: recent.get(&id).copied().unwrap_or_default(),
                usage_total: total.get(&id).copied().unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

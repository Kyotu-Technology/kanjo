//! Append-only, hash-chained audit log.
//!
//! Each entry's `hash` is the lowercase hex SHA-256 of the fields
//! `prev_hash, ts, actor, action, target, detail`, in that order, each encoded
//! as its byte length (u64, big-endian) followed by its UTF-8 bytes. `ts` is
//! written as a decimal string and `detail` is the stored JSON text. The first
//! entry chains from 64 zeros, or from the anchor that retention leaves in the
//! settings table (`audit.anchor`) after deleting the oldest entries.

use kanjo_core::api::{AuditEntry, AuditPage, Query};
use kanjo_core::{settings, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Settings key holding the hash of the newest entry removed by retention.
pub const ANCHOR_KEY: &str = "audit.anchor";

/// Records an action by the current OS user, now.
pub fn record(store: &Store, action: &str, target: &str, detail: serde_json::Value) -> Result<()> {
    let actor = actor();
    let ts = kanjo_core::time::now_ms();
    store.write(|tx| record_tx(tx, ts, &actor, action, target, &detail))
}

/// Appends an entry inside an open write transaction.
pub fn record_tx(tx: &Connection, ts: i64, actor: &str, action: &str, target: &str, detail: &serde_json::Value) -> Result<()> {
    let detail = serde_json::to_string(detail)?;
    let prev: Option<String> = tx.query_row("SELECT hash FROM audit_log ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).optional()?;
    let prev = match prev {
        Some(h) => h,
        None => anchor(tx)?,
    };
    let hash = entry_hash(&prev, ts, actor, action, target, &detail);
    tx.execute(
        "INSERT INTO audit_log (ts, actor, action, target, detail, prev_hash, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![ts, actor, action, target, detail, prev, hash],
    )?;
    Ok(())
}

pub fn entry_hash(prev_hash: &str, ts: i64, actor: &str, action: &str, target: &str, detail: &str) -> String {
    let mut h = Sha256::new();
    for field in [prev_hash, &ts.to_string(), actor, action, target, detail] {
        h.update((field.len() as u64).to_be_bytes());
        h.update(field.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn anchor(conn: &Connection) -> Result<String> {
    Ok(settings::get::<String>(conn, ANCHOR_KEY)?.unwrap_or_else(|| GENESIS.to_string()))
}

/// Newest first. Honours `limit` (default 50, at most 1000), `offset`,
/// `from`, `to` and `q` (substring of action, target or actor).
pub fn list(store: &Store, q: &Query) -> Result<AuditPage> {
    let chain_ok = verify(store)?;
    store.read(|c| {
        let limit = q.limit.unwrap_or(50).clamp(1, 1000);
        let offset = q.offset.unwrap_or(0).max(0);
        let like =
            q.q.as_ref()
                .filter(|s| !s.is_empty())
                .map(|s| format!("%{}%", s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
        let filter = "WHERE (?1 IS NULL OR ts >= ?1) AND (?2 IS NULL OR ts < ?2)
                      AND (?3 IS NULL OR action LIKE ?3 ESCAPE '\\' OR target LIKE ?3 ESCAPE '\\' OR actor LIKE ?3 ESCAPE '\\')";
        let total: i64 = c.query_row(&format!("SELECT count(*) FROM audit_log {filter}"), params![q.from, q.to, like], |r| r.get(0))?;
        let mut stmt = c.prepare(&format!(
            "SELECT id, ts, actor, action, target, detail, hash FROM audit_log {filter} ORDER BY id DESC LIMIT ?4 OFFSET ?5"
        ))?;
        let rows = stmt
            .query_map(params![q.from, q.to, like, limit, offset], |r| {
                Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get::<_, String>(5)?, r.get(6)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let items = rows
            .into_iter()
            .map(|(id, ts, actor, action, target, detail, hash)| AuditEntry {
                id,
                ts,
                actor,
                action,
                target,
                detail: serde_json::from_str(&detail).unwrap_or(serde_json::Value::String(detail)),
                hash,
            })
            .collect();
        Ok(AuditPage { items, total, chain_ok })
    })
}

/// Walks the whole chain: every entry must link to its predecessor (or to the
/// anchor) and carry the hash of its own fields.
pub fn verify(store: &Store) -> Result<bool> {
    store.read(verify_conn)
}

pub(crate) fn verify_conn(c: &Connection) -> Result<bool> {
    let mut expected = anchor(c)?;
    let mut stmt = c.prepare("SELECT ts, actor, action, target, detail, prev_hash, hash FROM audit_log ORDER BY id")?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let (ts, actor, action, target, detail, prev, hash): (i64, String, String, String, String, String, String) =
            (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?);
        if prev != expected || entry_hash(&prev, ts, &actor, &action, &target, &detail) != hash {
            return Ok(false);
        }
        expected = hash;
    }
    Ok(true)
}

/// Deletes entries older than `cutoff` and moves the anchor to the newest
/// deleted hash, in the caller's transaction. Returns the number removed.
pub(crate) fn prune_before(tx: &Connection, cutoff: i64) -> Result<i64> {
    let last: Option<(i64, String)> = tx
        .query_row("SELECT id, hash FROM audit_log WHERE ts < ?1 ORDER BY id DESC LIMIT 1", [cutoff], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((id, hash)) = last else { return Ok(0) };
    let n = tx.execute("DELETE FROM audit_log WHERE id <= ?1", [id])?;
    settings::set(tx, ANCHOR_KEY, &hash)?;
    Ok(n as i64)
}

/// The OS user name of this process.
pub fn actor() -> String {
    ["USER", "LOGNAME", "USERNAME"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanjo_core::testkit::TestStore;
    use serde_json::json;

    fn add(t: &TestStore, ts: i64, action: &str) {
        t.store.write(|tx| record_tx(tx, ts, "tester", action, "target", &json!({"n": ts}))).unwrap();
    }

    #[test]
    fn chain_verifies_and_lists_newest_first() {
        let t = TestStore::new();
        assert!(verify(&t.store).unwrap());
        record(&t.store, "export", "usage", json!({"format": "csv"})).unwrap();
        for i in 1..=4 {
            add(&t, i * 1000, "settings.update");
        }
        assert!(verify(&t.store).unwrap());
        let page = list(&t.store, &Query { limit: Some(2), ..Default::default() }).unwrap();
        assert_eq!((page.total, page.items.len(), page.chain_ok), (5, 2, true));
        assert!(page.items[0].id > page.items[1].id);
        let page = list(&t.store, &Query { q: Some("export".into()), ..Default::default() }).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].detail, json!({"format": "csv"}));
        assert!(!actor().is_empty());
    }

    #[test]
    fn tampering_is_detected() {
        for sql in [
            "UPDATE audit_log SET target = 'other' WHERE id = 2",
            "UPDATE audit_log SET detail = '{\"n\":1}' WHERE id = 3",
            "DELETE FROM audit_log WHERE id = 2",
            "UPDATE audit_log SET ts = ts + 1 WHERE id = 3",
        ] {
            let t = TestStore::new();
            for i in 1..=4 {
                add(&t, i * 1000, "export");
            }
            t.store.write(|tx| Ok(tx.execute(sql, [])?)).unwrap();
            assert!(!verify(&t.store).unwrap(), "{sql}");
            assert!(!list(&t.store, &Query::default()).unwrap().chain_ok);
        }
    }

    #[test]
    fn field_boundaries_are_unambiguous() {
        assert_ne!(entry_hash(GENESIS, 1, "ab", "c", "t", "{}"), entry_hash(GENESIS, 1, "a", "bc", "t", "{}"));
    }

    #[test]
    fn anchor_keeps_the_rest_verifiable_after_pruning() {
        let t = TestStore::new();
        for i in 1..=5 {
            add(&t, i * 1000, "export");
        }
        assert_eq!(t.store.write(|tx| prune_before(tx, 3500)).unwrap(), 3);
        assert!(verify(&t.store).unwrap());
        add(&t, 6000, "export");
        assert!(verify(&t.store).unwrap());
        t.store.write(|tx| prune_before(tx, 10_000)).unwrap();
        assert_eq!(t.store.read(|c| Ok(c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_, i64>(0))?)).unwrap(), 0);
        add(&t, 11_000, "export");
        let prev: String = t.store.read(|c| Ok(c.query_row("SELECT prev_hash FROM audit_log", [], |r| r.get(0))?)).unwrap();
        assert_ne!(prev, GENESIS);
        assert!(verify(&t.store).unwrap());
        t.store.write(|tx| settings::set(tx, ANCHOR_KEY, &GENESIS.to_string())).unwrap();
        assert!(!verify(&t.store).unwrap());
    }
}

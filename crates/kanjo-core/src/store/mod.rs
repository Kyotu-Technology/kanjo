//! SQLite storage (SQLCipher build). One writer connection, a small pool of
//! read connections, WAL journal. Schema changes go through `MIGRATIONS` only.

use crate::{Error, Result};
use rusqlite::{Connection, Transaction};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use zeroize::Zeroizing;

pub mod write;

const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("schema.sql"))];

pub const SCHEMA_VERSION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].0;

/// 256-bit database key as 64 hex characters. Zeroed on drop.
#[derive(Clone)]
pub struct DbKey(Zeroizing<String>);

impl DbKey {
    pub fn from_hex(hex: impl Into<String>) -> Result<DbKey> {
        let hex = Zeroizing::new(hex.into());
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Invalid("database key must be 64 hex characters".into()));
        }
        Ok(DbKey(hex))
    }

    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for DbKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DbKey(***)")
    }
}

pub struct Store {
    path: PathBuf,
    key: Option<DbKey>,
    writer: Mutex<Connection>,
    readers: Mutex<Vec<Connection>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Store {
    /// Opens (creating if needed) the database and brings the schema up to date.
    /// When an existing database needs a migration and `backup_dir` is given,
    /// a copy is written there first.
    pub fn open(path: &Path, key: Option<DbKey>, backup_dir: Option<&Path>) -> Result<Store> {
        if let Some(parent) = path.parent() {
            crate::paths::create_private_dir(parent)?;
        }
        let existed = path.exists();
        let mut conn = connect(path, key.as_ref(), false)?;
        if !existed {
            crate::paths::make_private_file(path)?;
        }
        migrate(&mut conn, backup_dir)?;
        Ok(Store { path: path.to_path_buf(), key, writer: Mutex::new(conn), readers: Mutex::new(Vec::new()) })
    }

    /// Runs `f` in a write transaction; commits on `Ok`, rolls back on `Err`.
    pub fn write<T>(&self, f: impl FnOnce(&Transaction) -> Result<T>) -> Result<T> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// Runs `f` on a pooled query-only connection. Safe to call from many threads.
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = match lock(&self.readers).pop() {
            Some(c) => c,
            None => connect(&self.path, self.key.as_ref(), true)?,
        };
        let out = f(&conn);
        let mut pool = lock(&self.readers);
        if pool.len() < 8 {
            pool.push(conn);
        }
        out
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_encrypted(&self) -> bool {
        self.key.is_some()
    }

    pub fn schema_version(&self) -> Result<i64> {
        self.read(|c| Ok(c.query_row("PRAGMA user_version", [], |r| r.get(0))?))
    }

    /// Writes a consistent copy of the live database to `dest`. The copy is
    /// encrypted with the same key as the source.
    pub fn backup_to(&self, dest: &Path) -> Result<()> {
        if dest.exists() {
            return Err(Error::Invalid(format!("backup target already exists: {}", dest.display())));
        }
        if let Some(parent) = dest.parent() {
            crate::paths::create_private_dir(parent)?;
        }
        let conn = lock(&self.writer);
        vacuum_into(&conn, dest)
    }

    pub fn size_bytes(&self) -> u64 {
        std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)
    }
}

fn vacuum_into(conn: &Connection, dest: &Path) -> Result<()> {
    let target = dest.to_str().ok_or_else(|| Error::Invalid("backup path is not valid UTF-8".into()))?;
    conn.execute("VACUUM INTO ?1", [target])?;
    crate::paths::make_private_file(dest)
}

fn connect(path: &Path, key: Option<&DbKey>, query_only: bool) -> Result<Connection> {
    let conn = Connection::open(path)?;
    if let Some(key) = key {
        // The key is validated hex, so formatting it into the pragma is safe.
        conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", key.as_hex()))?;
    }
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::NotADatabase => {
            Error::Invalid("the database cannot be opened: wrong encryption key or not a Kanjo database".into())
        }
        other => other.into(),
    })?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;
         PRAGMA cache_size = -20000;",
    )?;
    if query_only {
        conn.execute_batch("PRAGMA query_only = ON;")?;
    }
    Ok(conn)
}

fn migrate(conn: &mut Connection, backup_dir: Option<&Path>) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(Error::Invalid(format!(
            "the database was created by a newer Kanjo (schema {current}, this build supports {SCHEMA_VERSION})"
        )));
    }
    if current == SCHEMA_VERSION {
        return Ok(());
    }
    if current > 0 {
        if let Some(dir) = backup_dir {
            crate::paths::create_private_dir(dir)?;
            let name = format!("pre-migration-v{current}-{}.db", crate::time::now_ms());
            vacuum_into(conn, &dir.join(name))?;
        }
    }
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        tracing::info!(version, "applied schema migration");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    #[test]
    fn opens_encrypted_and_rejects_wrong_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kanjo.db");
        {
            let store = Store::open(&path, Some(DbKey::from_hex(KEY).unwrap()), None).unwrap();
            assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
            store.write(|tx| Ok(tx.execute("INSERT INTO settings VALUES ('k', '1', 0)", [])?)).unwrap();
        }
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.starts_with(b"SQLite format 3"), "file must not be a plaintext database");
        assert!(Store::open(&path, None, None).is_err());
        assert!(Store::open(&path, Some(DbKey::from_hex(KEY.replace('0', "1")).unwrap()), None).is_err());
        let store = Store::open(&path, Some(DbKey::from_hex(KEY).unwrap()), None).unwrap();
        let n: i64 = store.read(|c| Ok(c.query_row("SELECT count(*) FROM settings", [], |r| r.get(0))?)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn backup_is_encrypted_with_the_same_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("a.db"), Some(DbKey::from_hex(KEY).unwrap()), None).unwrap();
        store.write(|tx| Ok(tx.execute("INSERT INTO settings VALUES ('k', '1', 0)", [])?)).unwrap();
        let copy = dir.path().join("backups").join("copy.db");
        store.backup_to(&copy).unwrap();
        assert!(Store::open(&copy, None, None).is_err());
        let restored = Store::open(&copy, Some(DbKey::from_hex(KEY).unwrap()), None).unwrap();
        let n: i64 = restored.read(|c| Ok(c.query_row("SELECT count(*) FROM settings", [], |r| r.get(0))?)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn readers_cannot_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("a.db"), None, None).unwrap();
        let err = store.read(|c| Ok(c.execute("INSERT INTO settings VALUES ('k', '1', 0)", [])?));
        assert!(err.is_err());
    }
}

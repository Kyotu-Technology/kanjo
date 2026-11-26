//! SQLite storage (SQLCipher build). One writer connection, a small pool of
//! read connections, WAL journal. Schema changes go through `MIGRATIONS` only.

use crate::{Error, Result};
use rusqlite::{Connection, Transaction, TransactionBehavior};
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
        catch_up_v1(&mut conn)?;
        Ok(Store { path: path.to_path_buf(), key, writer: Mutex::new(conn), readers: Mutex::new(Vec::new()) })
    }

    /// Runs `f` in a write transaction; commits on `Ok`, rolls back on `Err`.
    /// The write lock is taken when the transaction begins, so a writer in
    /// another process makes this wait (up to the busy timeout) instead of failing
    /// when `f` first reads and then writes.
    pub fn write<T>(&self, f: impl FnOnce(&Transaction) -> Result<T>) -> Result<T> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
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

    /// Copies the write-ahead log into the database file and truncates it, so
    /// deleted rows do not linger in the log. Best effort while readers are active.
    pub fn checkpoint(&self) -> Result<()> {
        let conn = lock(&self.writer);
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    /// Rebuilds the database file without free pages (after a large purge), then truncates the log.
    pub fn compact(&self) -> Result<()> {
        {
            let conn = lock(&self.writer);
            conn.execute_batch("VACUUM;")?;
        }
        self.checkpoint()
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
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::NotADatabase => {
            Error::Invalid("the database cannot be opened: wrong encryption key or not a Kanjo database".into())
        }
        other => other.into(),
    })?;
    set_wal(&conn)?;
    conn.execute_batch(
        "PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA secure_delete = ON;
         PRAGMA temp_store = MEMORY;
         PRAGMA cache_size = -20000;",
    )?;
    if query_only {
        conn.execute_batch("PRAGMA query_only = ON;")?;
    }
    Ok(conn)
}

const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Switching a new database to WAL needs an exclusive lock and SQLite does not
/// call the busy handler for it, so a second process opening the same new
/// database at the same moment is retried here.
fn set_wal(conn: &Connection) -> Result<()> {
    let deadline = std::time::Instant::now() + BUSY_TIMEOUT;
    loop {
        match conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0)) {
            Ok(_) => return Ok(()),
            Err(e) if e.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy) && std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => return Err(e.into()),
        }
    }
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
        // Another process may open the same new database at the same moment;
        // the version is read again under the write lock.
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if now >= *version {
            tx.commit()?;
            continue;
        }
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        tracing::info!(version, "applied schema migration");
    }
    Ok(())
}

/// Schema v1 was extended before its first release. Databases created by an
/// earlier development build get the additions here.
fn catch_up_v1(conn: &mut Connection) -> Result<()> {
    let complete: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'purge_marks')
            AND EXISTS (SELECT 1 FROM settings WHERE key = 'alias_secret')",
        [],
        |r| r.get(0),
    )?;
    if complete {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS purge_marks (scope TEXT NOT NULL, target TEXT NOT NULL, until INTEGER NOT NULL,
           PRIMARY KEY (scope, target)) WITHOUT ROWID;",
    )?;
    let added = tx.execute(
        "INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES ('alias_secret', '\"' || lower(hex(randomblob(32))) || '\"', 0)",
        [],
    )?;
    if added > 0 {
        write::refresh_project_aliases(&tx)?;
    }
    tx.commit()?;
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
        let n: i64 = store.read(|c| Ok(c.query_row("SELECT count(*) FROM settings WHERE key = 'k'", [], |r| r.get(0))?)).unwrap();
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
        let n: i64 = restored.read(|c| Ok(c.query_row("SELECT count(*) FROM settings WHERE key = 'k'", [], |r| r.get(0))?)).unwrap();
        assert_eq!(n, 1);
    }

    /// RUSTSEC-2026-0459 (CVE-2026-119375) is fixed in SQLite 3.54.0; the bundled SQLCipher must carry it.
    #[test]
    fn bundled_sqlcipher_is_past_the_wal_advisory() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("a.db"), Some(DbKey::from_hex(KEY).unwrap()), None).unwrap();
        let (version, cipher): (String, String) = store
            .read(|c| {
                let v = c.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
                let cv = c.query_row("PRAGMA cipher_version", [], |r| r.get(0))?;
                Ok((v, cv))
            })
            .unwrap();
        let parts: Vec<u32> = version.split('.').map(|p| p.parse().unwrap()).collect();
        assert!(parts[0] > 3 || (parts[0] == 3 && parts[1] >= 54), "SQLite {version} is affected by RUSTSEC-2026-0459");
        assert!(!cipher.is_empty(), "SQLCipher is not linked");
    }

    #[test]
    fn concurrent_first_open_creates_the_schema_once() {
        for round in 0..8 {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("kanjo.db");
            let barrier = std::sync::Barrier::new(4);
            let results: Vec<Result<i64>> = std::thread::scope(|s| {
                let handles: Vec<_> = (0..4)
                    .map(|_| {
                        s.spawn(|| {
                            barrier.wait();
                            Store::open(&path, Some(DbKey::from_hex(KEY).unwrap()), None).and_then(|st| st.schema_version())
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            for r in results {
                assert_eq!(r.unwrap(), SCHEMA_VERSION, "round {round}");
            }
        }
    }

    #[test]
    fn a_write_waits_for_a_writer_in_another_connection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kanjo.db");
        let a = Store::open(&path, None, None).unwrap();
        let b = Store::open(&path, None, None).unwrap();
        let started = std::sync::Barrier::new(2);
        std::thread::scope(|s| {
            s.spawn(|| {
                a.write(|tx| {
                    tx.execute("INSERT INTO settings VALUES ('a', '1', 0)", [])?;
                    started.wait();
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    Ok(())
                })
                .unwrap()
            });
            started.wait();
            b.write(|tx| {
                let n: i64 = tx.query_row("SELECT count(*) FROM settings", [], |r| r.get(0))?;
                tx.execute("INSERT INTO settings VALUES ('b', ?1, 0)", [n.to_string()])?;
                Ok(())
            })
            .unwrap();
        });
        let n: i64 = b.read(|c| Ok(c.query_row("SELECT count(*) FROM settings WHERE key IN ('a', 'b')", [], |r| r.get(0))?)).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn busy_is_not_reported_as_a_damaged_database() {
        let e: Error = rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY), None).into();
        assert!(matches!(e, Error::Other(ref m) if m.contains("busy")), "{e:?}");
        let e: Error = rusqlite::Error::QueryReturnedNoRows.into();
        assert!(matches!(e, Error::Db(_)));
    }

    #[test]
    fn deleted_rows_do_not_linger_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kanjo.db");
        let store = Store::open(&path, None, None).unwrap();
        store
            .write(|tx| {
                for i in 0..50 {
                    tx.execute("INSERT INTO settings VALUES (?1, '\"feature/invented-secret-branch\"', 0)", [format!("x{i}")])?;
                }
                Ok(())
            })
            .unwrap();
        store.checkpoint().unwrap();
        store.write(|tx| Ok(tx.execute("DELETE FROM settings WHERE key LIKE 'x%'", [])?)).unwrap();
        store.checkpoint().unwrap();
        let needle = b"invented-secret-branch";
        for f in ["kanjo.db", "kanjo.db-wal"] {
            let bytes = std::fs::read(dir.path().join(f)).unwrap_or_default();
            assert!(!bytes.windows(needle.len()).any(|w| w == needle), "deleted text still in {f}");
        }
        store.compact().unwrap();
    }

    #[test]
    fn a_development_v1_database_is_brought_up_to_date() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kanjo.db");
        {
            let store = Store::open(&path, None, None).unwrap();
            store
                .write(|tx| {
                    write::upsert_project(tx, "-work-a", None, 0)?;
                    tx.execute_batch("DROP TABLE purge_marks; DELETE FROM settings WHERE key = 'alias_secret'; UPDATE projects SET alias = 'project-old';")?;
                    Ok(())
                })
                .unwrap();
        }
        let store = Store::open(&path, None, None).unwrap();
        let (marks, alias): (i64, String) = store
            .read(|c| {
                Ok(c.query_row("SELECT (SELECT count(*) FROM purge_marks), (SELECT alias FROM projects)", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?)
            })
            .unwrap();
        assert_eq!(marks, 0);
        assert_eq!(alias, store.read(|c| write::alias(c, "project", "-work-a")).unwrap());
    }

    #[test]
    fn readers_cannot_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("a.db"), None, None).unwrap();
        let err = store.read(|c| Ok(c.execute("INSERT INTO settings VALUES ('k', '1', 0)", [])?));
        assert!(err.is_err());
    }
}

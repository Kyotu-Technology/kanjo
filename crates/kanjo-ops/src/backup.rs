//! Database backups in `paths.backups_dir()` and restore from one.

use crate::keys::{self, Keyring};
use kanjo_core::api::BackupInfo;
use kanjo_core::config::Config;
use kanjo_core::paths::Paths;
use kanjo_core::store::SCHEMA_VERSION;
use kanjo_core::{Error, Result, Store};
use std::path::{Path, PathBuf};

fn stamp(ts: i64) -> String {
    jiff::Timestamp::from_millisecond(ts).map(|t| t.strftime("%Y%m%d-%H%M%S").to_string()).unwrap_or_else(|_| ts.to_string())
}

/// Writes a consistent copy (encrypted with the live key) named
/// `kanjo-YYYYMMDD-HHMMSS.db` (UTC), with a counter on collisions.
pub fn create(store: &Store, paths: &Paths, now: i64) -> Result<BackupInfo> {
    let dir = paths.backups_dir();
    kanjo_core::paths::create_private_dir(&dir)?;
    let base = format!("kanjo-{}", stamp(now));
    let mut dest = dir.join(format!("{base}.db"));
    let mut n = 1;
    while dest.exists() {
        n += 1;
        dest = dir.join(format!("{base}-{n}.db"));
    }
    store.backup_to(&dest)?;
    let size_bytes = std::fs::metadata(&dest)?.len() as i64;
    Ok(BackupInfo { file: file_name(&dest), created_at: now, size_bytes })
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Every `.db` file in the backups directory (including the copies made
/// before schema migrations), newest first. `file` is the bare file name.
pub fn list(paths: &Paths) -> Result<Vec<BackupInfo>> {
    let dir = paths.backups_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("db") {
            continue;
        }
        let meta = entry.metadata()?;
        if !meta.is_file() {
            continue;
        }
        let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64);
        let name = file_name(&path);
        let created_at = created_from_name(&name).or(mtime).unwrap_or(0);
        out.push(BackupInfo { file: name, created_at, size_bytes: meta.len() as i64 });
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.file.cmp(&a.file)));
    Ok(out)
}

fn created_from_name(name: &str) -> Option<i64> {
    let stem = name.strip_suffix(".db")?;
    if let Some(rest) = stem.strip_prefix("kanjo-") {
        let ts: String = rest.chars().take(15).collect();
        if !ts.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        let dt = jiff::civil::DateTime::strptime("%Y%m%d-%H%M%S", &ts).ok()?;
        return Some(kanjo_core::time::utc_millis(dt));
    }
    stem.strip_prefix("pre-migration-v").and_then(|rest| rest.split_once('-')).and_then(|(_, ms)| ms.parse().ok())
}

/// Replaces the live database with `file` (a path, or a name from `list`).
/// The backup must open with the current key and carry a supported schema.
/// The live database (with its WAL files) is kept beside it as
/// `kanjo.db.pre-restore-YYYYMMDD-HHMMSS`. Call only while no `Store` is open.
pub fn restore(paths: &Paths, config: &Config, file: &Path) -> Result<()> {
    let keyring = keys::system_keyring(paths);
    restore_with(paths, config, file, keyring.as_deref())
}

/// `restore` with an explicit keyring backend (`None` = no keyring).
pub fn restore_with(paths: &Paths, config: &Config, file: &Path, keyring: Option<&dyn Keyring>) -> Result<()> {
    let source = locate(paths, file)?;
    let mode = config.storage.encryption;
    let key = match keys::lookup(paths, mode, keyring)? {
        Some((key, _)) => Some(key),
        None if mode == kanjo_core::config::EncryptionMode::Off => None,
        None => return Err(Error::Config("no database key found; a backup can only be restored with the key it was made with".into())),
    };
    paths.ensure_home()?;
    let live = paths.db();
    let staging = sibling(&live, ".restoring");
    remove_with_wal(&staging)?;
    std::fs::copy(&source, &staging)?;
    kanjo_core::paths::make_private_file(&staging)?;
    let checked = check(&staging, key);
    if let Err(e) = checked {
        remove_with_wal(&staging)?;
        return Err(e);
    }
    let aside = sibling(&live, &format!(".pre-restore-{}", stamp(kanjo_core::time::now_ms())));
    for suffix in ["", "-wal", "-shm"] {
        let from = sibling(&live, suffix);
        if from.exists() {
            std::fs::rename(&from, sibling(&aside, suffix))?;
        }
    }
    std::fs::rename(&staging, &live)?;
    for suffix in ["-wal", "-shm"] {
        let stale = sibling(&staging, suffix);
        if stale.exists() {
            std::fs::rename(&stale, sibling(&live, suffix))?;
        }
    }
    tracing::info!(from = %source.display(), aside = %aside.display(), "database restored");
    Ok(())
}

fn check(staging: &Path, key: Option<kanjo_core::store::DbKey>) -> Result<()> {
    let found = stored_version(staging, key.as_ref())
        .map_err(|e| Error::Invalid(format!("the backup cannot be opened with the current key: {e}")))?;
    if !(1..=SCHEMA_VERSION).contains(&found) {
        return Err(Error::Invalid(format!("not a Kanjo backup this build can read (schema {found})")));
    }
    let store = Store::open(staging, key, None)
        .map_err(|e| Error::Invalid(format!("the backup cannot be opened with the current key or schema: {e}")))?;
    let version = store.schema_version()?;
    if version != SCHEMA_VERSION {
        return Err(Error::Invalid(format!("unsupported backup schema {version}")));
    }
    let ok: String = store.read(|c| Ok(c.query_row("PRAGMA quick_check", [], |r| r.get(0))?))?;
    if ok != "ok" {
        return Err(Error::Invalid(format!("the backup is damaged: {ok}")));
    }
    Ok(())
}

/// `PRAGMA user_version` without migrating, so an empty or foreign file is not mistaken for a backup.
fn stored_version(path: &Path, key: Option<&kanjo_core::store::DbKey>) -> Result<i64> {
    let conn = rusqlite::Connection::open(path)?;
    if let Some(key) = key {
        // DbKey only holds validated hex, so formatting it into the pragma is safe.
        conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", key.as_hex()))?;
    }
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))?;
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

fn locate(paths: &Paths, file: &Path) -> Result<PathBuf> {
    if file.is_file() {
        return Ok(file.to_path_buf());
    }
    let in_dir = paths.backups_dir().join(file);
    if file.components().count() == 1 && in_dir.is_file() {
        return Ok(in_dir);
    }
    Err(Error::NotFound(format!("backup {}", file.display())))
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

fn remove_with_wal(path: &Path) -> Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        match std::fs::remove_file(sibling(path, suffix)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::tests::FakeKeyring;
    use kanjo_core::config::EncryptionMode;

    fn setup() -> (tempfile::TempDir, Paths, Config, FakeKeyring) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path().join("home"), dir.path().join("claude"));
        let mut config = Config::default();
        config.storage.encryption = EncryptionMode::Auto;
        (dir, paths, config, FakeKeyring::default())
    }

    fn count(store: &Store) -> i64 {
        store.read(|c| Ok(c.query_row("SELECT count(*) FROM budgets", [], |r| r.get(0))?)).unwrap()
    }

    fn add(store: &Store, name: &str) {
        store
            .write(|tx| {
                Ok(tx.execute(
                    "INSERT INTO budgets (name, scope, period, amount_usd, created_at) VALUES (?1, 'global', 'month', 100, 0)",
                    [name],
                )?)
            })
            .unwrap();
    }

    #[test]
    fn encrypted_round_trip() {
        let (_d, paths, config, kr) = setup();
        let backup = {
            let (store, source) = crate::open_store_with(&paths, &config, Some(&kr)).unwrap();
            assert_eq!(source, kanjo_core::api::KeySource::Keyring);
            assert!(store.is_encrypted());
            add(&store, "one");
            let info = create(&store, &paths, 1_795_648_000_000).unwrap();
            assert_eq!(info.file, "kanjo-20261125-230640.db");
            let second = create(&store, &paths, 1_795_648_000_000).unwrap();
            assert_eq!(second.file, "kanjo-20261125-230640-2.db");
            add(&store, "two");
            info
        };
        let listed = list(&paths).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].created_at, 1_795_648_000_000);
        let raw = std::fs::read(paths.backups_dir().join(&backup.file)).unwrap();
        assert!(!raw.starts_with(b"SQLite format 3"));

        restore_with(&paths, &config, Path::new(&backup.file), Some(&kr)).unwrap();
        let (store, _) = crate::open_store_with(&paths, &config, Some(&kr)).unwrap();
        assert_eq!(count(&store), 1);
        let aside: Vec<_> = std::fs::read_dir(&paths.home)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("kanjo.db.pre-restore-"))
            .collect();
        assert!(!aside.is_empty(), "the live database is kept aside");
        assert!(!paths.home.join("kanjo.db.restoring").exists());
    }

    #[test]
    fn foreign_key_or_garbage_is_refused_and_live_db_untouched() {
        let (_d, paths, config, kr) = setup();
        {
            let (store, _) = crate::open_store_with(&paths, &config, Some(&kr)).unwrap();
            add(&store, "live");
        }
        let other_dir = tempfile::tempdir().unwrap();
        let other = other_dir.path().join("other.db");
        {
            let key = kanjo_core::store::DbKey::from_hex("ab".repeat(32)).unwrap();
            let s = Store::open(&other, Some(key), None).unwrap();
            add(&s, "foreign");
        }
        assert!(restore_with(&paths, &config, &other, Some(&kr)).is_err());
        let junk = other_dir.path().join("junk.db");
        std::fs::write(&junk, b"not a database at all").unwrap();
        assert!(restore_with(&paths, &config, &junk, Some(&kr)).is_err());
        let empty = other_dir.path().join("empty.db");
        std::fs::write(&empty, b"").unwrap();
        assert!(restore_with(&paths, &config, &empty, Some(&kr)).is_err());
        let plain = other_dir.path().join("plain.db");
        rusqlite::Connection::open(&plain).unwrap().execute_batch("CREATE TABLE t (x INTEGER);").unwrap();
        assert!(restore_with(&paths, &config, &plain, Some(&kr)).is_err());
        assert!(restore_with(&paths, &config, Path::new("missing.db"), Some(&kr)).is_err());
        let (store, _) = crate::open_store_with(&paths, &config, Some(&kr)).unwrap();
        assert_eq!(count(&store), 1);
        assert!(!paths.home.join("kanjo.db.restoring").exists());
    }

    #[test]
    fn names_parse_back_to_times() {
        assert_eq!(created_from_name("kanjo-20261125-230640.db"), Some(1_795_648_000_000));
        assert_eq!(created_from_name("kanjo-20261125-230640-3.db"), Some(1_795_648_000_000));
        assert_eq!(created_from_name("pre-migration-v1-1795648000000.db"), Some(1_795_648_000_000));
        assert_eq!(created_from_name("other.db"), None);
    }
}

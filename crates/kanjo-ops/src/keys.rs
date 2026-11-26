//! Database key lookup and creation.
//!
//! Order: an existing key file wins, then the OS keyring entry. Only when
//! neither holds a key and the database file does not exist yet is a new
//! 256-bit key generated and stored. An existing database without a key is an
//! error: a new key would make it unreadable for good.
//!
//! Lookup and creation run under an exclusive lock on `kanjo.key.lock`, so a
//! service and a command started at the same moment on a new machine agree on
//! one key instead of each storing its own.
//!
//! `EncryptionMode::Keyfile` never consults the keyring; `KANJO_NO_KEYRING=1`
//! disables it for every mode (the key file is used instead, and `Keyring`
//! mode fails). The keyring calls block; run them off the async runtime.

use kanjo_core::api::KeySource;
use kanjo_core::config::EncryptionMode;
use kanjo_core::paths::Paths;
use kanjo_core::store::DbKey;
use kanjo_core::{Error, Result};

const SERVICE: &str = "kanjo";

/// Storage for the database key. The OS keyring implements it; tests use an in-memory fake.
pub trait Keyring {
    /// `Ok(None)` when there is no entry; `Err` when the keyring cannot be reached.
    fn get(&self) -> Result<Option<String>>;
    fn set(&self, secret: &str) -> Result<()>;
}

/// The platform keyring: Secret Service (D-Bus) on Linux and the BSDs, Keychain
/// on macOS, Credential Manager on Windows. One entry per data directory, stored
/// under the same service and user names as the keyring crate used, so entries
/// written by earlier releases are found.
pub struct OsKeyring {
    account: String,
}

impl OsKeyring {
    pub fn new(paths: &Paths) -> OsKeyring {
        OsKeyring { account: format!("database-key:{}", paths.home.display()) }
    }
}

impl Keyring for OsKeyring {
    fn get(&self) -> Result<Option<String>> {
        os::get(SERVICE, &self.account).map_err(|e| Error::Other(format!("keyring unavailable: {e}")))
    }

    fn set(&self, secret: &str) -> Result<()> {
        os::set(SERVICE, &self.account, secret).map_err(|e| Error::Other(format!("cannot store the key in the keyring: {e}")))
    }
}

/// Secret Service through secret-service's blocking API. Items are matched on
/// the `service` and `username` attributes; new items go to the default
/// collection with `target = default`, as keyring 3 created them.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
mod os {
    use secret_service::blocking::{Item, SecretService};
    use secret_service::EncryptionType;
    use std::collections::HashMap;

    fn find<'a>(ss: &'a SecretService<'a>, service: &str, user: &str) -> Result<Option<Item<'a>>, String> {
        let found = ss.search_items(HashMap::from([("service", service), ("username", user)])).map_err(|e| e.to_string())?;
        let mut items: Vec<Item> = found.unlocked.into_iter().chain(found.locked).collect();
        match items.len() {
            0 => Ok(None),
            1 => {
                let item = items.remove(0);
                item.ensure_unlocked().map_err(|e| e.to_string())?;
                Ok(Some(item))
            }
            n => Err(format!("{n} Secret Service items match service {service:?} and user {user:?}")),
        }
    }

    pub fn get(service: &str, user: &str) -> Result<Option<String>, String> {
        let ss = SecretService::connect(EncryptionType::Dh).map_err(|e| e.to_string())?;
        match find(&ss, service, user)? {
            None => Ok(None),
            Some(item) => {
                let secret = item.get_secret().map_err(|e| e.to_string())?;
                String::from_utf8(secret).map(Some).map_err(|_| "the stored secret is not UTF-8 text".to_string())
            }
        }
    }

    pub fn set(service: &str, user: &str, secret: &str) -> Result<(), String> {
        let ss = SecretService::connect(EncryptionType::Dh).map_err(|e| e.to_string())?;
        if let Some(item) = find(&ss, service, user)? {
            return item.set_secret(secret.as_bytes(), "text/plain").map_err(|e| e.to_string());
        }
        let collection = ss.get_default_collection().map_err(|e| e.to_string())?;
        collection.ensure_unlocked().map_err(|e| e.to_string())?;
        let attributes = HashMap::from([("service", service), ("username", user), ("target", "default")]);
        collection
            .create_item(&format!("{user}@{service}"), attributes, secret.as_bytes(), true, "text/plain")
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Keychain (macOS) and Credential Manager (Windows) through keyring-core and
/// the platform store crates, with their default naming.
#[cfg(any(target_os = "macos", windows))]
mod os {
    use std::sync::LazyLock;

    static STORE: LazyLock<Result<(), String>> = LazyLock::new(|| {
        #[cfg(target_os = "macos")]
        let store = apple_native_keyring_store::keychain::Store::new();
        #[cfg(windows)]
        let store = windows_native_keyring_store::Store::new();
        keyring_core::set_default_store(store.map_err(|e| e.to_string())?);
        Ok(())
    });

    fn entry(service: &str, user: &str) -> Result<keyring_core::Entry, String> {
        STORE.clone()?;
        keyring_core::Entry::new(service, user).map_err(|e| e.to_string())
    }

    pub fn get(service: &str, user: &str) -> Result<Option<String>, String> {
        match entry(service, user)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn set(service: &str, user: &str, secret: &str) -> Result<(), String> {
        entry(service, user)?.set_password(secret).map_err(|e| e.to_string())
    }
}

#[cfg(not(any(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))), target_os = "macos", windows)))]
mod os {
    pub fn get(_: &str, _: &str) -> Result<Option<String>, String> {
        Err("no OS keyring on this platform".to_string())
    }

    pub fn set(_: &str, _: &str, _: &str) -> Result<(), String> {
        Err("no OS keyring on this platform".to_string())
    }
}

/// The OS keyring, or `None` when `KANJO_NO_KEYRING=1`.
pub fn system_keyring(paths: &Paths) -> Option<Box<dyn Keyring>> {
    if std::env::var("KANJO_NO_KEYRING").is_ok_and(|v| v == "1") {
        return None;
    }
    Some(Box::new(OsKeyring::new(paths)))
}

/// Resolves the database key with the OS keyring.
pub fn resolve(paths: &Paths, mode: EncryptionMode) -> Result<(Option<DbKey>, KeySource)> {
    let keyring = system_keyring(paths);
    resolve_with(paths, mode, keyring.as_deref())
}

/// Resolves the database key with an explicit keyring backend (`None` = no keyring).
pub fn resolve_with(paths: &Paths, mode: EncryptionMode, keyring: Option<&dyn Keyring>) -> Result<(Option<DbKey>, KeySource)> {
    if mode == EncryptionMode::Off {
        return Ok((None, KeySource::None));
    }
    let _lock = lock_keys(paths)?;
    let staging = paths.home.join("kanjo.db.restoring");
    if !paths.db().exists() && staging.exists() {
        return Err(Error::Config(format!(
            "{} is missing but {} exists: a restore was interrupted; rename it to {} to finish the restore",
            paths.db().display(),
            staging.display(),
            paths.db().display()
        )));
    }
    let keyring_error = match find(paths, mode, keyring)? {
        Found::Key(key, source) => return Ok((Some(key), source)),
        Found::Missing { keyring_error } => keyring_error,
    };
    if paths.db().exists() {
        let mut msg = format!(
            "the database {} exists but its encryption key was found neither in {} nor in the OS keyring; \
             restore the key file or the keyring entry (Kanjo never creates a new key for an existing database)",
            paths.db().display(),
            paths.key_file().display()
        );
        if let Some(e) = keyring_error {
            msg.push_str(&format!(" ({e})"));
        }
        return Err(Error::Config(msg));
    }
    let key = DbKey::from_hex(crate::random_hex(32)?)?;
    match mode {
        EncryptionMode::Keyfile => write_key_file(paths, &key).map(|_| (Some(key), KeySource::Keyfile)),
        EncryptionMode::Keyring => {
            let kr = keyring.ok_or_else(|| Error::Config("encryption = \"keyring\" but the OS keyring is disabled".into()))?;
            Ok((Some(store_in_keyring(kr, key)?), KeySource::Keyring))
        }
        _ => {
            if let Some(kr) = keyring {
                match store_in_keyring(kr, key.clone()) {
                    Ok(stored) => return Ok((Some(stored), KeySource::Keyring)),
                    Err(e) => tracing::info!(error = %e, "OS keyring not usable, falling back to a key file"),
                }
            }
            write_key_file(paths, &key)?;
            Ok((Some(key), KeySource::Keyfile))
        }
    }
}

enum Found {
    Key(DbKey, KeySource),
    Missing { keyring_error: Option<Error> },
}

/// Looks the key up without ever creating one.
fn find(paths: &Paths, mode: EncryptionMode, keyring: Option<&dyn Keyring>) -> Result<Found> {
    let file = paths.key_file();
    match std::fs::read_to_string(&file) {
        Ok(text) => {
            let key = DbKey::from_hex(text.trim()).map_err(|_| Error::Config(format!("{} does not hold a valid key", file.display())))?;
            return Ok(Found::Key(key, KeySource::Keyfile));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    if mode == EncryptionMode::Keyfile {
        return Ok(Found::Missing { keyring_error: None });
    }
    let Some(kr) = keyring else {
        if mode == EncryptionMode::Keyring {
            return Err(Error::Config("encryption = \"keyring\" but the OS keyring is disabled".into()));
        }
        return Ok(Found::Missing { keyring_error: None });
    };
    match kr.get() {
        Ok(Some(hex)) => {
            let key = DbKey::from_hex(hex.trim()).map_err(|_| Error::Config("the keyring entry does not hold a valid key".into()))?;
            Ok(Found::Key(key, KeySource::Keyring))
        }
        Ok(None) => Ok(Found::Missing { keyring_error: None }),
        Err(e) if mode == EncryptionMode::Keyring => Err(e),
        Err(e) => Ok(Found::Missing { keyring_error: Some(e) }),
    }
}

/// The existing key, or `None` when none is stored. Never generates one.
pub(crate) fn lookup(paths: &Paths, mode: EncryptionMode, keyring: Option<&dyn Keyring>) -> Result<Option<(DbKey, KeySource)>> {
    if mode == EncryptionMode::Off {
        return Ok(None);
    }
    Ok(match find(paths, mode, keyring)? {
        Found::Key(key, source) => Some((key, source)),
        Found::Missing { .. } => None,
    })
}

/// Exclusive lock held while the key is looked up or created; released on drop.
fn lock_keys(paths: &Paths) -> Result<std::fs::File> {
    paths.ensure_home()?;
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = paths.key_lock_file();
    let f = opts.open(&file).map_err(|e| Error::Config(format!("cannot open the key lock {}: {e}", file.display())))?;
    f.lock().map_err(|e| Error::Config(format!("cannot lock {}: {e}", file.display())))?;
    Ok(f)
}

/// Stores `key` and returns the key now in the keyring. An entry that appeared
/// since the lookup (written by a process that does not take the key lock, such
/// as an older release) is used rather than overwritten.
fn store_in_keyring(kr: &dyn Keyring, key: DbKey) -> Result<DbKey> {
    if let Some(existing) = kr.get()?.and_then(|hex| DbKey::from_hex(hex.trim()).ok()) {
        return Ok(existing);
    }
    kr.set(key.as_hex())?;
    match kr.get()? {
        Some(back) if back.trim() == key.as_hex() => Ok(key),
        _ => Err(Error::Other("the keyring did not return the key that was stored".into())),
    }
}

fn write_key_file(paths: &Paths, key: &DbKey) -> Result<()> {
    paths.ensure_home()?;
    let file = paths.key_file();
    crate::create_private(&file, &format!("{}\n", key.as_hex()))
        .map_err(|e| Error::Config(format!("cannot create the key file {}: {e}", file.display())))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;

    /// In-memory keyring. `broken` simulates an unreachable Secret Service.
    #[derive(Default)]
    pub struct FakeKeyring {
        pub secret: RefCell<Option<String>>,
        pub broken: bool,
        pub drops_writes: bool,
    }

    impl Keyring for FakeKeyring {
        fn get(&self) -> Result<Option<String>> {
            if self.broken {
                return Err(Error::Other("no secret service".into()));
            }
            Ok(self.secret.borrow().clone())
        }
        fn set(&self, secret: &str) -> Result<()> {
            if self.broken {
                return Err(Error::Other("no secret service".into()));
            }
            if !self.drops_writes {
                *self.secret.borrow_mut() = Some(secret.to_string());
            }
            Ok(())
        }
    }

    fn paths() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let p = Paths::at(dir.path().join("home"), dir.path().join("claude"));
        (dir, p)
    }

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn touch_db(p: &Paths) {
        std::fs::create_dir_all(&p.home).unwrap();
        std::fs::write(p.db(), b"x").unwrap();
    }

    #[test]
    fn off_has_no_key() {
        let (_d, p) = paths();
        let kr = FakeKeyring::default();
        let (key, src) = resolve_with(&p, EncryptionMode::Off, Some(&kr)).unwrap();
        assert!(key.is_none() && src == KeySource::None);
        assert!(kr.secret.borrow().is_none() && !p.key_file().exists());
    }

    #[test]
    fn auto_generates_into_keyring_and_finds_it_again() {
        let (_d, p) = paths();
        let kr = FakeKeyring::default();
        let (key, src) = resolve_with(&p, EncryptionMode::Auto, Some(&kr)).unwrap();
        assert_eq!(src, KeySource::Keyring);
        let hex = key.unwrap().as_hex().to_string();
        assert_eq!(hex.len(), 64);
        assert_eq!(kr.secret.borrow().as_deref(), Some(hex.as_str()));
        assert!(!p.key_file().exists());
        touch_db(&p);
        let (again, src) = resolve_with(&p, EncryptionMode::Auto, Some(&kr)).unwrap();
        assert_eq!((again.unwrap().as_hex(), src), (hex.as_str(), KeySource::Keyring));
    }

    #[test]
    fn auto_falls_back_to_owner_only_key_file() {
        for kr in [FakeKeyring { broken: true, ..Default::default() }, FakeKeyring { drops_writes: true, ..Default::default() }] {
            let (_d, p) = paths();
            let (key, src) = resolve_with(&p, EncryptionMode::Auto, Some(&kr)).unwrap();
            assert_eq!(src, KeySource::Keyfile);
            let text = std::fs::read_to_string(p.key_file()).unwrap();
            assert_eq!(text.trim(), key.unwrap().as_hex());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(std::fs::metadata(p.key_file()).unwrap().permissions().mode() & 0o777, 0o600);
            }
        }
        let (_d, p) = paths();
        let (_, src) = resolve_with(&p, EncryptionMode::Auto, None).unwrap();
        assert_eq!(src, KeySource::Keyfile);
    }

    #[test]
    fn key_file_wins_over_keyring() {
        let (_d, p) = paths();
        std::fs::create_dir_all(&p.home).unwrap();
        std::fs::write(p.key_file(), format!("{KEY}\n")).unwrap();
        let kr = FakeKeyring { secret: RefCell::new(Some(KEY.replace('0', "f"))), ..Default::default() };
        for mode in [EncryptionMode::Auto, EncryptionMode::Keyring, EncryptionMode::Keyfile] {
            let (key, src) = resolve_with(&p, mode, Some(&kr)).unwrap();
            assert_eq!((key.unwrap().as_hex(), src), (KEY, KeySource::Keyfile));
        }
    }

    #[test]
    fn keyring_mode_requires_a_working_keyring() {
        let (_d, p) = paths();
        assert!(resolve_with(&p, EncryptionMode::Keyring, None).is_err());
        let broken = FakeKeyring { broken: true, ..Default::default() };
        assert!(resolve_with(&p, EncryptionMode::Keyring, Some(&broken)).is_err());
        assert!(!p.key_file().exists());
        let kr = FakeKeyring::default();
        let (_, src) = resolve_with(&p, EncryptionMode::Keyring, Some(&kr)).unwrap();
        assert_eq!(src, KeySource::Keyring);
    }

    #[test]
    fn keyfile_mode_never_touches_the_keyring() {
        let (_d, p) = paths();
        let kr = FakeKeyring::default();
        let (_, src) = resolve_with(&p, EncryptionMode::Keyfile, Some(&kr)).unwrap();
        assert_eq!(src, KeySource::Keyfile);
        assert!(kr.secret.borrow().is_none() && p.key_file().exists());
    }

    #[test]
    fn existing_database_without_key_is_an_error_and_no_key_is_made() {
        for mode in [EncryptionMode::Auto, EncryptionMode::Keyring, EncryptionMode::Keyfile] {
            let (_d, p) = paths();
            touch_db(&p);
            let kr = FakeKeyring::default();
            let err = resolve_with(&p, mode, Some(&kr)).unwrap_err().to_string();
            assert!(err.contains("never creates a new key"), "{mode:?}: {err}");
            assert!(kr.secret.borrow().is_none() && !p.key_file().exists());
        }
        let (_d, p) = paths();
        touch_db(&p);
        let broken = FakeKeyring { broken: true, ..Default::default() };
        let err = resolve_with(&p, EncryptionMode::Auto, Some(&broken)).unwrap_err().to_string();
        assert!(err.contains("no secret service"), "{err}");
    }

    /// A keyring shared between threads, slow enough to widen any race.
    #[derive(Default)]
    struct SharedKeyring(std::sync::Mutex<Option<String>>);

    impl Keyring for SharedKeyring {
        fn get(&self) -> Result<Option<String>> {
            std::thread::sleep(std::time::Duration::from_millis(5));
            Ok(self.0.lock().unwrap().clone())
        }
        fn set(&self, secret: &str) -> Result<()> {
            std::thread::sleep(std::time::Duration::from_millis(5));
            *self.0.lock().unwrap() = Some(secret.to_string());
            Ok(())
        }
    }

    #[test]
    fn simultaneous_first_starts_agree_on_one_key() {
        for mode in [EncryptionMode::Keyfile, EncryptionMode::Auto, EncryptionMode::Keyring] {
            for _ in 0..4 {
                let (_d, p) = paths();
                let kr = SharedKeyring::default();
                let barrier = std::sync::Barrier::new(4);
                let keys: Vec<String> = std::thread::scope(|s| {
                    let handles: Vec<_> = (0..4)
                        .map(|_| {
                            s.spawn(|| {
                                barrier.wait();
                                let keyring: Option<&dyn Keyring> = if mode == EncryptionMode::Keyfile { None } else { Some(&kr) };
                                let mut config = kanjo_core::config::Config::default();
                                config.storage.encryption = mode;
                                let opened = crate::open_store_with(&p, &config, keyring).and_then(|(store, _)| store.schema_version());
                                opened.unwrap_or_else(|e| panic!("{mode:?}: {e}"));
                                let (key, _) = resolve_with(&p, mode, keyring).unwrap();
                                key.unwrap().as_hex().to_string()
                            })
                        })
                        .collect();
                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                });
                assert!(keys.windows(2).all(|w| w[0] == w[1]), "{mode:?}: every process uses the same key");
                if mode != EncryptionMode::Keyfile {
                    assert_eq!(kr.0.lock().unwrap().as_deref(), Some(keys[0].as_str()));
                }
            }
        }
    }

    #[test]
    fn an_entry_that_appeared_meanwhile_is_not_overwritten() {
        let kr = FakeKeyring { secret: RefCell::new(Some(KEY.to_string())), ..Default::default() };
        let fresh = DbKey::from_hex(KEY.replace('0', "a")).unwrap();
        assert_eq!(store_in_keyring(&kr, fresh).unwrap().as_hex(), KEY);
        assert_eq!(kr.secret.borrow().as_deref(), Some(KEY));
    }

    #[test]
    fn an_interrupted_restore_is_not_replaced_by_an_empty_database() {
        let (_d, p) = paths();
        std::fs::create_dir_all(&p.home).unwrap();
        std::fs::write(p.key_file(), format!("{KEY}\n")).unwrap();
        std::fs::write(p.home.join("kanjo.db.restoring"), b"x").unwrap();
        let err = resolve_with(&p, EncryptionMode::Auto, None).unwrap_err().to_string();
        assert!(err.contains("restore was interrupted"), "{err}");
    }

    #[test]
    fn corrupt_key_file_is_reported_not_replaced() {
        let (_d, p) = paths();
        std::fs::create_dir_all(&p.home).unwrap();
        std::fs::write(p.key_file(), "not-a-key").unwrap();
        assert!(resolve_with(&p, EncryptionMode::Auto, None).is_err());
        assert_eq!(std::fs::read_to_string(p.key_file()).unwrap(), "not-a-key");
    }
}

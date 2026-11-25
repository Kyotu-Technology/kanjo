//! Database key lookup and creation.
//!
//! Order: an existing key file wins, then the OS keyring entry. Only when
//! neither holds a key and the database file does not exist yet is a new
//! 256-bit key generated and stored. An existing database without a key is an
//! error: a new key would make it unreadable for good.
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

/// The platform keyring: Secret Service (D-Bus) on Linux, Keychain on macOS,
/// Credential Manager on Windows. One entry per data directory.
pub struct OsKeyring {
    account: String,
}

impl OsKeyring {
    pub fn new(paths: &Paths) -> OsKeyring {
        OsKeyring { account: format!("database-key:{}", paths.home.display()) }
    }

    fn entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, &self.account).map_err(|e| Error::Other(format!("keyring unavailable: {e}")))
    }
}

impl Keyring for OsKeyring {
    fn get(&self) -> Result<Option<String>> {
        match self.entry()?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::Other(format!("keyring unavailable: {e}"))),
        }
    }

    fn set(&self, secret: &str) -> Result<()> {
        self.entry()?.set_password(secret).map_err(|e| Error::Other(format!("cannot store the key in the keyring: {e}")))
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
            store_in_keyring(kr, &key)?;
            Ok((Some(key), KeySource::Keyring))
        }
        _ => {
            if let Some(kr) = keyring {
                match store_in_keyring(kr, &key) {
                    Ok(()) => return Ok((Some(key), KeySource::Keyring)),
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

fn store_in_keyring(kr: &dyn Keyring, key: &DbKey) -> Result<()> {
    kr.set(key.as_hex())?;
    match kr.get()? {
        Some(back) if back.trim() == key.as_hex() => Ok(()),
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

    #[test]
    fn corrupt_key_file_is_reported_not_replaced() {
        let (_d, p) = paths();
        std::fs::create_dir_all(&p.home).unwrap();
        std::fs::write(p.key_file(), "not-a-key").unwrap();
        assert!(resolve_with(&p, EncryptionMode::Auto, None).is_err());
        assert_eq!(std::fs::read_to_string(p.key_file()).unwrap(), "not-a-key");
    }
}

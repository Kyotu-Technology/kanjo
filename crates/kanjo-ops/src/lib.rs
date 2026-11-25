//! kanjo-ops: database key management, audit log, retention and purge,
//! backup and restore, signed price import, access token and demo data.

pub mod audit;
pub mod backup;
pub mod demo;
pub mod keys;
pub mod pricing;
pub mod retention;
pub mod token;

use kanjo_core::api::KeySource;
use kanjo_core::config::Config;
use kanjo_core::paths::Paths;
use kanjo_core::{Result, Store};

/// Resolves the database key for `config.storage.encryption` and opens the
/// database, writing a pre-migration copy to `paths.backups_dir()` when the
/// schema has to be upgraded.
pub fn open_store(paths: &Paths, config: &Config) -> Result<(Store, KeySource)> {
    let keyring = keys::system_keyring(paths);
    open_store_with(paths, config, keyring.as_deref())
}

/// `open_store` with an explicit keyring backend (`None` = no keyring).
pub fn open_store_with(paths: &Paths, config: &Config, keyring: Option<&dyn keys::Keyring>) -> Result<(Store, KeySource)> {
    let (key, source) = keys::resolve_with(paths, config.storage.encryption, keyring)?;
    let store = Store::open(&paths.db(), key, Some(&paths.backups_dir()))?;
    Ok((store, source))
}

pub(crate) fn random_hex(bytes: usize) -> Result<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| kanjo_core::Error::Other(format!("no OS randomness: {e}")))?;
    let mut out = String::with_capacity(bytes * 2);
    for b in &buf {
        out.push_str(&format!("{b:02x}"));
    }
    buf.fill(0);
    Ok(out)
}

/// Writes `content` to a new file that is owner-only from the moment it exists.
pub(crate) fn create_private(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(content.as_bytes())?;
    f.sync_all()
}

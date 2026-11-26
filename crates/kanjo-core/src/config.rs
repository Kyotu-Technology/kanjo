use crate::{paths::Paths, Error, Result};
use serde::{Deserialize, Serialize};

/// Bootstrap configuration read before the database opens (`config.toml`).
/// Everything a user changes from the UI lives in `settings::Settings` instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub statusline: StatuslineConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Loopback by default. A non-loopback address is refused unless TLS is configured.
    pub bind: String,
    pub port: u16,
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    pub auth: AuthMode,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig { bind: "127.0.0.1".into(), port: DEFAULT_PORT, tls_cert: None, tls_key: None, auth: AuthMode::Token }
    }
}

pub const DEFAULT_PORT: u16 = 4747;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// Access token (cookie or bearer). Default.
    Token,
    /// No authentication; only honoured on loopback.
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub encryption: EncryptionMode,
}

impl Default for StorageConfig {
    fn default() -> Self {
        StorageConfig { encryption: EncryptionMode::Auto }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionMode {
    /// OS keyring when available, otherwise an owner-only key file.
    Auto,
    Keyring,
    Keyfile,
    Off,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StatuslineConfig {
    /// The user's previous statusLine command, run after Kanjo records the payload (chain mode).
    pub chain_command: Option<String>,
    /// Print Kanjo's own segment even when chaining.
    pub show_segment: bool,
    /// Mirror of the "store session titles" privacy setting for the statusline
    /// fast path, which never opens the database. Off: the session name is
    /// dropped before anything is written to the spool.
    pub capture_titles: bool,
}

impl Config {
    pub fn load(paths: &Paths) -> Result<Config> {
        let file = paths.config_file();
        match std::fs::read_to_string(&file) {
            Ok(text) => toml::from_str(&text).map_err(|e| Error::Config(format!("{}: {e}", file.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        paths.ensure_home()?;
        let text = toml::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))?;
        crate::paths::write_private_atomic(&paths.config_file(), text.as_bytes())
    }
}

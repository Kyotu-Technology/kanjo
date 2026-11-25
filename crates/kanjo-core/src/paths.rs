use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// Every filesystem location Kanjo touches. Nothing else in the workspace may
/// build paths on its own; overrides (`KANJO_HOME`, `KANJO_CLAUDE_DIR`) exist so
/// tests and `kanjo demo` never touch the real `~/.claude`.
#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
    pub claude_dir: PathBuf,
}

impl Paths {
    pub fn resolve(home: Option<PathBuf>, claude_dir: Option<PathBuf>) -> Result<Paths> {
        let home = match home.or_else(|| std::env::var_os("KANJO_HOME").map(PathBuf::from)) {
            Some(p) => p,
            None => directories::ProjectDirs::from("io", "Kyotu", "kanjo")
                .ok_or_else(|| Error::Config("cannot determine the user data directory".into()))?
                .data_dir()
                .to_path_buf(),
        };
        let claude_dir = match claude_dir
            .or_else(|| std::env::var_os("KANJO_CLAUDE_DIR").map(PathBuf::from))
            .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
        {
            Some(p) => p,
            None => directories::BaseDirs::new()
                .ok_or_else(|| Error::Config("cannot determine the home directory".into()))?
                .home_dir()
                .join(".claude"),
        };
        Ok(Paths { home, claude_dir })
    }

    pub fn at(home: impl AsRef<Path>, claude_dir: impl AsRef<Path>) -> Paths {
        Paths { home: home.as_ref().to_path_buf(), claude_dir: claude_dir.as_ref().to_path_buf() }
    }

    pub fn db(&self) -> PathBuf {
        self.home.join("kanjo.db")
    }
    pub fn config_file(&self) -> PathBuf {
        self.home.join("config.toml")
    }
    pub fn spool_dir(&self) -> PathBuf {
        self.home.join("spool")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.home.join("backups")
    }
    pub fn key_file(&self) -> PathBuf {
        self.home.join("kanjo.key")
    }
    pub fn token_file(&self) -> PathBuf {
        self.home.join("access.token")
    }
    pub fn log_dir(&self) -> PathBuf {
        self.home.join("logs")
    }
    pub fn claude_projects(&self) -> PathBuf {
        self.claude_dir.join("projects")
    }
    pub fn claude_settings(&self) -> PathBuf {
        self.claude_dir.join("settings.json")
    }

    /// Creates the data directory, owner-only on Unix.
    pub fn ensure_home(&self) -> Result<()> {
        create_private_dir(&self.home)
    }
}

pub fn create_private_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Restricts a file to its owner on Unix. On Windows the per-user profile ACL applies.
pub fn make_private_file(file: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

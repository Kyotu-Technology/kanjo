use crate::{Error, Result};
use std::ffi::OsString;
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
            None => UserDirs::current().ok_or_else(|| Error::Config("cannot determine the user data directory".into()))?.kanjo_data_dir(),
        };
        let claude_dir = match claude_dir
            .or_else(|| std::env::var_os("KANJO_CLAUDE_DIR").map(PathBuf::from))
            .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
        {
            Some(p) => p,
            None => std::env::home_dir().ok_or_else(|| Error::Config("cannot determine the home directory".into()))?.join(".claude"),
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
    /// Held while the database key is looked up or created, so two processes
    /// starting at once agree on one key. Never deleted.
    pub fn key_lock_file(&self) -> PathBuf {
        self.home.join("kanjo.key.lock")
    }
    pub fn token_file(&self) -> PathBuf {
        self.home.join("access.token")
    }
    /// Written by a running server (address, process id) so the command line can find it.
    pub fn server_file(&self) -> PathBuf {
        self.home.join("server.json")
    }
    /// Single-use browser login codes handed from the command line to the running server.
    pub fn login_dir(&self) -> PathBuf {
        self.home.join("login")
    }
    /// Short-lived owner-only pages that pass a sign-in URL to the browser, so
    /// the opener's command line names a file instead of the URL.
    pub fn open_dir(&self) -> PathBuf {
        self.home.join("open")
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

/// How the platform lays out per-user directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Linux and the BSDs: the XDG base directory variables, then `~/.config` and `~/.local/share`.
    Xdg,
    /// `~/Library/Application Support`.
    MacOs,
    /// `%APPDATA%` (roaming) and `%LOCALAPPDATA%`, then their default places under the profile.
    Windows,
}

impl Layout {
    pub fn current() -> Layout {
        if cfg!(target_os = "macos") {
            Layout::MacOs
        } else if cfg!(windows) {
            Layout::Windows
        } else {
            Layout::Xdg
        }
    }
}

/// The per-user base directories. They resolve as the `directories` crate
/// resolved them in earlier releases, so existing installs keep their paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDirs {
    pub layout: Layout,
    pub home: PathBuf,
    pub config: PathBuf,
    pub data: PathBuf,
    pub data_local: PathBuf,
}

impl UserDirs {
    /// The directories of the current user, or `None` without a home directory.
    pub fn current() -> Option<UserDirs> {
        Some(UserDirs::resolve(Layout::current(), std::env::home_dir()?, |name| std::env::var_os(name)))
    }

    /// The directories for `layout` given the home directory and an environment lookup.
    pub fn resolve(layout: Layout, home: PathBuf, env: impl Fn(&str) -> Option<OsString>) -> UserDirs {
        match layout {
            Layout::Xdg => {
                let xdg = |name: &str, default: &str| {
                    env(name).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(default))
                };
                let config = xdg("XDG_CONFIG_HOME", ".config");
                let data = xdg("XDG_DATA_HOME", ".local/share");
                UserDirs { layout, config, data_local: data.clone(), data, home }
            }
            Layout::MacOs => {
                let support = home.join("Library/Application Support");
                UserDirs { layout, config: support.clone(), data: support.clone(), data_local: support, home }
            }
            Layout::Windows => {
                let known = |name: &str, default: &str| {
                    env(name).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join(default))
                };
                let data = known("APPDATA", "Roaming");
                UserDirs { layout, config: data.clone(), data, data_local: known("LOCALAPPDATA", "Local"), home }
            }
        }
    }

    /// Kanjo's default data directory.
    pub fn kanjo_data_dir(&self) -> PathBuf {
        match self.layout {
            Layout::Xdg => self.data.join("kanjo"),
            Layout::MacOs => self.data.join("io.Kyotu.kanjo"),
            Layout::Windows => self.data.join("Kyotu").join("kanjo").join("data"),
        }
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

/// Replaces `path` with `bytes` so a reader sees the old or the new content,
/// never a torn file: written to a temporary owner-only file in the same
/// directory, flushed to disk, then renamed over the target.
pub fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    let result = (|| -> Result<()> {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        make_private_file(&tmp)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.toml");
        std::fs::write(&file, "old").unwrap();
        write_private_atomic(&file, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| OsString::from(v))
    }

    #[test]
    fn xdg_layout_honours_absolute_variables_only() {
        let home = PathBuf::from("/home/u");
        let d = UserDirs::resolve(Layout::Xdg, home.clone(), env(&[]));
        assert_eq!(d.kanjo_data_dir(), PathBuf::from("/home/u/.local/share/kanjo"));
        assert_eq!(d.config, PathBuf::from("/home/u/.config"));
        let d = UserDirs::resolve(Layout::Xdg, home.clone(), env(&[("XDG_DATA_HOME", "/x/data"), ("XDG_CONFIG_HOME", "/x/cfg")]));
        assert_eq!(d.kanjo_data_dir(), PathBuf::from("/x/data/kanjo"));
        assert_eq!(d.config, PathBuf::from("/x/cfg"));
        let d = UserDirs::resolve(Layout::Xdg, home, env(&[("XDG_DATA_HOME", "rel"), ("XDG_CONFIG_HOME", "")]));
        assert_eq!(d.kanjo_data_dir(), PathBuf::from("/home/u/.local/share/kanjo"));
        assert_eq!(d.config, PathBuf::from("/home/u/.config"));
    }

    #[test]
    fn macos_layout_uses_the_bundle_id() {
        let d = UserDirs::resolve(Layout::MacOs, PathBuf::from("/Users/u"), env(&[("XDG_DATA_HOME", "/x")]));
        assert_eq!(d.kanjo_data_dir(), PathBuf::from("/Users/u/Library/Application Support/io.Kyotu.kanjo"));
        assert_eq!(d.home, PathBuf::from("/Users/u"));
    }

    #[test]
    fn windows_layout_uses_appdata_with_profile_fallback() {
        let home = PathBuf::from("C:/Users/u");
        let d = UserDirs::resolve(Layout::Windows, home.clone(), env(&[("APPDATA", "D:/roam"), ("LOCALAPPDATA", "D:/local")]));
        assert_eq!(d.kanjo_data_dir(), PathBuf::from("D:/roam").join("Kyotu").join("kanjo").join("data"));
        assert_eq!(d.data_local, PathBuf::from("D:/local"));
        let d = UserDirs::resolve(Layout::Windows, home.clone(), env(&[("APPDATA", "")]));
        assert_eq!(d.data, home.join("AppData").join("Roaming"));
        assert_eq!(d.data_local, home.join("AppData").join("Local"));
    }
}

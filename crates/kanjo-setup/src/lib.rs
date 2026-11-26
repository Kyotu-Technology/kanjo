//! kanjo-setup: Kanjo's integration with the user's machine: Claude Code's
//! settings.json (statusLine and session hooks), the record a running server
//! leaves in the data directory and the login codes handed to it,
//! the background service, `kanjo doctor` and the statusline text.

pub mod claude;
pub mod doctor;
pub mod server_file;
pub mod service;
pub mod shell;
pub mod statusline;

use kanjo_core::api::HookStatus;
use kanjo_core::config::Config;
use kanjo_core::paths::Paths;
use std::path::{Path, PathBuf};

#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The path of the running executable that Kanjo records in settings.json and
/// in the service definition, and compares recorded entries with; see [`stable_exe`].
pub fn current_exe() -> std::io::Result<PathBuf> {
    std::env::current_exe().map(|exe| stable_exe(&exe))
}

/// `exe` itself, unless it is a Homebrew keg binary
/// (`<prefix>/Cellar/kanjo/<version>/bin/kanjo`) and `<prefix>/bin/kanjo`
/// resolves to the same file: then that link, which survives `brew upgrade`.
pub fn stable_exe(exe: &Path) -> PathBuf {
    homebrew_link(exe).unwrap_or_else(|| exe.to_path_buf())
}

fn homebrew_link(exe: &Path) -> Option<PathBuf> {
    let name = exe.file_name()?;
    let bin = exe.parent()?;
    let keg = bin.parent()?;
    let formula = keg.parent()?;
    let cellar = formula.parent()?;
    let shaped = name == "kanjo" && bin.file_name()? == "bin" && keg.file_name().is_some() && formula.file_name()? == "kanjo";
    if !shaped || cellar.file_name()? != "Cellar" {
        return None;
    }
    let link = cellar.parent()?.join("bin").join(name);
    let same = std::fs::canonicalize(&link).ok()? == std::fs::canonicalize(exe).ok()?;
    same.then_some(link)
}

/// The `HookStatus` of the API. `last_statusline_at` is left `None` for the
/// caller to fill from the database. Starts no process: the service counts as
/// installed when its definition file exists.
pub fn hook_status(paths: &Paths, config: &Config) -> HookStatus {
    let st = claude::status(paths, config).ok();
    HookStatus {
        statusline_installed: st.as_ref().is_some_and(|s| s.statusline_installed()),
        statusline_chain: st.as_ref().and_then(|s| s.chain_command.clone()),
        session_hooks_installed: st.as_ref().is_some_and(|s| s.session_hooks_installed()),
        last_statusline_at: None,
        service_installed: service::is_installed(),
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    fn keg(prefix: &Path, version: &str) -> PathBuf {
        let bin = prefix.join("Cellar/kanjo").join(version).join("bin");
        fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("kanjo");
        fs::write(&exe, b"").unwrap();
        exe
    }

    #[test]
    fn homebrew_keg_records_the_prefix_link() {
        let dir = tempfile::tempdir().unwrap();
        let exe = keg(dir.path(), "0.1.0");
        fs::create_dir_all(dir.path().join("bin")).unwrap();
        symlink("../Cellar/kanjo/0.1.0/bin/kanjo", dir.path().join("bin/kanjo")).unwrap();
        assert_eq!(stable_exe(&exe), dir.path().join("bin/kanjo"));
        assert_eq!(stable_exe(&dir.path().join("bin/kanjo")), dir.path().join("bin/kanjo"));
    }

    #[test]
    fn keg_without_a_matching_link_stays_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let exe = keg(dir.path(), "0.1.0");
        assert_eq!(stable_exe(&exe), exe);

        let newer = keg(dir.path(), "0.2.0");
        fs::create_dir_all(dir.path().join("bin")).unwrap();
        symlink(&newer, dir.path().join("bin/kanjo")).unwrap();
        assert_eq!(stable_exe(&exe), exe);

        let broken = tempfile::tempdir().unwrap();
        let exe = keg(broken.path(), "0.1.0");
        fs::create_dir_all(broken.path().join("bin")).unwrap();
        symlink(broken.path().join("Cellar/kanjo/9.9.9/bin/kanjo"), broken.path().join("bin/kanjo")).unwrap();
        assert_eq!(stable_exe(&exe), exe);
    }

    #[test]
    fn other_layouts_are_not_touched() {
        let dir = tempfile::tempdir().unwrap();
        for rel in ["Cellar/other/0.1.0/bin/kanjo", "Cellar/kanjo/0.1.0/libexec/kanjo", "Kegs/kanjo/0.1.0/bin/kanjo"] {
            let exe = dir.path().join(rel);
            fs::create_dir_all(exe.parent().unwrap()).unwrap();
            fs::write(&exe, b"").unwrap();
            fs::create_dir_all(dir.path().join("bin")).unwrap();
            let link = dir.path().join("bin/kanjo");
            let _ = fs::remove_file(&link);
            symlink(&exe, &link).unwrap();
            assert_eq!(stable_exe(&exe), exe, "{rel}");
        }
        let plain = dir.path().join("bin/kanjo");
        assert_eq!(stable_exe(&plain), plain);
        assert_eq!(
            stable_exe(Path::new("/nonexistent/Cellar/kanjo/0.1.0/bin/kanjo")),
            Path::new("/nonexistent/Cellar/kanjo/0.1.0/bin/kanjo")
        );
    }
}

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

#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

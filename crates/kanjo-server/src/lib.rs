//! kanjo-server: the HTTP API, the embedded web UI and the runtime that drives
//! ingest and alert loops. The CLI and a future desktop shell both start it
//! through this interface.

use kanjo_core::config::Config;
use kanjo_core::paths::Paths;
use kanjo_core::{Error, Result};

/// How to start the server.
#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub paths: Paths,
    pub config: Config,
    /// Open the dashboard in the default browser once the server listens.
    pub open_browser: bool,
    /// Serve synthetic data: populate a demo database under `paths.home`, never
    /// read the Claude directory, never touch the OS keyring or settings.json.
    pub demo: bool,
}

/// Runs the server until it is asked to stop (Ctrl-C or SIGTERM). Blocking;
/// creates its own async runtime.
pub fn run(opts: ServeOptions) -> Result<()> {
    let _ = opts;
    Err(Error::Other("the server is not built yet".into()))
}

/// The address a browser should open, including the access token when authentication is on.
pub fn access_url(paths: &Paths, config: &Config) -> Result<String> {
    let _ = (paths, config);
    Err(Error::Other("the server is not built yet".into()))
}

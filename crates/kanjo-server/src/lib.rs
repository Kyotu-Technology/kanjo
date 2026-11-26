//! kanjo-server: the HTTP API, the embedded web UI and the runtime that drives
//! ingest and alert loops. The CLI starts it through this interface. It offers
//! [`run`] for a blocking process, [`start`] or [`serve`] inside an existing
//! tokio runtime, and [`router`] when an embedding process wants to wrap the
//! router in its own layers.

mod assets;
mod error;
mod events;
mod handlers;
mod net;
mod runtime;
mod security;
mod state;

use kanjo_core::config::{AuthMode, Config};
use kanjo_core::paths::Paths;
use kanjo_core::Result;
use std::future::Future;
use std::time::Duration;

pub use handlers::router;
pub use net::{check_bind, start, Server};
pub use state::AppState;

/// Days of invented history a demo server starts with unless told otherwise.
pub const DEMO_DAYS: u32 = 30;

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
    /// Days of invented history in demo mode; `None` means [`DEMO_DAYS`].
    pub demo_days: Option<u32>,
}

/// Runs the server until it is asked to stop (Ctrl-C or SIGTERM). Blocking;
/// creates its own async runtime.
pub fn run(opts: ServeOptions) -> Result<()> {
    run_with(opts, |_| {})
}

/// [`run`], calling `ready` once the server listens (before the browser is
/// opened), so a front end can print the address it really got.
pub fn run_with(opts: ServeOptions, ready: impl FnOnce(&Server)) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let open = opts.open_browser;
    let result = rt.block_on(async move {
        let server = start(opts).await?;
        tracing::info!(url = %server.url(), "Kanjo is listening");
        ready(&server);
        if open {
            open_in_browser(&server.state().inner.paths, &server.login_url());
        }
        tokio::select! {
            _ = shutdown_signal() => tracing::info!("shutting down"),
            _ = server.stopped() => {}
        }
        server.shutdown().await
    });
    rt.shutdown_timeout(Duration::from_secs(10));
    result
}

/// Serves until `shutdown` resolves, inside the caller's tokio runtime. A host
/// that needs the bound address (port 0) uses [`start`] instead.
pub async fn serve(opts: ServeOptions, shutdown: impl Future<Output = ()> + Send) -> Result<()> {
    let server = start(opts).await?;
    tokio::select! {
        _ = shutdown => {}
        _ = server.stopped() => {}
    }
    server.shutdown().await
}

/// `AuthMode::None` is honoured only on a loopback bind.
pub(crate) fn auth_enabled(config: &Config) -> bool {
    config.server.auth == AuthMode::Token || !net::is_loopback_bind(&config.server.bind)
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "cannot listen for SIGTERM; only Ctrl-C stops the server");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Starts the platform's opener on an owner-only local page that forwards to
/// `url`, so the login code is not on the opener's command line. The page is
/// removed when the code is redeemed, or after a short delay. No network
/// library is involved.
fn open_in_browser(paths: &kanjo_core::paths::Paths, url: &str) {
    use kanjo_setup::server_file::{launch_page, login_code_of, LAUNCH_PAGE_TTL};
    use std::process::{Command, Stdio};
    let page = match login_code_of(url).map(|_| launch_page(paths, url)) {
        Some(Ok(p)) => Some(p),
        Some(Err(e)) => {
            tracing::warn!(error = %e, "could not write the sign-in page; run `kanjo open` instead");
            return;
        }
        None => None,
    };
    let target = page.as_ref().map_or(std::ffi::OsStr::new(url), |p| p.as_os_str());
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(target);
        c
    };
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler").arg(target);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(target);
        c
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match cmd.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
                if let Some(page) = page {
                    // Redeeming the code removes the page; one still here was never used.
                    let deadline = std::time::Instant::now() + LAUNCH_PAGE_TTL;
                    while page.exists() && std::time::Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(250));
                    }
                    if std::fs::remove_file(&page).is_ok() {
                        tracing::warn!("the browser did not sign in; a sandboxed browser may be unable to open the local sign-in page, so use `kanjo open` or the printed link");
                    }
                }
            });
        }
        Err(e) => {
            if let Some(page) = &page {
                let _ = std::fs::remove_file(page);
            }
            tracing::warn!(error = %e, "could not open a browser; run `kanjo open` instead");
        }
    }
}

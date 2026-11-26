//! Binding, TLS and the accept loop.

use crate::runtime::{self, Runtime};
use crate::security::{ConnState, LocalAddr};
use crate::state::AppState;
use crate::ServeOptions;
use axum::extract::ConnectInfo;
use axum::Router;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use kanjo_core::config::{AuthMode, Config};
use kanjo_core::{Error, Result};
use kanjo_setup::server_file::{self, Announcement, ServerRecord};
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt;

const MAX_CONNECTIONS: usize = 256;
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

fn parse_bind(bind: &str) -> Result<IpAddr> {
    let b = bind.trim();
    if b.eq_ignore_ascii_case("localhost") {
        return Ok(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    b.trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .map_err(|_| Error::Config(format!("server.bind must be an IP address or \"localhost\", not {b:?}")))
}

pub(crate) fn is_loopback_bind(bind: &str) -> bool {
    parse_bind(bind).is_ok_and(|ip| ip.is_loopback())
}

/// Validates `config.server` and returns the address to listen on. A
/// non-loopback address is refused unless a TLS certificate and key are
/// configured and token authentication is on. Binds nothing.
pub fn check_bind(config: &Config) -> Result<SocketAddr> {
    let s = &config.server;
    let ip = parse_bind(&s.bind)?;
    let tls = match (&s.tls_cert, &s.tls_key) {
        (Some(_), Some(_)) => true,
        (None, None) => false,
        _ => return Err(Error::Config("server.tls_cert and server.tls_key must be set together".into())),
    };
    if !ip.is_loopback() && !(tls && s.auth == AuthMode::Token) {
        return Err(Error::Config(format!(
            "refusing to listen on {ip}: an address other than loopback needs server.tls_cert, server.tls_key and auth = \"token\""
        )));
    }
    Ok(SocketAddr::new(ip, s.port))
}

/// `http(s)://host:port/` for a browser on this machine.
pub(crate) fn base_url(config: &Config, addr: SocketAddr) -> String {
    let scheme = if config.server.tls_cert.is_some() { "https" } else { "http" };
    let host = if config.server.bind.trim().eq_ignore_ascii_case("localhost") {
        "localhost".to_string()
    } else {
        match addr.ip() {
            IpAddr::V4(v4) if v4.is_unspecified() => "127.0.0.1".to_string(),
            IpAddr::V6(v6) if v6.is_unspecified() => "[::1]".to_string(),
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => format!("[{v6}]"),
        }
    };
    format!("{scheme}://{host}:{}/", addr.port())
}

fn tls_acceptor(cert: &str, key: &str) -> Result<TlsAcceptor> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let bad = |what: &str, e: &dyn std::fmt::Display| Error::Config(format!("{what}: {e}"));
    let certs = CertificateDer::pem_file_iter(cert)
        .map_err(|e| bad(&format!("cannot read the TLS certificate {cert}"), &e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| bad(&format!("invalid TLS certificate {cert}"), &e))?;
    if certs.is_empty() {
        return Err(Error::Config(format!("no certificate found in {cert}")));
    }
    warn_if_readable_by_others(key);
    let key_der = PrivateKeyDer::from_pem_file(key).map_err(|e| bad(&format!("cannot read the TLS key {key}"), &e))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| bad("TLS setup", &e))?
        .with_no_client_auth()
        .with_single_cert(certs, key_der)
        .map_err(|e| bad("TLS certificate and key do not form a usable pair", &e))?;
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(cfg)))
}

fn warn_if_readable_by_others(path: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.permissions().mode() & 0o077 != 0 {
                tracing::warn!(file = path, "the TLS private key is readable by other users; restrict it to its owner (chmod 600)");
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn bind_error(addr: SocketAddr, e: std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::AddrInUse => Error::Io(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            format!(
                "{addr} is already in use: Kanjo is probably already running (open it with `kanjo open`); \
                 otherwise another program holds the port, so set server.port in config.toml"
            ),
        )),
        std::io::ErrorKind::PermissionDenied => Error::Other(format!("not allowed to listen on {addr}; choose a port above 1024")),
        _ => Error::Other(format!("cannot listen on {addr}: {e}")),
    }
}

/// A running server: listener, background loops and the shared state.
pub struct Server {
    addr: SocketAddr,
    url: String,
    state: AppState,
    accept: JoinHandle<()>,
    runtime: Runtime,
    done: watch::Receiver<bool>,
    announcement: Option<Announcement>,
}

/// Validates the configuration, binds, opens the store and starts serving and
/// the background loops. Port 0 picks a free port; read it from [`Server::local_addr`].
pub async fn start(opts: ServeOptions) -> Result<Server> {
    let addr = check_bind(&opts.config)?;
    if let Some(other) = server_file::live(&opts.paths) {
        return Err(server_file::already_running(&opts.paths, &other));
    }
    let tls = match (&opts.config.server.tls_cert, &opts.config.server.tls_key) {
        (Some(cert), Some(key)) => Some(tls_acceptor(cert, key)?),
        _ => None,
    };
    let listener = TcpListener::bind(addr).await.map_err(|e| bind_error(addr, e))?;
    let local = listener.local_addr()?;
    let state = AppState::open(&opts).await?;
    state.set_port(local.port());
    let runtime = runtime::start(&state);
    let app = crate::router(state.clone());
    let stop = state.inner.shutdown.subscribe();
    let (done_tx, done) = watch::channel(false);
    let accept = tokio::spawn(async move {
        accept_loop(listener, tls, app, stop).await;
        done_tx.send_replace(true);
    });
    let url = base_url(&opts.config, local);
    let record = ServerRecord {
        pid: std::process::id(),
        addr: local,
        url: url.clone(),
        auth: state.inner.token.is_some(),
        demo: opts.demo,
        instance: state.inner.instance.clone(),
        version: kanjo_core::VERSION.to_string(),
        started_at: kanjo_core::time::now_ms(),
    };
    let paths = opts.paths.clone();
    let announcement = match tokio::task::spawn_blocking(move || server_file::announce(&paths, &record)).await {
        Ok(Ok(a)) => Some(a),
        Ok(Err(e)) => {
            state.begin_shutdown();
            runtime.stop().await;
            return Err(e);
        }
        Err(e) => return Err(Error::Other(format!("start-up task failed: {e}"))),
    };
    Ok(Server { addr: local, url, state, accept, runtime, done, announcement })
}

impl Server {
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The dashboard URL without credentials.
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// A URL that signs a browser in: it carries a single-use login code valid
    /// for two minutes instead of the access token.
    pub fn login_url(&self) -> String {
        match self.state.inner.token.as_ref().and_then(|_| self.state.inner.issue_login_code()) {
            Some(code) => format!("{}?token={code}", self.url),
            None => self.url.clone(),
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Resolves if the server stops on its own.
    pub async fn stopped(&self) {
        let mut done = self.done.clone();
        let _ = done.wait_for(|d| *d).await;
    }

    /// Stops accepting, ends event streams, lets open requests finish (up to
    /// five seconds) and stops the background loops.
    pub async fn shutdown(mut self) -> Result<()> {
        self.state.begin_shutdown();
        let abort = self.accept.abort_handle();
        if tokio::time::timeout(DRAIN_TIMEOUT * 2, &mut self.accept).await.is_err() {
            abort.abort();
        }
        self.runtime.stop().await;
        drop(self.announcement.take());
        Ok(())
    }
}

struct Conn {
    proven: Arc<AtomicBool>,
    close: Arc<Notify>,
}

/// Open connections, oldest first. When all slots are taken, the oldest
/// connection that has not yet presented a valid credential is closed to make
/// room, so idle or unauthenticated sockets cannot keep the owner out.
#[derive(Default)]
struct Conns {
    next: u64,
    open: BTreeMap<u64, Conn>,
}

impl Conns {
    fn admit(&mut self, limit: usize) -> Option<(u64, Arc<AtomicBool>, Arc<Notify>)> {
        if self.open.len() >= limit {
            let victim = self.open.iter().find(|(_, c)| !c.proven.load(Ordering::Relaxed)).map(|(id, _)| *id)?;
            if let Some(c) = self.open.remove(&victim) {
                c.close.notify_one();
            }
        }
        let id = self.next;
        self.next += 1;
        let conn = Conn { proven: Arc::new(AtomicBool::new(false)), close: Arc::new(Notify::new()) };
        let out = (id, conn.proven.clone(), conn.close.clone());
        self.open.insert(id, conn);
        Some(out)
    }
}

async fn accept_loop(listener: TcpListener, tls: Option<TlsAcceptor>, app: Router, mut stop: watch::Receiver<bool>) {
    let graceful = GracefulShutdown::new();
    let conns = Arc::new(Mutex::new(Conns::default()));
    let mut http = hyper::server::conn::http1::Builder::new();
    http.timer(TokioTimer::new()).header_read_timeout(HEADER_TIMEOUT);
    loop {
        if *stop.borrow() {
            break;
        }
        let (tcp, peer) = tokio::select! {
            r = listener.accept() => match r {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::warn!(error = %e, "accept failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            },
            changed = stop.changed() => {
                if changed.is_err() { break }
                continue;
            }
        };
        let admitted = conns.lock().unwrap_or_else(|e| e.into_inner()).admit(MAX_CONNECTIONS);
        let Some((id, proven, close)) = admitted else {
            tracing::warn!("connection limit reached; closing a new connection");
            continue;
        };
        let release = {
            let conns = conns.clone();
            move || {
                conns.lock().unwrap_or_else(|e| e.into_inner()).open.remove(&id);
            }
        };
        let _ = tcp.set_nodelay(true);
        let local = tcp.local_addr().ok();
        let svc = app.clone().map_request(move |mut req: axum::http::Request<hyper::body::Incoming>| {
            req.extensions_mut().insert(ConnectInfo(peer));
            req.extensions_mut().insert(ConnState(proven.clone()));
            if let Some(l) = local {
                req.extensions_mut().insert(LocalAddr(l));
            }
            req
        });
        let svc = TowerToHyperService::new(svc);
        let http = http.clone();
        let watcher = graceful.watcher();
        match &tls {
            None => {
                tokio::spawn(async move {
                    let conn = http.serve_connection(TokioIo::new(tcp), svc);
                    tokio::select! {
                        _ = watcher.watch(conn) => {}
                        _ = close.notified() => {}
                    }
                    release();
                });
            }
            Some(acceptor) => {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let handshake = tokio::select! {
                        r = tokio::time::timeout(TLS_HANDSHAKE_TIMEOUT, acceptor.accept(tcp)) => r,
                        _ = close.notified() => {
                            release();
                            return;
                        }
                    };
                    let stream = match handshake {
                        Ok(Ok(s)) => s,
                        Ok(Err(e)) => {
                            tracing::debug!(error = %e, "TLS handshake failed");
                            release();
                            return;
                        }
                        Err(_) => {
                            release();
                            return;
                        }
                    };
                    let conn = http.serve_connection(TokioIo::new(stream), svc);
                    tokio::select! {
                        _ = watcher.watch(conn) => {}
                        _ = close.notified() => {}
                    }
                    release();
                });
            }
        }
    }
    drop(listener);
    if tokio::time::timeout(DRAIN_TIMEOUT, graceful.shutdown()).await.is_err() {
        tracing::warn!("connections still open after {} s; closing them", DRAIN_TIMEOUT.as_secs());
    }
}

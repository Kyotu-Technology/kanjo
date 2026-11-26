//! The record a running server leaves in the data directory
//! (`Paths::server_file`) and the single-use login codes the command line
//! hands to it (`Paths::login_dir`).
//!
//! The server holds a shared lock on its record for as long as it runs, so a
//! record left behind by a crash is recognised as stale: nobody holds the lock.
//! A login code names the instance id of the record it was written for, so a
//! code that reaches some other process listening on the port is worthless,
//! and the permanent access token never appears in a URL.

use kanjo_core::paths::{create_private_dir, Paths};
use kanjo_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a login code stays valid.
pub const LOGIN_CODE_TTL_MS: i64 = 120_000;
const CLOCK_SKEW_MS: i64 = 5_000;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

/// What a running server tells the command line about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerRecord {
    pub pid: u32,
    /// The address the server listens on.
    pub addr: SocketAddr,
    /// The dashboard address for a browser on this machine, without credentials.
    pub url: String,
    /// The server asks for the access token.
    pub auth: bool,
    pub demo: bool,
    /// Random per start; login codes are only accepted by the instance they name.
    pub instance: String,
    pub version: String,
    pub started_at: i64,
}

impl ServerRecord {
    /// The address to connect to from this machine (a wildcard bind answers on loopback).
    pub fn local_addr(&self) -> SocketAddr {
        let ip = match self.addr.ip() {
            IpAddr::V4(v4) if v4.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(v6) if v6.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
            ip => ip,
        };
        SocketAddr::new(ip, self.addr.port())
    }

    fn answers(&self) -> bool {
        TcpStream::connect_timeout(&self.local_addr(), CONNECT_TIMEOUT).is_ok()
    }
}

#[derive(Serialize, Deserialize)]
struct LoginCode {
    instance: String,
    created_at: i64,
}

/// `bytes` random bytes from the OS, as lowercase hex.
pub fn random_hex(bytes: usize) -> Result<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| Error::Other(format!("no OS randomness: {e}")))?;
    let out = buf.iter().map(|b| format!("{b:02x}")).collect();
    buf.fill(0);
    Ok(out)
}

fn create_private(path: &Path) -> std::io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

enum Probe {
    Missing,
    Stale,
    Live(ServerRecord),
}

fn probe(paths: &Paths) -> Probe {
    let Ok(mut f) = File::open(paths.server_file()) else { return Probe::Missing };
    let held = match f.try_lock() {
        Ok(()) => {
            let _ = f.unlock();
            Some(false)
        }
        Err(TryLockError::WouldBlock) => Some(true),
        Err(TryLockError::Error(_)) => None,
    };
    let mut text = String::new();
    let record = f.read_to_string(&mut text).ok().and_then(|_| serde_json::from_str::<ServerRecord>(&text).ok());
    match (held, record) {
        (Some(true), Some(r)) => Probe::Live(r),
        (None, Some(r)) if r.answers() => Probe::Live(r),
        _ => Probe::Stale,
    }
}

/// The server using this data directory, if one is alive (its process holds
/// the record), whether or not it answers yet.
pub fn live(paths: &Paths) -> Option<ServerRecord> {
    match probe(paths) {
        Probe::Live(r) => Some(r),
        _ => None,
    }
}

/// The server using this data directory, if one is alive and accepts connections.
pub fn answering(paths: &Paths) -> Option<ServerRecord> {
    live(paths).filter(ServerRecord::answers)
}

/// Held by a running server. Dropping it releases the lock and removes the
/// record, unless another server has replaced it since.
#[derive(Debug)]
pub struct Announcement {
    file: Option<File>,
    path: PathBuf,
    instance: String,
}

/// Writes `record` to `paths.server_file()` (owner-only) and keeps it locked.
/// Fails when another live server already uses this data directory. Clears
/// login codes left from earlier instances.
pub fn announce(paths: &Paths, record: &ServerRecord) -> Result<Announcement> {
    if let Some(other) = live(paths) {
        return Err(already_running(paths, &other));
    }
    paths.ensure_home()?;
    let path = paths.server_file();
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    let mut f = create_private(&path)?;
    f.write_all(serde_json::to_string_pretty(record)?.as_bytes())?;
    f.sync_all()?;
    if let Err(e) = f.lock_shared() {
        tracing::warn!(error = %e, file = %path.display(), "cannot lock the server record; other commands check the port instead");
    }
    clear_login_codes(paths);
    Ok(Announcement { file: Some(f), path, instance: record.instance.clone() })
}

/// The error `announce` gives when the data directory is already served.
pub fn already_running(paths: &Paths, other: &ServerRecord) -> Error {
    Error::Other(format!(
        "Kanjo is already running for {} at {} (process {}); open it with `kanjo open`",
        paths.home.display(),
        other.url,
        other.pid
    ))
}

impl Drop for Announcement {
    fn drop(&mut self) {
        drop(self.file.take());
        let ours = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|t| serde_json::from_str::<ServerRecord>(&t).ok())
            .is_some_and(|r| r.instance == self.instance);
        if ours {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn is_code(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn remove_codes(dir: &Path, keep: impl Fn(&LoginCode) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let code = std::fs::read_to_string(e.path()).ok().and_then(|t| serde_json::from_str::<LoginCode>(&t).ok());
        if !code.as_ref().is_some_and(&keep) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Removes every login code (a server does this when it starts).
pub fn clear_login_codes(paths: &Paths) {
    remove_codes(&paths.login_dir(), |_| false);
}

/// Writes a single-use login code for `record` and returns the URL that signs
/// a browser in with it. A server without authentication gets its plain URL.
pub fn login_url(paths: &Paths, record: &ServerRecord, now: i64) -> Result<String> {
    if !record.auth {
        return Ok(record.url.clone());
    }
    let dir = paths.login_dir();
    create_private_dir(&dir)?;
    remove_codes(&dir, |c| now - c.created_at <= LOGIN_CODE_TTL_MS);
    let code = random_hex(32)?;
    let body = serde_json::to_string(&LoginCode { instance: record.instance.clone(), created_at: now })?;
    let mut f = create_private(&dir.join(&code))?;
    f.write_all(body.as_bytes())?;
    f.sync_all()?;
    Ok(format!("{}?token={code}", record.url))
}

/// Accepts `given` once if it is a login code written for `instance` within
/// the last two minutes. The code file is deleted whatever the outcome.
pub fn claim_login_code(paths: &Paths, instance: &str, given: &str, now: i64) -> bool {
    if !is_code(given) {
        return false;
    }
    let path = paths.login_dir().join(given);
    let Ok(text) = std::fs::read_to_string(&path) else { return false };
    if std::fs::remove_file(&path).is_err() {
        return false;
    }
    serde_json::from_str::<LoginCode>(&text)
        .is_ok_and(|c| c.instance == instance && c.created_at <= now + CLOCK_SKEW_MS && now - c.created_at <= LOGIN_CODE_TTL_MS)
}

#[cfg(test)]
#[path = "server_file_tests.rs"]
mod tests;

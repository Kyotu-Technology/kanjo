//! Request guards and response headers: Host and Origin checks against DNS
//! rebinding and cross-site requests, the CSRF header, token authentication
//! with a failure rate limit, security headers, the handler timeout and an
//! access log that never contains query strings or credentials.

use crate::error::ApiErr;
use crate::state::{lock, AppState, Inner};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) const COOKIE: &str = "kanjo_session";
pub(crate) const CSRF_HEADER: &str = "x-kanjo-csrf";
const HEALTH: &str = "/api/v1/health";
pub(crate) const EVENTS: &str = "/api/v1/events";

const FAILURE_WINDOW: Duration = Duration::from_secs(300);
const MAX_FAILURES: usize = 10;
const LOCKOUT: Duration = Duration::from_secs(300);
const MAX_TRACKED_PEERS: usize = 4096;
const LOGIN_CODE_TTL: Duration = Duration::from_secs(120);
const MAX_LOGIN_CODES: usize = 8;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAINTENANCE_TIMEOUT: Duration = Duration::from_secs(300);

/// The local address a connection arrived on, set by the accept loop.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalAddr(pub SocketAddr);

/// Set by the accept loop; becomes true once a request on the connection
/// presented a valid credential, which protects it from eviction.
#[derive(Debug, Clone)]
pub(crate) struct ConnState(pub Arc<AtomicBool>);

fn mark_proven(req: &Request) {
    if let Some(c) = req.extensions().get::<ConnState>() {
        c.0.store(true, Ordering::Relaxed);
    }
}

/// The cookie value for a token. The cookie is a derivative, so the token
/// itself never sits in the browser's cookie store.
pub(crate) fn session_value(token: &str) -> String {
    hex(&Sha256::digest(format!("kanjo-session-v1:{token}").as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn is_api(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

fn changes_state(m: &Method) -> bool {
    !matches!(*m, Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE)
}

/// Failed credential attempts per peer. Only distinct wrong values count, so a
/// browser repeating one stale cookie in parallel requests does not lock its
/// user out, while guessing does. A lockout only refuses wrong credentials:
/// the right token, cookie or login code is always accepted. All of loopback
/// counts as one peer and IPv6 peers count per /64, so rotating source
/// addresses buys nothing; once the table is full, untracked peers share one
/// overflow entry, which is enforced like any other.
#[derive(Default)]
pub(crate) struct Limiter {
    peers: HashMap<Option<IpAddr>, Peer>,
}

struct Peer {
    since: Instant,
    seen: Vec<[u8; 8]>,
    locked_until: Option<Instant>,
}

impl Peer {
    fn expired(&self, now: Instant) -> bool {
        self.locked_until.is_none_or(|u| u <= now) && now.duration_since(self.since) > FAILURE_WINDOW
    }
}

fn peer_key(ip: Option<IpAddr>) -> Option<IpAddr> {
    let ip = match ip? {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        v4 => v4,
    };
    Some(match ip {
        ip if ip.is_loopback() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(v6) => {
            let s = v6.segments();
            IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
        }
        v4 => v4,
    })
}

impl Limiter {
    /// Seconds left in a lockout, if the peer is locked out.
    pub fn locked(&mut self, key: Option<IpAddr>, now: Instant) -> Option<u64> {
        let key = peer_key(key);
        let key = if self.peers.contains_key(&key) { key } else { None };
        let until = self.peers.get(&key)?.locked_until?;
        if until > now {
            return Some(until.duration_since(now).as_secs().max(1));
        }
        self.peers.remove(&key);
        None
    }

    /// Records a wrong credential; true when this failure started a lockout.
    pub fn fail(&mut self, key: Option<IpAddr>, credential: &str, now: Instant) -> bool {
        let mut key = peer_key(key);
        if !self.peers.contains_key(&key) && self.peers.len() >= MAX_TRACKED_PEERS {
            self.peers.retain(|_, p| !p.expired(now));
            if self.peers.len() >= MAX_TRACKED_PEERS {
                key = None;
            }
        }
        let peer = self.peers.entry(key).or_insert(Peer { since: now, seen: Vec::new(), locked_until: None });
        if now.duration_since(peer.since) > FAILURE_WINDOW {
            peer.since = now;
            peer.seen.clear();
        }
        let mut fp = [0u8; 8];
        fp.copy_from_slice(&Sha256::digest(credential.as_bytes())[..8]);
        if !peer.seen.contains(&fp) {
            peer.seen.push(fp);
        }
        if peer.seen.len() >= MAX_FAILURES {
            peer.locked_until = Some(now + LOCKOUT);
            peer.seen.clear();
            peer.since = now;
            return true;
        }
        false
    }
}

fn ip_name(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
    }
}

/// The Host name the configured bind address answers to; `None` for a wildcard bind.
fn bind_name(bind: &str) -> Option<String> {
    let b = bind.trim().to_ascii_lowercase();
    match b.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>() {
        Ok(ip) if ip.is_unspecified() => None,
        Ok(ip) => Some(ip_name(ip)),
        Err(_) => Some(b),
    }
}

fn split_authority(a: &str) -> Option<(&str, Option<u16>)> {
    if a.starts_with('[') {
        let end = a.find(']')?;
        let (name, rest) = a.split_at(end + 1);
        return match rest {
            "" => Some((name, None)),
            r => Some((name, Some(r.strip_prefix(':')?.parse().ok()?))),
        };
    }
    match a.rsplit_once(':') {
        Some((name, port)) => Some((name, Some(port.parse().ok()?))),
        None => Some((a, None)),
    }
}

impl Inner {
    /// True when `authority` (a Host header or the host part of an Origin)
    /// names this server: a loopback name or the configured bind host, or the
    /// address the connection arrived on, always with the listening port.
    pub fn host_allowed(&self, authority: &str, local: Option<SocketAddr>) -> bool {
        let a = authority.trim().to_ascii_lowercase();
        let Some((name, port)) = split_authority(&a) else { return false };
        let default_port = if self.tls { 443 } else { 80 };
        if port.unwrap_or(default_port) != self.port.load(std::sync::atomic::Ordering::Relaxed) {
            return false;
        }
        if matches!(name, "127.0.0.1" | "localhost" | "[::1]") {
            return true;
        }
        let bind = self.config.read().unwrap_or_else(|e| e.into_inner()).server.bind.clone();
        if bind_name(&bind).is_some_and(|b| b == name) {
            return true;
        }
        local.is_some_and(|l| !l.ip().is_unspecified() && ip_name(l.ip()) == name)
    }

    fn origin_allowed(&self, origin: &str, local: Option<SocketAddr>) -> bool {
        let scheme = if self.tls { "https://" } else { "http://" };
        origin.strip_prefix(scheme).is_some_and(|rest| !rest.is_empty() && !rest.contains('/') && self.host_allowed(rest, local))
    }

    /// Checks the bearer token or the session cookie.
    fn authenticate(&self, headers: &HeaderMap, peer: Option<IpAddr>) -> Result<(), ApiErr> {
        let (Some(token), Some(session)) = (&self.token, &self.session) else { return Ok(()) };
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
            .map(str::trim);
        let cookies = session_cookies(headers);
        if bearer.is_none() && cookies.is_empty() {
            let open = match kanjo_setup::claude::home_arg(&self.paths) {
                Some(home) => format!("kanjo --home {} open", home.display()),
                None => "kanjo open".to_string(),
            };
            return Err(ApiErr::unauthorized(format!("an access token is required; run `{open}` to sign in")));
        }
        let ok =
            bearer.is_some_and(|b| kanjo_ops::token::matches(token, b)) || cookies.iter().any(|c| kanjo_ops::token::matches(session, c));
        if ok {
            return Ok(());
        }
        let now = Instant::now();
        let mut limiter = lock(&self.limiter);
        if let Some(wait) = limiter.locked(peer, now) {
            return Err(too_many(wait));
        }
        let wrong = bearer.map(str::to_string).unwrap_or_else(|| cookies.join(";"));
        if limiter.fail(peer, &wrong, now) {
            tracing::warn!(peer = ?peer, "too many wrong access tokens; refusing this peer for {} s", LOCKOUT.as_secs());
        } else {
            tracing::warn!(peer = ?peer, "request with a wrong access token");
        }
        Err(ApiErr::unauthorized("wrong access token"))
    }

    /// A random single-use code that `/?token=` accepts for two minutes, so the
    /// URL handed to a browser opener on the command line is not the token.
    pub fn issue_login_code(&self) -> Option<String> {
        let mut buf = [0u8; 32];
        if let Err(e) = getrandom::fill(&mut buf) {
            tracing::warn!(error = %e, "no OS randomness for a login code");
            return None;
        }
        let code = hex(&buf);
        let mut codes = lock(&self.login_codes);
        let now = Instant::now();
        codes.retain(|(_, until)| *until > now);
        if codes.len() >= MAX_LOGIN_CODES {
            codes.remove(0);
        }
        codes.push((code.clone(), now + LOGIN_CODE_TTL));
        Some(code)
    }

    fn take_login_code(&self, given: &str) -> bool {
        let mut codes = lock(&self.login_codes);
        let now = Instant::now();
        codes.retain(|(_, until)| *until > now);
        match codes.iter().position(|(c, _)| kanjo_ops::token::matches(c, given)) {
            Some(i) => {
                codes.remove(i);
                true
            }
            None => false,
        }
    }

    fn session_cookie(&self) -> Option<String> {
        let session = self.session.as_ref()?;
        let secure = if self.tls { "; Secure" } else { "" };
        Some(format!("{COOKIE}={session}; Path=/; HttpOnly; SameSite=Strict{secure}"))
    }
}

fn too_many(wait: u64) -> ApiErr {
    let mut e = ApiErr::new(StatusCode::TOO_MANY_REQUESTS, "unauthorized", format!("too many wrong access tokens; try again in {wait} s"));
    e.retry_after = Some(wait);
    e
}

fn session_cookies(headers: &HeaderMap) -> Vec<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .filter(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value.trim())
        .collect()
}

fn peer_ip(req: &Request) -> Option<IpAddr> {
    req.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip())
}

fn local_addr(req: &Request) -> Option<SocketAddr> {
    req.extensions().get::<LocalAddr>().map(|l| l.0)
}

/// Host, Origin, Sec-Fetch-Site, CSRF header and authentication, in that order.
pub(crate) async fn guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let inner = &state.inner;
    let path = req.uri().path();
    let api = is_api(path);
    let local = local_addr(&req);
    let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok());
    if !host.is_some_and(|h| inner.host_allowed(h, local)) {
        tracing::warn!(host = ?host, "refused a request for a host this server does not listen on");
        return ApiErr::forbidden("the Host header does not name this server").into_response();
    }
    if let Some(origin) = req.headers().get(header::ORIGIN) {
        if !origin.to_str().is_ok_and(|o| inner.origin_allowed(o, local)) {
            tracing::warn!("refused a cross-origin request");
            return ApiErr::forbidden("cross-origin requests are not allowed").into_response();
        }
    }
    if api {
        let site = req.headers().get("sec-fetch-site").and_then(|v| v.to_str().ok());
        if matches!(site, Some("cross-site") | Some("same-site")) {
            return ApiErr::forbidden("cross-site requests are not allowed").into_response();
        }
    }
    if changes_state(req.method()) && req.headers().get(CSRF_HEADER).is_none_or(|v| v.as_bytes() != b"1") {
        return ApiErr::forbidden("state-changing requests need the header X-Kanjo-CSRF: 1").into_response();
    }
    if api && path != HEALTH {
        if let Err(e) = inner.authenticate(req.headers(), peer_ip(&req)) {
            return e.into_response();
        }
        mark_proven(&req);
    }
    next.run(req).await
}

/// Handles `?token=` on a UI path: sets the session cookie when the token (or a
/// login code) is right and redirects to the same path without the token.
pub(crate) fn token_login(inner: &Inner, req: &Request, query: &str) -> Response {
    let mut given = None;
    let rest: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair.split_once('=').map_or(*pair, |(k, _)| k);
            if key == "token" {
                given = Some(pair.split_once('=').map_or("", |(_, v)| v));
                return false;
            }
            !pair.is_empty()
        })
        .collect();
    let mut target = safe_path(req.uri().path());
    if !rest.is_empty() {
        target.push('?');
        target.push_str(&rest.join("&"));
    }
    let Some(token) = &inner.token else { return see_other(&target, None) };
    let given = given.unwrap_or("");
    if kanjo_ops::token::matches(token, given)
        || inner.take_login_code(given)
        || kanjo_setup::server_file::claim_login_code(&inner.paths, &inner.instance, given, kanjo_core::time::now_ms())
    {
        mark_proven(req);
        return see_other(&target, inner.session_cookie());
    }
    let peer = peer_ip(req);
    let now = Instant::now();
    let mut limiter = lock(&inner.limiter);
    if let Some(wait) = limiter.locked(peer, now) {
        return too_many(wait).into_response();
    }
    if counts_against_peer(req.headers()) {
        limiter.fail(peer, given, now);
        tracing::warn!(peer = ?peer, "rejected the access token of a login link");
    } else {
        tracing::warn!("ignored a wrong login link that came from another site");
    }
    see_other(&target, None)
}

/// A wrong login link counts towards a lockout only when the browser says it
/// is a navigation that did not come from another site. Any page can make a
/// browser fetch `/?token=...` as an image or a cross-site link; counting those
/// would let it lock the owner out.
fn counts_against_peer(headers: &HeaderMap) -> bool {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let cross = matches!(get("sec-fetch-site"), Some("cross-site" | "same-site"));
    let navigation = get("sec-fetch-mode").is_none_or(|m| m == "navigate");
    !cross && navigation
}

fn safe_path(path: &str) -> String {
    let ok = path.starts_with('/') && !path.starts_with("//") && !path.contains('\\') && !path.split('/').any(|s| s == "..");
    if ok {
        path.to_string()
    } else {
        "/".to_string()
    }
}

fn see_other(target: &str, cookie: Option<String>) -> Response {
    let mut res = StatusCode::SEE_OTHER.into_response();
    let location = HeaderValue::from_str(target).unwrap_or(HeaderValue::from_static("/"));
    res.headers_mut().insert(header::LOCATION, location);
    if let Some(c) = cookie.and_then(|c| HeaderValue::from_str(&c).ok()) {
        res.headers_mut().insert(header::SET_COOKIE, c);
    }
    res
}

/// Security headers on every response, `Cache-Control: no-store` on the API.
pub(crate) async fn headers(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let api = is_api(req.uri().path());
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    let csp = if api { HeaderValue::from_static(API_CSP) } else { crate::assets::csp() };
    h.insert(header::CONTENT_SECURITY_POLICY, csp);
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
    h.insert("cross-origin-resource-policy", HeaderValue::from_static("same-origin"));
    h.insert("permissions-policy", HeaderValue::from_static("camera=(), microphone=(), geolocation=(), usb=(), payment=()"));
    if api || !h.contains_key(header::CACHE_CONTROL) {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    if state.inner.tls && !crate::net::is_loopback_bind(&state.inner.config().server.bind) {
        h.insert(header::STRICT_TRANSPORT_SECURITY, HeaderValue::from_static("max-age=31536000"));
    }
    res
}

const API_CSP: &str = "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

/// Gives every API error that left the router without a JSON body (axum's
/// own 404, 405, 413 and the like) an `ApiError` body.
pub(crate) async fn api_errors(req: Request, next: Next) -> Response {
    let api = is_api(req.uri().path());
    let res = next.run(req).await;
    let status = res.status();
    if !api || !(status.is_client_error() || status.is_server_error()) {
        return res;
    }
    let json = res.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("application/json"));
    if json {
        return res;
    }
    let retry = res.headers().get(header::RETRY_AFTER).cloned();
    let mut out = ApiErr::from_status(status).into_response();
    if let Some(r) = retry {
        out.headers_mut().insert(header::RETRY_AFTER, r);
    }
    out
}

/// Bounds handler time. The event stream is exempt; maintenance actions
/// (backup, purge, hooks) get a longer limit.
pub(crate) async fn timeout(req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if path == EVENTS {
        return next.run(req).await;
    }
    let limit = if path.starts_with("/api/v1/system/") && changes_state(req.method()) { MAINTENANCE_TIMEOUT } else { REQUEST_TIMEOUT };
    match tokio::time::timeout(limit, next.run(req)).await {
        Ok(res) => res,
        Err(_) => {
            tracing::warn!(timeout_s = limit.as_secs(), "request timed out");
            ApiErr::new(StatusCode::SERVICE_UNAVAILABLE, "internal", "the request took too long").into_response()
        }
    }
}

/// Access log: method, path (never the query string, which may carry a
/// token), status and duration. Headers are never logged.
pub(crate) async fn access_log(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let started = Instant::now();
    let res = next.run(req).await;
    let status = res.status().as_u16();
    let ms = started.elapsed().as_millis() as u64;
    if res.status().is_server_error() {
        tracing::warn!(%method, %path, status, ms, "request failed");
    } else {
        tracing::debug!(%method, %path, status, ms, "request");
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_parsing() {
        assert_eq!(split_authority("127.0.0.1:4747"), Some(("127.0.0.1", Some(4747))));
        assert_eq!(split_authority("[::1]:80"), Some(("[::1]", Some(80))));
        assert_eq!(split_authority("[::1]"), Some(("[::1]", None)));
        assert_eq!(split_authority("localhost"), Some(("localhost", None)));
        assert_eq!(split_authority("host:x"), None);
        assert_eq!(bind_name("0.0.0.0"), None);
        assert_eq!(bind_name("::1").as_deref(), Some("[::1]"));
        assert_eq!(bind_name("192.168.1.5").as_deref(), Some("192.168.1.5"));
    }

    #[test]
    fn limiter_counts_distinct_failures() {
        let mut l = Limiter::default();
        let now = Instant::now();
        let ip = Some(IpAddr::from([127, 0, 0, 1]));
        for _ in 0..50 {
            assert!(!l.fail(ip, "same stale cookie", now));
        }
        assert_eq!(l.locked(ip, now), None);
        for i in 0..MAX_FAILURES - 2 {
            assert!(!l.fail(ip, &format!("guess {i}"), now));
        }
        assert!(l.fail(ip, "last guess", now));
        assert!(l.locked(ip, now).is_some());
        assert!(l.locked(Some(IpAddr::from([10, 0, 0, 1])), now).is_none());
        assert_eq!(l.locked(ip, now + LOCKOUT + Duration::from_secs(1)), None);
    }

    #[test]
    fn loopback_and_ipv6_prefixes_share_one_entry() {
        let mut l = Limiter::default();
        let now = Instant::now();
        for i in 0..MAX_FAILURES {
            let ip = IpAddr::from([127, 0, (i / 256) as u8, (i % 256) as u8 + 1]);
            l.fail(Some(ip), &format!("guess {i}"), now);
        }
        assert!(l.locked(Some(IpAddr::from([127, 9, 9, 9])), now).is_some());
        assert!(l.locked(Some("::1".parse().unwrap()), now).is_some());
        for i in 0..MAX_FAILURES {
            l.fail(Some(format!("2001:db8:1:2::{i:x}").parse().unwrap()), &format!("g{i}"), now);
        }
        assert!(l.locked(Some("2001:db8:1:2:ffff::1".parse().unwrap()), now).is_some());
        assert!(l.locked(Some("2001:db8:1:3::1".parse().unwrap()), now).is_none());
    }

    #[test]
    fn the_overflow_bucket_is_enforced() {
        let mut l = Limiter::default();
        let now = Instant::now();
        for i in 0..MAX_TRACKED_PEERS as u32 {
            l.fail(Some(IpAddr::from((0x0a00_0000 + i).to_be_bytes())), "one guess", now);
        }
        for i in 0..MAX_FAILURES as u32 {
            let ip = IpAddr::from((0x0b00_0000 + i).to_be_bytes());
            assert!(l.locked(Some(ip), now).is_none());
            l.fail(Some(ip), &format!("guess {i}"), now);
        }
        assert!(l.locked(Some(IpAddr::from([11, 1, 2, 3])), now).is_some(), "an untracked peer falls under the overflow lockout");
        assert!(l.locked(Some(IpAddr::from([10, 0, 0, 1])), now).is_none(), "a tracked peer keeps its own entry");
    }

    #[test]
    fn only_same_site_navigations_count() {
        let h = |pairs: &[(&'static str, &'static str)]| {
            let mut m = HeaderMap::new();
            for (k, v) in pairs {
                m.insert(*k, HeaderValue::from_static(v));
            }
            m
        };
        assert!(counts_against_peer(&h(&[])));
        assert!(counts_against_peer(&h(&[("sec-fetch-site", "none"), ("sec-fetch-mode", "navigate")])));
        assert!(counts_against_peer(&h(&[("sec-fetch-site", "same-origin"), ("sec-fetch-mode", "navigate")])));
        assert!(!counts_against_peer(&h(&[("sec-fetch-site", "cross-site"), ("sec-fetch-mode", "navigate")])));
        assert!(!counts_against_peer(&h(&[("sec-fetch-site", "cross-site"), ("sec-fetch-mode", "no-cors")])));
        assert!(!counts_against_peer(&h(&[("sec-fetch-site", "same-origin"), ("sec-fetch-mode", "no-cors")])));
        assert!(!counts_against_peer(&h(&[("sec-fetch-site", "same-site")])));
    }

    #[test]
    fn redirect_targets_stay_local() {
        assert_eq!(safe_path("/"), "/");
        assert_eq!(safe_path("//evil.example/"), "/");
        assert_eq!(safe_path("/a/../b"), "/");
        assert_eq!(safe_path("/kanjo/"), "/kanjo/");
    }
}

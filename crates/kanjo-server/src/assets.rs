//! The embedded web UI. `build.rs` copies web/dist (or a placeholder page)
//! into OUT_DIR; it is compiled into the binary in every profile.

use crate::error::ApiErr;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

#[derive(RustEmbed)]
#[folder = "$KANJO_WEB_DIR"]
struct Web;

const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// The UI's Content-Security-Policy. Inline scripts are allowed only by the
/// hashes of the inline scripts in the embedded index.html. The built UI needs
/// no inline styles (Svelte writes dynamic styles through the CSSOM, which CSP
/// does not restrict), so style-src is 'self' only.
pub(crate) fn csp() -> HeaderValue {
    static CSP: LazyLock<HeaderValue> = LazyLock::new(|| {
        let index = Web::get("index.html").map(|f| f.data.into_owned()).unwrap_or_default();
        let policy = policy_for(&String::from_utf8_lossy(&index));
        HeaderValue::from_str(&policy).unwrap_or(HeaderValue::from_static("default-src 'self'; frame-ancestors 'none'"))
    });
    CSP.clone()
}

pub(crate) fn policy_for(index_html: &str) -> String {
    let mut script_src = String::from("'self'");
    for body in inline_scripts(index_html) {
        let digest = Sha256::digest(body.as_bytes());
        script_src.push_str(&format!(" 'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(digest)));
    }
    format!(
        "default-src 'self'; script-src {script_src}; style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; \
         object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'"
    )
}

/// The text of every `<script>` element without a `src` attribute.
fn inline_scripts(html: &str) -> Vec<&str> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = lower[at..].find("<script").map(|i| i + at) {
        let Some(open_end) = lower[start..].find('>').map(|i| i + start) else { break };
        let Some(close) = lower[open_end..].find("</script").map(|i| i + open_end) else { break };
        let attrs = &lower[start + "<script".len()..open_end];
        let has_src = attrs.split(|c: char| c.is_whitespace()).any(|a| a == "src" || a.starts_with("src="));
        if !has_src {
            out.push(&html[open_end + 1..close]);
        }
        at = close;
    }
    out
}

fn mime(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        Some("json" | "map") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn file(path: &str, cache: &'static str) -> Option<Response> {
    let f = Web::get(path)?;
    let mut res = Response::new(Body::from(f.data.into_owned()));
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime(path)));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    Some(res)
}

fn index() -> Response {
    file("index.html", "no-store").unwrap_or_else(|| (StatusCode::NOT_FOUND, "the web UI is not embedded").into_response())
}

/// Everything outside the API: embedded files, `index.html` for unknown paths,
/// and the `?token=` login.
pub(crate) async fn ui(State(state): State<AppState>, req: Request) -> Response {
    let path = req.uri().path();
    if crate::security::is_api(path) {
        return ApiErr::not_found("no such API endpoint").into_response();
    }
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if let Some(query) = req.uri().query() {
        if query.split('&').any(|p| p == "token" || p.starts_with("token=")) {
            return crate::security::token_login(&state.inner, &req, query);
        }
    }
    let rel = path.trim_start_matches('/');
    if rel.is_empty() || rel.ends_with('/') || rel == "index.html" {
        return index();
    }
    if rel.starts_with("assets/") {
        return file(rel, IMMUTABLE).unwrap_or_else(|| (StatusCode::NOT_FOUND, "not found").into_response());
    }
    file(rel, "no-cache").unwrap_or_else(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_only_inline_scripts() {
        let html = "<html><head><script>a()</script><script type=\"module\" src=\"./x.js\"></script>\
                    <SCRIPT nonce=x>b()</SCRIPT></head></html>";
        assert_eq!(inline_scripts(html), vec!["a()", "b()"]);
        let p = policy_for(html);
        let a = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(b"a()"));
        assert!(p.contains(&format!("'sha256-{a}'")), "{p}");
        assert!(p.contains("frame-ancestors 'none'"));
        assert!(!p.contains("unsafe"));
    }

    #[test]
    fn embedded_index_exists() {
        assert!(Web::get("index.html").is_some());
    }
}

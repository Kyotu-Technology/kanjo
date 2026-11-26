//! The router driven in-process over a demo store, plus a few runs of the real
//! server on 127.0.0.1 port 0. Everything lives in temporary directories.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use kanjo_core::api::*;
use kanjo_core::config::{AuthMode, Config, EncryptionMode};
use kanjo_core::paths::Paths;
use kanjo_core::settings::{PlanKind, Settings};
use kanjo_server::{check_bind, router, start, AppState, ServeOptions};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tower::ServiceExt;

const PORT: u16 = 4747;
const HOST: &str = "127.0.0.1:4747";

fn options(dir: &Path, demo: bool) -> ServeOptions {
    static NO_KEYRING: std::sync::Once = std::sync::Once::new();
    NO_KEYRING.call_once(|| std::env::set_var("KANJO_NO_KEYRING", "1"));
    ServeOptions {
        paths: Paths::at(dir.join("home"), dir.join("claude")),
        config: Config::default(),
        open_browser: false,
        demo,
        demo_days: None,
    }
}

struct Fx {
    _dir: tempfile::TempDir,
    state: AppState,
    app: Router,
    token: String,
}

async fn demo() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let opts = options(dir.path(), true);
    let state = AppState::open(&opts).await.unwrap();
    state.set_port(PORT);
    let token = kanjo_ops::token::load_or_create(&opts.paths).unwrap();
    Fx { app: router(state.clone()), state, token, _dir: dir }
}

struct Res {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Res {
    fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&self.body)))
    }
    fn error_code(&self) -> String {
        self.json::<ApiError>().code
    }
    fn header(&self, name: &str) -> String {
        self.headers.get(name).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default()
    }
}

impl Fx {
    fn req(&self, method: &str, uri: &str) -> axum::http::request::Builder {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, HOST)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token))
    }

    async fn send(&self, req: Request<Body>) -> Res {
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        Res { status, headers, body }
    }

    async fn get<T: DeserializeOwned>(&self, uri: &str) -> T {
        let res = self.send(self.req("GET", uri).body(Body::empty()).unwrap()).await;
        assert_eq!(res.status, StatusCode::OK, "{uri}: {}", String::from_utf8_lossy(&res.body));
        assert_eq!(res.header("cache-control"), "no-store", "{uri}");
        res.json()
    }

    async fn call(&self, method: &str, uri: &str, body: Option<Value>) -> Res {
        let b = self.req(method, uri).header("x-kanjo-csrf", "1").header(header::CONTENT_TYPE, "application/json");
        let body = body.map(|v| Body::from(v.to_string())).unwrap_or_else(Body::empty);
        self.send(b.body(body).unwrap()).await
    }

    async fn audit_actions(&self) -> Vec<String> {
        let page: AuditPage = self.get("/api/v1/audit?limit=1000").await;
        assert!(page.chain_ok);
        page.items.into_iter().map(|e| e.action).collect()
    }
}

#[tokio::test]
async fn every_get_route_answers_with_its_documented_type() {
    let fx = demo().await;
    let health: Health = fx.get("/api/v1/health").await;
    assert_eq!((health.status.as_str(), health.demo), ("ok", true));
    let overview: Overview = fx.get("/api/v1/overview").await;
    assert!(overview.month.usage.requests > 0);
    let sys: SystemInfo = fx.get("/api/v1/system").await;
    assert!(sys.demo);
    assert!(sys.ingest.requests_stored >= overview.month.usage.requests, "ingest figures come from the database: {:?}", sys.ingest);
    assert_eq!(sys.ingest.last_event_at, overview.data.last_request_at);
    let _: UsageReport = fx.get("/api/v1/usage?group=week").await;
    let _: Breakdown = fx.get("/api/v1/breakdown?by=model").await;
    let list: SessionList = fx.get("/api/v1/sessions?sort=recent&limit=5").await;
    assert!(!list.items.is_empty() && list.items.len() <= 5);
    let detail: SessionDetail = fx.get(&format!("/api/v1/sessions/{}", list.items[0].id)).await;
    assert_eq!(detail.summary.id, list.items[0].id);
    let _: Vec<Turn> = fx.get("/api/v1/turns?limit=3").await;
    let projects: Vec<Project> = fx.get("/api/v1/projects").await;
    assert!(!projects.is_empty());
    let _: Vec<Block> = fx.get("/api/v1/blocks").await;
    let _: LimitsDetail = fx.get("/api/v1/limits").await;
    let _: Heatmap = fx.get("/api/v1/heatmap").await;
    let _: Comparison = fx.get("/api/v1/compare?period=month&offset=1").await;
    let _: PlanFit = fx.get("/api/v1/plan").await;
    let _: Insights = fx.get("/api/v1/insights").await;
    let alerts: AlertList = fx.get("/api/v1/alerts?lang=pl").await;
    assert!(alerts.total > 0, "demo mode runs the alert rules once");
    let budgets: Vec<Budget> = fx.get("/api/v1/budgets").await;
    assert!(!budgets.is_empty());
    let _: Settings = fx.get("/api/v1/settings").await;
    let system: SystemInfo = fx.get("/api/v1/system").await;
    assert!(!system.checks.is_empty());
    for c in &system.checks {
        if ["claude_dir", "statusline", "session_hooks", "encryption", "service", "quarantine"].contains(&c.id.as_str()) {
            assert_eq!(c.status, CheckStatus::Ok, "a demo is healthy: {} {}", c.id, c.detail);
        }
        assert_ne!(c.status, CheckStatus::Fail, "{} {}", c.id, c.detail);
    }
    let pl: SystemInfo = fx.get("/api/v1/system?lang=pl").await;
    assert_eq!(pl.checks.iter().find(|c| c.id == "service").unwrap().title, "Usługa w tle");
    let en: SystemInfo = fx.get("/api/v1/system?lang=en").await;
    assert_eq!(en.checks.iter().find(|c| c.id == "service").unwrap().title, "Background service");
    let pl_pl: SystemInfo = fx.get("/api/v1/system?lang=pl-PL").await;
    assert_eq!(pl_pl.checks.iter().find(|c| c.id == "service").unwrap().title, "Usługa w tle");
    let other: SystemInfo = fx.get("/api/v1/system?lang=de").await;
    let titles = |s: &SystemInfo| s.checks.iter().map(|c| c.title.clone()).collect::<Vec<_>>();
    assert_eq!(titles(&other), titles(&system), "another language falls back to the language setting");
    assert!(!system.database.encrypted, "demo data is not encrypted");
    assert_eq!(system.server.port, PORT);
    assert!(system.server.auth);
    assert!(system.claude_dir.starts_with(&system.data_dir), "demo mode never points at a real Claude directory");
    assert!(!system.hooks.statusline_installed);
    let _: Vec<BackupInfo> = fx.get("/api/v1/system/backups").await;
    let _: AuditPage = fx.get("/api/v1/audit?limit=5").await;
    let _: Vec<SearchResult> = fx.get("/api/v1/search?q=a").await;
}

#[tokio::test]
async fn after_a_restart_the_import_figures_come_from_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let opts = options(dir.path(), false);
    std::fs::create_dir_all(&opts.paths.claude_dir).unwrap();
    let ts = kanjo_core::time::now_ms() - 3_600_000;
    {
        let (store, _) = kanjo_ops::open_store(&opts.paths, &opts.config).unwrap();
        store
            .write(|tx| {
                let project = kanjo_core::store::write::upsert_project(tx, "-work-restart", Some("/work/restart"), ts)?;
                let row = kanjo_core::model::RequestRow {
                    message_id: "msg_restart".into(),
                    session_id: "s-restart".into(),
                    project_id: project,
                    ts,
                    model: "claude-opus-5-5".into(),
                    ..Default::default()
                };
                kanjo_core::store::write::insert_request(tx, &row, kanjo_core::pricing::PriceTable::embedded())
            })
            .unwrap();
    }
    let state = AppState::open(&opts).await.unwrap();
    state.set_port(PORT);
    let token = kanjo_ops::token::load_or_create(&opts.paths).unwrap();
    let fx = Fx { app: router(state.clone()), state, token, _dir: dir };
    let health: Health = fx.get("/api/v1/health").await;
    assert!(!health.demo);
    let sys: SystemInfo = fx.get("/api/v1/system").await;
    assert!(!sys.demo);
    assert_eq!((sys.ingest.requests_stored, sys.ingest.last_event_at), (1, Some(ts)));
}

#[tokio::test]
async fn errors_carry_an_api_error_body() {
    let fx = demo().await;
    let r = fx.send(fx.req("GET", "/api/v1/nope").body(Body::empty()).unwrap()).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::NOT_FOUND, "not_found"));
    let r = fx.call("POST", "/api/v1/overview", None).await;
    assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(r.error_code(), "invalid");
    let r = fx.send(fx.req("GET", "/api/v1/sessions/does-not-exist").body(Body::empty()).unwrap()).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::NOT_FOUND, "not_found"));
    let r = fx.send(fx.req("GET", "/api/v1/sessions?sort=sideways").body(Body::empty()).unwrap()).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::BAD_REQUEST, "invalid"));
    let r = fx.send(fx.req("GET", "/api/v1/usage?project=abc").body(Body::empty()).unwrap()).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::BAD_REQUEST, "invalid"));
    let r = fx.send(fx.req("GET", "/api/v1/usage?from=10&to=5").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = fx.call("POST", "/api/v1/alerts/ack", Some(json!({ "id": "x" }))).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::UNPROCESSABLE_ENTITY, "invalid"));
    let big = json!({ "name": "x".repeat(400 * 1024) });
    let r = fx.call("PATCH", "/api/v1/projects/1", Some(big)).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::PAYLOAD_TOO_LARGE, "invalid"));
    let list: SessionList = fx.get("/api/v1/sessions?limit=100000").await;
    assert!(list.items.len() <= 1000);
}

#[tokio::test]
async fn requests_without_valid_credentials_or_from_elsewhere_are_refused() {
    let fx = demo().await;
    let plain = |method: &str, uri: &str| Request::builder().method(method).uri(uri).header(header::HOST, HOST);

    let r = fx.send(plain("GET", "/api/v1/overview").body(Body::empty()).unwrap()).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::UNAUTHORIZED, "unauthorized"));
    let r = fx.send(plain("GET", "/api/v1/overview").header(header::AUTHORIZATION, "Bearer 0123").body(Body::empty()).unwrap()).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::UNAUTHORIZED, "unauthorized"));
    let r = fx.send(plain("GET", "/api/v1/overview").header(header::COOKIE, "kanjo_session=forged").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = fx.send(plain("GET", "/api/v1/health").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::OK, "health needs no token");
    let r = fx.send(plain("GET", "/").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::OK, "the UI shell needs no token");

    let r = fx
        .send(fx.req("POST", "/api/v1/alerts/ack").header(header::CONTENT_TYPE, "application/json").body(Body::from("{}")).unwrap())
        .await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::FORBIDDEN, "forbidden"), "missing CSRF header");
    let r = fx
        .send(
            fx.req("POST", "/api/v1/alerts/ack")
                .header("x-kanjo-csrf", "yes")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "the CSRF header must be 1");

    for host in ["evil.example:4747", "evil.example", "127.0.0.1:4748", "127.0.0.1", "localhost.evil.example:4747"] {
        let req = Request::builder().uri("/api/v1/health").header(header::HOST, host).body(Body::empty()).unwrap();
        let r = fx.send(req).await;
        assert_eq!((r.status, r.error_code().as_str()), (StatusCode::FORBIDDEN, "forbidden"), "{host}");
    }
    let r = fx.send(Request::builder().uri("/").header(header::HOST, "rebound.example:4747").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "the UI is behind the Host check too");
    let r = fx.send(Request::builder().uri("/api/v1/health").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "no Host header");
    for host in ["localhost:4747", "[::1]:4747", "LOCALHOST:4747"] {
        let r = fx.send(Request::builder().uri("/api/v1/health").header(header::HOST, host).body(Body::empty()).unwrap()).await;
        assert_eq!(r.status, StatusCode::OK, "{host}");
    }

    for origin in ["http://evil.example", "null", "https://127.0.0.1:4747", "http://127.0.0.1:4748", "http://localhost:3000"] {
        let r = fx.send(fx.req("GET", "/api/v1/overview").header(header::ORIGIN, origin).body(Body::empty()).unwrap()).await;
        assert_eq!((r.status, r.error_code().as_str()), (StatusCode::FORBIDDEN, "forbidden"), "{origin}");
    }
    let r = fx.send(fx.req("GET", "/api/v1/overview").header(header::ORIGIN, "http://127.0.0.1:4747").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::OK);
    let r = fx.send(fx.req("GET", "/api/v1/overview").header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = fx.send(fx.req("GET", "/api/v1/overview").header("sec-fetch-site", "same-origin").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::OK);
}

#[tokio::test]
async fn repeated_wrong_tokens_are_rate_limited() {
    let fx = demo().await;
    let with = |token: String| {
        Request::builder()
            .uri("/api/v1/overview")
            .header(header::HOST, HOST)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    for _ in 0..20 {
        assert_eq!(fx.send(with("stale".into())).await.status, StatusCode::UNAUTHORIZED, "one repeated value is not guessing");
    }
    for i in 0..8 {
        assert_eq!(fx.send(with(format!("guess-{i}"))).await.status, StatusCode::UNAUTHORIZED);
    }
    let r = fx.send(with("last".into())).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = fx.send(with("one more".into())).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "further guesses are refused");
    assert!(!r.header("retry-after").is_empty());
    assert_eq!(r.error_code(), "unauthorized");
    let r = fx.send(with(fx.token.clone())).await;
    assert_eq!(r.status, StatusCode::OK, "the right token is never locked out");
    let r = fx.send(Request::builder().uri(format!("/?token={}", fx.token)).header(header::HOST, HOST).body(Body::empty()).unwrap()).await;
    assert!(r.header("set-cookie").starts_with("kanjo_session="), "nor is the right login link");
}

#[tokio::test]
async fn wrong_login_links_from_other_sites_do_not_count() {
    let fx = demo().await;
    let link = |token: String, site: &'static str, mode: &'static str| {
        Request::builder()
            .uri(format!("/?token={token}"))
            .header(header::HOST, HOST)
            .header("sec-fetch-site", site)
            .header("sec-fetch-mode", mode)
            .body(Body::empty())
            .unwrap()
    };
    for i in 0..30 {
        assert_eq!(fx.send(link(format!("img-{i}"), "cross-site", "no-cors")).await.status, StatusCode::SEE_OTHER);
        assert_eq!(fx.send(link(format!("nav-{i}"), "cross-site", "navigate")).await.status, StatusCode::SEE_OTHER);
        assert_eq!(fx.send(link(format!("sub-{i}"), "same-origin", "no-cors")).await.status, StatusCode::SEE_OTHER);
    }
    let r = fx
        .send(
            Request::builder()
                .uri("/api/v1/overview")
                .header(header::HOST, HOST)
                .header(header::AUTHORIZATION, "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "the peer is not locked out");
    let r = fx.send(link(fx.token.clone(), "none", "navigate")).await;
    assert!(r.header("set-cookie").starts_with("kanjo_session="));
}

#[tokio::test]
async fn the_token_link_sets_a_session_cookie_and_drops_the_token() {
    let fx = demo().await;
    let open = |uri: String| Request::builder().uri(uri).header(header::HOST, HOST).body(Body::empty()).unwrap();
    let r = fx.send(open(format!("/?token={}&lang=pl", fx.token))).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.header("location"), "/?lang=pl");
    let cookie = r.header("set-cookie");
    assert!(cookie.starts_with("kanjo_session="), "{cookie}");
    for attr in ["HttpOnly", "SameSite=Strict", "Path=/"] {
        assert!(cookie.contains(attr), "{cookie}");
    }
    assert!(!cookie.contains("Secure"), "no Secure attribute without TLS");
    assert!(!cookie.contains(&fx.token), "the cookie is not the token itself");
    let pair = cookie.split(';').next().unwrap().to_string();

    let r = fx
        .send(
            Request::builder()
                .uri("/api/v1/overview")
                .header(header::HOST, HOST)
                .header(header::COOKIE, &pair)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let r = fx
        .send(
            Request::builder()
                .uri("/api/v1/overview")
                .header(header::HOST, HOST)
                .header(header::COOKIE, format!("theme=dark; {pair}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);

    let r = fx.send(open("/?token=wrong".into())).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.header("location"), "/");
    assert!(r.header("set-cookie").is_empty());
    let r = fx.send(open(format!("//evil.example/?token={}", fx.token))).await;
    assert_eq!(r.header("location"), "/", "no open redirect");
}

#[tokio::test]
async fn ui_shell_assets_and_headers() {
    let fx = demo().await;
    let get = |uri: &str| Request::builder().uri(uri).header(header::HOST, HOST).body(Body::empty()).unwrap();
    let r = fx.send(get("/")).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.header("content-type").starts_with("text/html"));
    let csp = r.header("content-security-policy");
    assert!(csp.contains("default-src 'self'") && csp.contains("frame-ancestors 'none'") && !csp.contains("unsafe-inline"), "{csp}");
    let html = String::from_utf8_lossy(&r.body).to_string();
    if html.contains("<script>") {
        assert!(csp.contains("'sha256-"), "inline scripts are allowed by hash: {csp}");
    }
    assert_eq!(r.header("x-content-type-options"), "nosniff");
    assert_eq!(r.header("referrer-policy"), "no-referrer");
    let r = fx.send(get("/sessions/whatever")).await;
    assert_eq!(r.status, StatusCode::OK, "unknown UI paths get index.html");
    assert_eq!(String::from_utf8_lossy(&r.body), html);
    let r = fx.send(get("/assets/missing-0000.js")).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    if let Some(asset) = html.split("./assets/").nth(1).and_then(|s| s.split('"').next()) {
        let r = fx.send(get(&format!("/assets/{asset}"))).await;
        assert_eq!(r.status, StatusCode::OK);
        assert!(r.header("cache-control").contains("immutable"));
    }
    let r = fx.send(fx.req("GET", "/api/v1/overview").body(Body::empty()).unwrap()).await;
    assert!(r.header("content-security-policy").starts_with("default-src 'none'"));
}

#[tokio::test]
async fn settings_round_trip_and_audit() {
    let fx = demo().await;
    let mut s: Settings = fx.get("/api/v1/settings").await;
    s.plan.kind = PlanKind::Max5x;
    s.week_start = 6;
    s.privacy.redact_paths = !s.privacy.redact_paths;
    let r = fx.call("PUT", "/api/v1/settings", Some(serde_json::to_value(&s).unwrap())).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json::<Settings>(), s);
    assert_eq!(fx.get::<Settings>("/api/v1/settings").await, s);
    let page: AuditPage = fx.get("/api/v1/audit?limit=1").await;
    assert_eq!(page.items[0].action, "settings.update");
    let changed = page.items[0].detail["changed"].as_array().unwrap().clone();
    assert!(changed.contains(&json!("plan")) && changed.contains(&json!("privacy")), "{changed:?}");
    assert_eq!(page.items[0].actor, "local");
    assert_eq!(page.items[0].detail["user"], "demo");

    type Change = Box<dyn Fn(&mut Settings)>;
    let cases: Vec<(&str, Change)> = vec![
        ("week_start 7", Box::new(|b| b.week_start = 7)),
        ("currency code of two letters", Box::new(|b| b.currency.code = "ZL".into())),
        ("currency code of four letters", Box::new(|b| b.currency.code = "PLNX".into())),
        ("currency code with a digit", Box::new(|b| b.currency.code = "PL1".into())),
        ("rate 0", Box::new(|b| b.currency.usd_rate = 0.0)),
        ("rate above 1e6", Box::new(|b| b.currency.usd_rate = 1_000_001.0)),
        ("negative price", Box::new(|b| b.plan.monthly_usd = Some(-1.0))),
        ("no thresholds", Box::new(|b| b.alerts.limit_thresholds.clear())),
        ("threshold 0.5", Box::new(|b| b.alerts.limit_thresholds = vec![0.5])),
        ("threshold 101", Box::new(|b| b.alerts.limit_thresholds = vec![80.0, 101.0])),
        ("context 9", Box::new(|b| b.alerts.context_threshold = 9.0)),
        ("context 100.5", Box::new(|b| b.alerts.context_threshold = 100.5)),
        ("retention 36501", Box::new(|b| b.retention.audit_days = 36_501)),
    ];
    for (name, change) in cases {
        let mut bad = s.clone();
        change(&mut bad);
        let r = fx.call("PUT", "/api/v1/settings", Some(serde_json::to_value(&bad).unwrap())).await;
        assert_eq!((r.status, r.error_code().as_str()), (StatusCode::BAD_REQUEST, "invalid"), "{name}");
    }
    let mut edge = s.clone();
    edge.week_start = 0;
    edge.currency.code = "PLN".into();
    edge.currency.usd_rate = 1e6;
    edge.plan.monthly_usd = Some(0.0);
    edge.alerts.limit_thresholds = vec![1.0, 100.0];
    edge.alerts.context_threshold = 10.0;
    edge.retention.requests_days = 0;
    edge.retention.audit_days = 36_500;
    let r = fx.call("PUT", "/api/v1/settings", Some(serde_json::to_value(&edge).unwrap())).await;
    assert_eq!(r.status, StatusCode::OK, "the edges of every range are allowed: {}", String::from_utf8_lossy(&r.body));
}

#[tokio::test]
async fn budgets_create_update_delete() {
    let fx = demo().await;
    let input = json!({ "name": "Team month", "scope": "global", "scope_value": null, "period": "month", "amount_usd": 500.0, "warn_pct": 80.0, "enabled": true });
    let r = fx.call("POST", "/api/v1/budgets", Some(input.clone())).await;
    assert_eq!(r.status, StatusCode::OK, "{}", String::from_utf8_lossy(&r.body));
    let created: Budget = r.json();
    assert_eq!(created.name, "Team month");
    let mut changed = input.clone();
    changed["amount_usd"] = json!(750.0);
    changed["name"] = json!("Team month (raised)");
    let r = fx.call("PUT", &format!("/api/v1/budgets/{}", created.id), Some(changed)).await;
    assert_eq!(r.status, StatusCode::OK);
    let updated: Budget = r.json();
    assert_eq!((updated.id, updated.amount_usd, updated.name.as_str()), (created.id, 750.0, "Team month (raised)"));
    let r = fx.call("DELETE", &format!("/api/v1/budgets/{}", created.id), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = fx.call("DELETE", &format!("/api/v1/budgets/{}", created.id), None).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::NOT_FOUND, "not_found"));
    let list: Vec<Budget> = fx.get("/api/v1/budgets").await;
    assert!(list.iter().all(|b| b.id != created.id));
    let mut bad = input;
    bad["amount_usd"] = json!(-1.0);
    let r = fx.call("POST", "/api/v1/budgets", Some(bad)).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn alerts_are_acknowledged() {
    let fx = demo().await;
    let list: AlertList = fx.get("/api/v1/alerts?q=open").await;
    assert!(list.unacknowledged > 0);
    let first = list.items[0].id;
    let r = fx.call("POST", "/api/v1/alerts/ack", Some(json!({ "id": first }))).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json::<Value>()["acknowledged"], 1);
    let r = fx.call("POST", "/api/v1/alerts/ack", Some(json!({ "id": null }))).await;
    assert_eq!(r.json::<Value>()["acknowledged"].as_i64().unwrap(), list.unacknowledged - 1);
    let open: AlertList = fx.get("/api/v1/alerts?q=open").await;
    assert_eq!(open.total, 0);
    let r = fx.call("POST", "/api/v1/alerts/ack", Some(json!({ "id": 99_999_999 }))).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn projects_are_patched() {
    let fx = demo().await;
    let projects: Vec<Project> = fx.get("/api/v1/projects").await;
    let id = projects[0].id;
    let r = fx.call("PATCH", &format!("/api/v1/projects/{id}"), Some(json!({ "name": "Renamed project", "client": "Client Z" }))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", String::from_utf8_lossy(&r.body));
    let p: Project = r.json();
    assert_eq!((p.id, p.name.as_str(), p.client.as_deref()), (id, "Renamed project", Some("Client Z")));
    let r = fx.call("PATCH", "/api/v1/projects/987654", Some(json!({ "name": "x" }))).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn backup_and_purge_are_audited() {
    let fx = demo().await;
    let r = fx.call("POST", "/api/v1/system/backup", None).await;
    assert_eq!(r.status, StatusCode::OK, "{}", String::from_utf8_lossy(&r.body));
    let info: BackupInfo = r.json();
    let list: Vec<BackupInfo> = fx.get("/api/v1/system/backups").await;
    assert!(list.iter().any(|b| b.file == info.file));

    let sessions: SessionList = fx.get("/api/v1/sessions").await;
    let id = sessions.items[0].id.clone();
    let r = fx.call("POST", "/api/v1/system/purge", Some(json!({ "session": id }))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", String::from_utf8_lossy(&r.body));
    let report: PurgeReport = r.json();
    assert!(report.sessions >= 1 && report.requests >= 1);
    let r = fx.send(fx.req("GET", &format!("/api/v1/sessions/{id}")).body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    let r = fx.call("POST", "/api/v1/system/purge", Some(json!({}))).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::BAD_REQUEST, "invalid"));

    let page: AuditPage = fx.get("/api/v1/audit?limit=1000").await;
    assert!(page.chain_ok);
    let purge = page.items.iter().find(|e| e.action == "data.purge").expect("purge audited");
    assert_eq!(purge.target, format!("session:{id}"));
    assert_eq!(purge.actor, "local");
    assert_eq!(purge.detail["deleted"], json!(id));
    assert!(purge.detail["report"]["sessions"].as_i64().unwrap() >= 1);

    let projects: Vec<Project> = fx.get("/api/v1/projects").await;
    let p = &projects[0];
    let r = fx.call("POST", "/api/v1/system/purge", Some(json!({ "project": p.id }))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", String::from_utf8_lossy(&r.body));
    let last: AuditPage = fx.get("/api/v1/audit?limit=1").await;
    assert_eq!(last.items[0].target, format!("project:{}", p.id));
    assert_eq!(last.items[0].detail["deleted"], json!(p.name), "the entry names the project");
    let backup = page.items.iter().find(|e| e.action == "backup.create").expect("backup audited");
    assert_eq!(backup.target, info.file);
}

#[tokio::test]
async fn exports_download_with_a_file_name_and_are_audited() {
    let fx = demo().await;
    let before = fx.audit_actions().await.iter().filter(|a| *a == "export").count();
    let r = fx.send(fx.req("GET", "/api/v1/export/usage?format=csv").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.header("content-type").starts_with("text/csv"));
    let cd = r.header("content-disposition");
    assert!(cd.starts_with("attachment; filename=\"kanjo-usage-") && cd.ends_with(".csv\""), "{cd}");
    let text = String::from_utf8(r.body.clone()).unwrap();
    assert!(text.lines().count() > 2, "{text}");
    for view in ["breakdown", "sessions", "blocks", "turns", "alerts", "audit"] {
        let r = fx.send(fx.req("GET", &format!("/api/v1/export/{view}")).body(Body::empty()).unwrap()).await;
        assert_eq!(r.status, StatusCode::OK, "{view}");
        assert!(r.header("content-type").starts_with("text/csv"), "{view}");
    }
    let r = fx.send(fx.req("GET", "/api/v1/export/sessions?format=json").body(Body::empty()).unwrap()).await;
    assert!(r.header("content-disposition").ends_with(".json\""));
    let _: SessionList = r.json();
    let r = fx.send(fx.req("GET", "/api/v1/export/secrets").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = fx.send(fx.req("GET", "/api/v1/export/usage?format=xml").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let actions = fx.audit_actions().await;
    assert_eq!(actions.iter().filter(|a| *a == "export").count(), before + 8);
}

#[tokio::test]
async fn transcript_is_refused_by_default() {
    let fx = demo().await;
    let sessions: SessionList = fx.get("/api/v1/sessions?limit=1").await;
    let r = fx.send(fx.req("GET", &format!("/api/v1/sessions/{}/transcript", sessions.items[0].id)).body(Body::empty()).unwrap()).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::FORBIDDEN, "disabled"));
    assert!(!fx.audit_actions().await.iter().any(|a| a == "transcript.view"));
}

#[tokio::test]
async fn demo_mode_refuses_the_hook_installer() {
    let fx = demo().await;
    let r = fx.call("POST", "/api/v1/system/hooks", Some(json!({ "statusline": true, "session_hooks": true, "service": false }))).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::FORBIDDEN, "disabled"));
    let r = fx.call("DELETE", "/api/v1/system/hooks", None).await;
    assert_eq!((r.status, r.error_code().as_str()), (StatusCode::FORBIDDEN, "disabled"));
    let r = fx.call("POST", "/api/v1/system/rescan", None).await;
    assert_eq!(r.status, StatusCode::OK);
    let _: IngestStatus = r.json();
}

#[tokio::test]
async fn published_events_arrive_on_the_stream() {
    let fx = demo().await;
    let res = fx.app.clone().oneshot(fx.req("GET", "/api/v1/events").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/event-stream"));
    fx.state.publish(LiveEvent::Status { session_id: "s-1".into() });
    let mut body = res.into_body();
    let mut seen = String::new();
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame = body.frame().await.expect("stream ended").unwrap();
            if let Ok(data) = frame.into_data() {
                seen.push_str(&String::from_utf8_lossy(&data));
            }
            if let Some(line) = seen.lines().find(|l| l.starts_with("data:")) {
                return serde_json::from_str::<LiveEvent>(line.trim_start_matches("data:").trim()).unwrap();
            }
        }
    })
    .await
    .expect("event within 5 s");
    assert!(matches!(event, LiveEvent::Status { ref session_id } if session_id == "s-1"), "{event:?}");
}

#[test]
fn non_loopback_binds_need_tls_and_token_auth() {
    let mut c = Config::default();
    assert!(check_bind(&c).unwrap().ip().is_loopback());
    c.server.bind = "0.0.0.0".into();
    let e = check_bind(&c).unwrap_err().to_string();
    assert!(e.contains("refusing"), "{e}");
    c.server.bind = "192.168.10.20".into();
    assert!(check_bind(&c).is_err());
    c.server.tls_cert = Some("cert.pem".into());
    assert!(check_bind(&c).is_err(), "a certificate without a key");
    c.server.tls_key = Some("key.pem".into());
    assert!(check_bind(&c).is_ok());
    c.server.auth = AuthMode::None;
    assert!(check_bind(&c).is_err(), "auth = none is honoured only on loopback");
    c.server.bind = "example.org".into();
    assert!(check_bind(&c).is_err(), "no name resolution");
    let mut l = Config::default();
    l.server.auth = AuthMode::None;
    l.server.bind = "::1".into();
    assert!(check_bind(&l).is_ok());
}

#[tokio::test]
async fn start_refuses_a_non_loopback_bind_without_tls() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(dir.path(), true);
    opts.config.server.bind = "0.0.0.0".into();
    opts.config.server.port = 0;
    assert!(start(opts).await.is_err());
    assert!(!dir.path().join("home").join("kanjo.db").exists(), "refused before anything was opened");
}

async fn http_exchange<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, request: String) -> (u16, String, Vec<u8>) {
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut buf)).await;
    let Some(split) = buf.windows(4).position(|w| w == b"\r\n\r\n") else { return (0, String::new(), buf) };
    let head = String::from_utf8_lossy(&buf[..split]).to_string();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, head, buf[split + 4..].to_vec())
}

fn request(path: &str, host: &str, extra: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n{extra}\r\n")
}

async fn plain(addr: SocketAddr, path: &str, extra: &str) -> (u16, String, Vec<u8>) {
    let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    http_exchange(tcp, request(path, &format!("127.0.0.1:{}", addr.port()), extra)).await
}

#[tokio::test]
async fn real_server_serves_the_ui_and_the_api_over_a_socket() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(dir.path(), true);
    opts.config.server.port = 0;
    let paths = opts.paths.clone();
    let server = start(opts).await.unwrap();
    let addr = server.local_addr();
    assert_ne!(addr.port(), 0);
    assert_eq!(server.url(), format!("http://127.0.0.1:{}/", addr.port()));
    let token = kanjo_ops::token::load_or_create(&paths).unwrap();

    let (status, head, body) = plain(addr, "/", "").await;
    assert_eq!(status, 200, "{head}");
    assert!(String::from_utf8_lossy(&body).to_lowercase().contains("<html"));
    let (status, _, _) = plain(addr, "/api/v1/overview", "").await;
    assert_eq!(status, 401);
    let (status, head, body) = plain(addr, "/api/v1/overview", &format!("Authorization: Bearer {token}\r\n")).await;
    assert_eq!(status, 200, "{head}");
    let overview: Overview = serde_json::from_slice(&body).unwrap();
    assert!(overview.month.usage.cost_usd > 0.0);

    let login = server.login_url();
    let code_path = &login[login.find("/?").unwrap()..];
    assert!(!code_path.contains(&token), "the opener URL does not carry the token");
    let (status, head, _) = plain(addr, code_path, "").await;
    assert_eq!(status, 303);
    assert!(head.to_lowercase().contains("set-cookie: kanjo_session="), "{head}");
    let (_, head, _) = plain(addr, code_path, "").await;
    assert!(!head.to_lowercase().contains("set-cookie"), "a login code works once");

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let sse = request("/api/v1/events", &format!("127.0.0.1:{}", addr.port()), &format!("Authorization: Bearer {token}\r\n"));
    stream.write_all(sse.as_bytes()).await.unwrap();
    let mut first = [0u8; 256];
    let n = stream.read(&mut first).await.unwrap();
    assert!(String::from_utf8_lossy(&first[..n]).contains("text/event-stream"));

    tokio::time::timeout(Duration::from_secs(15), server.shutdown()).await.expect("shutdown with an open event stream").unwrap();
    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut rest)).await.expect("the event stream ends").unwrap();
    assert!(tokio::net::TcpStream::connect(addr).await.is_err(), "the port is released");
}

#[tokio::test]
async fn the_server_announces_itself_and_takes_login_codes_from_the_command_line() {
    use kanjo_setup::server_file;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(dir.path(), true);
    opts.config.server.port = 0;
    let paths = opts.paths.clone();
    let server = start(opts.clone()).await.unwrap();
    let addr = server.local_addr();
    let rec = server_file::answering(&paths).expect("server.json names the running server");
    assert_eq!((rec.addr, rec.url.clone(), rec.auth, rec.demo), (addr, server.url(), true, true));
    assert_eq!(rec.pid, std::process::id());

    let err = start(opts.clone()).await.err().expect("a second server on the same data directory").to_string();
    assert!(err.contains("already running") && err.contains(&rec.url), "{err}");

    let token = kanjo_ops::token::load_or_create(&paths).unwrap();
    let url = server_file::login_url(&paths, &rec, kanjo_core::time::now_ms()).unwrap();
    assert!(url.starts_with(&rec.url) && !url.contains(&token), "{url}");
    let path = url[url.find("/?").unwrap()..].to_string();
    let (status, head, _) = plain(addr, &path, "").await;
    assert_eq!(status, 303);
    assert!(head.to_lowercase().contains("set-cookie: kanjo_session="), "{head}");
    let (_, head, _) = plain(addr, &path, "").await;
    assert!(!head.to_lowercase().contains("set-cookie"), "a code works once");

    let mut stranger = rec.clone();
    stranger.instance = "0".repeat(32);
    let url = server_file::login_url(&paths, &stranger, kanjo_core::time::now_ms()).unwrap();
    let (_, head, _) = plain(addr, &url[url.find("/?").unwrap()..], "").await;
    assert!(!head.to_lowercase().contains("set-cookie"), "a code written for another instance is refused");

    tokio::time::timeout(Duration::from_secs(15), server.shutdown()).await.unwrap().unwrap();
    assert!(!paths.server_file().exists(), "removed on a clean shutdown");
    assert!(server_file::live(&paths).is_none());
    let again = start(opts).await.expect("the data directory is free again");
    again.shutdown().await.unwrap();
}

#[tokio::test]
async fn idle_connections_cannot_keep_the_owner_out() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(dir.path(), true);
    opts.config.server.port = 0;
    let paths = opts.paths.clone();
    let server = start(opts).await.unwrap();
    let addr = server.local_addr();
    let token = kanjo_ops::token::load_or_create(&paths).unwrap();
    let mut idle = Vec::new();
    for _ in 0..300 {
        idle.push(tokio::net::TcpStream::connect(addr).await.unwrap());
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (status, head, _) = tokio::time::timeout(Duration::from_secs(5), plain(addr, "/api/v1/health", ""))
        .await
        .expect("a new client is served while 300 idle sockets are open");
    assert_eq!(status, 200, "{head}");
    let (status, head, _) =
        tokio::time::timeout(Duration::from_secs(5), plain(addr, "/api/v1/overview", &format!("Authorization: Bearer {token}\r\n")))
            .await
            .expect("the owner gets in");
    assert_eq!(status, 200, "{head}");
    drop(idle);
    tokio::time::timeout(Duration::from_secs(15), server.shutdown()).await.unwrap().unwrap();
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

#[tokio::test]
async fn turning_session_titles_off_forgets_the_stored_ones() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(dir.path(), false);
    opts.config.server.port = 0;
    opts.config.storage.encryption = EncryptionMode::Off;
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/projects");
    copy_dir(&fixtures, &opts.paths.claude_projects());
    let paths = opts.paths.clone();
    let server = start(opts).await.unwrap();
    let token = kanjo_ops::token::load_or_create(&paths).unwrap();
    let fx = Fx { _dir: tempfile::tempdir().unwrap(), state: server.state().clone(), app: router(server.state().clone()), token };
    fx.state.set_port(PORT);
    let titles = || async {
        let list: SessionList = fx.get("/api/v1/sessions?limit=100&from=0").await;
        list.items.into_iter().filter_map(|s| s.title).collect::<Vec<_>>()
    };

    let mut s: Settings = fx.get("/api/v1/settings").await;
    s.privacy.session_titles = true;
    assert_eq!(fx.call("PUT", "/api/v1/settings", Some(serde_json::to_value(&s).unwrap())).await.status, StatusCode::OK);
    let mut stored = Vec::new();
    for _ in 0..100 {
        let _ = fx.call("POST", "/api/v1/system/rescan", None).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        stored = titles().await;
        if !stored.is_empty() {
            break;
        }
    }
    assert!(stored.iter().any(|t| t.contains("invented checkout")), "{stored:?}");

    s.privacy.session_titles = false;
    assert_eq!(fx.call("PUT", "/api/v1/settings", Some(serde_json::to_value(&s).unwrap())).await.status, StatusCode::OK);
    assert!(titles().await.is_empty(), "stored titles are cleared when the setting goes off");
    let found: Vec<SearchResult> = fx.get("/api/v1/search?q=checkout").await;
    assert!(found.iter().all(|r| !r.label.contains("invented checkout")), "{found:?}");
    let csv = fx.send(fx.req("GET", "/api/v1/export/sessions?format=csv&from=0").body(Body::empty()).unwrap()).await;
    assert!(!String::from_utf8_lossy(&csv.body).contains("invented checkout"));

    fx.state.set_port(server.local_addr().port());
    tokio::time::timeout(Duration::from_secs(15), server.shutdown()).await.unwrap().unwrap();
}

#[tokio::test]
async fn a_port_in_use_reports_that_kanjo_is_probably_running() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(dir.path(), true);
    opts.config.server.port = taken.local_addr().unwrap().port();
    let err = start(opts).await.err().expect("port in use").to_string();
    assert!(err.contains("already running"), "{err}");
}

#[tokio::test]
async fn tls_is_served_with_rustls_and_the_cookie_is_secure() {
    use rustls::pki_types::ServerName;
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let (cert, key) = (dir.path().join("cert.pem"), dir.path().join("key.pem"));
    std::fs::write(&cert, ck.cert.pem()).unwrap();
    std::fs::write(&key, ck.signing_key.serialize_pem()).unwrap();
    let mut opts = options(dir.path(), true);
    opts.config.server.port = 0;
    opts.config.server.tls_cert = Some(cert.display().to_string());
    opts.config.server.tls_key = Some(key.display().to_string());
    let paths = opts.paths.clone();
    let server = start(opts).await.unwrap();
    let addr = server.local_addr();
    assert!(server.url().starts_with("https://"));
    let token = kanjo_ops::token::load_or_create(&paths).unwrap();

    let mut roots = rustls::RootCertStore::empty();
    roots.add(ck.cert.der().clone()).unwrap();
    let client = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client));
    let host = format!("localhost:{}", addr.port());
    let connect = || async {
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        connector.connect(ServerName::try_from("localhost").unwrap(), tcp).await.unwrap()
    };
    let (status, head, _) = http_exchange(connect().await, request("/api/v1/health", &host, "")).await;
    assert_eq!(status, 200, "{head}");
    let (status, head, _) = http_exchange(connect().await, request(&format!("/?token={token}"), &host, "")).await;
    assert_eq!(status, 303);
    let cookie = head.lines().find(|l| l.to_lowercase().starts_with("set-cookie:")).unwrap().to_string();
    assert!(cookie.contains("; Secure"), "{cookie}");
    let (status, _, _) = plain(addr, "/api/v1/health", "").await;
    assert_eq!(status, 0, "plain HTTP on the TLS port gets no HTTP answer");
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_real_installation_imports_installs_hooks_and_shuts_down() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(dir.path(), false);
    opts.config.server.port = 0;
    opts.config.storage.encryption = EncryptionMode::Keyfile;
    std::fs::create_dir_all(opts.paths.claude_projects()).unwrap();
    let paths = opts.paths.clone();
    let server = start(opts).await.unwrap();
    let addr = server.local_addr();
    let token = kanjo_ops::token::load_or_create(&paths).unwrap();
    let state = server.state().clone();
    let app = router(state);
    let fx = Fx { _dir: tempfile::tempdir().unwrap(), state: server.state().clone(), app, token: token.clone() };
    fx.state.set_port(PORT);

    let system: SystemInfo = fx.get("/api/v1/system").await;
    assert!(system.database.encrypted);
    assert_eq!(system.database.key_source, KeySource::Keyfile);
    assert_eq!(system.claude_dir, paths.claude_dir.display().to_string());

    let r = fx.call("POST", "/api/v1/system/hooks", Some(json!({ "statusline": true, "session_hooks": true, "service": false }))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", String::from_utf8_lossy(&r.body));
    let hooks: HookStatus = r.json();
    assert!(hooks.statusline_installed && hooks.session_hooks_installed);
    assert!(paths.claude_settings().exists(), "settings.json written in the temporary Claude directory");
    let r = fx.call("DELETE", "/api/v1/system/hooks", None).await;
    let hooks: HookStatus = r.json();
    assert!(!hooks.statusline_installed && !hooks.session_hooks_installed);
    let actions = fx.audit_actions().await;
    assert!(actions.contains(&"hooks.install".to_string()) && actions.contains(&"hooks.uninstall".to_string()));

    let capture = || Config::load(&paths).unwrap().statusline.capture_titles;
    assert!(!capture());
    let mut s: Settings = fx.get("/api/v1/settings").await;
    s.privacy.session_titles = true;
    assert_eq!(fx.call("PUT", "/api/v1/settings", Some(serde_json::to_value(&s).unwrap())).await.status, StatusCode::OK);
    assert!(capture(), "the statusline may keep titles once they are allowed");
    s.privacy.session_titles = false;
    assert_eq!(fx.call("PUT", "/api/v1/settings", Some(serde_json::to_value(&s).unwrap())).await.status, StatusCode::OK);
    assert!(!capture(), "and drops them again when they are not");
    assert_eq!(Config::load(&paths).unwrap().server.port, Config::default().server.port, "the port override stays out of config.toml");
    let r = fx.call("POST", "/api/v1/system/rescan", None).await;
    assert_eq!(r.status, StatusCode::OK);

    fx.state.set_port(addr.port());
    tokio::time::timeout(Duration::from_secs(15), server.shutdown()).await.expect("clean shutdown").unwrap();
}

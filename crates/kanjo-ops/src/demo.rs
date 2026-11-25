//! Deterministic synthetic data for screenshots and `kanjo demo`: one
//! engineer's month across six invented projects for three invented clients.
//! Every name, path and id here is made up.

use kanjo_core::limits::{self, LimitKind, LimitSample};
use kanjo_core::model::{RequestRow, TokenCounts, ToolKind, ToolUse};
use kanjo_core::pricing::PriceTable;
use kanjo_core::settings::{PlanKind, Settings};
use kanjo_core::store::write;
use kanjo_core::time::{hour_floor, Ctx, DAY_MS, FIVE_HOURS_MS, HOUR_MS, MINUTE_MS, SEVEN_DAYS_MS};
use kanjo_core::{Error, Result, Store};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;

const SECOND: i64 = 1000;
const CTX_SIZE: i64 = 200_000;
const ACTOR: &str = "dev";
/// Settings key marking a database filled by `populate`, so a second run may replace it.
pub const DEMO_MARKER: &str = "demo.seed";

/// Fills an empty (or demo-only) database with `days` of activity ending at
/// `now`, bucketed by the local time zone of this machine. The same seed
/// gives the same data. Refuses to touch a database that holds real usage.
pub fn populate(store: &Store, now: i64, days: u32, seed: u64) -> Result<()> {
    use chrono::{Offset, TimeZone};
    let offset = chrono::Local.timestamp_millis_opt(now).single().map(|d| d.offset().fix().local_minus_utc() / 60).unwrap_or(0);
    populate_in(store, &Ctx { now, tz_offset_min: offset, week_start: 0 }, days, seed)
}

/// `populate` with an explicit time context (`ctx.now` is the end of the data).
pub fn populate_in(store: &Store, ctx: &Ctx, days: u32, seed: u64) -> Result<()> {
    let days = days.max(1) as i64;
    store.write(|tx| {
        let marked = kanjo_core::settings::get::<u64>(tx, DEMO_MARKER)?.is_some();
        let real: i64 = tx.query_row("SELECT count(*) FROM requests WHERE message_id NOT LIKE 'msg_demo_%'", [], |r| r.get(0))?;
        let mut data: i64 = 0;
        for table in ["requests", "sessions", "limit_samples", "session_status", "session_events", "budgets", "alerts"] {
            data += tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_, i64>(0))?;
        }
        if real > 0 || (!marked && data > 0) {
            return Err(Error::Invalid("demo data is only written into an empty database".into()));
        }
        crate::retention::purge_tx(tx, &kanjo_core::api::PurgeRequest { everything: true, ..Default::default() })?;
        tx.execute("DELETE FROM budgets", [])?;
        let mut g = Gen::new(tx, ctx, days, seed)?;
        g.history();
        g.today();
        g.write(tx)?;
        kanjo_core::settings::set(tx, DEMO_MARKER, &seed)
    })?;
    let mut s = Settings::load(store)?;
    s.onboarding_done = true;
    s.plan.kind = PlanKind::Max20x;
    s.privacy.session_titles = true;
    s.save(store)
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Inclusive range.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1).max(1) as u64) as i64
    }
    fn chance(&mut self, p: f64) -> bool {
        self.f() < p
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[(self.next() % xs.len() as u64) as usize]
    }
    fn weighted<T: Copy>(&mut self, xs: &[(T, f64)]) -> T {
        let total: f64 = xs.iter().map(|x| x.1).sum();
        let mut x = self.f() * total;
        for (v, w) in xs {
            if x < *w {
                return *v;
            }
            x -= w;
        }
        xs[xs.len() - 1].0
    }
    fn hex(&mut self, n: usize) -> String {
        let mut s = String::with_capacity(n);
        while s.len() < n {
            s.push_str(&format!("{:016x}", self.next()));
        }
        s.truncate(n);
        s
    }
}

struct Project {
    slug: &'static str,
    client: &'static str,
    weight: f64,
    weekend: f64,
    titles: &'static [&'static str],
    branches: &'static [&'static str],
    mcp: &'static [(&'static str, &'static str)],
}

const NORTHWIND: &str = "Northwind Freight";
const BLUEFIN: &str = "Bluefin Health";
const KESTREL: &str = "Kestrel Energy";

const PROJECTS: &[Project] = &[
    Project {
        slug: "atlas-api",
        client: NORTHWIND,
        weight: 0.27,
        weekend: 0.05,
        titles: &[
            "Add cursor pagination to /shipments",
            "Fix N+1 query in carrier lookup",
            "Rate limiter for public endpoints",
            "Move auth middleware to tower layers",
            "Flaky integration test in routing module",
            "OpenAPI schema for tracking events",
        ],
        branches: &["feat/cursor-pagination", "fix/carrier-n-plus-one", "feat/rate-limit", "refactor/auth-layer", "main"],
        mcp: &[("postgres", "query"), ("tracker", "get_issue")],
    },
    Project {
        slug: "billing-web",
        client: NORTHWIND,
        weight: 0.19,
        weekend: 0.0,
        titles: &[
            "Stripe webhook retry handling",
            "Invoice PDF layout regression",
            "Dark mode for billing settings",
            "Proration preview component",
            "Upgrade Svelte and fix type errors",
        ],
        branches: &["feat/webhook-retries", "fix/invoice-layout", "feat/dark-mode", "feat/proration-preview", "chore/svelte-upgrade"],
        mcp: &[("tracker", "get_issue"), ("playwright", "browser_snapshot")],
    },
    Project {
        slug: "mobile-app",
        client: BLUEFIN,
        weight: 0.14,
        weekend: 0.0,
        titles: &[
            "Offline sync conflict resolution",
            "Deep links from push notifications",
            "Camera permission crash on Android",
            "Appointment reminder screen",
            "Accessibility labels audit",
        ],
        branches: &["feat/offline-sync", "feat/deep-links", "fix/camera-permission", "feat/reminders", "chore/a11y-labels"],
        mcp: &[("tracker", "get_issue"), ("figma", "get_frame")],
    },
    Project {
        slug: "docs-site",
        client: BLUEFIN,
        weight: 0.07,
        weekend: 0.35,
        titles: &["Rewrite the getting-started guide", "Broken links after nav restructure", "API reference generator config"],
        branches: &["docs/getting-started", "fix/broken-links", "main"],
        mcp: &[("docs-search", "search")],
    },
    Project {
        slug: "infra-terraform",
        client: KESTREL,
        weight: 0.15,
        weekend: 0.1,
        titles: &[
            "Split the staging VPC module",
            "Rotate database credentials via secrets manager",
            "Plan drift in the monitoring stack",
            "Autoscaling policy for ingest workers",
        ],
        branches: &["infra/split-vpc", "infra/rotate-db-creds", "fix/monitoring-drift", "infra/ingest-autoscaling"],
        mcp: &[("terraform-registry", "search_modules")],
    },
    Project {
        slug: "ml-pipeline",
        client: KESTREL,
        weight: 0.18,
        weekend: 0.5,
        titles: &[
            "Feature store backfill job",
            "Speed up the parquet loader",
            "Weekly model drift report",
            "Retry logic for the training DAG",
            "Evaluate forecast model v3",
        ],
        branches: &["feat/backfill", "perf/parquet-loader", "feat/drift-report", "fix/dag-retries", "exp/forecast-v3"],
        mcp: &[("jupyter", "run_cell"), ("postgres", "query")],
    },
];

const OPUS: &str = "claude-opus-5-5";
const SONNET: &str = "claude-sonnet-5-5";

const AGENTS: &[((&str, &str), f64)] = &[
    (("Explore", "claude-haiku-5-5"), 0.36),
    (("general-purpose", "claude-sonnet-5-5"), 0.22),
    (("code-reviewer", "claude-sonnet-5-5"), 0.18),
    (("test-runner", "claude-haiku-5-5"), 0.14),
    (("architect", "claude-fable-5-2"), 0.10),
];

const TOOLS: &[(&str, f64)] = &[
    ("Read", 0.26),
    ("Edit", 0.17),
    ("Bash", 0.20),
    ("Grep", 0.12),
    ("Glob", 0.06),
    ("Write", 0.05),
    ("TodoWrite", 0.05),
    ("WebFetch", 0.02),
];

const SKILLS: &[&str] = &["commit", "review-pr", "changelog", "db-migration", "release-notes"];

const HOURS_WEEKDAY: &[(i64, f64)] =
    &[(9, 0.10), (10, 0.17), (11, 0.15), (12, 0.05), (13, 0.09), (14, 0.16), (15, 0.13), (16, 0.09), (17, 0.04), (20, 0.02)];
const HOURS_WEEKEND: &[(i64, f64)] = &[(10, 0.2), (11, 0.2), (15, 0.2), (16, 0.2), (21, 0.2)];

fn tool(kind: ToolKind, name: &str, use_id: &str) -> ToolUse {
    ToolUse { use_id: use_id.to_string(), kind, name: name.to_string() }
}

struct Spec {
    project: usize,
    start: i64,
    end: i64,
    model: &'static str,
    pace: f64,
    subagents: f64,
    change_mid: bool,
    near_limit: bool,
    active: bool,
    /// No long breaks: the scripted sessions of the last hours.
    focus: bool,
}

#[derive(Default)]
struct CacheStats {
    requests: i64,
    misses: i64,
    rebuilds: i64,
    read: i64,
    write: i64,
    input: i64,
    miss_recache: i64,
    last_miss_at: Option<i64>,
    causes: BTreeMap<String, i64>,
}

struct SessionOut {
    id: String,
    project: usize,
    title: String,
    model: &'static str,
    start: i64,
    last: i64,
    ctx: i64,
    cost: f64,
    turns: i64,
    active: bool,
    cache: CacheStats,
}

struct Miss {
    session: String,
    ts: i64,
    causes: Vec<&'static str>,
    tools_added: Option<i64>,
    tools_removed: Option<i64>,
    system_char_delta: Option<i64>,
}

struct Event {
    ts: i64,
    session: String,
    event: &'static str,
    source: Option<&'static str>,
    project: usize,
    model: &'static str,
}

struct Change {
    ts: i64,
    kind: &'static str,
    target: &'static str,
    project: Option<usize>,
    fingerprint: String,
}

struct Gen<'p> {
    rng: Rng,
    ctx: Ctx,
    days: i64,
    tag: String,
    n: u64,
    prices: &'p PriceTable,
    project_ids: Vec<i64>,
    rows: Vec<(RequestRow, f64)>,
    sessions: Vec<SessionOut>,
    misses: Vec<Miss>,
    events: Vec<Event>,
    changes: Vec<Change>,
}

impl Gen<'static> {
    fn new(tx: &Connection, ctx: &Ctx, days: i64, seed: u64) -> Result<Gen<'static>> {
        let mut rng = Rng(seed ^ 0x6B61_6E6A_6F00_0000);
        let tag = rng.hex(8);
        let first = ctx.day_start(ctx.now) - (days - 1) * DAY_MS;
        let mut project_ids = Vec::new();
        for p in PROJECTS {
            let id = write::upsert_project(tx, &format!("-home-dev-work-{}", p.slug), Some(&format!("/home/dev/work/{}", p.slug)), first)?;
            tx.execute("UPDATE projects SET client = ?2 WHERE id = ?1", params![id, p.client])?;
            project_ids.push(id);
        }
        Ok(Gen {
            rng,
            ctx: *ctx,
            days,
            tag,
            n: 0,
            prices: PriceTable::embedded(),
            project_ids,
            rows: Vec::new(),
            sessions: Vec::new(),
            misses: Vec::new(),
            events: Vec::new(),
            changes: Vec::new(),
        })
    }
}

impl Gen<'_> {
    fn weekday(&self, day_start: i64) -> i64 {
        let local = (day_start + self.ctx.tz_offset_min as i64 * MINUTE_MS).div_euclid(DAY_MS);
        (local + 3).rem_euclid(7)
    }

    fn history(&mut self) {
        let now = self.ctx.now;
        let today = self.ctx.day_start(now);
        let reserve = now - 8 * HOUR_MS;
        let spike = {
            let candidates: Vec<i64> = (0..self.days)
                .filter(|d| *d >= self.days - 14 && *d <= self.days - 5 && self.weekday(today - (self.days - 1 - d) * DAY_MS) < 5)
                .collect();
            (!candidates.is_empty()).then(|| *self.rng.pick(&candidates))
        };
        let mut specs = Vec::new();
        for d in 0..self.days {
            let day = today - (self.days - 1 - d) * DAY_MS;
            let dow = self.weekday(day);
            let weekend = dow >= 5;
            let is_spike = spike == Some(d);
            let mut n = match dow {
                0..=3 => self.rng.range(4, 6),
                4 => self.rng.range(3, 4),
                _ if self.rng.chance(0.4) => self.rng.range(1, 2),
                _ => 0,
            };
            if is_spike {
                n += 2;
            }
            for i in 0..n {
                let hour = self.rng.weighted(if weekend { HOURS_WEEKEND } else { HOURS_WEEKDAY });
                let start = day + hour * HOUR_MS + self.rng.range(0, 59) * MINUTE_MS + self.rng.range(0, 59) * SECOND;
                let minutes = if weekend { self.rng.range(15, 60) } else { self.rng.range(35, 150) };
                let weights: Vec<(usize, f64)> =
                    PROJECTS.iter().enumerate().map(|(i, p)| (i, if weekend { p.weekend } else { p.weight })).collect();
                let mut project = self.rng.weighted(&weights);
                let runaway = is_spike && i == 0;
                if runaway {
                    project = 5;
                }
                let spec = Spec {
                    project,
                    start: if runaway { day + 10 * HOUR_MS + 5 * MINUTE_MS } else { start },
                    end: if runaway { day + 15 * HOUR_MS + 40 * MINUTE_MS } else { start + minutes * MINUTE_MS },
                    model: if self.rng.chance(0.8) { OPUS } else { SONNET },
                    pace: if is_spike { 1.6 } else { 0.9 + self.rng.f() * 0.5 },
                    subagents: if runaway {
                        0.07
                    } else if is_spike {
                        0.04
                    } else {
                        0.02
                    },
                    change_mid: self.rng.chance(0.16),
                    near_limit: false,
                    active: false,
                    focus: false,
                };
                if spec.end < reserve {
                    specs.push(spec);
                }
            }
            if !weekend && self.rng.chance(0.25) {
                let ts = day + self.rng.range(8 * 60, 18 * 60) * MINUTE_MS;
                if ts < now {
                    let fingerprint = self.rng.hex(16);
                    self.changes.push(Change { ts, kind: "settings", target: "user settings", project: None, fingerprint });
                }
            }
        }
        if let Some(s) = specs.iter_mut().filter(|s| s.end - s.start >= 50 * MINUTE_MS).max_by_key(|s| s.start) {
            s.near_limit = true;
            s.focus = true;
            s.model = OPUS;
        }
        for s in &specs {
            self.session(s);
        }
    }

    /// The last hours are scripted relative to `now`, whatever weekday it is:
    /// a finished session and an active one that pushes the 5-hour window to
    /// roughly 80% with a burn rate that runs out before the reset.
    fn today(&mut self) {
        let now = self.ctx.now;
        let first = Spec {
            project: 0,
            start: now - 170 * MINUTE_MS,
            end: now - 70 * MINUTE_MS,
            model: OPUS,
            pace: 2.0,
            subagents: 0.03,
            change_mid: true,
            near_limit: false,
            active: false,
            focus: true,
        };
        let active = Spec {
            project: 1,
            start: now - 62 * MINUTE_MS,
            end: now - MINUTE_MS,
            model: OPUS,
            pace: 3.0,
            subagents: 0.06,
            change_mid: false,
            near_limit: false,
            active: true,
            focus: true,
        };
        let parallel = Spec {
            project: 5,
            start: now - 48 * MINUTE_MS,
            end: now - 6 * MINUTE_MS,
            model: SONNET,
            pace: 1.8,
            subagents: 0.05,
            change_mid: false,
            near_limit: false,
            active: false,
            focus: true,
        };
        self.session(&first);
        self.session(&active);
        self.session(&parallel);
    }

    fn uuid(&mut self) -> String {
        let h = self.rng.hex(32);
        format!("{}-{}-4{}-a{}-{}", &h[0..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
    }

    fn cc_version(&self, ts: i64) -> &'static str {
        if ts < self.ctx.now - self.days * DAY_MS / 2 {
            "2.3.14"
        } else {
            "2.3.17"
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        session: &str,
        project: usize,
        ts: i64,
        model: &str,
        tokens: TokenCounts,
        tools: Vec<ToolUse>,
        stop: &str,
        agent: Option<(&str, &str)>,
        branch: &str,
    ) -> f64 {
        self.n += 1;
        let cost = self.prices.cost(model, &tokens).unwrap_or(0.0);
        let row = RequestRow {
            message_id: format!("msg_demo_{}{:08}", self.tag, self.n),
            request_id: Some(format!("req_demo_{}{:08}", self.tag, self.n)),
            session_id: session.to_string(),
            project_id: self.project_ids[project],
            ts,
            model: model.to_string(),
            tokens,
            service_tier: Some("standard".into()),
            is_sidechain: agent.is_some(),
            agent_id: agent.map(|a| a.0.to_string()),
            agent_type: agent.map(|a| a.1.to_string()),
            stop_reason: Some(stop.to_string()),
            git_branch: Some(branch.to_string()),
            cc_version: Some(self.cc_version(ts).to_string()),
            entrypoint: Some("cli".into()),
            tools,
            ..Default::default()
        };
        self.rows.push((row, cost));
        cost
    }

    fn delta(&mut self) -> i64 {
        let r = self.rng.f();
        if r < 0.75 {
            self.rng.range(150, 1200)
        } else if r < 0.96 {
            self.rng.range(1200, 4500)
        } else {
            self.rng.range(4500, 15_000)
        }
    }

    fn output(&mut self) -> i64 {
        let r = self.rng.f();
        if r < 0.78 {
            self.rng.range(40, 700)
        } else if r < 0.96 {
            self.rng.range(700, 2500)
        } else {
            self.rng.range(2500, 7000)
        }
    }

    fn session(&mut self, s: &Spec) {
        let id = self.uuid();
        let p = &PROJECTS[s.project];
        let title = self.rng.pick(p.titles).to_string();
        let branch = self.rng.pick(p.branches).to_string();
        let source = if self.rng.chance(0.2) { "resume" } else { "startup" };
        self.events.push(Event {
            ts: s.start - 2 * SECOND,
            session: id.clone(),
            event: "SessionStart",
            source: Some(source),
            project: s.project,
            model: s.model,
        });
        let change_at = s.change_mid.then(|| s.start + (s.end - s.start) * self.rng.range(30, 70) / 100);
        let mut change_done = false;
        let cap = if s.near_limit { i64::MAX } else { self.rng.range(150_000, 165_000) };
        let near_target = 182_000 + self.rng.range(0, 6000);
        let ctx0 = self.rng.range(15_000, 22_000);
        let mut ctx = ctx0;
        let mut cold = true;
        let mut pending: Option<Miss> = None;
        let mut st = CacheStats::default();
        let mut cost = 0.0;
        let mut turns = 0;
        let mut t = s.start;
        let mut last = t;
        while t <= s.end {
            if s.near_limit && ctx >= near_target {
                break;
            }
            if let Some(at) = change_at.filter(|at| !change_done && *at <= t) {
                change_done = true;
                let fingerprint = self.rng.hex(16);
                let r = self.rng.f();
                let (kind, target, project, miss) = if r < 0.6 {
                    let delta = self.rng.range(120, 2400) * if self.rng.chance(0.3) { -1 } else { 1 };
                    ("claude_md", "project CLAUDE.md", Some(s.project), (vec!["system_prompt_changed"], None, None, Some(delta)))
                } else if r < 0.85 {
                    let delta = self.rng.range(200, 1800);
                    ("claude_md", "user CLAUDE.md", None, (vec!["system_prompt_changed"], None, None, Some(delta)))
                } else {
                    let added = self.rng.range(1, 6);
                    let removed = self.rng.range(0, 1);
                    ("plugins", "plugins", None, (vec!["tools_changed"], Some(added), Some(removed), None))
                };
                self.changes.push(Change { ts: at, kind, target, project, fingerprint });
                if !cold {
                    pending = Some(Miss {
                        session: id.clone(),
                        ts: 0,
                        causes: miss.0,
                        tools_added: miss.1,
                        tools_removed: miss.2,
                        system_char_delta: miss.3,
                    });
                }
            }
            let mut delta = if cold { 0 } else { self.delta() };
            if s.near_limit && !cold {
                let span = (s.end - s.start).max(1) as f64;
                let want = ctx0 as f64 + (near_target - ctx0) as f64 * ((t - s.start) as f64 / (0.7 * span)).min(1.0);
                delta = (want as i64 - ctx).clamp(0, 12_000).min(near_target - ctx);
            }
            let prefix = ctx;
            ctx += delta;
            let miss = pending.take();
            let (read, write) = if cold || miss.is_some() { (0, ctx) } else { (prefix, delta) };
            if let Some(mut m) = miss {
                m.ts = t;
                st.misses += 1;
                st.miss_recache += ctx;
                st.last_miss_at = Some(t);
                for c in &m.causes {
                    *st.causes.entry(c.to_string()).or_default() += 1;
                }
                self.misses.push(m);
            }
            cold = false;
            let input = self.rng.range(2, 60);
            let mut output = self.output();
            if s.near_limit {
                output = output.min((near_target + 2000 - ctx).max(40));
            }
            let tokens = TokenCounts { input, output, cache_read: read, cache_write_5m: write, cache_write_1h: 0 };
            st.requests += 1;
            st.read += read;
            st.write += write;
            st.input += input;

            let use_id = format!("toolu_demo_{}", self.rng.hex(20));
            let mut tools = Vec::new();
            let mut stop = "tool_use";
            let mut spawned = Vec::new();
            if turns > 2 && !s.near_limit && self.rng.chance(s.subagents) {
                let k = self.rng.weighted(&[(1usize, 0.7), (2, 0.2), (3, 0.1)]);
                for j in 0..k {
                    let agent = self.rng.weighted(AGENTS);
                    let uid = format!("{use_id}{j}");
                    tools.push(tool(ToolKind::Tool, "Agent", &uid));
                    tools.push(tool(ToolKind::Agent, agent.0, &uid));
                    spawned.push(agent);
                }
            } else if self.rng.chance(0.015) {
                let skill = *self.rng.pick(SKILLS);
                tools.push(tool(ToolKind::Tool, "Skill", &use_id));
                tools.push(tool(ToolKind::Skill, skill, &use_id));
            } else if self.rng.chance(0.05) {
                let (server, name) = *self.rng.pick(p.mcp);
                tools.push(tool(ToolKind::Tool, &format!("mcp__{server}__{name}"), &use_id));
                tools.push(tool(ToolKind::Mcp, server, &use_id));
            } else if self.rng.chance(0.88) {
                let n = self.rng.weighted(&[(1usize, 0.75), (2, 0.2), (3, 0.05)]);
                for j in 0..n {
                    let name = self.rng.weighted(TOOLS);
                    tools.push(tool(ToolKind::Tool, name, &format!("{use_id}{j}")));
                }
            } else {
                stop = "end_turn";
            }
            cost += self.push(&id, s.project, t, s.model, tokens, tools, stop, None, &branch);
            turns += 1;
            last = t;
            ctx += output;

            let mut resume = t;
            for (j, agent) in spawned.into_iter().enumerate() {
                let deadline = s.end.min(self.ctx.now - 30 * SECOND);
                let (end, c) = self.subagent(&id, s.project, &branch, t + (j as i64 + 1) * 2 * SECOND, agent, deadline);
                cost += c;
                resume = resume.max(end);
                last = last.max(end);
                ctx += self.rng.range(1500, 4000);
            }
            t = resume;

            if ctx > cap {
                t += self.rng.range(15, 40) * SECOND;
                let summary = self.rng.range(6000, 10_000);
                let tokens =
                    TokenCounts { input: self.rng.range(5, 40), output: summary, cache_read: ctx, cache_write_5m: 0, cache_write_1h: 0 };
                cost += self.push(&id, s.project, t, s.model, tokens, Vec::new(), "end_turn", None, &branch);
                st.requests += 1;
                st.rebuilds += 1;
                st.read += ctx;
                last = t;
                ctx = self.rng.range(26_000, 38_000);
                cold = true;
            }

            let r = self.rng.f();
            let gap = if r < 0.008 && !s.focus {
                self.events.push(Event {
                    ts: t + 5 * SECOND,
                    session: id.clone(),
                    event: "Stop",
                    source: None,
                    project: s.project,
                    model: s.model,
                });
                self.rng.range(6 * 60, 30 * 60) * SECOND
            } else if r < 0.11 {
                self.events.push(Event {
                    ts: t + 3 * SECOND,
                    session: id.clone(),
                    event: "Stop",
                    source: None,
                    project: s.project,
                    model: s.model,
                });
                (self.rng.range(20, 150) as f64 * SECOND as f64 / s.pace) as i64
            } else {
                (self.rng.range(5, 35) as f64 * SECOND as f64 / s.pace) as i64
            };
            if gap >= 5 * MINUTE_MS && !cold && pending.is_none() {
                pending = Some(Miss {
                    session: id.clone(),
                    ts: 0,
                    causes: vec!["ttl_expired_5m"],
                    tools_added: None,
                    tools_removed: None,
                    system_char_delta: None,
                });
            }
            t += gap;
        }
        if !s.active {
            self.events.push(Event {
                ts: last + 20 * SECOND,
                session: id.clone(),
                event: "Stop",
                source: None,
                project: s.project,
                model: s.model,
            });
            let reason = *self.rng.pick(&["prompt_input_exit", "clear", "logout", "other"]);
            self.events.push(Event {
                ts: last + 40 * SECOND,
                session: id.clone(),
                event: "SessionEnd",
                source: Some(reason),
                project: s.project,
                model: s.model,
            });
        }
        self.sessions.push(SessionOut {
            id,
            project: s.project,
            title,
            model: s.model,
            start: s.start,
            last,
            ctx,
            cost,
            turns,
            active: s.active,
            cache: st,
        });
    }

    /// One subagent run; it wraps up early once `deadline` is passed.
    fn subagent(&mut self, session: &str, project: usize, branch: &str, start: i64, agent: (&str, &str), deadline: i64) -> (i64, f64) {
        let (kind, model) = agent;
        let aid = format!("a{}", self.rng.hex(16));
        let architect = kind == "architect";
        let n = if architect { self.rng.range(4, 10) } else { self.rng.range(5, 22) };
        let mut ctx = self.rng.range(9000, 14_000);
        let mut t = start;
        let mut cost = 0.0;
        for i in 0..n {
            let delta = if i == 0 { 0 } else { self.rng.range(800, 7000) };
            let (read, write) = if i == 0 { (0, ctx) } else { (ctx, delta) };
            ctx += delta;
            let last = i == n - 1 || t + 18 * SECOND > deadline;
            let output = if last && architect {
                self.rng.range(2500, 6000)
            } else if last {
                self.rng.range(400, 1500)
            } else {
                self.rng.range(60, 600)
            };
            let tokens = TokenCounts { input: self.rng.range(2, 30), output, cache_read: read, cache_write_5m: write, cache_write_1h: 0 };
            let tools = if last {
                Vec::new()
            } else {
                let name = *self.rng.pick(&["Read", "Grep", "Glob", "Bash", "Read"]);
                vec![tool(ToolKind::Tool, name, &format!("toolu_demo_{}", self.rng.hex(20)))]
            };
            cost += self.push(
                session,
                project,
                t,
                model,
                tokens,
                tools,
                if last { "end_turn" } else { "tool_use" },
                Some((&aid, kind)),
                branch,
            );
            ctx += output;
            t += self.rng.range(4, 18) * SECOND;
            if last {
                break;
            }
        }
        (t, cost)
    }

    fn write(mut self, tx: &Connection) -> Result<()> {
        let now = self.ctx.now;
        self.rows.sort_by(|a, b| a.0.ts.cmp(&b.0.ts).then_with(|| a.0.message_id.cmp(&b.0.message_id)));
        for (row, _) in &self.rows {
            write::insert_request(tx, row, self.prices)?;
        }
        let path = |p: usize| format!("/home/dev/work/{}", PROJECTS[p].slug);
        for s in &self.sessions {
            tx.execute("UPDATE sessions SET title = ?2 WHERE id = ?1", params![s.id, s.title])?;
        }
        for e in &self.events {
            tx.execute(
                "INSERT INTO session_events (ts, session_id, event, source, cwd, model) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![e.ts, e.session, e.event, e.source, path(e.project), e.model],
            )?;
        }
        for m in &self.misses {
            tx.execute(
                "INSERT OR IGNORE INTO cache_misses (session_id, ts, causes, tools_added, tools_removed, system_char_delta) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![m.session, m.ts, serde_json::to_string(&m.causes)?, m.tools_added, m.tools_removed, m.system_char_delta],
            )?;
        }
        self.changes.sort_by_key(|c| c.ts);
        for c in &self.changes {
            tx.execute(
                "INSERT INTO config_changes (ts, kind, target, project_id, fingerprint) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![c.ts, c.kind, c.target, c.project.map(|p| self.project_ids[p]), c.fingerprint],
            )?;
        }
        self.statuses(tx)?;
        self.limits(tx)?;
        self.budgets(tx)?;
        write::record_quarantine(
            tx,
            "system",
            "content,level,subtype,timestamp,type,uuid",
            "unknown_subtype",
            Some("2.3.14"),
            now - 9 * DAY_MS,
        )?;
        for i in 0..3 {
            write::record_quarantine(
                tx,
                "assistant",
                "message.content,message.id,message.model,message.usage.server_tool_use,type,uuid",
                "unknown_usage_field",
                Some("2.3.17"),
                now - 2 * DAY_MS + i * HOUR_MS,
            )?;
        }
        let audited: i64 = tx.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))?;
        if audited == 0 {
            let first = self.ctx.day_start(now) - (self.days - 1) * DAY_MS + 9 * HOUR_MS + 12 * MINUTE_MS;
            let entries = [
                (first, "hooks.install", "statusline", serde_json::json!({"statusline": true, "session_hooks": true, "service": true})),
                (first + 3 * MINUTE_MS, "settings.update", "plan", serde_json::json!({"plan": "max20x"})),
                (now - 4 * DAY_MS, "export", "usage", serde_json::json!({"format": "csv", "group": "day"})),
                (now - 2 * DAY_MS, "backup.create", "backup", serde_json::json!({"trigger": "manual"})),
            ];
            for (ts, action, target, detail) in entries.iter().filter(|e| e.0 < now) {
                crate::audit::record_tx(tx, *ts, ACTOR, action, target, detail)?;
            }
        }
        Ok(())
    }

    fn statuses(&self, tx: &Connection) -> Result<()> {
        let now = self.ctx.now;
        let mut rng = Rng(self.rng.0 ^ 0x5354_4154);
        for s in self.sessions.iter().filter(|s| s.last >= now - 3 * DAY_MS) {
            let c = &s.cache;
            let denom = c.read + c.write + c.input;
            let hit = (denom > 0).then(|| c.read as f64 / denom as f64);
            let duration = s.last - s.start + 30 * SECOND;
            let added = s.turns * rng.range(2, 9);
            let removed = added * rng.range(20, 50) / 100;
            let name = if s.model == OPUS { "Opus 5.5" } else { "Sonnet 5.5" };
            tx.execute(
                "INSERT OR REPLACE INTO session_status (session_id, ts, model_id, model_name, cc_version, project_dir, session_name, cost_usd,
                    duration_ms, api_duration_ms, lines_added, lines_removed, ctx_used_pct, ctx_size, ctx_tokens, exceeds_200k, fast_mode, effort,
                    cache_warm, cache_ttl, cache_expires_at, cache_requests, cache_misses, cache_expected_rebuilds, cache_hit_ratio,
                    cache_write_tokens, cache_miss_recache_tokens, cache_last_miss_at, cache_recache_if_cold, cache_miss_causes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 0, 0, 'high', ?16, '5m', ?17, ?18, ?19, ?20, ?21,
                    ?22, ?23, ?24, ?25, ?26)",
                params![
                    s.id,
                    if s.active { now - 40 * SECOND } else { s.last + 10 * SECOND },
                    s.model,
                    name,
                    self.cc_version(s.last),
                    format!("/home/dev/work/{}", PROJECTS[s.project].slug),
                    s.title,
                    (s.cost * 100.0).round() / 100.0,
                    duration,
                    duration * rng.range(38, 62) / 100,
                    added,
                    removed,
                    (s.ctx as f64 / CTX_SIZE as f64 * 1000.0).round() / 10.0,
                    CTX_SIZE,
                    s.ctx,
                    now - s.last < 5 * MINUTE_MS,
                    s.last + 5 * MINUTE_MS,
                    c.requests,
                    c.misses,
                    c.rebuilds,
                    hit,
                    c.write,
                    c.miss_recache,
                    c.last_miss_at,
                    s.ctx,
                    serde_json::to_string(&c.causes)?,
                ],
            )?;
        }
        Ok(())
    }

    /// Statusline readings that follow the generated spend: 5-hour windows
    /// open at the first request after the previous one closed, 7-day windows
    /// reset weekly; the ceilings are chosen so the current 5-hour window
    /// sits near 80% and no ordinary week tops out.
    fn limits(&mut self, tx: &Connection) -> Result<()> {
        let now = self.ctx.now;
        let reset7 = hour_floor(now) + self.rng.range(30, 130) * HOUR_MS;
        let week_of = |ts: i64| reset7 - (reset7 - 1 - ts).div_euclid(SEVEN_DAYS_MS) * SEVEN_DAYS_MS;
        let mut points: Vec<(i64, f64, i64, f64, i64)> = Vec::new();
        let (mut end5, mut cum5, mut last_sample) = (i64::MIN, 0.0, i64::MIN);
        let mut windows5: Vec<f64> = Vec::new();
        let mut weeks: BTreeMap<i64, f64> = BTreeMap::new();
        let count = self.rows.len();
        for (i, (row, cost)) in self.rows.iter().enumerate() {
            if row.ts >= end5 {
                if end5 != i64::MIN {
                    windows5.push(cum5);
                }
                end5 = hour_floor(row.ts) + FIVE_HOURS_MS;
                cum5 = 0.0;
                last_sample = i64::MIN;
            }
            cum5 += cost;
            let w = week_of(row.ts);
            let cum7 = {
                let e = weeks.entry(w).or_default();
                *e += cost;
                *e
            };
            if row.ts.saturating_sub(last_sample) >= 4 * MINUTE_MS || i + 1 == count {
                points.push((row.ts, cum5, end5, cum7, w));
                last_sample = row.ts;
            }
        }
        let Some(&(_, current5, _, current7, _)) = points.last() else { return Ok(()) };
        windows5.sort_by(|a, b| a.total_cmp(b));
        let p90 = windows5.get(windows5.len() * 9 / 10).copied().unwrap_or(0.0);
        let cap5 = (p90 / 0.88).clamp(current5 / 0.83, current5 / (0.72 + self.rng.f() * 0.04));
        let busiest = weeks.values().copied().fold(0.0, f64::max);
        let cap7 = (current7 / (0.50 + self.rng.f() * 0.12)).max(busiest / 0.96);
        let pct = |v: f64, cap: f64| if cap > 0.0 { ((v / cap * 100.0).min(100.0) * 10.0).round() / 10.0 } else { 0.0 };
        for (kind, which) in [(LimitKind::FiveHour, 0), (LimitKind::SevenDay, 1)] {
            for &(ts, c5, r5, c7, r7) in &points {
                let (used_pct, resets_at) = if which == 0 { (pct(c5, cap5), r5) } else { (pct(c7, cap7), r7) };
                let sample = LimitSample { ts, used_pct, resets_at, used_usd: None, limit_usd: None, period: None };
                limits::insert_sample(tx, kind, &sample)?;
            }
        }
        Ok(())
    }

    fn budgets(&self, tx: &Connection) -> Result<()> {
        let now = self.ctx.now;
        let month = self.ctx.month_start(now);
        let northwind: Vec<i64> =
            PROJECTS.iter().zip(&self.project_ids).filter(|(p, _)| p.client == NORTHWIND).map(|(_, id)| *id).collect();
        let (mut total, mut client) = (0.0, 0.0);
        for (row, cost) in self.rows.iter().filter(|r| r.0.ts >= month) {
            total += cost;
            if northwind.contains(&row.project_id) {
                client += cost;
            }
        }
        let global = ((total / 0.58 / 100.0).ceil() * 100.0).max(100.0);
        let raw = client / 0.77;
        let step = if raw >= 400.0 {
            10.0
        } else if raw >= 100.0 {
            5.0
        } else if raw >= 10.0 {
            1.0
        } else {
            0.5
        };
        let near = ((raw / step).ceil() * step).max(step);
        let created = now - self.days * DAY_MS;
        tx.execute(
            "INSERT INTO budgets (name, scope, scope_value, period, amount_usd, warn_pct, enabled, created_at)
             VALUES ('Monthly total', 'global', NULL, 'month', ?1, 80, 1, ?3), (?4, 'client', ?4, 'month', ?2, 80, 1, ?3)",
            params![global, near, created, NORTHWIND],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanjo_core::testkit::TestStore;

    const NOW: i64 = 1_795_648_000_000;

    fn ctx() -> Ctx {
        Ctx { now: NOW, tz_offset_min: 60, week_start: 0 }
    }

    fn totals(store: &Store) -> (i64, i64, i64, i64, i64) {
        store
            .read(|c| {
                Ok(c.query_row(
                    "SELECT (SELECT count(*) FROM requests), (SELECT SUM(output) FROM requests),
                            (SELECT CAST(ROUND(SUM(cost_usd) * 1e6) AS INTEGER) FROM usage_hourly),
                            (SELECT count(*) FROM limit_samples), (SELECT count(*) FROM session_events)",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )?)
            })
            .unwrap()
    }

    fn count(store: &Store, sql: &str) -> i64 {
        store.read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap()
    }

    #[test]
    fn deterministic_and_repeatable() {
        let (a, b) = (TestStore::new(), TestStore::new());
        populate_in(&a.store, &ctx(), 14, 7).unwrap();
        populate_in(&b.store, &ctx(), 14, 7).unwrap();
        let first = totals(&a.store);
        assert_eq!(first, totals(&b.store));
        populate_in(&a.store, &ctx(), 14, 7).unwrap();
        assert_eq!(totals(&a.store), first, "a second run replaces the demo data");
        assert_eq!(count(&a.store, "SELECT count(*) FROM budgets"), 2);
        let c = TestStore::new();
        populate_in(&c.store, &ctx(), 14, 8).unwrap();
        assert_ne!(totals(&c.store), first);
    }

    #[test]
    fn refuses_real_data() {
        let t = TestStore::new();
        let mut r = kanjo_core::testkit::request("msg_real", "s", NOW, OPUS, 1, 1);
        r.project_id = t.project("p");
        t.insert(&r);
        assert!(populate_in(&t.store, &ctx(), 3, 1).is_err());
    }

    #[test]
    fn month_looks_real() {
        let t = TestStore::new();
        populate_in(&t.store, &ctx(), 30, 42).unwrap();
        let s = &t.store;
        for table in [
            "projects",
            "sessions",
            "requests",
            "request_tools",
            "usage_hourly",
            "limit_samples",
            "session_status",
            "cache_misses",
            "session_events",
            "config_changes",
            "quarantine",
            "budgets",
            "audit_log",
            "settings",
        ] {
            assert!(count(s, &format!("SELECT count(*) FROM {table}")) > 0, "{table} is empty");
        }
        assert_eq!(count(s, "SELECT count(DISTINCT client) FROM projects"), 3);
        assert_eq!(count(s, "SELECT count(*) FROM sessions WHERE title IS NULL"), 0);
        for kind in ["tool", "mcp", "skill", "agent"] {
            assert!(count(s, &format!("SELECT count(*) FROM request_tools WHERE kind = '{kind}'")) > 0, "{kind}");
        }
        for model in [OPUS, SONNET, "claude-fable-5-2", "claude-haiku-5-5"] {
            assert!(count(s, &format!("SELECT count(*) FROM requests WHERE model = '{model}'")) > 0, "{model}");
        }
        let top: String = s
            .read(|c| Ok(c.query_row("SELECT model FROM requests GROUP BY model ORDER BY SUM(cost_usd) DESC LIMIT 1", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(top, OPUS);
        assert_eq!(count(s, "SELECT count(*) FROM requests WHERE cost_usd IS NULL"), 0);

        let month: f64 = s.read(|c| Ok(c.query_row("SELECT SUM(cost_usd) FROM sessions", [], |r| r.get(0))?)).unwrap();
        assert!((300.0..3000.0).contains(&month), "month cost {month}");

        let ctx_max: f64 = s.read(|c| Ok(c.query_row("SELECT MAX(ctx_used_pct) FROM session_status", [], |r| r.get(0))?)).unwrap();
        assert!(ctx_max >= 88.0, "near-limit session at {ctx_max}");
        assert!(count(s, &format!("SELECT count(*) FROM session_status WHERE ts >= {}", NOW - 5 * MINUTE_MS)) == 1);
        assert!(
            count(s, "SELECT count(*) FROM cache_misses WHERE causes LIKE '%system_prompt_changed%' OR causes LIKE '%tools_changed%'") > 0
        );
        assert!(count(s, "SELECT count(*) FROM cache_misses WHERE causes LIKE '%ttl_expired_5m%'") > 0);

        let latest = s.read(|c| limits::latest(c, LimitKind::FiveHour)).unwrap().unwrap();
        assert!(NOW - latest.ts < 15 * MINUTE_MS);
        assert!((70.0..=85.0).contains(&latest.used_pct), "5h at {}", latest.used_pct);
        let window = s.read(|c| limits::window_samples(c, LimitKind::FiveHour, latest.resets_at)).unwrap();
        let f = limits::forecast(LimitKind::FiveHour, &window, NOW);
        assert!(f.will_exhaust, "{f:?}");
        let resets = count(s, "SELECT count(DISTINCT resets_at) FROM limit_samples WHERE kind = 'five_hour'");
        assert!(resets > 15, "{resets} five-hour windows");
        assert!(count(s, "SELECT count(DISTINCT resets_at) FROM limit_samples WHERE kind = 'seven_day'") >= 4);
        let week = s.read(|c| limits::latest(c, LimitKind::SevenDay)).unwrap().unwrap();
        assert!(week.used_pct < 90.0, "7d at {}", week.used_pct);

        let weekday_req = count(s, "SELECT count(*) FROM requests WHERE ((ts + 3600000) / 86400000 + 3) % 7 < 5");
        let weekend_req = count(s, "SELECT count(*) FROM requests WHERE ((ts + 3600000) / 86400000 + 3) % 7 >= 5");
        assert!(weekend_req * 4 < weekday_req, "weekend {weekend_req} vs weekday {weekday_req}");
        let daily: Vec<f64> = s
            .read(|c| {
                let mut st = c.prepare("SELECT SUM(cost_usd) FROM requests GROUP BY (ts + 3600000) / 86400000")?;
                let v = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<f64>>>()?;
                Ok(v)
            })
            .unwrap();
        let mut sorted = daily.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let median = sorted[sorted.len() / 2];
        assert!(sorted[sorted.len() - 1] > median * 2.0, "one spike day: max {} median {median}", sorted[sorted.len() - 1]);

        let budgets: Vec<(f64, String)> = s
            .read(|c| {
                let mut st = c.prepare("SELECT amount_usd, scope FROM budgets ORDER BY id")?;
                let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap();
        let month_start = ctx().month_start(NOW);
        let spent: f64 = s
            .read(|c| {
                Ok(c.query_row(
                    "SELECT SUM(r.cost_usd) FROM requests r JOIN projects p ON p.id = r.project_id WHERE r.ts >= ?1 AND p.client = ?2",
                    params![month_start, NORTHWIND],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let near = spent / budgets[1].0 * 100.0;
        assert!((70.0..80.0).contains(&near), "client budget at {near}%");
        let settings = Settings::load(s).unwrap();
        assert!(settings.onboarding_done && settings.plan.kind == PlanKind::Max20x);
        assert!(crate::audit::verify(s).unwrap());
    }

    #[test]
    fn scripted_hours_hold_for_any_now_and_seed() {
        for seed in 1..4u64 {
            for shift in [0, 13 * HOUR_MS, 82 * HOUR_MS, 5 * DAY_MS + 2 * HOUR_MS] {
                let now = NOW + shift;
                let c = Ctx { now, tz_offset_min: 60, week_start: 0 };
                let t = TestStore::new();
                populate_in(&t.store, &c, 21, seed).unwrap();
                let s = &t.store;
                let label = format!("seed {seed} shift {shift}");
                for table in ["requests", "limit_samples", "session_events", "cache_misses", "session_status", "config_changes"] {
                    assert!(count(s, &format!("SELECT COALESCE(MAX(ts), 0) FROM {table}")) <= now, "{label}: {table} after now");
                }
                assert!(count(s, "SELECT MAX(last_at) FROM sessions") <= now, "{label}");
                let latest = s.read(|c| limits::latest(c, LimitKind::FiveHour)).unwrap().unwrap();
                assert!(now - latest.ts < 15 * MINUTE_MS && (70.0..=85.0).contains(&latest.used_pct), "{label}: {latest:?}");
                let window = s.read(|c| limits::window_samples(c, LimitKind::FiveHour, latest.resets_at)).unwrap();
                assert!(limits::forecast(LimitKind::FiveHour, &window, now).will_exhaust, "{label}");
                let ctx_max: f64 = s.read(|c| Ok(c.query_row("SELECT MAX(ctx_used_pct) FROM session_status", [], |r| r.get(0))?)).unwrap();
                assert!((88.0..100.0).contains(&ctx_max), "{label}: ctx {ctx_max}");
                let (amount, spent): (f64, f64) = s
                    .read(|cn| {
                        Ok(cn.query_row(
                            "SELECT b.amount_usd, (SELECT COALESCE(SUM(r.cost_usd), 0) FROM requests r JOIN projects p ON p.id = r.project_id
                                                   WHERE r.ts >= ?1 AND p.client = b.scope_value)
                             FROM budgets b WHERE b.scope = 'client'",
                            params![c.month_start(now)],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )?)
                    })
                    .unwrap();
                let pct = spent / amount * 100.0;
                assert!((70.0..80.0).contains(&pct), "{label}: budget {pct}% of {amount}");
            }
        }
    }

    #[test]
    fn refuses_a_database_with_other_data() {
        let t = TestStore::new();
        t.store
            .write(|tx| {
                Ok(tx.execute(
                    "INSERT INTO budgets (name, scope, period, amount_usd, created_at) VALUES ('b', 'global', 'month', 1, 0)",
                    [],
                )?)
            })
            .unwrap();
        assert!(populate_in(&t.store, &ctx(), 3, 1).is_err());
        assert_eq!(count(&t.store, "SELECT count(*) FROM budgets"), 1);
    }
}

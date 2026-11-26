#![allow(clippy::unwrap_used)]

use kanjo_analytics as an;
use kanjo_core::api::{Breakdown, BreakdownRow, Dimension, Granularity, PlanAction, Query, Usage};
use kanjo_core::limits::{insert_sample, LimitKind, LimitSample};
use kanjo_core::model::{RequestRow, ToolKind, ToolUse};
use kanjo_core::pricing::PriceTable;
use kanjo_core::settings::{PlanKind, Settings};
use kanjo_core::testkit::{request, TestStore};
use kanjo_core::time::{DAY_MS, HOUR_MS, MINUTE_MS};
use kanjo_core::Ctx;

const M: i64 = MINUTE_MS;
const H: i64 = HOUR_MS;
/// 2026-11-25T14:30:00Z: Wednesday 15:30 in a UTC+1 zone.
const NOW: i64 = 1_795_617_000_000;
const OPUS: &str = "claude-opus-5-5";
const SONNET: &str = "claude-sonnet-5-5";

fn ctx() -> Ctx {
    Ctx { now: NOW, tz_offset_min: 60, week_start: 0 }
}

/// Local wall-clock time (UTC+1) to epoch ms.
fn local(y: i16, mo: i8, d: i8, h: i8, mi: i8) -> i64 {
    kanjo_core::time::utc_millis(jiff::civil::date(y, mo, d).at(h, mi, 0, 0)) - H
}

struct Fx {
    t: TestStore,
    n: std::cell::Cell<u32>,
}

impl Fx {
    fn new() -> Fx {
        Fx { t: TestStore::new(), n: std::cell::Cell::new(0) }
    }

    fn project(&self, key: &str) -> i64 {
        self.t.project(key)
    }

    fn row(&self, project: i64, session: &str, ts: i64, model: &str, input: i64, output: i64) -> RequestRow {
        self.n.set(self.n.get() + 1);
        let mut r = request(&format!("msg_{}", self.n.get()), session, ts, model, input, output);
        r.project_id = project;
        r
    }

    fn add(&self, project: i64, session: &str, ts: i64, model: &str, input: i64, output: i64) -> RequestRow {
        let r = self.row(project, session, ts, model, input, output);
        self.t.insert(&r);
        r
    }

    fn sample(&self, kind: LimitKind, ts: i64, pct: f64, resets_at: i64) {
        let s = LimitSample { ts, used_pct: pct, resets_at, used_usd: None, limit_usd: None, period: None };
        self.t.store.write(|tx| insert_sample(tx, kind, &s)).unwrap();
    }

    fn settings(&self, f: impl FnOnce(&mut Settings)) {
        let mut s = Settings::load(&self.t.store).unwrap();
        f(&mut s);
        s.save(&self.t.store).unwrap();
    }
}

fn cost(r: &RequestRow) -> f64 {
    PriceTable::embedded().cost(&r.model, &r.tokens).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn local_day_week_and_month_boundaries() {
    let fx = Fx::new();
    let p = fx.project("alpha");
    fx.add(p, "s1", local(2026, 11, 24, 23, 30), OPUS, 100, 10);
    fx.add(p, "s1", local(2026, 11, 25, 0, 30), OPUS, 200, 20);
    let q = Query { from: Some(local(2026, 11, 24, 0, 0)), ..Query::default() };
    let day = an::usage_report(&fx.t.store, &ctx(), &q).unwrap();
    let labels: Vec<_> = day.buckets.iter().map(|b| (b.label.as_str(), b.usage.requests)).collect();
    assert_eq!(labels, vec![("2026-11-24", 1), ("2026-11-25", 1)]);
    assert_eq!(day.buckets[1].start, local(2026, 11, 25, 0, 0));

    fx.add(p, "s2", local(2026, 11, 22, 23, 30), OPUS, 100, 10);
    fx.add(p, "s2", local(2026, 11, 23, 0, 30), OPUS, 100, 10);
    let q = Query { from: Some(local(2026, 11, 16, 0, 0)), group: Some(Granularity::Week), ..Query::default() };
    let week = an::usage_report(&fx.t.store, &ctx(), &q).unwrap();
    let labels: Vec<_> = week.buckets.iter().map(|b| (b.label.as_str(), b.usage.requests)).collect();
    assert_eq!(labels, vec![("2026-W47", 1), ("2026-W48", 3)]);

    fx.add(p, "s3", local(2026, 10, 31, 23, 30), OPUS, 100, 10);
    fx.add(p, "s3", local(2026, 11, 1, 0, 30), OPUS, 100, 10);
    let q = Query { from: Some(local(2026, 10, 1, 0, 0)), group: Some(Granularity::Month), ..Query::default() };
    let month = an::usage_report(&fx.t.store, &ctx(), &q).unwrap();
    let labels: Vec<_> = month.buckets.iter().map(|b| (b.label.as_str(), b.usage.requests)).collect();
    assert_eq!(labels, vec![("2026-10", 1), ("2026-11", 5)]);

    let hourly = an::usage_report(&fx.t.store, &ctx(), &Query { group: Some(Granularity::Hour), ..q }).unwrap();
    assert!(hourly.buckets.iter().any(|b| b.label == "2026-11-01 00:00" && b.usage.requests == 1));
}

#[test]
fn period_summaries_compare_like_with_like() {
    let fx = Fx::new();
    let p = fx.project("alpha");
    fx.add(p, "t", local(2026, 11, 25, 10, 0), OPUS, 100, 10);
    fx.add(p, "y", local(2026, 11, 24, 10, 0), OPUS, 100, 10);
    fx.add(p, "y", local(2026, 11, 24, 15, 10), OPUS, 100, 10);
    fx.add(p, "y", local(2026, 11, 24, 15, 45), OPUS, 100, 10);
    fx.add(p, "w", local(2026, 11, 18, 15, 0), OPUS, 100, 10);
    fx.add(p, "w", local(2026, 11, 18, 16, 0), OPUS, 100, 10);
    fx.add(p, "m", local(2026, 10, 25, 15, 0), OPUS, 100, 10);
    fx.add(p, "m", local(2026, 10, 25, 16, 0), OPUS, 100, 10);
    let o = an::overview(&fx.t.store, &ctx()).unwrap();
    assert_eq!(o.today.from, local(2026, 11, 25, 0, 0));
    assert_eq!((o.today.usage.requests, o.today.previous.requests), (1, 2));
    assert!(close(o.today.delta_pct.unwrap(), -50.0));
    // This week: Mon 23 to now (today and yesterday, 4 requests); last week up to Wed 15:30.
    assert_eq!(o.week.from, local(2026, 11, 23, 0, 0));
    assert_eq!((o.week.usage.requests, o.week.previous.requests), (4, 1));
    // This month: Nov 1 to now; October up to the 25th, 15:30.
    assert_eq!((o.month.usage.requests, o.month.previous.requests), (6, 1));
    assert_eq!(o.daily.len(), 30);
    assert_eq!(o.hourly.len(), 24);
    assert_eq!(o.hourly.iter().map(|b| b.usage.requests).sum::<i64>(), 1);

    let c = an::compare(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!((c.current.usage.requests, c.previous.usage.requests), (4, 1));
    assert_eq!(c.current.label, "2026-W48");
    assert!(c.drivers.iter().any(|d| d.dimension == Dimension::Project && d.label == "alpha"));
    let c1 = an::compare(&fx.t.store, &ctx(), &Query { offset: Some(1), ..Query::default() }).unwrap();
    assert_eq!(c1.current.usage.requests, 2);
    let cm = an::compare(&fx.t.store, &ctx(), &Query { period: Some("month".into()), ..Query::default() }).unwrap();
    assert_eq!((cm.current.label.as_str(), cm.previous.usage.requests), ("2026-11", 1));
    assert!(an::compare(&fx.t.store, &ctx(), &Query { period: Some("year".into()), ..Query::default() }).is_err());
}

#[test]
fn anchored_and_inferred_blocks() {
    let fx = Fx::new();
    let p = fx.project("alpha");
    // now = 14:30Z. Anchored window [11:30Z, 16:30Z).
    let reset = NOW + 2 * H;
    fx.sample(LimitKind::FiveHour, NOW - 2 * H, 20.0, reset);
    fx.sample(LimitKind::FiveHour, NOW - 30 * M, 55.0, reset + 30_000);
    let a = fx.add(p, "s", NOW - 10 * H, OPUS, 1000, 100); // 04:30Z, inferred [04:00, 09:00)
    let b = fx.add(p, "s", NOW - 8 * H, OPUS, 1000, 100);
    let c = fx.add(p, "s", NOW - 4 * H, SONNET, 1000, 100); // 10:30Z, inferred [10:00, 11:30)
    let d = fx.add(p, "s", NOW - 3 * H - 20 * M, SONNET, 1000, 100); // 11:10Z, same inferred block
    let e = fx.add(p, "s", NOW - 3 * H + 15 * M, OPUS, 1000, 100); // 11:45Z, anchored
    let f = fx.add(p, "s", NOW - H, OPUS, 1000, 100);
    let blocks = an::blocks(&fx.t.store, &ctx(), &Query { from: Some(NOW - DAY_MS), ..Query::default() }).unwrap();
    let shape: Vec<_> = blocks.iter().map(|b| (b.start, b.end, b.anchored, b.usage.requests)).collect();
    let hour = |ts: i64| ts.div_euclid(H) * H;
    assert_eq!(
        shape,
        vec![(hour(a.ts), hour(a.ts) + 5 * H, false, 2), (hour(c.ts), reset - 5 * H, false, 2), (reset - 5 * H, reset, true, 2)]
    );
    assert!(close(blocks[0].usage.cost_usd, cost(&a) + cost(&b)));
    assert!(close(blocks[1].usage.cost_usd, cost(&c) + cost(&d)));
    let active = &blocks[2];
    assert!(active.is_active && !blocks[0].is_active);
    assert_eq!(active.peak_pct, Some(55.0));
    let spent = cost(&e) + cost(&f);
    assert!(close(active.usage.cost_usd, spent));
    let burn = active.burn_usd_per_hour.unwrap();
    assert!(close(burn, spent / (NOW - e.ts) as f64 * H as f64));
    assert!(close(active.projected_usd.unwrap(), spent + burn * 2.0));
    assert!(blocks[0].burn_usd_per_hour.is_none());
    assert_eq!(blocks[1].by_model[0].key, SONNET);

    let o = an::overview(&fx.t.store, &ctx()).unwrap();
    assert_eq!(o.active_block.map(|b| (b.start, b.anchored)), Some((reset - 5 * H, true)));
    assert!(o.limits.five_hour.is_some() && o.limits.estimate.is_none());

    let json = an::export::ccusage_blocks(&fx.t.store, &ctx(), &Query { from: Some(NOW - DAY_MS), ..Query::default() }).unwrap();
    let list = json["blocks"].as_array().unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[2]["isActive"], true);
    assert_eq!(list[2]["entries"], 2);
    assert!(list[2]["burnRate"]["costPerHour"].as_f64().unwrap() > 0.0);
    assert_eq!(list[2]["projection"]["remainingMinutes"], 120);
    assert!(list[0]["burnRate"].is_null());
    assert_eq!(list[0]["tokenCounts"]["inputTokens"], 2000);
    assert_eq!(list[0]["actualEndTime"], "2026-11-25T06:30:00.000Z");
    assert!(list[0].get("usageLimitResetTime").is_none());
    assert_eq!(list[2]["usageLimitResetTime"], "2026-11-25T16:30:00.000Z");
}

#[test]
fn block_chain_does_not_depend_on_the_range_start() {
    let fx = Fx::new();
    let p = fx.project("alpha");
    for i in 0..12 {
        fx.add(p, "s", NOW - 30 * H + i * 2 * H + 10 * M, OPUS, 100, 10);
    }
    let wide = an::blocks(&fx.t.store, &ctx(), &Query { from: Some(NOW - 2 * DAY_MS), ..Query::default() }).unwrap();
    let narrow = an::blocks(&fx.t.store, &ctx(), &Query { from: Some(NOW - 12 * H), ..Query::default() }).unwrap();
    let starts = |v: &[kanjo_core::api::Block]| v.iter().map(|b| b.start).collect::<Vec<_>>();
    assert!(starts(&wide).ends_with(&starts(&narrow)));
    assert!(wide.windows(2).all(|w| w[0].end <= w[1].start));
}

#[test]
fn limits_expired_stale_and_estimate() {
    let fx = Fx::new();
    let p = fx.project("alpha");
    let r = fx.add(p, "s", NOW - 2 * DAY_MS, OPUS, 10_000, 1000);
    fx.sample(LimitKind::FiveHour, NOW - 6 * H, 70.0, NOW - H);
    fx.sample(LimitKind::SevenDay, NOW - 20 * M, 40.0, NOW + 2 * DAY_MS);
    let l = an::limits(&fx.t.store, &ctx()).unwrap();
    assert!(l.five_hour.is_none(), "expired window is omitted");
    let w = l.seven_day.unwrap();
    assert!(w.stale);
    assert!(close(w.daily_budget_pct.unwrap(), 30.0));
    assert!(close(w.usd_in_window, cost(&r)));
    assert!(close(w.usd_per_pct.unwrap(), cost(&r) / 40.0));
    assert!(l.estimate.is_none(), "too few blocks for a calibration");

    fx.sample(LimitKind::SevenDay, NOW - 5 * M, 41.0, NOW + 2 * DAY_MS);
    assert!(!an::limits(&fx.t.store, &ctx()).unwrap().seven_day.unwrap().stale);

    for day in 3..9 {
        fx.add(p, "old", NOW - day * DAY_MS, OPUS, 1000 * day, 100);
    }
    let cur = fx.add(p, "now", NOW - 20 * M, OPUS, 3000, 100);
    let est = an::limits(&fx.t.store, &ctx()).unwrap().estimate.unwrap();
    assert_eq!(est.blocks_observed, 7);
    assert!(close(est.current_block_usd, cost(&cur)));
    assert!(est.p90_block_usd > 0.0 && close(est.current_block_pct, cost(&cur) / est.p90_block_usd * 100.0));

    let detail = an::limits_detail(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!(detail.history_5h.len(), 1);
    assert_eq!(detail.history_7d.len(), 2);
    assert_eq!(detail.usd_per_pct_weekly.len(), 1);
    assert!(close(detail.usd_per_pct_weekly[0].pct, 41.0));
    assert!(detail.blocks.iter().any(|b| b.is_active));
}

#[test]
fn redaction_hides_names_paths_branches_and_titles() {
    let fx = Fx::new();
    let p = fx.project("acme-secret");
    let mut r = fx.row(p, "sess-1", NOW - H, OPUS, 100, 10);
    r.git_branch = Some("feature/hidden".into());
    fx.t.insert(&r);
    fx.t.store.write(|tx| Ok(tx.execute("UPDATE sessions SET title = 'Private title' WHERE id = 'sess-1'", [])?)).unwrap();
    let visible = an::sessions(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!(visible.items[0].title.as_deref(), Some("Private title"));
    assert_eq!(an::search(&fx.t.store, &ctx(), &Query { q: Some("secret".into()), ..Query::default() }).unwrap().len(), 2);

    fx.settings(|s| s.privacy.redact_paths = true);
    let alias = kanjo_core::store::write::project_alias("acme-secret");
    let list = an::sessions(&fx.t.store, &ctx(), &Query::default()).unwrap();
    let s = &list.items[0];
    assert_eq!((s.project.as_str(), s.title.as_deref(), s.git_branch.as_deref()), (alias.as_str(), None, None));
    let projects = an::projects(&fx.t.store, &ctx()).unwrap();
    assert_eq!((projects[0].name.as_str(), projects[0].path.as_deref()), (alias.as_str(), None));
    let by_branch = an::breakdown(&fx.t.store, &ctx(), &Query { by: Some(Dimension::Branch), ..Query::default() }).unwrap();
    assert!(by_branch.rows[0].key.starts_with("branch-") && !by_branch.rows[0].label.contains("hidden"));
    for text in ["secret", "Private", "hidden"] {
        let q = Query { q: Some(text.into()), ..Query::default() };
        assert!(an::search(&fx.t.store, &ctx(), &q).unwrap().is_empty(), "{text} must not match");
        assert_eq!(an::sessions(&fx.t.store, &ctx(), &q).unwrap().total, 0);
    }
    let found = an::search(&fx.t.store, &ctx(), &Query { q: Some(alias.clone()), ..Query::default() }).unwrap();
    assert!(found.iter().any(|f| f.kind == "project" && f.sublabel.is_none()));
    let csv = an::export::sessions_csv(&list);
    assert!(csv.contains(&alias) && !csv.contains("acme") && !csv.contains("hidden") && !csv.contains("Private"));
    let turns = an::top_turns(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!(turns[0].project, alias);
    let detail = an::session_detail(&fx.t.store, &ctx(), "sess-1").unwrap().unwrap();
    assert!(detail.summary.git_branch.is_none() && detail.summary.title.is_none());
    let json = an::export::ccusage_session(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!(json["sessions"][0]["projectPath"], alias.as_str());
    let o = an::overview(&fx.t.store, &ctx()).unwrap();
    assert_eq!(o.top_projects[0].label, alias);
}

#[test]
fn unpriced_requests_are_counted_not_costed() {
    let fx = Fx::new();
    let p = fx.project("alpha");
    let priced = fx.add(p, "s", NOW - 2 * H, OPUS, 100, 10);
    fx.add(p, "s", NOW - H, "mystery-model", 5000, 500);
    let rep = an::usage_report(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!((rep.total.requests, rep.total.unpriced_requests), (2, 1));
    assert!(close(rep.total.cost_usd, cost(&priced)));
    let status = an::data_status(&fx.t.store, &ctx()).unwrap();
    assert_eq!(status.unpriced_models, vec!["mystery-model".to_string()]);
    let turns = an::top_turns(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!((turns[0].model.as_str(), turns[1].cost_usd), (OPUS, None));
    let json = an::export::ccusage_daily(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!(json["totals"]["unpricedModels"][0], "mystery-model");
    let day = &json["daily"][0];
    let mystery = day["modelBreakdowns"].as_array().unwrap().iter().find(|m| m["modelName"] == "mystery-model").unwrap();
    assert_eq!(mystery["missingPricing"], true);
    assert_eq!(mystery["cost"], 0.0);
}

#[test]
fn breakdowns_attribute_tools_agents_and_clients() {
    let fx = Fx::new();
    let a = fx.project("alpha");
    let b = fx.project("beta");
    an::update_project(&fx.t.store, &ctx(), a, &kanjo_core::api::ProjectPatch { client: Some("Initech".into()), ..Default::default() })
        .unwrap();
    let tool = |id: &str, kind, name: &str| ToolUse { use_id: id.into(), kind, name: name.into() };
    let mut r1 = fx.row(a, "s1", NOW - 3 * H, OPUS, 1000, 100);
    r1.tools = vec![tool("t1", ToolKind::Tool, "Bash"), tool("t2", ToolKind::Tool, "Bash"), tool("t3", ToolKind::Tool, "Read")];
    fx.t.insert(&r1);
    let mut r2 = fx.row(b, "s2", NOW - 2 * H, SONNET, 1000, 100);
    r2.agent_type = Some("explorer".into());
    r2.is_sidechain = true;
    r2.tools = vec![tool("t4", ToolKind::Tool, "Bash"), tool("t5", ToolKind::Mcp, "tracker"), tool("t6", ToolKind::Skill, "commit")];
    fx.t.insert(&r2);
    let q = |by| Query { by: Some(by), ..Query::default() };
    let tools = an::breakdown(&fx.t.store, &ctx(), &q(Dimension::Tool)).unwrap();
    assert_eq!(tools.confidence, kanjo_core::api::Confidence::Estimate);
    let bash = tools.rows.iter().find(|r| r.key == "Bash").unwrap();
    assert_eq!(bash.usage.requests, 2);
    assert!(close(bash.share, 1.0));
    assert!(tools.rows.iter().map(|r| r.share).sum::<f64>() > 1.0);
    assert_eq!(an::breakdown(&fx.t.store, &ctx(), &q(Dimension::Mcp)).unwrap().rows[0].key, "tracker");
    assert_eq!(an::breakdown(&fx.t.store, &ctx(), &q(Dimension::Skill)).unwrap().rows[0].usage.requests, 1);
    let agents = an::breakdown(&fx.t.store, &ctx(), &q(Dimension::Agent)).unwrap();
    let mut keys: Vec<_> = agents.rows.iter().map(|r| r.key.as_str()).collect();
    keys.sort();
    assert_eq!(keys, vec!["explorer", "main"]);
    let clients = an::breakdown(&fx.t.store, &ctx(), &q(Dimension::Client)).unwrap();
    let mut keys: Vec<_> = clients.rows.iter().map(|r| r.key.as_str()).collect();
    keys.sort();
    assert_eq!(keys, vec!["Initech", "unassigned"]);
    assert!(close(clients.rows.iter().map(|r| r.share).sum::<f64>(), 1.0));
    let main_only = an::usage_report(&fx.t.store, &ctx(), &Query { agent: Some("main".into()), ..Query::default() }).unwrap();
    assert_eq!(main_only.total.requests, 1);
    let unassigned = an::usage_report(&fx.t.store, &ctx(), &Query { client: Some("unassigned".into()), ..Query::default() }).unwrap();
    assert!(close(unassigned.total.cost_usd, cost(&r2)));
    let sessions = an::breakdown(&fx.t.store, &ctx(), &q(Dimension::Session)).unwrap();
    assert_eq!(sessions.rows.len(), 2);

    let detail = an::session_detail(&fx.t.store, &ctx(), "s2").unwrap().unwrap();
    assert!(close(detail.summary.subagent_cost_usd, cost(&r2)));
    assert_eq!(detail.by_agent[0].key, "explorer");
    let mut tool_keys: Vec<_> = detail.tools.iter().map(|t| t.key.as_str()).collect();
    tool_keys.sort();
    assert_eq!(tool_keys, vec!["mcp:tracker", "skill:commit", "tool:Bash"]);
    assert_eq!(detail.timeline.iter().map(|b| b.usage.requests).sum::<i64>(), 1);
    assert_eq!(detail.top_turns[0].tools, vec!["Bash".to_string()]);
    assert!(an::session_detail(&fx.t.store, &ctx(), "nope").unwrap().is_none());
    let turns = an::top_turns(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!(turns.iter().find(|t| t.session_id == "s1").unwrap().tools, vec!["Bash".to_string(), "Read".to_string()]);
}

#[test]
fn sessions_sort_filter_and_paginate() {
    let fx = Fx::new();
    let a = fx.project("alpha");
    let b = fx.project("beta");
    fx.add(a, "aaa-long", NOW - 5 * H, OPUS, 100, 10);
    fx.add(a, "aaa-long", NOW - 4 * H, OPUS, 100, 10);
    fx.add(b, "bbb-big", NOW - 3 * H, OPUS, 100_000, 10_000);
    fx.add(b, "bbb-big", NOW - 3 * H + M, SONNET, 100, 10);
    fx.add(b, "ccc-now", NOW - 2 * M, SONNET, 10, 1);
    let ids = |sort: &str| {
        let q = Query { sort: Some(sort.into()), ..Query::default() };
        an::sessions(&fx.t.store, &ctx(), &q).unwrap().items.into_iter().map(|s| s.id).collect::<Vec<_>>()
    };
    assert_eq!(ids("cost"), vec!["bbb-big", "aaa-long", "ccc-now"]);
    assert_eq!(ids("recent"), vec!["ccc-now", "bbb-big", "aaa-long"]);
    assert_eq!(ids("duration")[0], "aaa-long");
    assert!(an::sessions(&fx.t.store, &ctx(), &Query { sort: Some("x; DROP".into()), ..Query::default() }).is_err());
    let page = an::sessions(&fx.t.store, &ctx(), &Query { limit: Some(1), offset: Some(1), ..Query::default() }).unwrap();
    assert_eq!((page.total, page.items[0].id.as_str()), (3, "aaa-long"));
    let big = an::sessions(&fx.t.store, &ctx(), &Query { q: Some("bbb".into()), ..Query::default() }).unwrap();
    assert_eq!(big.items[0].main_model.as_deref(), Some(OPUS));
    assert_eq!(big.items[0].models, vec![OPUS.to_string(), SONNET.to_string()]);
    assert!(!big.items[0].is_active);
    let by_project = an::sessions(&fx.t.store, &ctx(), &Query { q: Some("alp".into()), ..Query::default() }).unwrap();
    assert_eq!(by_project.total, 1);
    let percent = an::sessions(&fx.t.store, &ctx(), &Query { q: Some("%".into()), ..Query::default() }).unwrap();
    assert_eq!(percent.total, 0, "LIKE wildcards are escaped");
    let o = an::overview(&fx.t.store, &ctx()).unwrap();
    assert_eq!(o.active_sessions.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["ccc-now"]);
    assert!(o.active_sessions[0].is_active && o.burn.active);
}

#[test]
fn projects_update_and_heatmap() {
    let fx = Fx::new();
    let a = fx.project("alpha");
    fx.add(a, "s", local(2026, 11, 23, 9, 15), OPUS, 100, 10);
    fx.add(a, "s", NOW - 40 * DAY_MS, OPUS, 100, 10);
    let p = an::update_project(
        &fx.t.store,
        &ctx(),
        a,
        &kanjo_core::api::ProjectPatch { name: Some(" Renamed ".into()), client: Some("Acme".into()) },
    )
    .unwrap();
    assert_eq!((p.name.as_str(), p.client.as_deref(), p.path.as_deref()), ("Renamed", Some("Acme"), Some("/work/alpha")));
    assert_eq!((p.usage_30d.requests, p.usage_total.requests, p.sessions), (1, 2, 1));
    let p =
        an::update_project(&fx.t.store, &ctx(), a, &kanjo_core::api::ProjectPatch { client: Some(String::new()), ..Default::default() })
            .unwrap();
    assert!(p.client.is_none());
    assert!(an::update_project(&fx.t.store, &ctx(), 999, &Default::default()).is_err());
    assert!(an::update_project(&fx.t.store, &ctx(), a, &kanjo_core::api::ProjectPatch { name: Some(" ".into()), ..Default::default() })
        .is_err());

    let h = an::heatmap(&fx.t.store, &ctx(), &Query { from: Some(local(2026, 11, 1, 0, 0)), ..Query::default() }).unwrap();
    assert_eq!(h.cells.len(), 168);
    let hot: Vec<_> = h.cells.iter().filter(|c| c.requests > 0).map(|c| (c.dow, c.hour)).collect();
    assert_eq!(hot, vec![(0, 9)]);
    assert_eq!(h.days.len(), 25);
    assert_eq!(h.days.iter().find(|d| d.requests > 0).unwrap().date, "2026-11-23");
}

#[test]
fn plan_fit_recommendations() {
    let fx = Fx::new();
    let p = fx.project("alpha");
    assert_eq!(an::plan_fit(&fx.t.store, &ctx()).unwrap().reason, "no_plan");
    fx.settings(|s| s.plan.kind = PlanKind::Max20x);
    fx.add(p, "s", NOW - DAY_MS, OPUS, 1000, 100);
    let fit = an::plan_fit(&fx.t.store, &ctx()).unwrap();
    assert_eq!((fit.action, fit.reason.as_str()), (PlanAction::Unknown, "no_history"));
    assert_eq!(fit.months.len(), 6);
    assert_eq!((fit.months[0].month.as_str(), fit.months[5].month.as_str()), ("2026-06", "2026-11"));

    fx.add(p, "s", NOW - 20 * DAY_MS, OPUS, 1000, 100);
    for i in 0..3 {
        fx.sample(LimitKind::FiveHour, NOW - (i + 2) * DAY_MS, 30.0, NOW - (i + 2) * DAY_MS + H);
    }
    fx.sample(LimitKind::SevenDay, NOW - 2 * DAY_MS, 20.0, NOW + DAY_MS);
    let fit = an::plan_fit(&fx.t.store, &ctx()).unwrap();
    assert_eq!((fit.action, fit.target, fit.reason.as_str()), (PlanAction::Downgrade, Some(PlanKind::Max5x), "low_utilisation"));
    assert_eq!(fit.max_seven_day_pct, Some(20.0));

    fx.settings(|s| s.plan.kind = PlanKind::Pro);
    for i in 0..3 {
        fx.sample(LimitKind::FiveHour, NOW - DAY_MS + i * 2 * H, 95.0, NOW - DAY_MS + i * 2 * H + H);
    }
    let fit = an::plan_fit(&fx.t.store, &ctx()).unwrap();
    assert_eq!((fit.action, fit.target, fit.blocks_near_limit), (PlanAction::Upgrade, Some(PlanKind::Max5x), 3));

    let fresh = Fx::new();
    let q = fresh.project("beta");
    fresh.add(q, "s", NOW - 20 * DAY_MS, OPUS, 1000, 100);
    fresh.settings(|s| s.plan.kind = PlanKind::Max5x);
    let fit = an::plan_fit(&fresh.t.store, &ctx()).unwrap();
    assert_eq!((fit.action, fit.target, fit.reason.as_str()), (PlanAction::Downgrade, Some(PlanKind::Api), "api_cheaper"));
    assert!(fit.roi.unwrap() < 1.0);
}

#[test]
fn csv_quotes_and_neutralises_formulas() {
    let row = |key: &str, label: &str, cost: f64| BreakdownRow {
        key: key.into(),
        label: label.into(),
        usage: Usage { cost_usd: cost, requests: 1, ..Usage::default() },
        share: 0.5,
    };
    let b = Breakdown {
        by: Dimension::Project,
        from: 0,
        to: 0,
        rows: vec![
            row("1", "=HYPERLINK(\"http://x\")", -3.5),
            row("a,b", "line\nbreak", 1.0),
            row("@x", "+1", 0.0),
            row("-", "plain", 2.0),
        ],
        total: Usage::default(),
        confidence: kanjo_core::api::Confidence::LowerBound,
    };
    let csv = an::export::breakdown_csv(&b);
    let lines: Vec<&str> = csv.split("\r\n").collect();
    assert_eq!(
        lines[0],
        "dimension,key,label,requests,unpriced_requests,input_tokens,output_tokens,cache_read_tokens,cache_write_5m_tokens,cache_write_1h_tokens,total_tokens,cost_usd,share"
    );
    assert_eq!(lines[1], "project,1,\"'=HYPERLINK(\"\"http://x\"\")\",1,0,0,0,0,0,0,0,-3.5,0.5");
    assert_eq!(lines[2], "project,\"a,b\",\"line\nbreak\",1,0,0,0,0,0,0,0,1,0.5");
    assert_eq!(lines[3], "project,'@x,'+1,1,0,0,0,0,0,0,0,0,0.5");
    assert_eq!(lines[4], "project,'-,plain,1,0,0,0,0,0,0,0,2,0.5");
    assert!(csv.ends_with("\r\n"));
}

#[test]
fn ccusage_daily_shape() {
    let fx = Fx::new();
    let p = fx.project("alpha");
    let mut r = fx.row(p, "s", local(2026, 11, 24, 12, 0), OPUS, 100, 10);
    r.tokens.cache_read = 1000;
    r.tokens.cache_write_5m = 50;
    r.tokens.cache_write_1h = 25;
    fx.t.insert(&r);
    let s = fx.add(p, "s", local(2026, 11, 24, 13, 0), SONNET, 10, 1);
    let json = an::export::ccusage_daily(&fx.t.store, &ctx(), &Query::default()).unwrap();
    let day = &json["daily"][0];
    assert_eq!(json["daily"].as_array().unwrap().len(), 1, "only days with usage");
    assert_eq!(day["date"], "2026-11-24");
    assert_eq!((day["inputTokens"].as_i64(), day["outputTokens"].as_i64()), (Some(110), Some(11)));
    assert_eq!((day["cacheCreationTokens"].as_i64(), day["cacheReadTokens"].as_i64()), (Some(75), Some(1000)));
    assert_eq!(day["totalTokens"], 1196);
    assert!(close(day["totalCost"].as_f64().unwrap(), cost(&r) + cost(&s)));
    assert_eq!(day["modelsUsed"], serde_json::json!([OPUS, SONNET]));
    let mb = &day["modelBreakdowns"][0];
    let keys: Vec<&str> = mb.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    assert_eq!(keys.len(), 6);
    for k in ["modelName", "inputTokens", "outputTokens", "cacheCreationTokens", "cacheReadTokens", "cost"] {
        assert!(keys.contains(&k), "{k}");
    }
    assert_eq!(json["totals"]["totalTokens"], 1196);
    assert!(json["totals"].get("unpricedModels").is_none());
    let weekly = an::export::ccusage_weekly(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!(weekly["weekly"][0]["week"], "2026-11-23");
    let monthly = an::export::ccusage_monthly(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!(monthly["monthly"][0]["month"], "2026-11");
    let sessions = an::export::ccusage_session(&fx.t.store, &ctx(), &Query::default()).unwrap();
    let s0 = &sessions["sessions"][0];
    assert_eq!((s0["sessionId"].as_str(), s0["projectPath"].as_str()), (Some("s"), Some("/work/alpha")));
    assert_eq!(s0["lastActivity"], "2026-11-24T12:00:00.000Z");
    assert_eq!(s0["modelBreakdowns"].as_array().unwrap().len(), 2);
}

#[test]
fn reports_survive_the_purge_of_request_rows() {
    let fx = Fx::new();
    let p = fx.project("alpha");
    let mut r = fx.row(p, "s1", NOW - 3 * H - 20 * M, OPUS, 1000, 100);
    r.tools = vec![ToolUse { use_id: "t1".into(), kind: ToolKind::Tool, name: "Bash".into() }];
    fx.t.insert(&r);
    fx.add(p, "s1", NOW - 2 * DAY_MS, SONNET, 1000, 100);
    let before = an::usage_report(&fx.t.store, &ctx(), &Query::default()).unwrap().total;
    let blocks_before = an::blocks(&fx.t.store, &ctx(), &Query::default()).unwrap();
    // Retention purge, simulated: request rows (and their tool rows) go, aggregates and session totals stay.
    fx.t.store.write(|tx| Ok(tx.execute("DELETE FROM requests", [])?)).unwrap();
    let after = an::usage_report(&fx.t.store, &ctx(), &Query::default()).unwrap().total;
    assert_eq!(before, after);
    let o = an::overview(&fx.t.store, &ctx()).unwrap();
    assert_eq!((o.today.usage.requests, o.month.usage.requests), (1, 2));
    let blocks = an::blocks(&fx.t.store, &ctx(), &Query::default()).unwrap();
    let shape = |v: &[kanjo_core::api::Block]| v.iter().map(|b| (b.start, b.end, b.usage.requests)).collect::<Vec<_>>();
    assert_eq!(shape(&blocks), shape(&blocks_before));
    let s = an::sessions(&fx.t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!((s.items[0].usage.requests, s.items[0].models.len()), (2, 0));
    assert!(an::top_turns(&fx.t.store, &ctx(), &Query::default()).unwrap().is_empty());
    let tools = an::breakdown(&fx.t.store, &ctx(), &Query { by: Some(Dimension::Tool), ..Query::default() }).unwrap();
    assert!(tools.rows.is_empty());
    assert!(an::session_detail(&fx.t.store, &ctx(), "s1").unwrap().unwrap().timeline.is_empty());
}

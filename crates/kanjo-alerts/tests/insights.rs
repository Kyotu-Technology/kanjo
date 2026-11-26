#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use kanjo_alerts::insights;
use kanjo_core::api::{AnomalyKind, Confidence};
use kanjo_core::model::TokenCounts;
use kanjo_core::time::DAY_MS;

#[test]
fn cache_ratios_causes_and_breakers() {
    let t = store();
    let shop = t.project("shop");
    let blog = t.project("blog");
    let mut a = row(shop, "s1", NOW - 2 * DAY_MS, 0, 0);
    a.tokens = TokenCounts { input: 1000, output: 10, cache_read: 6000, cache_write_5m: 3000, cache_write_1h: 0 };
    t.insert(&a);
    let mut b = row(blog, "s2", NOW - 2 * DAY_MS, 0, 0);
    b.tokens = TokenCounts { input: 1000, output: 10, cache_read: 1000, ..Default::default() };
    t.insert(&b);
    let old = row(shop, "s0", NOW - 40 * DAY_MS, 50_000, 0);
    t.insert(&old);

    let t1 = NOW - DAY_MS;
    sql(
        &t,
        &format!(
            "INSERT INTO cache_misses (session_id, ts, causes) VALUES
               ('s1', {t1}, '[\"tools_changed\"]'),
               ('s1', {}, '[\"tools_changed\",\"system_changed\"]'),
               ('s2', {}, '[\"model_changed\"]'),
               ('s1', {}, '[\"tools_changed\"]');
             INSERT INTO config_changes (ts, kind, target, project_id, fingerprint) VALUES
               ({}, 'settings', 'project settings', {shop}, 'f1'),
               ({}, 'claude_md', 'user CLAUDE.md', NULL, 'f2'),
               ({}, 'plugins', 'project plugins', {blog}, 'f3'),
               ({}, 'tools', 'another-session', {shop}, 'f4');",
            t1 + 40 * MIN,
            t1 + 50 * MIN,
            NOW - 40 * DAY_MS,
            t1 - 10 * MIN,
            t1 + 35 * MIN,
            t1 - 5 * MIN,
            t1 - 3 * MIN,
        ),
    );

    let i = insights(&t.store, &ctx()).unwrap();
    let c = &i.cache;
    assert!((c.hit_ratio.unwrap() - 7000.0 / 12000.0).abs() < 1e-12);
    let rows: Vec<(&str, Option<f64>, i64, i64, i64)> =
        c.by_project.iter().map(|r| (r.label.as_str(), r.hit_ratio, r.cache_read, r.cache_write, r.input)).collect();
    assert_eq!(rows, vec![("shop", Some(0.6), 6000, 3000, 1000), ("blog", Some(0.5), 1000, 0, 1000)]);

    let causes: Vec<(&str, i64)> = c.miss_causes.iter().map(|m| (m.cause.as_str(), m.count)).collect();
    assert_eq!(causes, vec![("tools_changed", 2), ("model_changed", 1), ("system_changed", 1)]);

    let recent: Vec<(&str, &str, i64, Vec<&str>)> = c
        .recent_misses
        .iter()
        .map(|m| (m.session_id.as_str(), m.project.as_str(), m.ts, m.correlated_changes.iter().map(|ch| ch.target.as_str()).collect()))
        .collect();
    assert_eq!(
        recent,
        vec![
            ("s2", "blog", t1 + 50 * MIN, vec!["user CLAUDE.md"]),
            ("s1", "shop", t1 + 40 * MIN, vec!["user CLAUDE.md"]),
            ("s1", "shop", t1, vec!["project settings"]),
        ]
    );

    let breakers: Vec<(&str, i64, i64)> =
        c.breakers.iter().map(|b| (b.change.target.as_str(), b.misses_after, b.sessions_affected)).collect();
    assert_eq!(breakers, vec![("user CLAUDE.md", 2, 2), ("project settings", 1, 1)]);
}

#[test]
fn model_mix_reprices_subagents_and_short_turns() {
    let t = store();
    let p = t.project("shop");
    for _ in 0..10 {
        let mut r = row(p, "s1", NOW - DAY_MS, 1_000_000, 100_000);
        r.is_sidechain = true;
        r.agent_type = Some("Explore".into());
        t.insert(&r);
    }
    let mut small = row(p, "s1", NOW - DAY_MS, 100_000, 100);
    small.is_sidechain = true;
    small.agent_type = Some("Plan".into());
    t.insert(&small);
    for _ in 0..20 {
        t.insert(&row(p, "s1", NOW - DAY_MS, 500_000, 200));
    }
    for _ in 0..5 {
        t.insert(&row(p, "s1", NOW - DAY_MS, 500_000, 5_000));
    }
    let mut sonnet = row(p, "s1", NOW - DAY_MS, 5_000_000, 100_000);
    sonnet.model = "claude-sonnet-5-5".into();
    sonnet.agent_type = Some("Explore".into());
    t.insert(&sonnet);

    let mix = insights(&t.store, &ctx()).unwrap().model_mix;
    assert_eq!(mix.len(), 2, "{mix:?}");
    let s = &mix[0];
    assert_eq!(
        (s.code.as_str(), s.from_model.as_str(), s.to_model.as_str(), s.scope.as_str(), s.affected_requests),
        ("subagents_to_haiku", "claude-opus-5-5", "claude-haiku-5-5", "Explore", 10)
    );
    assert!((s.current_usd - 60.0).abs() < 1e-6 && (s.projected_usd - 1.5).abs() < 1e-6, "{s:?}");
    assert!((s.saving_usd - 58.5).abs() < 1e-6);
    assert_eq!(s.confidence, Confidence::Estimate);
    let s = &mix[1];
    assert_eq!(
        (s.code.as_str(), s.to_model.as_str(), s.scope.as_str(), s.affected_requests),
        ("short_turns_to_sonnet", "claude-sonnet-5-5", "main", 20)
    );
    assert!((s.current_usd - 40.08).abs() < 1e-6 && (s.saving_usd - 20.04).abs() < 1e-6, "{s:?}");
}

#[test]
fn context_warnings_and_anomalies_over_thirty_days() {
    let t = store();
    let p = t.project("shop");
    for d in 7..=20 {
        spend(&t, p, &format!("day-{d}"), NOW - d * DAY_MS, 5.0);
    }
    spend(&t, p, "pricey", NOW - 5 * DAY_MS, 40.0);
    for i in 0..70 {
        t.insert(&row(p, "loop", NOW - 3 * DAY_MS + i * 20_000, 1000, 0));
    }
    status(&t, "loop", NOW - 3 * DAY_MS + HOUR, &[("lines_added", "0"), ("lines_removed", "0")]);
    status(&t, "day-7", NOW - HOUR, &[("ctx_used_pct", "92"), ("ctx_size", "200000")]);
    status(&t, "day-8", NOW - HOUR, &[("ctx_used_pct", "50")]);

    let i = insights(&t.store, &ctx()).unwrap();
    let warnings: Vec<(&str, &str, f64)> =
        i.context_warnings.iter().map(|w| (w.session_id.as_str(), w.project.as_str(), w.ctx_used_pct)).collect();
    assert_eq!(warnings, vec![("day-7", "shop", 92.0)]);

    let found: Vec<(AnomalyKind, Option<&str>, f64, f64)> =
        i.anomalies.iter().map(|a| (a.kind, a.session_id.as_deref(), a.value, a.baseline)).collect();
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(found.contains(&(AnomalyKind::AgentLoop, Some("loop"), 70.0, 60.0)));
    let spike = i.anomalies.iter().find(|a| a.kind == AnomalyKind::CostSpike).unwrap();
    assert_eq!(spike.ts, ctx().day_start(NOW - 5 * DAY_MS));
    assert!((spike.value - 40.0).abs() < 1e-9 && (spike.baseline - 5.0).abs() < 1e-9);
    let runaway = i.anomalies.iter().find(|a| a.kind == AnomalyKind::RunawaySession).unwrap();
    assert_eq!(runaway.session_id.as_deref(), Some("pricey"));
    assert!((runaway.baseline - 5.0).abs() < 1e-9);
}

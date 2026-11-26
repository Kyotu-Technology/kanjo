#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use kanjo_alerts::{acknowledge, evaluate, list, render};
use kanjo_core::api::{AlertKind, Query, Severity};
use kanjo_core::limits::{insert_sample, LimitKind, LimitSample};
use kanjo_core::time::DAY_MS;
use kanjo_core::Error;
use serde_json::json;

fn sample(t: &kanjo_core::testkit::TestStore, kind: LimitKind, ts: i64, pct: f64, resets_at: i64) {
    let s = LimitSample { ts, used_pct: pct, resets_at, used_usd: None, limit_usd: None, period: None };
    t.store.write(|tx| insert_sample(tx, kind, &s)).unwrap();
}

#[test]
fn limit_threshold_fires_once_per_window_and_only_the_highest() {
    let t = store();
    configure(&t, |s| s.alerts.forecast = false);
    let reset = NOW + 2 * HOUR;
    sample(&t, LimitKind::FiveHour, NOW - 10 * MIN, 85.0, reset);
    let a = evaluate(&t.store, &ctx()).unwrap();
    let hits = of(&a, AlertKind::LimitThreshold);
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].params["threshold"].as_f64(), hits[0].severity), (Some(80.0), Severity::Info));
    assert_eq!(hits[0].title, "5-hour limit at 80%");
    assert_eq!(hits[0].message, "You have used 85% of the 5-hour limit. It resets today at 13:00.");
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());

    sample(&t, LimitKind::FiveHour, NOW - 5 * MIN, 96.0, reset + 30_000);
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!((a[0].params["threshold"].as_f64(), a[0].severity), (Some(95.0), Severity::Critical));
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());

    sample(&t, LimitKind::SevenDay, NOW - 5 * MIN, 91.0, NOW + 3 * DAY_MS);
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!((a[0].params["window"].as_str(), a[0].severity), (Some("seven_day"), Severity::Warning));

    let later = NOW + 3 * HOUR;
    sample(&t, LimitKind::FiveHour, later, 82.0, later + 4 * HOUR);
    let a = evaluate(&t.store, &at(later)).unwrap();
    assert_eq!(of(&a, AlertKind::LimitThreshold).len(), 1, "a new window fires again");

    let pl = list(&t.store, &ctx(), &Query { lang: Some("pl".into()), limit: Some(1), ..Default::default() }).unwrap();
    assert_eq!(pl.items[0].title, "Wykorzystano 80% limitu 5-godzinnego");
    assert_eq!(pl.items[0].message, "Zużycie limitu 5-godzinnego wynosi 82%. Reset nastąpi dziś o 18:00.");
}

#[test]
fn alerts_read_on_a_later_day_name_the_date() {
    let t = store();
    configure(&t, |s| s.alerts.forecast = false);
    sample(&t, LimitKind::FiveHour, NOW - 10 * MIN, 85.0, NOW + 2 * HOUR);
    assert_eq!(evaluate(&t.store, &ctx()).unwrap()[0].message, "You have used 85% of the 5-hour limit. It resets today at 13:00.");
    let same_day = list(&t.store, &at(NOW + 10 * HOUR), &Query::default()).unwrap();
    assert!(same_day.items[0].message.ends_with("It resets today at 13:00."));
    let next_day = list(&t.store, &at(NOW + DAY_MS), &Query { lang: Some("pl".into()), ..Default::default() }).unwrap();
    assert_eq!(next_day.items[0].message, "Zużycie limitu 5-godzinnego wynosi 85%. Reset nastąpi 26 listopada o 13:00.");
}

#[test]
fn limit_threshold_respects_an_empty_threshold_list() {
    let t = store();
    configure(&t, |s| {
        s.alerts.limit_thresholds.clear();
        s.alerts.forecast = false;
    });
    sample(&t, LimitKind::FiveHour, NOW - 10 * MIN, 99.0, NOW + HOUR);
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());
}

#[test]
fn forecast_fires_once_and_names_the_local_time() {
    let t = store();
    sample(&t, LimitKind::FiveHour, NOW - HOUR, 20.0, NOW + 4 * HOUR);
    sample(&t, LimitKind::FiveHour, NOW, 60.0, NOW + 4 * HOUR);
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].kind, AlertKind::LimitForecast);
    assert_eq!(a[0].params["exhausts_at"].as_i64(), Some(NOW + HOUR));
    assert_eq!(a[0].title, "5-hour limit will run out before the reset");
    assert_eq!(
        a[0].message,
        "At the current pace the 5-hour limit runs out today at 12:00, before it resets today at 15:00. Used so far: 60%."
    );
    let (title, message) = render(a[0].kind, &a[0].params, "pl");
    assert_eq!(title, "Limit 5-godzinny skończy się przed resetem");
    assert_eq!(
        message,
        "Przy obecnym tempie limit 5-godzinny skończy się dziś o 12:00, a reset nastąpi dopiero dziś o 15:00. Dotąd zużyto 60%."
    );
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());
}

#[test]
fn forecast_respects_its_toggle_and_skips_past_the_highest_threshold() {
    let t = store();
    configure(&t, |s| s.alerts.forecast = false);
    sample(&t, LimitKind::FiveHour, NOW - HOUR, 20.0, NOW + 4 * HOUR);
    sample(&t, LimitKind::FiveHour, NOW, 60.0, NOW + 4 * HOUR);
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());

    let t = store();
    sample(&t, LimitKind::FiveHour, NOW - HOUR, 50.0, NOW + 4 * HOUR);
    sample(&t, LimitKind::FiveHour, NOW, 96.0, NOW + 4 * HOUR);
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert!(of(&a, AlertKind::LimitForecast).is_empty());
    assert_eq!(of(&a, AlertKind::LimitThreshold).len(), 1);
}

/// 14 earlier days at $5 and `today_usd` today, each day in its own session.
fn spike_history(today_usd: f64) -> kanjo_core::testkit::TestStore {
    let t = store();
    let p = t.project("shop");
    for d in 1..=14 {
        spend(&t, p, &format!("day-{d}"), NOW - d * DAY_MS, 5.0);
    }
    spend(&t, p, "today", NOW - HOUR, today_usd);
    t
}

#[test]
fn cost_spike_fires_once_per_day() {
    let t = spike_history(30.0);
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].kind, AlertKind::CostSpike);
    assert_eq!(a[0].params["date"], "2026-11-26");
    assert_eq!(a[0].message, "Cost on 26 November reached $30.00, 6.0 times the median day ($5.00 over the last 14 active days).");
    let (_, pl) = render(a[0].kind, &a[0].params, "pl");
    assert_eq!(
        pl,
        "Koszt w dniu 26 listopada sięgnął 30,00\u{a0}$, czyli 6,0 raza więcej niż mediana (5,00\u{a0}$ z ostatnich 14 aktywnych dni)."
    );
    spend(&t, 1, "today", NOW - 40 * MIN, 10.0);
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());
    assert!(evaluate(&spike_history(12.0).store, &ctx()).unwrap().is_empty(), "below the floor");
    let off = spike_history(30.0);
    configure(&off, |s| s.alerts.anomalies = false);
    assert!(evaluate(&off.store, &ctx()).unwrap().is_empty());
}

fn runaway_history() -> kanjo_core::testkit::TestStore {
    let t = store();
    let p = t.project("shop");
    for d in 1..=6 {
        spend(&t, p, &format!("old-{d}"), NOW - d * DAY_MS, 2.0);
    }
    spend(&t, p, "big-session", NOW - 5 * MIN, 15.0);
    t
}

#[test]
fn runaway_session_fires_once_per_session() {
    let t = runaway_history();
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].kind, AlertKind::RunawaySession);
    assert_eq!(a[0].message, "Session big-sess in shop has reached $15.00, 7.5 times a typical session ($2.00).");
    let (title, pl) = render(a[0].kind, &a[0].params, "pl");
    assert_eq!(title, "Sesja wymyka się spod kontroli");
    assert_eq!(
        pl,
        "Koszt sesji big-sess w projekcie shop sięgnął 15,00\u{a0}$, czyli 7,5 raza więcej niż koszt typowej sesji (2,00\u{a0}$)."
    );
    spend(&t, 1, "big-session", NOW - MIN, 1.0);
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());

    let off = runaway_history();
    configure(&off, |s| s.alerts.anomalies = false);
    assert!(evaluate(&off.store, &ctx()).unwrap().is_empty());

    let ended = runaway_history();
    sql(&ended, &format!("INSERT INTO session_events (ts, session_id, event) VALUES ({}, 'big-session', 'SessionEnd')", NOW - 2 * MIN));
    assert!(evaluate(&ended.store, &ctx()).unwrap().is_empty(), "an ended session is not active");
}

fn loop_session(lines_added: i64) -> kanjo_core::testkit::TestStore {
    let t = store();
    let p = t.project("shop");
    for i in 0..65 {
        t.insert(&row(p, "looping", NOW - i * 20_000, 1000, 50));
    }
    status(&t, "looping", NOW - MIN, &[("lines_added", &lines_added.to_string()), ("lines_removed", "1")]);
    t
}

#[test]
fn agent_loop_fires_once_per_session_and_hour() {
    let t = loop_session(2);
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].kind, AlertKind::AgentLoop);
    assert_eq!(a[0].message, "Session looping in shop made 65 requests in 30 minutes but changed only 3 lines of code.");
    let (_, pl) = render(a[0].kind, &a[0].params, "pl");
    assert_eq!(pl, "Sesja looping w projekcie shop wysłała 65 zapytań w ciągu 30 minut, a zmieniła tylko 3 linie kodu.");
    assert!(evaluate(&t.store, &at(NOW + 10 * MIN)).unwrap().is_empty());
    assert!(evaluate(&loop_session(200).store, &ctx()).unwrap().is_empty(), "real code changes");
    let off = loop_session(2);
    configure(&off, |s| s.alerts.anomalies = false);
    assert!(evaluate(&off.store, &ctx()).unwrap().is_empty());
}

fn cold_session(recache: i64) -> kanjo_core::testkit::TestStore {
    let t = store();
    let p = t.project("shop");
    t.insert(&row(p, "cold-one", NOW - 10 * MIN, 1000, 50));
    status(
        &t,
        "cold-one",
        NOW - 10 * MIN,
        &[
            ("model_id", "'claude-opus-5-5'"),
            ("cache_ttl", "'5m'"),
            ("cache_expires_at", &(NOW - 5 * MIN).to_string()),
            ("cache_recache_if_cold", &recache.to_string()),
        ],
    );
    t
}

#[test]
fn cache_cold_without_expiry_time_fires_once_per_idle_stretch() {
    let t = store();
    let p = t.project("shop");
    t.insert(&row(p, "cold-two", NOW - 10 * MIN, 1000, 50));
    let snapshot = |ts: i64| status(&t, "cold-two", ts, &[("cache_warm", "0"), ("cache_recache_if_cold", "90000")]);
    snapshot(NOW - 5 * MIN);
    assert_eq!(of(&evaluate(&t.store, &ctx()).unwrap(), AlertKind::CacheCold).len(), 1);
    snapshot(NOW - MIN);
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty(), "a refreshed snapshot is the same idle stretch");
    t.insert(&row(p, "cold-two", NOW + MIN, 1000, 50));
    snapshot(NOW + 6 * MIN);
    assert_eq!(of(&evaluate(&t.store, &at(NOW + 7 * MIN)).unwrap(), AlertKind::CacheCold).len(), 1, "new activity, new stretch");
}

#[test]
fn cache_cold_fires_once_per_expiry() {
    let t = cold_session(120_000);
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].kind, AlertKind::CacheCold);
    assert!((a[0].params["recache_usd"].as_f64().unwrap() - 0.6).abs() < 1e-9);
    assert_eq!(
        a[0].message,
        "The prompt cache of session cold-one in shop expired today at 10:55. The next request will write about 120k tokens to the cache again (about $0.60)."
    );
    let (title, pl) = render(a[0].kind, &a[0].params, "pl");
    assert_eq!(title, "Cache promptu wygasł");
    assert_eq!(
        pl,
        "Cache promptu sesji cold-one w projekcie shop wygasł dziś o 10:55. Następne zapytanie zapisze w nim ponownie około 120 tys. tokenów (ok. 0,60\u{a0}$)."
    );
    assert!(evaluate(&t.store, &at(NOW + MIN)).unwrap().is_empty());
    assert!(evaluate(&cold_session(10_000).store, &ctx()).unwrap().is_empty(), "small context");
    let off = cold_session(120_000);
    configure(&off, |s| s.alerts.cache = false);
    assert!(evaluate(&off.store, &ctx()).unwrap().is_empty());
}

fn miss(t: &kanjo_core::testkit::TestStore, session: &str, ts: i64, causes: &str) {
    sql(t, &format!("INSERT INTO cache_misses (session_id, ts, causes) VALUES ('{session}', {ts}, '{causes}')"));
}

fn missing_session() -> kanjo_core::testkit::TestStore {
    let t = store();
    let p = t.project("shop");
    t.insert(&row(p, "missy", NOW - 2 * MIN, 1000, 50));
    miss(&t, "missy", NOW - 20 * MIN, r#"["tools_changed"]"#);
    miss(&t, "missy", NOW - 10 * MIN, r#"["tools_changed"]"#);
    miss(&t, "missy", NOW - MIN, r#"["system_changed"]"#);
    t
}

#[test]
fn cache_miss_streak_fires_once_per_streak() {
    let t = missing_session();
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].kind, AlertKind::CacheMissStreak);
    assert_eq!(a[0].params["top_cause"], "tools_changed");
    assert_eq!(
        a[0].message,
        "Session missy in shop had to rebuild the prompt cache 3 times in 30 minutes. Most common cause: tool set changed."
    );
    let (_, pl) = render(a[0].kind, &a[0].params, "pl");
    assert_eq!(
        pl,
        "Sesja missy w projekcie shop musiała 3 razy w ciągu 30 minut odbudować cache promptu. Najczęstsza przyczyna: zmiana zestawu narzędzi."
    );
    miss(&t, "missy", NOW + MIN, r#"["tools_changed"]"#);
    assert!(evaluate(&t.store, &at(NOW + 2 * MIN)).unwrap().is_empty(), "the same streak continues");
    let off = missing_session();
    configure(&off, |s| s.alerts.cache = false);
    assert!(evaluate(&off.store, &ctx()).unwrap().is_empty());
}

fn wide_context(pct: f64) -> kanjo_core::testkit::TestStore {
    let t = store();
    let p = t.project("shop");
    t.insert(&row(p, "wide", NOW - 2 * MIN, 1000, 50));
    status(&t, "wide", NOW - MIN, &[("ctx_used_pct", &pct.to_string()), ("ctx_size", "200000")]);
    t
}

#[test]
fn context_near_limit_fires_once_per_session() {
    let t = wide_context(90.0);
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].kind, AlertKind::ContextNearLimit);
    assert_eq!(a[0].message, "Session wide in shop uses 90% of its context window. Consider /compact or a fresh session.");
    let (title, pl) = render(a[0].kind, &a[0].params, "pl");
    assert_eq!(title, "Okno kontekstu prawie pełne");
    assert_eq!(pl, "Sesja wide w projekcie shop wykorzystuje 90% okna kontekstu. Warto użyć /compact albo zacząć nową sesję.");
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());
    assert!(evaluate(&wide_context(80.0).store, &ctx()).unwrap().is_empty());
    let off = wide_context(90.0);
    configure(&off, |s| s.alerts.context = false);
    assert!(evaluate(&off.store, &ctx()).unwrap().is_empty());
}

#[test]
fn list_filters_paginates_and_acknowledges() {
    let t = wide_context(90.0);
    sample(&t, LimitKind::FiveHour, NOW - 10 * MIN, 85.0, NOW + HOUR);
    configure(&t, |s| s.alerts.forecast = false);
    assert_eq!(evaluate(&t.store, &ctx()).unwrap().len(), 2);

    let all = list(&t.store, &ctx(), &Query::default()).unwrap();
    assert_eq!((all.items.len(), all.total, all.unacknowledged), (2, 2, 2));
    let page = list(&t.store, &ctx(), &Query { limit: Some(1), offset: Some(1), ..Default::default() }).unwrap();
    assert_eq!((page.items.len(), page.total), (1, 2));

    let id = all.items[0].id;
    assert_eq!(acknowledge(&t.store, &ctx(), Some(id)).unwrap(), 1);
    assert_eq!(acknowledge(&t.store, &ctx(), Some(id)).unwrap(), 0);
    let open = list(&t.store, &ctx(), &Query { q: Some("open".into()), ..Default::default() }).unwrap();
    assert_eq!((open.items.len(), open.total, open.unacknowledged), (1, 1, 1));
    assert!(open.items.iter().all(|a| a.acknowledged_at.is_none()));
    assert_eq!(acknowledge(&t.store, &ctx(), None).unwrap(), 1);
    assert!(matches!(acknowledge(&t.store, &ctx(), Some(9_999)), Err(Error::NotFound(_))));

    configure(&t, |s| s.language = kanjo_core::settings::Language::Pl);
    let pl = list(&t.store, &ctx(), &Query::default()).unwrap();
    assert!(pl.items.iter().any(|a| a.title == "Okno kontekstu prawie pełne"), "settings language applies to old alerts");
}

#[test]
fn project_labels_follow_redaction_at_read_time() {
    let t = wide_context(90.0);
    evaluate(&t.store, &ctx()).unwrap();
    configure(&t, |s| s.privacy.redact_paths = true);
    let alias = t.store.read(|c| kanjo_core::store::write::project_alias(c, "shop")).unwrap();
    let items = list(&t.store, &ctx(), &Query::default()).unwrap().items;
    assert_eq!(items[0].params["project"], alias.as_str());
    assert!(items[0].message.contains(&alias));
}

#[test]
fn every_kind_renders_in_both_languages() {
    let params = json!({
        "window": "seven_day", "threshold": 90.0, "used_pct": 91.0, "resets_at": NOW + DAY_MS, "exhausts_at": NOW + HOUR,
        "evaluated_at": NOW, "tz_offset_min": 60, "name": "Team", "scope": "client", "scope_value": "Acme", "scope_label": "Acme",
        "period": "month", "amount_usd": 100.0, "spent_usd": 85.0, "spent_pct": 85.0, "projected_usd": 140.0, "date": "2026-11-26",
        "cost_usd": 50.0, "median_usd": 10.0, "factor": 5.0, "baseline_days": 14, "session_id": "abcdef123456", "project": "shop",
        "requests": 62, "window_minutes": 30, "lines_changed": 0, "expired_at": NOW - MIN, "recache_tokens": 1_500_000,
        "misses": 4, "top_cause": "model_changed", "ctx_used_pct": 88.0
    });
    let kinds = [
        AlertKind::LimitThreshold,
        AlertKind::LimitForecast,
        AlertKind::BudgetWarning,
        AlertKind::BudgetExceeded,
        AlertKind::CostSpike,
        AlertKind::RunawaySession,
        AlertKind::AgentLoop,
        AlertKind::CacheCold,
        AlertKind::CacheMissStreak,
        AlertKind::ContextNearLimit,
    ];
    for kind in kinds {
        let en = render(kind, &params, "en");
        let pl = render(kind, &params, "pl-PL");
        for text in [&en.0, &en.1, &pl.0, &pl.1] {
            assert!(!text.is_empty() && !text.contains('{'), "{kind:?}: {text}");
        }
        assert_ne!(en, pl, "{kind:?}");
    }
    assert_eq!(
        render(AlertKind::AgentLoop, &params, "pl").1,
        "Sesja abcdef12 w projekcie shop wysłała 62 zapytania w ciągu 30 minut, nie zmieniając ani jednej linii kodu."
    );
    assert_eq!(
        render(AlertKind::BudgetExceeded, &params, "pl").1,
        "Wydatki wyniosły 85,00\u{a0}$ i przekroczyły miesięczny budżet 100,00\u{a0}$ dla klienta Acme (85%)."
    );
    assert!(render(AlertKind::CacheCold, &params, "en").1.contains("1.5M tokens"));
    assert!(render(AlertKind::CacheCold, &params, "pl").1.contains("1,5 mln tokenów"));
}

#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use kanjo_alerts::{budgets, create_budget, delete_budget, evaluate, render, update_budget};
use kanjo_core::api::{AlertKind, BudgetInput, BudgetPeriod, BudgetScope, BudgetState, Severity};
use kanjo_core::time::DAY_MS;
use kanjo_core::Error;

fn input(scope: BudgetScope, value: Option<&str>, period: BudgetPeriod, amount: f64) -> BudgetInput {
    BudgetInput {
        name: "Team".into(),
        scope,
        scope_value: value.map(str::to_string),
        period,
        amount_usd: amount,
        warn_pct: 80.0,
        enabled: true,
    }
}

/// Monday 2026-11-23 00:00 local (UTC+1).
const WEEK_START: i64 = NOW - 3 * DAY_MS - 11 * HOUR;

#[test]
fn weekly_project_budget_warns_then_exceeds_once_per_period() {
    let t = store();
    let p = t.project("shop");
    let other = t.project("blog");
    spend(&t, p, "last-week", WEEK_START - HOUR, 50.0);
    spend(&t, other, "elsewhere", NOW - HOUR, 50.0);
    spend(&t, p, "this-week", NOW - HOUR, 9.0);
    let b = create_budget(&t.store, &ctx(), &input(BudgetScope::Project, Some(&p.to_string()), BudgetPeriod::Week, 10.0)).unwrap();
    assert_eq!((b.period_start, b.period_end), (WEEK_START, WEEK_START + 7 * DAY_MS));
    assert!((b.spent_usd - 9.0).abs() < 1e-9 && (b.spent_pct - 90.0).abs() < 1e-6);
    let elapsed_h = (NOW - WEEK_START) as f64 / HOUR as f64;
    assert!((b.projected_usd - 9.0 * 168.0 / elapsed_h).abs() < 1e-6);
    assert_eq!((b.state, b.scope_label.as_deref()), (BudgetState::Warning, Some("shop")));

    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!((a[0].kind, a[0].severity), (AlertKind::BudgetWarning, Severity::Warning));
    assert_eq!(a[0].title, "Budget \"Team\" at 90%");
    assert_eq!(
        a[0].message,
        "Spent $9.00 of the $10.00 weekly budget for project shop (90%). At this pace the week would close at $18.22."
    );
    let (title, pl) = render(a[0].kind, &a[0].params, "pl");
    assert_eq!(title, "Budżet „Team” wykorzystany w 90%");
    assert_eq!(
        pl,
        "Wydano 9,00 USD z tygodniowego budżetu 10,00 USD dla projektu shop (90%). Przy tym tempie wydatki na koniec tygodnia wyniosłyby 18,22 USD."
    );
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());

    spend(&t, p, "this-week", NOW - 30 * MIN, 2.0);
    let a = evaluate(&t.store, &ctx()).unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!((a[0].kind, a[0].severity), (AlertKind::BudgetExceeded, Severity::Critical));
    assert_eq!(a[0].title, "Budget \"Team\" exceeded");
    assert_eq!(render(a[0].kind, &a[0].params, "pl").0, "Przekroczono budżet „Team”");
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());
    assert_eq!(budgets(&t.store, &ctx()).unwrap()[0].state, BudgetState::Exceeded);

    let next_week = at(WEEK_START + 7 * DAY_MS + HOUR);
    assert!(evaluate(&t.store, &next_week).unwrap().is_empty(), "nothing spent in the new week");
}

#[test]
fn a_recreated_budget_alerts_again_even_with_a_reused_id() {
    let t = store();
    let p = t.project("shop");
    spend(&t, p, "s", NOW - HOUR, 20.0);
    let first = create_budget(&t.store, &ctx(), &input(BudgetScope::Global, None, BudgetPeriod::Day, 10.0)).unwrap();
    assert_eq!(evaluate(&t.store, &ctx()).unwrap().len(), 1);
    delete_budget(&t.store, first.id).unwrap();
    let later = at(NOW + MIN);
    let second = create_budget(&t.store, &later, &input(BudgetScope::Global, None, BudgetPeriod::Day, 10.0)).unwrap();
    assert_eq!(second.id, first.id, "SQLite hands the freed id out again");
    assert_eq!(of(&evaluate(&t.store, &later).unwrap(), AlertKind::BudgetExceeded).len(), 1);
}

#[test]
fn project_budget_alerts_follow_redaction_at_read_time() {
    let t = store();
    let p = t.project("shop");
    spend(&t, p, "s", NOW - HOUR, 20.0);
    create_budget(&t.store, &ctx(), &input(BudgetScope::Project, Some(&p.to_string()), BudgetPeriod::Day, 10.0)).unwrap();
    evaluate(&t.store, &ctx()).unwrap();
    configure(&t, |s| s.privacy.redact_paths = true);
    let alias = kanjo_core::store::write::project_alias("shop");
    let items = kanjo_alerts::list(&t.store, &ctx(), &Default::default()).unwrap().items;
    assert_eq!(items[0].params["scope_label"], alias.as_str());
    assert!(items[0].message.contains(&alias) && !items[0].message.contains("shop"));
}

#[test]
fn budgets_respect_the_toggle_and_disabled_budgets() {
    let t = store();
    let p = t.project("shop");
    spend(&t, p, "s", NOW - HOUR, 20.0);
    let mut disabled = input(BudgetScope::Global, None, BudgetPeriod::Day, 10.0);
    disabled.enabled = false;
    create_budget(&t.store, &ctx(), &disabled).unwrap();
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());
    create_budget(&t.store, &ctx(), &input(BudgetScope::Global, None, BudgetPeriod::Day, 10.0)).unwrap();
    configure(&t, |s| s.alerts.budgets = false);
    assert!(evaluate(&t.store, &ctx()).unwrap().is_empty());
    configure(&t, |s| s.alerts.budgets = true);
    assert_eq!(of(&evaluate(&t.store, &ctx()).unwrap(), AlertKind::BudgetExceeded).len(), 1);
}

#[test]
fn scopes_and_periods_use_the_local_calendar() {
    let t = store();
    let shop = t.project("shop");
    let blog = t.project("blog");
    sql(&t, &format!("UPDATE projects SET client = 'Acme Corp' WHERE id = {shop}"));
    let day_start = NOW - 11 * HOUR;
    spend(&t, shop, "s-shop", day_start + 30 * MIN, 3.0);
    spend(&t, shop, "s-yesterday", day_start - 30 * MIN, 4.0);
    spend(&t, blog, "s-blog", NOW - HOUR, 5.0);
    spend(&t, blog, "s-oct", NOW - 30 * DAY_MS, 100.0);
    sql(&t, "UPDATE sessions SET account = 'acct-7' WHERE id IN ('s-blog', 's-yesterday')");

    let make = |scope, value: Option<&str>, period| create_budget(&t.store, &ctx(), &input(scope, value, period, 1000.0)).unwrap();
    let day = make(BudgetScope::Global, None, BudgetPeriod::Day);
    assert_eq!(day.period_start, day_start);
    assert!((day.spent_usd - 8.0).abs() < 1e-9);
    let month = make(BudgetScope::Global, None, BudgetPeriod::Month);
    assert_eq!(month.period_start, NOW - 25 * DAY_MS - 11 * HOUR);
    assert!((month.spent_usd - 12.0).abs() < 1e-9);
    let client = make(BudgetScope::Client, Some(" Acme Corp "), BudgetPeriod::Week);
    assert_eq!((client.scope_value.as_deref(), client.scope_label.as_deref()), (Some("Acme Corp"), Some("Acme Corp")));
    assert!((client.spent_usd - 7.0).abs() < 1e-9);
    let account = make(BudgetScope::Account, Some("acct-7"), BudgetPeriod::Week);
    assert!((account.spent_usd - 9.0).abs() < 1e-9);
    let sunday_weeks = kanjo_core::Ctx { week_start: 6, ..ctx() };
    let b = budgets(&t.store, &sunday_weeks).unwrap();
    assert_eq!(b[2].period_start, WEEK_START - DAY_MS);
}

#[test]
fn budget_input_is_validated_and_crud_reports_missing_ids() {
    let t = store();
    let p = t.project("shop");
    let bad = [
        BudgetInput { name: "  ".into(), ..input(BudgetScope::Global, None, BudgetPeriod::Day, 10.0) },
        input(BudgetScope::Global, None, BudgetPeriod::Day, 0.0),
        input(BudgetScope::Global, None, BudgetPeriod::Day, f64::NAN),
        BudgetInput { warn_pct: 120.0, ..input(BudgetScope::Global, None, BudgetPeriod::Day, 10.0) },
        input(BudgetScope::Project, None, BudgetPeriod::Day, 10.0),
        input(BudgetScope::Project, Some("999"), BudgetPeriod::Day, 10.0),
        input(BudgetScope::Client, Some(""), BudgetPeriod::Day, 10.0),
    ];
    for b in &bad {
        assert!(matches!(create_budget(&t.store, &ctx(), b), Err(Error::Invalid(_))), "{b:?}");
    }
    let b = create_budget(&t.store, &ctx(), &input(BudgetScope::Global, Some("ignored"), BudgetPeriod::Day, 10.0)).unwrap();
    assert_eq!(b.scope_value, None);
    let updated =
        update_budget(&t.store, &ctx(), b.id, &input(BudgetScope::Project, Some(&p.to_string()), BudgetPeriod::Month, 50.0)).unwrap();
    assert_eq!((updated.scope, updated.period, updated.amount_usd), (BudgetScope::Project, BudgetPeriod::Month, 50.0));
    assert!(matches!(
        update_budget(&t.store, &ctx(), 999, &input(BudgetScope::Global, None, BudgetPeriod::Day, 1.0)),
        Err(Error::NotFound(_))
    ));
    delete_budget(&t.store, b.id).unwrap();
    assert!(matches!(delete_budget(&t.store, b.id), Err(Error::NotFound(_))));
    assert!(budgets(&t.store, &ctx()).unwrap().is_empty());
}

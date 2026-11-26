//! Run with: cargo test -p kanjo-analytics --release --test perf -- --ignored --nocapture

#![allow(clippy::unwrap_used)]

use kanjo_core::limits::{insert_sample, LimitKind, LimitSample};
use kanjo_core::model::{RequestRow, TokenCounts, ToolKind, ToolUse};
use kanjo_core::pricing::PriceTable;
use kanjo_core::store::write;
use kanjo_core::testkit::TestStore;
use kanjo_core::time::{DAY_MS, HOUR_MS, MINUTE_MS};
use std::time::Instant;

const NOW: i64 = 1_795_617_000_000;
const REQUESTS: i64 = 500_000;
const MODELS: [&str; 3] = ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"];

fn build() -> TestStore {
    let t = TestStore::new();
    let projects: Vec<i64> = (0..8)
        .map(|i| t.store.write(|tx| write::upsert_project(tx, &format!("proj-{i}"), Some(&format!("/work/proj-{i}")), 0)).unwrap())
        .collect();
    let prices = PriceTable::embedded();
    let start = NOW - 365 * DAY_MS;
    let step = (NOW - start) / REQUESTS;
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let started = Instant::now();
    for chunk in 0..(REQUESTS / 25_000) {
        t.store
            .write(|tx| {
                for i in chunk * 25_000..(chunk + 1) * 25_000 {
                    let ts = start + i * step;
                    let day = (ts - start) / DAY_MS;
                    let project = projects[((day + (ts / HOUR_MS) / 3) % 8) as usize];
                    let x = rnd();
                    let row = RequestRow {
                        message_id: format!("msg_{i}"),
                        session_id: format!("sess-{}-{}", day, (ts / (2 * HOUR_MS)) % 12),
                        project_id: project,
                        ts,
                        model: MODELS[(x % 3) as usize].to_string(),
                        tokens: TokenCounts {
                            input: (x % 50) as i64,
                            output: (x % 2000) as i64,
                            cache_read: (x % 60_000) as i64,
                            cache_write_5m: (x % 3000) as i64,
                            cache_write_1h: 0,
                        },
                        is_sidechain: x % 5 == 0,
                        agent_type: (x % 5 == 0).then(|| "explorer".to_string()),
                        git_branch: Some(format!("branch-{}", x % 4)),
                        tools: if x % 3 == 0 {
                            vec![ToolUse {
                                use_id: format!("toolu_{i}"),
                                kind: ToolKind::Tool,
                                name: ["Bash", "Read", "Edit"][(x % 7 % 3) as usize].into(),
                            }]
                        } else {
                            Vec::new()
                        },
                        ..RequestRow::default()
                    };
                    write::insert_request(tx, &row, prices)?;
                }
                Ok(())
            })
            .unwrap();
    }
    println!("built {REQUESTS} requests in {:?}", started.elapsed());
    t
}

fn time<T>(label: &str, f: impl Fn() -> T) -> std::time::Duration {
    f();
    let runs = 5;
    let started = Instant::now();
    for _ in 0..runs {
        f();
    }
    let per = started.elapsed() / runs;
    println!("{label}: {per:?} per call");
    per
}

#[test]
#[ignore = "builds a 500k-request store; run in release with --ignored"]
fn overview_on_a_year_of_requests() {
    let t = build();
    let ctx = kanjo_core::testkit::warsaw(NOW);
    let q = kanjo_core::api::Query::default();
    let without = time("overview (no limit readings, estimate path)", || kanjo_analytics::overview(&t.store, &ctx).unwrap());
    t.store
        .write(|tx| {
            for i in 0..(60 * 24 * 6) {
                let ts = NOW - 60 * DAY_MS + i * 10 * MINUTE_MS;
                let five = (ts / (5 * HOUR_MS) + 1) * 5 * HOUR_MS;
                let s = LimitSample { ts, used_pct: (i % 30) as f64 * 3.0, resets_at: five, used_usd: None, limit_usd: None, period: None };
                insert_sample(tx, LimitKind::FiveHour, &s)?;
                let week = (ts / (7 * DAY_MS) + 1) * 7 * DAY_MS;
                let s = LimitSample { ts, used_pct: (i % 1008) as f64 / 10.0, resets_at: week, ..s };
                insert_sample(tx, LimitKind::SevenDay, &s)?;
            }
            Ok(())
        })
        .unwrap();
    let with = time("overview (live limit readings)", || kanjo_analytics::overview(&t.store, &ctx).unwrap());
    time("usage_report 30d", || kanjo_analytics::usage_report(&t.store, &ctx, &q).unwrap());
    time("breakdown tool 30d", || {
        kanjo_analytics::breakdown(&t.store, &ctx, &kanjo_core::api::Query { by: Some(kanjo_core::api::Dimension::Tool), ..q.clone() })
            .unwrap()
    });
    time("sessions 30d", || kanjo_analytics::sessions(&t.store, &ctx, &q).unwrap());
    time("blocks 30d", || kanjo_analytics::blocks(&t.store, &ctx, &q).unwrap());
    time("limits_detail", || kanjo_analytics::limits_detail(&t.store, &ctx, &q).unwrap());
    time("heatmap 365d", || kanjo_analytics::heatmap(&t.store, &ctx, &q).unwrap());
    time("plan_fit", || kanjo_analytics::plan_fit(&t.store, &ctx).unwrap());
    time("top_turns 30d", || kanjo_analytics::top_turns(&t.store, &ctx, &q).unwrap());
    assert!(without.as_millis() < 1500 && with.as_millis() < 1500, "overview far beyond its budget");
}

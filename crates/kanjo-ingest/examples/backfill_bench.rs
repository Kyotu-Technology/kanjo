//! Backfill benchmark. Prints aggregate numbers only: never paths, project
//! names or record content.
//!
//! cargo run --release -p kanjo-ingest --example backfill_bench -- --claude-dir DIR --db /tmp/kanjo-bench-x/kanjo.db
//! Options: --plain (unencrypted database), --verify-scanner (compare the byte scanner with serde on every line).

use kanjo_core::paths::Paths;
use kanjo_core::store::{DbKey, Store};
use kanjo_ingest::{diagnostics, Ingestor};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;

const BENCH_KEY: &str = "6b616e6a6f2d62656e63682d6b65792d6e6f742d612d7365637265742d2d2d2d";

fn main() {
    if let Err(e) = run() {
        eprintln!("bench failed: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut claude_dir = None;
    let mut db = None;
    let mut plain = false;
    let mut verify = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--claude-dir" => claude_dir = args.next().map(PathBuf::from),
            "--db" => db = args.next().map(PathBuf::from),
            "--plain" => plain = true,
            "--verify-scanner" => verify = true,
            _ => return Err("usage: backfill_bench --claude-dir DIR --db FILE [--plain] [--verify-scanner]".into()),
        }
    }
    let (Some(claude_dir), Some(db)) = (claude_dir, db) else { return Err("--claude-dir and --db are required".into()) };
    if db.exists() {
        return Err("the --db file must not exist yet".into());
    }
    let home = db.parent().ok_or("--db needs a parent directory")?.join("home");
    let paths = Paths::at(&home, &claude_dir);

    if verify {
        let t = Instant::now();
        let v = diagnostics::verify_scanner(&paths);
        println!("verify: files {} lines {} bytes {} seconds {:.2}", v.files, v.lines, v.bytes, t.elapsed().as_secs_f64());
        println!(
            "verify: scanner_fallbacks {} scanner_mismatches {} unknown_lines {} invalid_lines {}",
            v.scanner_fallbacks, v.scanner_mismatches, v.unknown_lines, v.invalid_lines
        );
        for (k, n) in &v.known_types {
            println!("verify: type {k} {n}");
        }
        for (k, n) in &v.meta_signatures {
            println!("verify: meta {k} {n}");
        }
    }

    let t = Instant::now();
    let p = diagnostics::parse_only(&paths);
    println!(
        "parse_only: files {} bytes {} seconds {:.2} rows {} rows_with_query_source {} rows_with_tools {} tool_sets {} quarantined {} read_errors {}",
        p.files,
        p.bytes,
        t.elapsed().as_secs_f64(),
        p.rows,
        p.rows_with_query_source,
        p.rows_with_tools,
        p.tool_sets,
        p.quarantined,
        p.read_errors
    );

    println!(
        "parse_only: assistant usage_records {} without_message {} without_usage {} synthetic {} synthetic_with_tokens {}",
        p.usage_records, p.assistant_without_message, p.assistant_without_usage, p.synthetic, p.synthetic_with_tokens
    );

    let key = if plain { None } else { Some(DbKey::from_hex(BENCH_KEY)?) };
    let store = Arc::new(Store::open(&db, key, None)?);
    let ingestor = Ingestor::new(store.clone(), paths)?;
    let shown = AtomicI64::new(-1);
    let t = Instant::now();
    let st = ingestor.backfill(&|s| {
        if s.bytes_total > 0 {
            let pct = s.bytes_done * 10 / s.bytes_total;
            if shown.swap(pct, Ordering::Relaxed) != pct {
                eprintln!("progress {}%", pct * 10);
            }
        }
    })?;
    let secs = t.elapsed().as_secs_f64();
    println!("backfill: files {} bytes {} seconds {:.2} encrypted {}", st.files_done, st.bytes_done, secs, !plain);
    let t = Instant::now();
    let again = ingestor.scan()?;
    println!("noop_scan: files {} seconds {:.3}", again.files, t.elapsed().as_secs_f64());

    store.read(|c| {
        let q = |sql: &str| -> rusqlite::Result<i64> { c.query_row(sql, [], |r| r.get(0)) };
        let (req, input, output, cr, w5, w1, cost, unpriced): (i64, i64, i64, i64, i64, i64, f64, i64) = c.query_row(
            "SELECT count(*), SUM(input), SUM(output), SUM(cache_read), SUM(cache_write_5m), SUM(cache_write_1h),
                    COALESCE(SUM(cost_usd), 0), SUM(cost_usd IS NULL) FROM requests",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )?;
        println!("requests {req} unpriced {unpriced} cost_usd {cost:.2}");
        println!("tokens input {input} output {output} cache_read {cr} cache_write_5m {w5} cache_write_1h {w1}");
        println!(
            "sessions {} projects {} sidechain_requests {} with_query_source {} agent_types {} web_search {} web_fetch {}",
            q("SELECT count(*) FROM sessions")?,
            q("SELECT count(*) FROM projects")?,
            q("SELECT count(*) FROM requests WHERE is_sidechain")?,
            q("SELECT count(*) FROM requests WHERE query_source IS NOT NULL")?,
            q("SELECT count(DISTINCT agent_type) FROM requests")?,
            q("SELECT COALESCE(SUM(web_search), 0) FROM requests")?,
            q("SELECT COALESCE(SUM(web_fetch), 0) FROM requests")?,
        );
        let hourly: (i64, f64) =
            c.query_row("SELECT SUM(requests), COALESCE(SUM(cost_usd), 0) FROM usage_hourly", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        println!("usage_hourly requests {} cost_usd {:.2}", hourly.0, hourly.1);
        let mut stmt = c.prepare("SELECT kind, count(*) FROM request_tools GROUP BY kind ORDER BY kind")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (k, n) = row?;
            println!("tool_rows {k} {n}");
        }
        let mut stmt = c.prepare(
            "SELECT kind, count(*), count(DISTINCT target || fingerprint), count(DISTINCT target) FROM config_changes GROUP BY kind ORDER BY kind",
        )?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))? {
            let (k, n, distinct, targets) = row?;
            println!("config_changes {k} rows {n} distinct_target_fingerprint {distinct} targets {targets}");
        }
        let mut stmt = c.prepare("SELECT model, count(*) FROM requests GROUP BY model ORDER BY 2 DESC")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (m, n) = row?;
            println!("model {m} {n}");
        }
        let mut stmt = c.prepare("SELECT record_type, signature, reason, SUM(count) FROM quarantine GROUP BY 1, 2, 3 ORDER BY 4 DESC")?;
        for row in
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?
        {
            let (t, s, reason, n) = row?;
            println!("quarantine {n} type={t} reason={reason} signature={s}");
        }
        Ok(())
    })?;
    Ok(())
}

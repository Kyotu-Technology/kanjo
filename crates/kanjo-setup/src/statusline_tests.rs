use super::*;
use kanjo_core::statusline::{CacheRecord, StatuslineInput};

const NOW: i64 = 1_738_417_560_000;

fn full() -> StatuslineRecord {
    let payload = br#"{
      "session_id": "s-1", "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
      "cost": {"total_cost_usd": 1.254},
      "prompt_cache": {"warm": true, "hit_ratio": 0.912},
      "rate_limits": {"five_hour": {"used_percentage": 23.4, "resets_at": 1738425600},
                      "seven_day": {"used_percentage": 92.0, "resets_at": 1738857600}}
    }"#;
    StatuslineInput::parse(payload).unwrap().to_record(NOW).unwrap()
}

fn plain(columns: Option<u16>) -> RenderOptions {
    RenderOptions { now: NOW, color: false, columns }
}

#[test]
fn renders_a_full_record() {
    assert_eq!(render(&full(), &plain(None)), "Opus | $1.25 | 5h 23% 2h14m | 7d 92% 5d2h | cache warm 91%");
    let colored = render(&full(), &RenderOptions { color: true, ..plain(None) });
    assert_eq!(
        colored,
        "\x1b[2mOpus\x1b[0m | $1.25 | \x1b[32m5h 23% 2h14m\x1b[0m | \x1b[31m7d 92% 5d2h\x1b[0m | \x1b[32mcache warm 91%\x1b[0m"
    );
}

#[test]
fn renders_minimal_and_partial_records() {
    let minimal = StatuslineRecord { session_id: "s".into(), ts: NOW, ..Default::default() };
    assert_eq!(render(&minimal, &plain(None)), "kanjo");
    let partial = StatuslineRecord {
        model_id: Some("claude-sonnet-4-6".into()),
        cost_usd: Some(123.4),
        cache: Some(CacheRecord { warm: Some(false), ..Default::default() }),
        ..minimal.clone()
    };
    assert_eq!(render(&partial, &plain(None)), "sonnet-4-6 | $123 | cache cold");
    let mut past = full();
    if let Some(w) = past.five_hour.as_mut() {
        w.resets_at = NOW - 1;
        w.used_pct = 75.0;
    }
    past.seven_day = None;
    past.cache = Some(CacheRecord::default());
    assert_eq!(render(&past, &plain(None)), "Opus | $1.25 | 5h 75%");
    assert!(render(&past, &RenderOptions { color: true, ..plain(None) }).contains("\x1b[33m5h 75%"));
}

#[test]
fn fits_narrow_terminals() {
    let rec = full();
    let wide = render(&rec, &plain(Some(200)));
    assert_eq!(wide, render(&rec, &plain(None)));
    for (cols, expected) in [
        (68u16, "Opus | $1.25 | 5h 23% 2h14m | 7d 92% 5d2h | cache warm 91%"),
        (60, "Opus | $1.25 | 5h 23% | 7d 92% | cache warm"),
        (40, "Opus | $1.25 | 5h 23% | 7d 92%"),
        (30, "$1.25 | 5h 23% | 7d 92%"),
        (20, "$1.25 | 5h 23%"),
        (12, "5h 23%"),
        (3, "5h 23%"),
    ] {
        let line = render(&rec, &plain(Some(cols)));
        assert_eq!(line, expected, "columns {cols}");
        assert!(cols < 10 || line.chars().count() <= cols as usize - 4);
    }
    let colored = render(&rec, &RenderOptions { color: true, ..plain(Some(30)) });
    assert_eq!(colored, "$1.25 | \x1b[32m5h 23%\x1b[0m | \x1b[31m7d 92%\x1b[0m");
}

#[test]
fn render_is_fast() {
    let rec = full();
    let opts = plain(Some(80));
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        std::hint::black_box(render(std::hint::black_box(&rec), &opts));
    }
    assert!(start.elapsed() < Duration::from_millis(200), "{:?}", start.elapsed());
}

#[cfg(unix)]
mod chain {
    use super::*;

    #[test]
    fn passes_the_payload_and_returns_stdout() {
        let out = run_chain("cat; printf '\\n\\n'", br#"{"session_id":"x"}"#, Duration::from_secs(5));
        assert_eq!(out.as_deref(), Some(r#"{"session_id":"x"}"#));
        assert_eq!(run_chain("printf 'a\\nb\\n'", b"", Duration::from_secs(5)).as_deref(), Some("a\nb"));
    }

    #[test]
    fn failure_gives_none() {
        assert_eq!(run_chain("echo partial; exit 3", b"{}", Duration::from_secs(5)), None);
        assert_eq!(run_chain("true", b"{}", Duration::from_secs(5)), None);
        assert_eq!(run_chain("/nonexistent/statusline-tool", b"{}", Duration::from_secs(5)), None);
    }

    #[test]
    fn large_payload_does_not_deadlock() {
        let payload = vec![b'x'; 1 << 20];
        assert_eq!(run_chain("cat > /dev/null; echo ok", &payload, Duration::from_secs(5)).as_deref(), Some("ok"));
    }

    #[test]
    fn enforces_the_timeout() {
        let start = Instant::now();
        assert_eq!(run_chain("sleep 5; echo late", b"{}", Duration::from_millis(200)), None);
        assert!(start.elapsed() < Duration::from_millis(1500), "{:?}", start.elapsed());

        let start = Instant::now();
        assert_eq!(run_chain("(sleep 5; echo late) & echo early", b"{}", Duration::from_secs(3)).as_deref(), Some("early"));
        assert!(start.elapsed() < Duration::from_millis(1000), "{:?}", start.elapsed());
    }
}

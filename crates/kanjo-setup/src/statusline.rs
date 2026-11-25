//! The text `kanjo statusline` prints, and the chained user command.

use kanjo_core::statusline::{StatuslineRecord, WindowRecord};
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SEP: &str = " | ";
const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const DIM: &str = "\x1b[2m";
/// How long `run_chain` waits for stdout to close after the command exited.
const EXIT_GRACE: Duration = Duration::from_millis(50);
/// Columns Claude Code keeps around the status line.
const EDGE: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Epoch milliseconds, for the time left until a reset.
    pub now: i64,
    pub color: bool,
    /// Terminal width; the line is fitted into `columns - 4`.
    pub columns: Option<u16>,
}

impl RenderOptions {
    /// Colour unless `NO_COLOR` is set to a non-empty value; width from `COLUMNS`.
    pub fn from_env(now: i64) -> RenderOptions {
        RenderOptions {
            now,
            color: std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
            columns: std::env::var("COLUMNS").ok().and_then(|c| c.trim().parse().ok()),
        }
    }
}

#[derive(Clone, Copy)]
struct Seg {
    start: usize,
    compact_end: usize,
    end: usize,
    /// Higher survives longer when the line is too wide.
    prio: u8,
    color: Option<&'static str>,
}

fn pct_color(pct: f64) -> &'static str {
    if pct >= 90.0 {
        RED
    } else if pct >= 70.0 {
        YELLOW
    } else {
        GREEN
    }
}

fn push_duration(buf: &mut String, ms: i64) {
    let mins = (ms / 60_000).max(0);
    let (d, h, m) = (mins / 1440, mins / 60 % 24, mins % 60);
    let _ = if d > 0 {
        write!(buf, "{d}d{h}h")
    } else if h > 0 {
        write!(buf, "{h}h{m:02}m")
    } else {
        write!(buf, "{m}m")
    };
}

fn push_window(buf: &mut String, segs: &mut Vec<Seg>, label: &str, w: &WindowRecord, prio: u8, now: i64) {
    let start = buf.len();
    let _ = write!(buf, "{label} {:.0}%", w.used_pct);
    let compact_end = buf.len();
    if w.resets_at > now {
        buf.push(' ');
        push_duration(buf, w.resets_at - now);
    }
    segs.push(Seg { start, compact_end, end: buf.len(), prio, color: Some(pct_color(w.used_pct)) });
}

fn short_model(id: &str) -> &str {
    id.strip_prefix("claude-").unwrap_or(id)
}

/// One compact line: model, session cost, 5-hour and 7-day usage with the time
/// to reset, and the prompt cache state. Segments whose data is missing are
/// left out; when `opts.columns` is too narrow, reset times and the hit ratio
/// go first, then whole segments by importance (cache, model, 7-day, cost; the
/// last remaining segment always stays).
pub fn render(rec: &StatuslineRecord, opts: &RenderOptions) -> String {
    let mut buf = String::with_capacity(96);
    let mut segs: Vec<Seg> = Vec::with_capacity(5);

    if let Some(model) = rec.model_name.as_deref().or(rec.model_id.as_deref().map(short_model)).filter(|m| !m.is_empty()) {
        let start = buf.len();
        buf.push_str(model);
        segs.push(Seg { start, compact_end: buf.len(), end: buf.len(), prio: 2, color: Some(DIM) });
    }
    if let Some(cost) = rec.cost_usd.filter(|c| c.is_finite()) {
        let start = buf.len();
        let _ = if cost >= 100.0 { write!(buf, "${cost:.0}") } else { write!(buf, "${cost:.2}") };
        segs.push(Seg { start, compact_end: buf.len(), end: buf.len(), prio: 4, color: None });
    }
    if let Some(w) = &rec.five_hour {
        push_window(&mut buf, &mut segs, "5h", w, 5, opts.now);
    }
    if let Some(w) = &rec.seven_day {
        push_window(&mut buf, &mut segs, "7d", w, 3, opts.now);
    }
    if let Some(cache) = &rec.cache {
        let start = buf.len();
        let color = match cache.warm {
            Some(true) => {
                buf.push_str("cache warm");
                Some(GREEN)
            }
            Some(false) => {
                buf.push_str("cache cold");
                Some(YELLOW)
            }
            None => {
                buf.push_str("cache");
                None
            }
        };
        let compact_end = buf.len();
        if let Some(r) = cache.hit_ratio.filter(|r| r.is_finite()) {
            let _ = write!(buf, " {:.0}%", r * 100.0);
        }
        if buf.len() > compact_end || cache.warm.is_some() {
            segs.push(Seg { start, compact_end, end: buf.len(), prio: 1, color });
        } else {
            buf.truncate(start);
        }
    }

    if segs.is_empty() {
        return "kanjo".to_string();
    }

    let text = |s: &Seg, compact: bool| &buf[s.start..if compact { s.compact_end } else { s.end }];
    let mut keep = [true; 5];
    let mut compact = false;
    if let Some(cols) = opts.columns {
        let max = (cols as usize).saturating_sub(EDGE);
        loop {
            let (n, w) =
                segs.iter().zip(keep).filter(|(_, k)| *k).fold((0, 0), |(n, w), (s, _)| (n + 1, w + text(s, compact).chars().count()));
            if n <= 1 || w + SEP.len() * (n - 1) <= max {
                break;
            }
            if !compact {
                compact = true;
                continue;
            }
            if let Some(i) = (0..segs.len()).filter(|i| keep[*i]).min_by_key(|i| segs[*i].prio) {
                keep[i] = false;
            }
        }
    }

    let mut out = String::with_capacity(buf.len() + segs.len() * 12);
    for s in segs.iter().zip(keep).filter(|(_, k)| *k).map(|(s, _)| s) {
        if !out.is_empty() {
            out.push_str(SEP);
        }
        match (opts.color, s.color) {
            (true, Some(c)) => {
                out.push_str(c);
                out.push_str(text(s, compact));
                out.push_str(RESET);
            }
            _ => out.push_str(text(s, compact)),
        }
    }
    out
}

fn shell_command(command: &str) -> Command {
    use crate::shell::{git_bash, Shell};
    let mut c;
    match Shell::current() {
        Shell::Posix => {
            c = Command::new("sh");
            c.arg("-c").arg(command);
        }
        Shell::GitBash => {
            c = Command::new(git_bash().unwrap_or_else(|| "bash".into()));
            c.arg("-c").arg(command);
        }
        Shell::PowerShell => {
            c = Command::new("powershell.exe");
            c.args(["-NoProfile", "-NonInteractive", "-Command", command]);
        }
    }
    c
}

/// Runs the user's previous statusLine command through the platform shell with
/// the original payload on stdin and returns its stdout without trailing
/// newlines. `None` when it cannot start, exits unsuccessfully, prints nothing
/// or runs past `timeout` (it is then killed), so Kanjo's own line is shown instead.
pub fn run_chain(command: &str, payload: &[u8], timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    let mut cmd = shell_command(command);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().ok()?;

    if let Some(mut stdin) = child.stdin.take() {
        let input = payload.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
    }
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    if let Some(mut stdout) = child.stdout.take() {
        let buffer = Arc::clone(&buffer);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            while let Ok(n @ 1..) = stdout.read(&mut chunk) {
                if let Ok(mut b) = buffer.lock() {
                    b.extend_from_slice(&chunk[..n]);
                }
            }
            let _ = done_tx.send(());
        });
    }

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(2)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    // A background job of the command can keep stdout open after the shell exits; take what was printed by then.
    let _ = done_rx.recv_timeout(deadline.saturating_duration_since(Instant::now()).min(EXIT_GRACE));
    let out = buffer.lock().ok()?;
    let text = String::from_utf8_lossy(&out);
    let text = text.trim_end_matches(['\n', '\r']);
    (!text.trim().is_empty()).then(|| text.to_string())
}

#[cfg(test)]
#[path = "statusline_tests.rs"]
mod tests;

//! The two commands Claude Code runs itself: `kanjo statusline` on every status
//! update and `kanjo hook` on session events. They must be fast and must never
//! fail: no database, no async runtime, no language lookup, always exit 0.

use kanjo_core::config::Config;
use kanjo_core::paths::Paths;
use kanjo_core::spool::{self, HookRecord, SpoolRecord};
use kanjo_core::statusline::{StatuslineInput, StatuslineRecord};
use kanjo_setup::statusline::{render, run_chain, RenderOptions};
use serde::Deserialize;
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const MAX_PAYLOAD: u64 = 16 * 1024 * 1024;
const CHAIN_TIMEOUT: Duration = Duration::from_secs(2);
const FALLBACK: &str = "kanjo";

fn read_stdin() -> Vec<u8> {
    let mut input = Vec::new();
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        let _ = stdin.lock().take(MAX_PAYLOAD).read_to_end(&mut input);
    }
    input
}

pub fn statusline(home: Option<PathBuf>, claude_dir: Option<PathBuf>, no_color: bool) -> ExitCode {
    std::panic::set_hook(Box::new(|_| {
        let _ = writeln!(std::io::stdout(), "{FALLBACK}");
        std::process::exit(0);
    }));
    let input = read_stdin();
    let now = kanjo_core::time::now_ms();
    let paths = Paths::resolve(home, claude_dir).ok();
    let config = match paths.as_ref().map(Config::load) {
        Some(Ok(c)) => c,
        Some(Err(e)) => {
            // Claude Code shows stderr of a statusline command only in its debug log, so the line costs nothing there.
            let first = e.to_string().lines().next().unwrap_or_default().to_string();
            let _ = writeln!(std::io::stderr(), "kanjo statusline: config.toml cannot be read, using the defaults ({first})");
            Config::default()
        }
        None => Config::default(),
    };
    let record = StatuslineInput::parse(&input).ok().and_then(|i| i.to_record(now)).map(SpoolRecord::Statusline);
    if let (Some(paths), Some(SpoolRecord::Statusline(r))) = (&paths, &record) {
        let mut spooled = r.clone();
        if !config.statusline.capture_titles {
            spooled.session_name = None;
        }
        let _ = spool::append(&paths.spool_dir(), now, &SpoolRecord::Statusline(spooled));
    }
    let mut opts = RenderOptions::from_env(now);
    if no_color {
        opts.color = false;
    }
    let own = || match &record {
        Some(SpoolRecord::Statusline(r)) => render(r, &opts),
        _ => render(&StatuslineRecord::default(), &opts),
    };
    let chain = config.statusline.chain_command.as_deref().map(str::trim).filter(|c| !c.is_empty());
    let line = match chain.and_then(|cmd| run_chain(cmd, &input, CHAIN_TIMEOUT)) {
        Some(out) if config.statusline.show_segment => format!("{}  {out}", own()),
        Some(out) => out,
        None => own(),
    };
    let line = if line.trim().is_empty() { FALLBACK.to_string() } else { line };
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
    ExitCode::SUCCESS
}

/// The fields of a hook payload Kanjo keeps. Each is read leniently so a type
/// change in a future Claude Code version drops that field, not the event.
#[derive(Default, Deserialize)]
#[serde(default)]
struct HookInput {
    hook_event_name: Option<serde_json::Value>,
    session_id: Option<serde_json::Value>,
    cwd: Option<serde_json::Value>,
    source: Option<serde_json::Value>,
    model: Option<serde_json::Value>,
    agent_type: Option<serde_json::Value>,
}

fn text(v: &Option<serde_json::Value>) -> Option<String> {
    match v {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn model_id(v: &Option<serde_json::Value>) -> Option<String> {
    match v {
        Some(serde_json::Value::Object(m)) => text(&m.get("id").cloned()),
        other => text(other),
    }
}

pub fn hook(home: Option<PathBuf>, claude_dir: Option<PathBuf>) -> ExitCode {
    std::panic::set_hook(Box::new(|_| std::process::exit(0)));
    let input = read_stdin();
    let Ok(h) = serde_json::from_slice::<HookInput>(&input) else { return ExitCode::SUCCESS };
    let (Some(event), Some(session_id)) = (text(&h.hook_event_name), text(&h.session_id)) else { return ExitCode::SUCCESS };
    let Ok(paths) = Paths::resolve(home, claude_dir) else { return ExitCode::SUCCESS };
    let now = kanjo_core::time::now_ms();
    let record = HookRecord {
        ts: now,
        event,
        session_id,
        cwd: text(&h.cwd),
        source: text(&h.source),
        model: model_id(&h.model),
        agent_type: text(&h.agent_type),
    };
    let _ = spool::append(&paths.spool_dir(), now, &SpoolRecord::Hook(record));
    ExitCode::SUCCESS
}

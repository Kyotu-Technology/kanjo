//! Kanjo's entries in Claude Code's user settings (`statusLine` and the
//! `SessionStart`, `Stop` and `SessionEnd` hooks).
//!
//! Everything else in settings.json is kept as it was, including key order and
//! indentation. Changes are written atomically, an existing file is copied to
//! `settings.json.kanjo-backup-<timestamp>` before it is changed, and every
//! operation is idempotent.

use crate::shell::{self, Shell};
use kanjo_core::api::HookInstallRequest;
use kanjo_core::config::Config;
use kanjo_core::paths::Paths;
use kanjo_core::{Error, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The hook events Kanjo registers for.
pub const HOOK_EVENTS: [&str; 3] = ["SessionStart", "Stop", "SessionEnd"];
pub const STATUSLINE_ARG: &str = "statusline";
pub const HOOK_ARG: &str = "hook";
/// `refreshInterval` of a newly created statusLine entry, in seconds.
pub const DEFAULT_REFRESH_SECS: u64 = 30;
pub const BACKUP_MARKER: &str = ".kanjo-backup-";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StatuslineSlot {
    /// No statusLine entry.
    Missing,
    /// Kanjo's own command (possibly from an earlier executable path).
    Kanjo { command: String },
    /// Somebody else's statusLine; `command` is `None` for entries without one.
    Foreign { command: Option<String> },
}

/// What settings.json contains right now, as far as Kanjo is concerned.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntegrationStatus {
    pub settings_path: PathBuf,
    pub settings_exists: bool,
    /// Why settings.json could not be read as a JSON object; the other fields are then empty.
    pub settings_error: Option<String>,
    pub statusline: StatuslineSlot,
    /// The command Kanjo forwards the payload to (from `config.toml`), when Kanjo's statusLine is installed.
    pub chain_command: Option<String>,
    /// Kanjo's hook is present, per event of `HOOK_EVENTS`.
    pub hooks: [bool; 3],
    /// `cleanupPeriodDays` when set.
    pub cleanup_period_days: Option<i64>,
}

impl IntegrationStatus {
    pub fn statusline_installed(&self) -> bool {
        matches!(self.statusline, StatuslineSlot::Kanjo { .. })
    }

    /// True when all three session hooks are installed.
    pub fn session_hooks_installed(&self) -> bool {
        self.hooks.iter().all(|h| *h)
    }
}

/// Outcome of `install` / `uninstall`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InstallReport {
    pub settings_path: PathBuf,
    /// settings.json was written.
    pub changed: bool,
    /// settings.json did not exist and was created.
    pub created: bool,
    /// Copy of the previous settings.json made by this call.
    pub backup: Option<PathBuf>,
    pub statusline_installed: bool,
    pub session_hooks_installed: bool,
    pub chain_command: Option<String>,
}

struct Doc {
    path: PathBuf,
    write_path: PathBuf,
    original: Option<Vec<u8>>,
    root: Map<String, Value>,
    indent: Vec<u8>,
    trailing_newline: bool,
}

fn parse_root(bytes: &[u8], path: &Path) -> Result<Map<String, Value>> {
    if bytes.iter().all(|b| b.is_ascii_whitespace()) {
        return Ok(Map::new());
    }
    match serde_json::from_slice::<Value>(bytes) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(Error::Invalid(format!("{}: the top level is not a JSON object", path.display()))),
        Err(e) => Err(Error::Invalid(format!("{}: {e}", path.display()))),
    }
}

fn detect_indent(text: &[u8]) -> Vec<u8> {
    for line in text.split(|b| *b == b'\n').skip(1) {
        let ws: Vec<u8> = line.iter().take_while(|b| **b == b' ' || **b == b'\t').copied().collect();
        if !ws.is_empty() && ws.len() < line.len() {
            return ws;
        }
    }
    b"  ".to_vec()
}

impl Doc {
    fn load(path: &Path) -> Result<Doc> {
        let write_path = match std::fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_symlink() => std::fs::canonicalize(path)?,
            _ => path.to_path_buf(),
        };
        let original = match std::fs::read(path) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let (root, indent, trailing_newline) = match &original {
            Some(bytes) => (parse_root(bytes, path)?, detect_indent(bytes), bytes.ends_with(b"\n") || bytes.is_empty()),
            None => (Map::new(), b"  ".to_vec(), true),
        };
        Ok(Doc { path: path.to_path_buf(), write_path, original, root, indent, trailing_newline })
    }

    fn render(&self) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(self.original.as_ref().map_or(256, |o| o.len() + 256));
        let fmt = serde_json::ser::PrettyFormatter::with_indent(&self.indent);
        let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
        Value::Object(self.root.clone()).serialize(&mut ser)?;
        if self.trailing_newline {
            out.push(b'\n');
        }
        Ok(out)
    }

    fn unchanged(&self) -> bool {
        match &self.original {
            Some(bytes) => {
                parse_root(bytes, &self.path).is_ok_and(|orig| same_order(&Value::Object(orig), &Value::Object(self.root.clone())))
            }
            None => false,
        }
    }

    /// Writes the document when it differs from the file. Returns (changed, created, backup).
    fn save(&self, now_ms: i64) -> Result<(bool, bool, Option<PathBuf>)> {
        if self.unchanged() {
            return Ok((false, false, None));
        }
        let bytes = self.render()?;
        let dir = self.write_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&dir)?;
        let backup = match &self.original {
            Some(orig) => backup(&self.path, orig, now_ms)?,
            None => None,
        };
        atomic_write(&self.write_path, &bytes)?;
        Ok((true, self.original.is_none(), backup))
    }
}

fn same_order(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|((ka, va), (kb, vb))| ka == kb && same_order(va, vb))
        }
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| same_order(p, q)),
        _ => a == b,
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "settings.json".into())
}

/// Copies `original` next to `path` unless the newest existing backup already holds the same bytes.
fn backup(path: &Path, original: &[u8], now_ms: i64) -> Result<Option<PathBuf>> {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let prefix = format!("{}{BACKUP_MARKER}", file_name(path));
    let newest = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(&prefix))
        .max();
    if let Some(name) = newest {
        if std::fs::read(dir.join(&name)).is_ok_and(|b| b == original) {
            return Ok(None);
        }
    }
    let stamp = kanjo_core::time::utc_datetime(now_ms).strftime("%Y%m%dT%H%M%SZ").to_string();
    let mut target = dir.join(format!("{prefix}{stamp}"));
    let mut n = 1;
    while target.exists() {
        target = dir.join(format!("{prefix}{stamp}-{n}"));
        n += 1;
    }
    std::fs::write(&target, original)?;
    if let Ok(meta) = std::fs::metadata(path) {
        std::fs::set_permissions(&target, meta.permissions())?;
    }
    Ok(Some(target))
}

/// Writes a temporary file in the target's directory, flushes it and renames it over the target.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let tmp = dir.join(format!(".{}.kanjo-tmp-{}", file_name(path), std::process::id()));
    let result = (|| -> Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        if let Ok(meta) = std::fs::metadata(path) {
            std::fs::set_permissions(&tmp, meta.permissions())?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn command_of(entry: &Value) -> Option<&str> {
    entry.get("command").and_then(Value::as_str)
}

fn is_kanjo_statusline(cmd: &str, exe: Option<&Path>) -> bool {
    shell::is_kanjo_command(cmd, STATUSLINE_ARG, exe)
}

fn is_kanjo_hook(handler: &Value, exe: Option<&Path>) -> bool {
    command_of(handler).is_some_and(|c| shell::is_kanjo_command(c, HOOK_ARG, exe))
}

fn statusline_slot(root: &Map<String, Value>, exe: Option<&Path>) -> StatuslineSlot {
    match root.get("statusLine") {
        None | Some(Value::Null) => StatuslineSlot::Missing,
        Some(entry) => match command_of(entry) {
            Some(c) if is_kanjo_statusline(c, exe) => StatuslineSlot::Kanjo { command: c.to_string() },
            c => StatuslineSlot::Foreign { command: c.map(str::to_string) },
        },
    }
}

fn hook_present(root: &Map<String, Value>, event: &str, exe: Option<&Path>) -> bool {
    root.get("hooks").and_then(|h| h.get(event)).and_then(Value::as_array).is_some_and(|groups| {
        groups.iter().any(|g| g.get("hooks").and_then(Value::as_array).is_some_and(|hs| hs.iter().any(|h| is_kanjo_hook(h, exe))))
    })
}

fn chain_of(config: &Config, exe: Option<&Path>) -> Option<String> {
    config.statusline.chain_command.clone().filter(|c| !c.trim().is_empty() && !is_kanjo_statusline(c, exe))
}

/// What is installed in settings.json right now. A settings.json that is not
/// valid JSON is reported in `settings_error`, not as an error.
pub fn status(paths: &Paths, config: &Config) -> Result<IntegrationStatus> {
    let path = paths.claude_settings();
    let exe = std::env::current_exe().ok();
    let mut st = IntegrationStatus {
        settings_exists: path.exists(),
        settings_path: path.clone(),
        settings_error: None,
        statusline: StatuslineSlot::Missing,
        chain_command: None,
        hooks: [false; 3],
        cleanup_period_days: None,
    };
    let doc = match Doc::load(&path) {
        Ok(d) => d,
        Err(Error::Invalid(msg)) => {
            st.settings_error = Some(msg);
            return Ok(st);
        }
        Err(e) => return Err(e),
    };
    let exe = exe.as_deref();
    st.statusline = statusline_slot(&doc.root, exe);
    if st.statusline_installed() {
        st.chain_command = chain_of(config, exe);
    }
    for (i, ev) in HOOK_EVENTS.iter().enumerate() {
        st.hooks[i] = hook_present(&doc.root, ev, exe);
    }
    st.cleanup_period_days = doc.root.get("cleanupPeriodDays").and_then(Value::as_i64);
    Ok(st)
}

/// Installs the statusLine and/or the session hooks named in `req`; a `false`
/// flag leaves that part as it is, and `req.service` is not handled here (see
/// `service::install`). A foreign statusLine command is remembered in
/// `config.statusline.chain_command`; the config is saved to `config.toml`
/// before settings.json is written whenever that value changes.
pub fn install(paths: &Paths, config: &mut Config, req: &HookInstallRequest, exe: &Path) -> Result<InstallReport> {
    install_with(paths, config, req, exe, Shell::current(), kanjo_core::time::now_ms())
}

/// Removes Kanjo's statusLine (restoring the remembered command, if any) and
/// its session hooks. A missing settings.json stays missing. The cleared chain
/// is saved to `config.toml` after settings.json is written.
pub fn uninstall(paths: &Paths, config: &mut Config, exe: &Path) -> Result<InstallReport> {
    uninstall_with(paths, config, exe, kanjo_core::time::now_ms())
}

fn hook_handler(shell: Shell, exe: &Path) -> Value {
    let mut h = json!({"type": "command", "command": shell::command_line(shell, exe, HOOK_ARG)});
    if shell == Shell::PowerShell {
        h["shell"] = json!("powershell");
    }
    h
}

fn as_object<'a>(v: &'a mut Value, what: &str) -> Result<&'a mut Map<String, Value>> {
    v.as_object_mut().ok_or_else(|| Error::Invalid(format!("settings.json: {what} is not an object")))
}

/// Removes Kanjo's handlers from one event's groups, dropping groups left
/// empty by the removal. Returns how many handlers were removed.
fn strip_kanjo_hooks(groups: &mut Vec<Value>, exe: Option<&Path>) -> usize {
    let mut removed = 0;
    groups.retain_mut(|g| {
        let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) else {
            return true;
        };
        let before = hs.len();
        hs.retain(|h| !is_kanjo_hook(h, exe));
        let gone = before - hs.len();
        removed += gone;
        !(gone > 0 && hs.is_empty())
    });
    removed
}

/// `install` with an explicit shell and clock.
pub fn install_with(
    paths: &Paths,
    config: &mut Config,
    req: &HookInstallRequest,
    exe: &Path,
    shell: Shell,
    now_ms: i64,
) -> Result<InstallReport> {
    let mut doc = Doc::load(&paths.claude_settings())?;
    let mut chain = chain_of(config, Some(exe));

    if req.statusline {
        let cmd = shell::command_line(shell, exe, STATUSLINE_ARG);
        match doc.root.get_mut("statusLine") {
            Some(Value::Object(entry)) => {
                match entry.get("command").and_then(Value::as_str) {
                    Some(c) if is_kanjo_statusline(c, Some(exe)) => {}
                    Some(c) if !c.trim().is_empty() => chain = Some(c.to_string()),
                    _ => chain = None,
                }
                entry.insert("type".into(), json!("command"));
                entry.insert("command".into(), json!(cmd));
            }
            _ => {
                chain = None;
                doc.root.insert("statusLine".into(), json!({"type": "command", "command": cmd, "refreshInterval": DEFAULT_REFRESH_SECS}));
            }
        }
    }

    if req.session_hooks {
        let handler = hook_handler(shell, exe);
        let hooks = as_object(doc.root.entry("hooks").or_insert_with(|| json!({})), "hooks")?;
        for ev in HOOK_EVENTS {
            let groups = hooks.entry(ev).or_insert_with(|| json!([]));
            let groups = groups.as_array_mut().ok_or_else(|| Error::Invalid(format!("settings.json: hooks.{ev} is not an array")))?;
            let ours: Vec<&Value> = groups
                .iter()
                .filter_map(|g| g.get("hooks").and_then(Value::as_array))
                .flatten()
                .filter(|h| is_kanjo_hook(h, Some(exe)))
                .collect();
            if ours.len() == 1 && *ours[0] == handler {
                continue;
            }
            strip_kanjo_hooks(groups, Some(exe));
            groups.push(json!({"hooks": [handler.clone()]}));
        }
    }

    if req.statusline && chain != config.statusline.chain_command {
        config.statusline.chain_command = chain;
        config.save(paths)?;
    }
    let (changed, created, backup) = doc.save(now_ms)?;
    let exe = Some(exe);
    let installed = matches!(statusline_slot(&doc.root, exe), StatuslineSlot::Kanjo { .. });
    Ok(InstallReport {
        settings_path: doc.path.clone(),
        changed,
        created,
        backup,
        statusline_installed: installed,
        session_hooks_installed: HOOK_EVENTS.iter().all(|ev| hook_present(&doc.root, ev, exe)),
        chain_command: if installed { chain_of(config, exe) } else { None },
    })
}

/// `uninstall` with an explicit clock.
pub fn uninstall_with(paths: &Paths, config: &mut Config, exe: &Path, now_ms: i64) -> Result<InstallReport> {
    let path = paths.claude_settings();
    let mut doc = Doc::load(&path)?;
    let chain = chain_of(config, Some(exe));

    if let StatuslineSlot::Kanjo { .. } = statusline_slot(&doc.root, Some(exe)) {
        match (&chain, doc.root.get_mut("statusLine")) {
            (Some(c), Some(Value::Object(entry))) => {
                entry.insert("command".into(), json!(c));
            }
            _ => {
                doc.root.shift_remove("statusLine");
            }
        }
    }

    let mut hooks_now_empty = false;
    if let Some(Value::Object(hooks)) = doc.root.get_mut("hooks") {
        for ev in HOOK_EVENTS {
            let Some(Value::Array(groups)) = hooks.get_mut(ev) else {
                continue;
            };
            if strip_kanjo_hooks(groups, Some(exe)) > 0 && groups.is_empty() {
                hooks.shift_remove(ev);
                hooks_now_empty = hooks.is_empty();
            }
        }
    }
    if hooks_now_empty {
        doc.root.shift_remove("hooks");
    }

    let (changed, created, backup) = if doc.original.is_some() { doc.save(now_ms)? } else { (false, false, None) };
    if config.statusline.chain_command.is_some() {
        config.statusline.chain_command = None;
        config.save(paths)?;
    }
    Ok(InstallReport {
        settings_path: path,
        changed,
        created,
        backup,
        statusline_installed: false,
        session_hooks_installed: false,
        chain_command: None,
    })
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;

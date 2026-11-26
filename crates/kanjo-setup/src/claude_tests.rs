use super::*;
use std::fs;

const EXE: &str = "/opt/kanjo/bin/kanjo";

struct Env {
    _dir: tempfile::TempDir,
    paths: Paths,
}

fn env(settings: Option<&str>) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path().join("home"), dir.path().join("claude"));
    if let Some(text) = settings {
        fs::create_dir_all(&paths.claude_dir).unwrap();
        fs::write(paths.claude_settings(), text).unwrap();
    }
    Env { _dir: dir, paths }
}

fn all() -> HookInstallRequest {
    HookInstallRequest { statusline: true, session_hooks: true, service: false }
}

fn install(e: &Env, cfg: &mut Config, exe: &str) -> InstallReport {
    install_with(&e.paths, cfg, &all(), Path::new(exe), None, Shell::Posix, 1_795_645_800_000).unwrap()
}

fn uninstall(e: &Env, cfg: &mut Config, exe: &str) -> InstallReport {
    uninstall_with(&e.paths, cfg, Path::new(exe), 1_795_645_860_000).unwrap()
}

fn text(e: &Env) -> String {
    fs::read_to_string(e.paths.claude_settings()).unwrap()
}

fn json_of(e: &Env) -> Value {
    serde_json::from_str(&text(e)).unwrap()
}

fn backups(e: &Env) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(&e.paths.claude_dir)
        .unwrap()
        .map(|d| d.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(BACKUP_MARKER))
        .collect();
    v.sort();
    v
}

fn kanjo_hook_count(v: &Value, event: &str) -> usize {
    v["hooks"][event]
        .as_array()
        .map(|groups| groups.iter().flat_map(|g| g["hooks"].as_array().unwrap().iter()).filter(|h| is_kanjo_hook(h, None)).count())
        .unwrap_or(0)
}

const FOREIGN_STATUSLINE: &str = r#"{
  "model": "opus",
  "statusLine": {
    "type": "command",
    "command": "npx -y ccstatusline@latest",
    "padding": 0
  },
  "theme": "dark"
}
"#;

const FOREIGN_HOOKS: &str = r#"{
  "permissions": {
    "allow": [
      "Bash(git status)"
    ]
  },
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "/usr/local/bin/guard"
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "notify-send done"
          }
        ]
      }
    ]
  },
  "cleanupPeriodDays": 90
}
"#;

#[test]
fn creates_missing_settings() {
    let e = env(None);
    let mut cfg = Config::default();
    let r = install(&e, &mut cfg, EXE);
    assert!(r.changed && r.created && r.backup.is_none() && r.statusline_installed && r.session_hooks_installed);
    assert_eq!(r.chain_command, None);
    let v = json_of(&e);
    assert_eq!(v["statusLine"], json!({"type": "command", "command": "'/opt/kanjo/bin/kanjo' statusline", "refreshInterval": 30}));
    for ev in HOOK_EVENTS {
        assert_eq!(v["hooks"][ev], json!([{"hooks": [{"type": "command", "command": "'/opt/kanjo/bin/kanjo' hook"}]}]));
    }
    assert!(text(&e).ends_with("}\n"));
    assert!(backups(&e).is_empty());

    let again = install(&e, &mut cfg, EXE);
    assert!(!again.changed && again.backup.is_none());

    let u = uninstall(&e, &mut cfg, EXE);
    assert!(u.changed && !u.statusline_installed);
    assert_eq!(text(&e), "{}\n");
}

#[test]
fn empty_file_counts_as_empty_object() {
    let e = env(Some(""));
    let mut cfg = Config::default();
    let r = install(&e, &mut cfg, EXE);
    assert!(r.changed && !r.created);
    assert_eq!(kanjo_hook_count(&json_of(&e), "Stop"), 1);
}

#[test]
fn chains_foreign_statusline_and_restores_it() {
    let e = env(Some(FOREIGN_STATUSLINE));
    let mut cfg = Config::default();
    let r = install(&e, &mut cfg, EXE);
    assert_eq!(r.chain_command.as_deref(), Some("npx -y ccstatusline@latest"));
    assert_eq!(cfg.statusline.chain_command.as_deref(), Some("npx -y ccstatusline@latest"));
    assert_eq!(Config::load(&e.paths).unwrap().statusline.chain_command.as_deref(), Some("npx -y ccstatusline@latest"));

    let after = text(&e);
    let keys: Vec<String> = json_of(&e).as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["model", "statusLine", "theme", "hooks"]);
    assert!(after.starts_with("{\n  \"model\": \"opus\",\n  \"statusLine\": {\n    \"type\": \"command\",\n    \"command\": \"'/opt/kanjo/bin/kanjo' statusline\",\n    \"padding\": 0\n  },"), "{after}");
    assert!(json_of(&e)["statusLine"].get("refreshInterval").is_none());

    let b = backups(&e);
    assert_eq!(b, vec!["settings.json.kanjo-backup-20261125T223000Z".to_string()]);
    assert_eq!(fs::read_to_string(e.paths.claude_dir.join(&b[0])).unwrap(), FOREIGN_STATUSLINE);

    let st = status(&e.paths, &cfg).unwrap();
    assert!(st.statusline_installed() && st.session_hooks_installed());
    assert_eq!(st.chain_command.as_deref(), Some("npx -y ccstatusline@latest"));

    let u = uninstall(&e, &mut cfg, EXE);
    assert!(u.changed);
    assert_eq!(text(&e), FOREIGN_STATUSLINE);
    assert_eq!(cfg.statusline.chain_command, None);
    assert_eq!(Config::load(&e.paths).unwrap().statusline.chain_command, None);
    assert_eq!(backups(&e).len(), 2);
}

#[test]
fn leaves_other_tools_hooks_alone() {
    let e = env(Some(FOREIGN_HOOKS));
    let mut cfg = Config::default();
    install(&e, &mut cfg, EXE);
    let v = json_of(&e);
    assert_eq!(v["hooks"]["PreToolUse"], serde_json::from_str::<Value>(FOREIGN_HOOKS).unwrap()["hooks"]["PreToolUse"]);
    let stop = v["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 2);
    assert_eq!(stop[0]["hooks"][0]["command"], "notify-send done");
    assert_eq!(kanjo_hook_count(&v, "Stop"), 1);
    assert_eq!(status(&e.paths, &cfg).unwrap().cleanup_period_days, Some(90));

    uninstall(&e, &mut cfg, EXE);
    assert_eq!(text(&e), FOREIGN_HOOKS);
}

#[test]
fn reinstall_is_a_no_op() {
    let e = env(Some(FOREIGN_HOOKS));
    let mut cfg = Config::default();
    install(&e, &mut cfg, EXE);
    let first = text(&e);
    let r = install(&e, &mut cfg, EXE);
    assert!(!r.changed && r.backup.is_none());
    assert_eq!(text(&e), first);
    assert_eq!(backups(&e).len(), 1);
    assert!(uninstall(&e, &mut cfg, EXE).changed);
    assert!(!uninstall(&e, &mut cfg, EXE).changed);
    assert_eq!(text(&e), FOREIGN_HOOKS);
}

#[test]
fn follows_a_moved_executable() {
    let e = env(Some(FOREIGN_STATUSLINE));
    let mut cfg = Config::default();
    install(&e, &mut cfg, "/old/place/kanjo");
    let r = install(&e, &mut cfg, "/new home/bin/kanjo");
    assert!(r.changed);
    assert_eq!(r.chain_command.as_deref(), Some("npx -y ccstatusline@latest"));
    let v = json_of(&e);
    assert_eq!(v["statusLine"]["command"], "'/new home/bin/kanjo' statusline");
    for ev in HOOK_EVENTS {
        assert_eq!(kanjo_hook_count(&v, ev), 1, "{ev}");
        assert_eq!(v["hooks"][ev][0]["hooks"][0]["command"], "'/new home/bin/kanjo' hook");
    }

    uninstall(&e, &mut cfg, "/somewhere/else/kanjo");
    assert_eq!(text(&e), FOREIGN_STATUSLINE);
}

#[test]
fn never_chains_to_itself() {
    let e = env(None);
    let mut cfg = Config::default();
    cfg.statusline.chain_command = Some("'/old/kanjo' statusline".into());
    let r = install(&e, &mut cfg, EXE);
    assert_eq!(r.chain_command, None);
    assert_eq!(cfg.statusline.chain_command, None);
}

#[test]
fn invalid_json_changes_nothing() {
    let bad = "{\n  \"model\": \"opus\",\n";
    let e = env(Some(bad));
    let mut cfg = Config::default();
    let err = install_with(&e.paths, &mut cfg, &all(), Path::new(EXE), None, Shell::Posix, 0).unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err}");
    assert!(uninstall_with(&e.paths, &mut cfg, Path::new(EXE), 0).is_err());
    assert_eq!(text(&e), bad);
    assert!(backups(&e).is_empty());
    assert!(!e.paths.config_file().exists());
    let st = status(&e.paths, &cfg).unwrap();
    assert!(st.settings_exists && st.settings_error.is_some() && !st.statusline_installed());

    let e = env(Some("[1, 2]"));
    assert!(install_with(&e.paths, &mut cfg, &all(), Path::new(EXE), None, Shell::Posix, 0).is_err());
    let e = env(Some(r#"{"hooks": []}"#));
    assert!(install_with(&e.paths, &mut cfg, &all(), Path::new(EXE), None, Shell::Posix, 0).is_err());
    assert_eq!(text(&e), r#"{"hooks": []}"#);
}

#[test]
fn keeps_indentation_and_partial_requests() {
    let e = env(Some("{\n    \"theme\": \"dark\"\n}"));
    let mut cfg = Config::default();
    let req = HookInstallRequest { statusline: false, session_hooks: true, service: true };
    let r = install_with(&e.paths, &mut cfg, &req, Path::new(EXE), None, Shell::Posix, 0).unwrap();
    assert!(!r.statusline_installed && r.session_hooks_installed);
    let t = text(&e);
    assert!(t.starts_with("{\n    \"theme\": \"dark\",\n    \"hooks\": {\n        \"SessionStart\""), "{t}");
    assert!(!t.ends_with('\n'));
    uninstall(&e, &mut cfg, EXE);
    assert_eq!(text(&e), "{\n    \"theme\": \"dark\"\n}");
}

#[test]
fn powershell_hooks_name_their_shell() {
    let e = env(None);
    let mut cfg = Config::default();
    let exe = Path::new(r"C:\Program Files\Kanjo\kanjo.exe");
    install_with(&e.paths, &mut cfg, &all(), exe, None, Shell::PowerShell, 0).unwrap();
    let v = json_of(&e);
    assert_eq!(v["statusLine"]["command"], "& 'C:/Program Files/Kanjo/kanjo.exe' statusline");
    assert_eq!(
        v["hooks"]["SessionEnd"][0]["hooks"][0],
        json!({"type": "command", "command": "& 'C:/Program Files/Kanjo/kanjo.exe' hook", "shell": "powershell"})
    );
    assert!(!install_with(&e.paths, &mut cfg, &all(), exe, None, Shell::PowerShell, 0).unwrap().changed);
    let r = install_with(&e.paths, &mut cfg, &all(), exe, None, Shell::GitBash, 0).unwrap();
    assert!(r.changed);
    assert_eq!(json_of(&e)["hooks"]["SessionEnd"][0]["hooks"][0]["command"], "'C:/Program Files/Kanjo/kanjo.exe' hook");
    assert_eq!(kanjo_hook_count(&json_of(&e), "SessionEnd"), 1);
}

#[cfg(unix)]
#[test]
fn writes_through_a_symlink() {
    let e = env(None);
    fs::create_dir_all(&e.paths.claude_dir).unwrap();
    let real = e.paths.claude_dir.join("dotfiles-settings.json");
    fs::write(&real, "{}").unwrap();
    std::os::unix::fs::symlink(&real, e.paths.claude_settings()).unwrap();
    let mut cfg = Config::default();
    install(&e, &mut cfg, EXE);
    assert!(fs::symlink_metadata(e.paths.claude_settings()).unwrap().file_type().is_symlink());
    assert!(fs::read_to_string(&real).unwrap().contains("statusline"));
}

#[test]
fn status_of_a_fresh_machine() {
    let e = env(None);
    let st = status(&e.paths, &Config::default()).unwrap();
    assert!(!st.settings_exists && st.settings_error.is_none());
    assert_eq!(st.statusline, StatuslineSlot::Missing);
    assert_eq!(st.hooks, [false; 3]);
    let hs = crate::hook_status(&e.paths, &Config::default());
    assert!(!hs.statusline_installed && !hs.session_hooks_installed && hs.last_statusline_at.is_none());

    let e = env(Some(FOREIGN_STATUSLINE));
    let st = status(&e.paths, &Config::default()).unwrap();
    assert_eq!(st.statusline, StatuslineSlot::Foreign { command: Some("npx -y ccstatusline@latest".into()) });
    assert_eq!(st.chain_command, None);
}

#[test]
fn a_custom_data_directory_is_named_in_every_command_and_still_recognised() {
    let e = env(Some(FOREIGN_HOOKS));
    let mut cfg = Config::default();
    let home = e.paths.home.clone();
    let r = install_with(&e.paths, &mut cfg, &all(), Path::new(EXE), Some(&home), Shell::Posix, 0).unwrap();
    assert!(r.statusline_installed && r.session_hooks_installed);
    let v = json_of(&e);
    let expect = |arg: &str| format!("'{EXE}' --home '{}' {arg}", home.display());
    assert_eq!(v["statusLine"]["command"], expect("statusline"));
    for ev in HOOK_EVENTS {
        assert_eq!(kanjo_hook_count(&v, ev), 1, "{ev}");
    }
    assert!(text(&e).contains(&expect("hook")));
    let again = install_with(&e.paths, &mut cfg, &all(), Path::new(EXE), Some(&home), Shell::Posix, 0).unwrap();
    assert!(!again.changed, "reinstalling with the same directory is a no-op");
    let moved = install_with(&e.paths, &mut cfg, &all(), Path::new(EXE), None, Shell::Posix, 0).unwrap();
    assert!(moved.changed && cfg.statusline.chain_command.is_none(), "Kanjo never chains to itself");
    for ev in HOOK_EVENTS {
        assert_eq!(kanjo_hook_count(&json_of(&e), ev), 1, "{ev}");
    }
    install_with(&e.paths, &mut cfg, &all(), Path::new(EXE), Some(&home), Shell::Posix, 0).unwrap();
    uninstall(&e, &mut cfg, EXE);
    let after = text(&e);
    assert!(!after.contains("statusline") && !after.contains(" hook"), "{after}");
    assert!(after.contains("/usr/local/bin/guard") && after.contains("notify-send done"));
}

#[test]
fn a_relative_data_directory_is_named_absolutely() {
    let paths = Paths::at("relative/kanjo-data", "/nowhere/claude");
    let home = home_arg(&paths).unwrap();
    assert!(home.is_absolute() && home.ends_with("relative/kanjo-data"), "{}", home.display());
}

use super::*;
use kanjo_core::api::HookInstallRequest;
use kanjo_core::testkit::TestStore;
use std::collections::BTreeMap;
use std::fs;

const NOW: i64 = 1_795_645_800_000;

struct Machine {
    dir: tempfile::TempDir,
    paths: Paths,
    exe: std::path::PathBuf,
}

fn machine() -> Machine {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path().join("data"), dir.path().join("claude"));
    let exe = dir.path().join("bin").join("kanjo");
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(&exe, b"").unwrap();
    Machine { dir, paths, exe }
}

fn healthy() -> (Machine, Config, DatabaseInfo) {
    let m = machine();
    let project = m.paths.claude_projects().join("-work-demo-app");
    fs::create_dir_all(project.join("sess-1").join("subagents")).unwrap();
    fs::write(project.join("sess-1.jsonl"), b"").unwrap();
    fs::write(project.join("sess-1").join("subagents").join("agent-a.jsonl"), b"").unwrap();
    fs::write(project.join("notes.txt"), b"").unwrap();
    fs::write(m.paths.claude_settings(), r#"{"cleanupPeriodDays": 60, "statusLine": {"type": "command", "command": "printf demo"}}"#)
        .unwrap();
    let mut cfg = Config::default();
    let req = HookInstallRequest { statusline: true, session_hooks: true, service: false };
    crate::claude::install_with(&m.paths, &mut cfg, &req, &m.exe, None, crate::shell::Shell::Posix, NOW).unwrap();
    m.paths.ensure_home().unwrap();

    let ts = TestStore::new();
    let db = DatabaseInfo {
        size_bytes: ts.store.size_bytes() as i64,
        encrypted: true,
        key_source: KeySource::Keyring,
        schema_version: ts.store.schema_version().unwrap(),
    };
    fs::copy(ts.store.path(), m.paths.db()).unwrap();
    kanjo_core::paths::make_private_file(&m.paths.db()).unwrap();
    (m, cfg, db)
}

fn input<'a>(m: &'a Machine, cfg: &'a Config, db: Option<DatabaseInfo>) -> DoctorInput<'a> {
    DoctorInput {
        exe: Some(&m.exe),
        database: db,
        last_statusline_at: Some(NOW - 5 * MINUTE_MS),
        quarantined_records: Some(0),
        claude_code_version: Some("2.1.332".into()),
        service: ServiceState::Running,
        ..DoctorInput::new(&m.paths, cfg, NOW)
    }
}

fn by_id(checks: &[DoctorCheck]) -> BTreeMap<&str, &DoctorCheck> {
    checks.iter().map(|c| (c.id.as_str(), c)).collect()
}

#[test]
fn healthy_machine_passes_in_both_languages() {
    let (m, cfg, db) = healthy();
    let inp = input(&m, &cfg, Some(db));
    for lang in ["en", "pl"] {
        let checks = run(&inp, lang);
        let ids: Vec<&str> = checks.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, CHECK_IDS, "{lang}");
        for c in &checks {
            assert_eq!(c.status, CheckStatus::Ok, "{lang} {}: {}", c.id, c.detail);
            assert!(c.fix.is_none() && !c.title.is_empty() && !c.detail.is_empty(), "{lang} {}", c.id);
        }
        let c = by_id(&checks);
        assert!(c["chain"].detail.contains("printf demo"));
        assert!(c["session_hooks"].detail.contains("SessionStart"));
        assert!(c["cleanup_period"].detail.contains("60"));
        if lang == "pl" {
            assert_eq!(c["projects"].detail, "2 transkrypty w 1 projekcie");
            assert_eq!(c["service"].title, "Usługa w tle");
            assert_eq!(c["statusline_payload"].detail, "5 min temu");
        } else {
            assert_eq!(c["projects"].detail, "2 transcripts in 1 project");
            assert_eq!(c["statusline_payload"].detail, "5 min ago");
        }
    }
}

#[test]
fn broken_machine_explains_every_problem() {
    let m = machine();
    fs::create_dir_all(&m.paths.claude_dir).unwrap();
    fs::write(m.paths.claude_settings(), "{ not json").unwrap();
    fs::create_dir_all(&m.paths.home).unwrap();
    fs::write(m.paths.db(), b"x").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&m.paths.home, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(m.paths.db(), fs::Permissions::from_mode(0o644)).unwrap();
    }
    let cfg = Config::default();
    let inp = DoctorInput {
        database_error: Some("file is not a database".into()),
        last_statusline_at: Some(NOW - 3 * DAY_MS),
        pricing: Some(PricingInfo {
            version: "2026-11-01".into(),
            source: "embedded".into(),
            models: 12,
            signed: false,
            unpriced_models: vec!["claude-future-9".into()],
        }),
        quarantined_records: Some(4),
        claude_code_version: Some("2.1.200".into()),
        service: ServiceState::Stopped,
        ..DoctorInput::new(&m.paths, &cfg, NOW)
    };
    for lang in ["en", "pl"] {
        let checks = run(&inp, lang);
        assert_eq!(checks.len(), CHECK_IDS.len());
        let c = by_id(&checks);
        assert_eq!(c["claude_dir"].status, CheckStatus::Ok);
        assert_eq!(c["settings_json"].status, CheckStatus::Fail);
        assert_eq!(c["database"].status, CheckStatus::Fail);
        for id in [
            "projects",
            "statusline",
            "chain",
            "session_hooks",
            "statusline_payload",
            "encryption",
            "schema",
            "pricing",
            "quarantine",
            "service",
            "claude_code_version",
        ] {
            assert_eq!(c[id].status, CheckStatus::Warn, "{lang} {id}: {}", c[id].detail);
        }
        #[cfg(unix)]
        {
            assert_eq!(c["data_dir"].status, CheckStatus::Warn);
            assert!(c["data_dir"].fix.as_deref().unwrap().starts_with("chmod 700"));
        }
        for id in ["settings_json", "database", "projects", "statusline_payload", "pricing", "quarantine", "service", "claude_code_version"]
        {
            assert!(c[id].fix.as_deref().is_some_and(|f| !f.is_empty()), "{lang} {id} has no fix");
        }
        assert!(c["pricing"].detail.contains("claude-future-9"));
        assert!(c["claude_code_version"].detail.contains(MIN_CC_CACHE_STATS));
        assert_eq!(c["cleanup_period"].status, CheckStatus::Ok);
    }
    let en = run(&inp, "en");
    let pl = run(&inp, "pl");
    for (e, p) in en.iter().zip(&pl) {
        assert_eq!(e.id, p.id);
        assert!(e.title != p.title || e.id == "statusline", "{} not translated", e.id);
    }
    assert_eq!(by_id(&pl)["database"].title, "Baza danych");
    assert!(by_id(&pl)["service"].detail.contains("zainstalowana, ale nie działa"));
    assert!(by_id(&pl)["quarantine"].detail.starts_with("odłożono na bok 4 rekordy o nieznanym kształcie"));
    assert!(by_id(&pl)["statusline_payload"].detail == "3 dni temu");
    assert!(by_id(&pl)["settings_json"].fix.as_deref().unwrap().contains("Popraw składnię JSON"));
}

#[test]
fn empty_machine_points_to_setup() {
    let m = machine();
    let cfg = Config::default();
    let checks = run(&DoctorInput::new(&m.paths, &cfg, NOW), "en");
    let c = by_id(&checks);
    assert_eq!(c["claude_dir"].status, CheckStatus::Fail);
    assert!(c["claude_dir"].fix.as_deref().unwrap().contains("KANJO_CLAUDE_DIR"));
    assert_eq!(c["settings_json"].status, CheckStatus::Warn);
    assert!(c["statusline"].fix.as_deref().unwrap().contains("kanjo up"));
    assert_eq!(c["database"].status, CheckStatus::Warn);
    assert!(c["database"].detail.contains("not created yet"));
    assert_eq!(c["schema"].status, CheckStatus::Warn);
    assert_eq!(c["pricing"].status, CheckStatus::Ok);
    assert_eq!(c["pricing"].detail, PriceTable::embedded().version);
    assert_eq!(c["cleanup_period"].status, CheckStatus::Ok);
    assert!(c["cleanup_period"].detail.contains("30 days (Claude Code default)"));
    let _ = &m.dir;
}

#[test]
fn edge_cases() {
    let (m, mut cfg, mut db) = healthy();
    fs::remove_file(&m.exe).unwrap();
    db.schema_version = SCHEMA_VERSION + 1;
    db.encrypted = false;
    db.key_source = KeySource::None;
    cfg.storage.encryption = EncryptionMode::Off;
    let inp = DoctorInput { claude_code_version: Some("2.1.255 (Claude Code)".into()), ..input(&m, &cfg, Some(db)) };
    let c = run(&inp, "en");
    let c = by_id(&c);
    assert_eq!(c["statusline"].status, CheckStatus::Fail);
    assert!(c["statusline"].detail.contains("missing file"));
    assert_eq!(c["schema"].status, CheckStatus::Fail);
    assert_eq!(c["encryption"].status, CheckStatus::Warn);
    assert!(c["encryption"].detail.contains("\"off\""));
    assert_eq!(c["claude_code_version"].status, CheckStatus::Warn);
    assert!(c["claude_code_version"].detail.contains(MIN_CC_MISS_CAUSES) && !c["claude_code_version"].detail.contains(MIN_CC_CACHE_STATS));

    let raw = fs::read_to_string(m.paths.claude_settings()).unwrap().replace("\"cleanupPeriodDays\": 60", "\"cleanupPeriodDays\": 3");
    fs::write(m.paths.claude_settings(), raw).unwrap();
    let c = run(&inp, "pl");
    let c = by_id(&c);
    assert_eq!(c["cleanup_period"].status, CheckStatus::Warn);
    assert!(c["cleanup_period"].fix.as_deref().unwrap().contains("Zwiększ cleanupPeriodDays"));
}

#[test]
fn polish_plurals() {
    let forms = |n| pl_plural(n, "rekord", "rekordy", "rekordów");
    assert_eq!(
        [1, 2, 4, 5, 11, 12, 14, 21, 22, 25, 102, 112].map(forms),
        [
            "rekord",
            "rekordy",
            "rekordy",
            "rekordów",
            "rekordów",
            "rekordów",
            "rekordów",
            "rekordów",
            "rekordy",
            "rekordów",
            "rekordy",
            "rekordów"
        ]
    );
    let l = Lang { pl: true };
    assert_eq!(ago(&l, DAY_MS + 5), "1 dzień temu");
    assert_eq!(ago(&Lang { pl: false }, DAY_MS), "1 day ago");
}

#[test]
fn parses_versions() {
    assert_eq!(parse_version("2.1.251"), Some((2, 1, 251)));
    assert_eq!(parse_version("2.1.9 (Claude Code)"), Some((2, 1, 9)));
    assert_eq!(parse_version("3"), Some((3, 0, 0)));
    assert_eq!(parse_version("beta"), None);
    assert!(parse_version("2.1.9") < parse_version("2.1.251"));
}

#[test]
fn an_unencrypted_database_is_never_told_to_switch_encryption_on() {
    let (m, mut cfg, mut db) = healthy();
    db.encrypted = false;
    db.key_source = KeySource::None;
    cfg.storage.encryption = EncryptionMode::Off;
    for lang in ["en", "pl"] {
        let checks = run(&input(&m, &cfg, Some(db.clone())), lang);
        let fix = by_id(&checks)["encryption"].fix.clone().unwrap();
        assert!(fix.contains("encryption = \"off\"") && fix.contains("kanjo import"), "{lang}: {fix}");
        assert!(fix.contains(&m.paths.db().display().to_string()), "{lang}: {fix}");
    }
}

#[test]
fn a_plain_database_that_fails_to_open_points_back_to_encryption_off() {
    let m = machine();
    m.paths.ensure_home().unwrap();
    let ts = TestStore::new();
    fs::copy(ts.store.path(), m.paths.db()).unwrap();
    assert!(is_plaintext_db(&m.paths.db()), "the test store is plain SQLite");
    let cfg = Config::default();
    let mut inp = input(&m, &cfg, None);
    inp.database_error = Some("its encryption key was found neither in the key file nor in the OS keyring".into());
    let checks = run(&inp, "en");
    let fix = by_id(&checks)["database"].fix.clone().unwrap();
    assert!(fix.contains("never encrypted") && fix.contains("encryption = \"off\""), "{fix}");
    fs::write(m.paths.db(), [0u8; 64]).unwrap();
    assert!(!is_plaintext_db(&m.paths.db()));
    let fix = by_id(&run(&inp, "en"))["database"].fix.clone().unwrap();
    assert!(fix.contains("restore a damaged database"), "{fix}");
}

#[cfg(unix)]
#[test]
fn the_permission_fix_names_only_files_that_exist() {
    use std::os::unix::fs::PermissionsExt;
    let (m, cfg, db) = healthy();
    fs::set_permissions(m.paths.db(), fs::Permissions::from_mode(0o644)).unwrap();
    let checks = run(&input(&m, &cfg, Some(db)), "en");
    let fix = by_id(&checks)["data_dir"].fix.clone().unwrap();
    assert_eq!(fix, format!("chmod 600 '{}'", m.paths.db().display()));
    assert!(!fix.contains("access.token") && !fix.contains("*"));
}

#[test]
fn a_demo_reports_its_deliberate_gaps_as_fine() {
    let m = machine();
    let ts = TestStore::new();
    let db = DatabaseInfo {
        size_bytes: ts.store.size_bytes() as i64,
        encrypted: false,
        key_source: KeySource::None,
        schema_version: ts.store.schema_version().unwrap(),
    };
    m.paths.ensure_home().unwrap();
    fs::copy(ts.store.path(), m.paths.db()).unwrap();
    kanjo_core::paths::make_private_file(&m.paths.db()).unwrap();
    let cfg = Config::default();
    let mut inp = input(&m, &cfg, Some(db));
    inp.exe = None;
    inp.quarantined_records = Some(4);
    inp.service = ServiceState::NotInstalled;
    inp.demo = true;
    for lang in ["en", "pl"] {
        for c in run(&inp, lang) {
            assert_eq!(c.status, CheckStatus::Ok, "{lang} {}: {}", c.id, c.detail);
            assert!(c.fix.is_none(), "{lang} {}", c.id);
        }
    }
    let checks = run(&inp, "en");
    assert!(by_id(&checks)["encryption"].detail.starts_with("demo:"));
}

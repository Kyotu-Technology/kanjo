use super::*;
use std::net::TcpListener;

fn record(paths: &Paths, addr: SocketAddr, auth: bool) -> ServerRecord {
    let _ = paths;
    ServerRecord {
        pid: std::process::id(),
        addr,
        url: format!("http://{addr}/"),
        auth,
        demo: false,
        instance: random_hex(16).unwrap(),
        version: "test".into(),
        started_at: 1,
    }
}

fn setup() -> (tempfile::TempDir, Paths, TcpListener) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path().join("home"), dir.path().join("claude"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    (dir, paths, listener)
}

#[test]
fn a_live_record_is_found_and_removed_on_drop() {
    let (_dir, paths, l) = setup();
    assert!(live(&paths).is_none());
    let rec = record(&paths, l.local_addr().unwrap(), true);
    let held = announce(&paths, &rec).unwrap();
    assert_eq!(live(&paths).as_ref(), Some(&rec));
    assert_eq!(answering(&paths).as_ref(), Some(&rec));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(paths.server_file()).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let err = announce(&paths, &record(&paths, l.local_addr().unwrap(), true)).unwrap_err().to_string();
    assert!(err.contains("already running"), "{err}");
    drop(held);
    assert!(!paths.server_file().exists());
    assert!(live(&paths).is_none());
}

#[test]
fn a_record_nobody_holds_is_stale() {
    let (_dir, paths, l) = setup();
    let rec = record(&paths, l.local_addr().unwrap(), true);
    paths.ensure_home().unwrap();
    std::fs::write(paths.server_file(), serde_json::to_string(&rec).unwrap()).unwrap();
    assert!(live(&paths).is_none(), "a crashed server's record must not count");
    let again = announce(&paths, &rec).unwrap();
    assert!(live(&paths).is_some());
    drop(again);
}

#[test]
fn a_server_that_stopped_answering_is_not_answering() {
    let (_dir, paths, l) = setup();
    let addr = l.local_addr().unwrap();
    drop(l);
    let _held = announce(&paths, &record(&paths, addr, true)).unwrap();
    assert!(live(&paths).is_some());
    assert!(answering(&paths).is_none());
}

#[test]
fn login_codes_are_single_use_bound_to_the_instance_and_expire() {
    let (_dir, paths, l) = setup();
    let rec = record(&paths, l.local_addr().unwrap(), true);
    let now = 1_000_000;
    let url = login_url(&paths, &rec, now).unwrap();
    let code = url.split_once("?token=").unwrap().1.to_string();
    assert!(is_code(&code));
    assert!(!claim_login_code(&paths, "another-instance", &code, now));
    assert!(!claim_login_code(&paths, &rec.instance, &code, now), "a refused code is gone too");

    let code = login_url(&paths, &rec, now).unwrap().split_once("?token=").unwrap().1.to_string();
    assert!(claim_login_code(&paths, &rec.instance, &code, now + 1000));
    assert!(!claim_login_code(&paths, &rec.instance, &code, now + 1000), "single use");

    let code = login_url(&paths, &rec, now).unwrap().split_once("?token=").unwrap().1.to_string();
    assert!(!claim_login_code(&paths, &rec.instance, &code, now + LOGIN_CODE_TTL_MS + 1));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let code = login_url(&paths, &rec, now).unwrap().split_once("?token=").unwrap().1.to_string();
        let mode = std::fs::metadata(paths.login_dir().join(&code)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(std::fs::metadata(paths.login_dir()).unwrap().permissions().mode() & 0o777, 0o700);
    }
}

#[test]
fn only_hex_codes_reach_the_filesystem() {
    let (_dir, paths, _l) = setup();
    paths.ensure_home().unwrap();
    std::fs::create_dir_all(paths.login_dir()).unwrap();
    std::fs::write(paths.token_file(), "x").unwrap();
    for given in ["../access.token", "", &"A".repeat(64), &format!("{}/", "a".repeat(63))] {
        assert!(!claim_login_code(&paths, "i", given, 0), "{given}");
    }
    assert!(paths.token_file().exists());
}

#[test]
fn without_authentication_the_plain_url_is_used() {
    let (_dir, paths, l) = setup();
    let rec = record(&paths, l.local_addr().unwrap(), false);
    assert_eq!(login_url(&paths, &rec, 0).unwrap(), rec.url);
    assert!(!paths.login_dir().exists());
}

#[test]
fn starting_clears_old_codes() {
    let (_dir, paths, l) = setup();
    let rec = record(&paths, l.local_addr().unwrap(), true);
    let url = login_url(&paths, &rec, 0).unwrap();
    let code = url.split_once("?token=").unwrap().1.to_string();
    let _held = announce(&paths, &rec).unwrap();
    assert!(!paths.login_dir().join(code).exists());
}

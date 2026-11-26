//! The access token of the HTTP API.

use kanjo_core::paths::Paths;
use kanjo_core::{Error, Result};

/// Reads the token from `paths.token_file()`, creating 32 random bytes (hex)
/// in an owner-only file when it is missing or malformed.
pub fn load_or_create(paths: &Paths) -> Result<String> {
    paths.ensure_home()?;
    let file = paths.token_file();
    for _ in 0..2 {
        match std::fs::read_to_string(&file) {
            Ok(text) if valid(text.trim()) => return Ok(text.trim().to_string()),
            Ok(_) => {
                tracing::warn!(file = %file.display(), "replacing a malformed access token");
                std::fs::remove_file(&file)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let token = crate::random_hex(32)?;
        match crate::create_private(&file, &format!("{token}\n")) {
            Ok(()) => return Ok(token),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::Other(format!("cannot create the access token {}", file.display())))
}

/// Replaces the token with a new random one. A running server keeps the
/// token it read at start-up until it restarts.
pub fn rotate(paths: &Paths) -> Result<()> {
    paths.ensure_home()?;
    let token = crate::random_hex(32)?;
    kanjo_core::paths::write_private_atomic(&paths.token_file(), format!("{token}\n").as_bytes())
}

fn valid(t: &str) -> bool {
    t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Compares in time that depends only on the length of `expected`.
pub fn matches(expected: &str, given: &str) -> bool {
    let (a, b) = (expected.as_bytes(), given.as_bytes());
    let mut diff = (a.len() != b.len()) as u8 | a.is_empty() as u8;
    for (i, x) in a.iter().enumerate() {
        diff |= x ^ b.get(i).copied().unwrap_or(!x);
    }
    std::hint::black_box(diff) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_once_owner_only_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path().join("home"), dir.path().join("claude"));
        let a = load_or_create(&paths).unwrap();
        assert!(valid(&a));
        assert_eq!(load_or_create(&paths).unwrap(), a);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(paths.token_file()).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::write(paths.token_file(), "short").unwrap();
        let b = load_or_create(&paths).unwrap();
        assert!(valid(&b) && b != a);
    }

    #[test]
    fn rotate_replaces_the_token_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path().join("home"), dir.path().join("claude"));
        rotate(&paths).unwrap();
        let a = load_or_create(&paths).unwrap();
        rotate(&paths).unwrap();
        let b = load_or_create(&paths).unwrap();
        assert!(valid(&a) && valid(&b) && a != b);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(paths.token_file()).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn compare() {
        let t = "ab".repeat(32);
        assert!(matches(&t, &t.clone()));
        assert!(!matches(&t, &"ab".repeat(31)));
        assert!(!matches(&t, &format!("{t}00")));
        assert!(!matches(&t, &t.replace('a', "c")));
        assert!(!matches(&t, ""));
        assert!(!matches("", ""));
        let mut last = t.clone().into_bytes();
        last[63] = b'0';
        assert!(!matches(&t, std::str::from_utf8(&last).unwrap()));
    }
}

//! Command lines for the shell Claude Code runs `statusLine` and hook commands
//! through: `sh -c` on macOS and Linux, Git Bash on Windows when it is
//! installed, PowerShell otherwise.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Posix,
    GitBash,
    PowerShell,
}

impl Shell {
    /// The shell Claude Code uses on this machine.
    pub fn current() -> Shell {
        if cfg!(windows) {
            if git_bash().is_some() {
                Shell::GitBash
            } else {
                Shell::PowerShell
            }
        } else {
            Shell::Posix
        }
    }
}

/// Git Bash as Claude Code finds it: `CLAUDE_CODE_GIT_BASH_PATH`, then the Git
/// installation that provides `git.exe` on `PATH`, then the default install locations.
pub fn git_bash() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").map(PathBuf::from) {
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join("git.exe").is_file() {
                if let Some(root) = dir.parent() {
                    let bash = root.join("bin").join("bash.exe");
                    if bash.is_file() {
                        return Some(bash);
                    }
                }
            }
        }
    }
    [("ProgramFiles", "Git"), ("ProgramW6432", "Git"), ("LOCALAPPDATA", "Programs/Git")]
        .iter()
        .filter_map(|(var, sub)| std::env::var_os(var).map(|base| PathBuf::from(base).join(sub).join("bin").join("bash.exe")))
        .find(|p| p.is_file())
}

fn is_plain(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '+' | ','))
}

fn single_quoted_posix(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// `exe arg` written so that the given shell runs `exe` with the single argument `arg`.
///
/// POSIX shells get a single-quoted path. On Windows the path uses forward
/// slashes, which both Git Bash and PowerShell accept; a path made of plain
/// characters stays unquoted so the command works in either shell, otherwise
/// Git Bash gets single quotes and PowerShell the call operator `& '...'`.
pub fn command_line(shell: Shell, exe: &Path, arg: &str) -> String {
    let raw = exe.to_string_lossy();
    match shell {
        Shell::Posix => format!("{} {arg}", single_quoted_posix(&raw)),
        Shell::GitBash | Shell::PowerShell => {
            let path = raw.replace('\\', "/");
            if is_plain(&path) {
                format!("{path} {arg}")
            } else if shell == Shell::GitBash {
                format!("{} {arg}", single_quoted_posix(&path))
            } else {
                format!("& '{}' {arg}", path.replace('\'', "''"))
            }
        }
    }
}

/// Splits a command line into words, honouring single and double quotes and
/// backslash escapes, and skipping a leading PowerShell call operator.
pub fn split_words(cmd: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = cmd.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    cur.push(q);
                }
            }
            '"' => {
                in_word = true;
                while let Some(q) = chars.next() {
                    match q {
                        '"' => break,
                        '\\' if matches!(chars.peek(), Some('"' | '\\' | '$' | '\x60')) => {
                            if let Some(n) = chars.next() {
                                cur.push(n);
                            }
                        }
                        _ => cur.push(q),
                    }
                }
            }
            '\\' if matches!(chars.peek(), Some(' ' | '\'' | '"' | '\\')) => {
                in_word = true;
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    if words.first().is_some_and(|w| w == "&") {
        words.remove(0);
    }
    words
}

/// File name of an executable path without directory and `.exe`, lowercased.
pub fn program_stem(program: &str) -> String {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program).to_ascii_lowercase();
    match name.strip_suffix(".exe") {
        Some(stem) => stem.to_string(),
        None => name,
    }
}

/// True when `cmd` runs a Kanjo executable with exactly the argument `arg`.
/// Kanjo is recognised by the program's file name (`kanjo`, or the name of
/// `exe` when given), so an entry survives a moved or reinstalled binary.
pub fn is_kanjo_command(cmd: &str, arg: &str, exe: Option<&Path>) -> bool {
    let words = split_words(cmd);
    let [program, a] = words.as_slice() else {
        return false;
    };
    if a != arg {
        return false;
    }
    let stem = program_stem(program);
    stem == "kanjo" || exe.is_some_and(|e| program_stem(&e.to_string_lossy()) == stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_for_each_shell() {
        let p = Path::new("/opt/kanjo tools/it's/kanjo");
        assert_eq!(command_line(Shell::Posix, p, "hook"), r"'/opt/kanjo tools/it'\''s/kanjo' hook");
        assert_eq!(command_line(Shell::Posix, Path::new("/usr/local/bin/kanjo"), "statusline"), "'/usr/local/bin/kanjo' statusline");

        let plain = Path::new(r"C:\Users\dev\AppData\Local\Programs\kanjo\kanjo.exe");
        assert_eq!(command_line(Shell::GitBash, plain, "hook"), "C:/Users/dev/AppData/Local/Programs/kanjo/kanjo.exe hook");
        assert_eq!(command_line(Shell::PowerShell, plain, "hook"), "C:/Users/dev/AppData/Local/Programs/kanjo/kanjo.exe hook");

        let spaced = Path::new(r"C:\Program Files\Kanjo\kanjo.exe");
        assert_eq!(command_line(Shell::GitBash, spaced, "statusline"), "'C:/Program Files/Kanjo/kanjo.exe' statusline");
        assert_eq!(command_line(Shell::PowerShell, spaced, "statusline"), "& 'C:/Program Files/Kanjo/kanjo.exe' statusline");
        let odd = Path::new(r"C:\Users\O'Neil Dev\kanjo.exe");
        assert_eq!(command_line(Shell::PowerShell, odd, "hook"), "& 'C:/Users/O''Neil Dev/kanjo.exe' hook");
    }

    #[test]
    fn splits_what_it_quotes() {
        for shell in [Shell::Posix, Shell::GitBash] {
            let p = Path::new("/opt/kanjo tools/it's/kanjo");
            assert_eq!(split_words(&command_line(shell, p, "hook")), vec!["/opt/kanjo tools/it's/kanjo", "hook"]);
        }
        assert_eq!(split_words("& 'C:/Program Files/Kanjo/kanjo.exe' statusline"), vec!["C:/Program Files/Kanjo/kanjo.exe", "statusline"]);
        assert_eq!(split_words(r#""C:\Program Files\Kanjo\kanjo.exe" hook"#), vec![r"C:\Program Files\Kanjo\kanjo.exe", "hook"]);
        assert_eq!(split_words(r"/a\ b/kanjo  hook "), vec!["/a b/kanjo", "hook"]);
    }

    #[test]
    fn recognises_kanjo_commands() {
        assert!(is_kanjo_command("'/old/place/kanjo' statusline", "statusline", None));
        assert!(is_kanjo_command("& 'C:/Program Files/Kanjo/KANJO.EXE' hook", "hook", None));
        assert!(is_kanjo_command("/x/kanjo-dev hook", "hook", Some(Path::new("/y/kanjo-dev"))));
        assert!(!is_kanjo_command("/x/kanjo-dev hook", "hook", None));
        assert!(!is_kanjo_command("'/x/kanjo' statusline --extra", "statusline", None));
        assert!(!is_kanjo_command("npx ccstatusline@latest", "statusline", None));
        assert!(!is_kanjo_command("/x/kanjo statusline", "hook", None));
    }
}

//! Local delivery of alerts: a desktop notification (D-Bus on Linux through a
//! pure-Rust client, the native centre on macOS and Windows) and the terminal
//! bell, which rings only in the terminal the server itself runs in. Nothing here
//! touches the network; failures are logged, never returned.

use crate::thresholds::MAX_PENDING_NOTIFICATIONS;
use kanjo_core::api::{Alert, Severity};
use kanjo_core::settings::AlertSettings;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static PENDING: AtomicUsize = AtomicUsize::new(0);

/// Delivers one alert through the channels enabled in `settings`. Returns at
/// once: the desktop notification is shown from a background thread.
pub fn deliver(alert: &Alert, settings: &AlertSettings) {
    if settings.terminal_bell {
        bell();
    }
    if settings.desktop_notifications {
        desktop(alert);
    }
}

/// Whether the terminal bell can ring: the server has a terminal, which is the case when
/// `kanjo serve` runs in the foreground of a terminal and never when Kanjo runs as a
/// background service. The bell rings in that terminal, not in Claude Code's.
pub fn bell_available() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal() || controlling_terminal()
}

#[cfg(unix)]
fn controlling_terminal() -> bool {
    std::fs::OpenOptions::new().write(true).open("/dev/tty").is_ok()
}

#[cfg(not(unix))]
fn controlling_terminal() -> bool {
    false
}

fn bell() {
    use std::io::IsTerminal;
    static WARNED: AtomicBool = AtomicBool::new(false);
    let stderr = std::io::stderr();
    let rung = if stderr.is_terminal() { stderr.lock().write_all(b"\x07").and_then(|_| stderr.lock().flush()) } else { tty_bell() };
    if let Err(e) = rung {
        if !WARNED.swap(true, Ordering::SeqCst) {
            tracing::warn!(error = %e, "the terminal bell is on but the server has no terminal (it rings only when kanjo serve runs in the foreground)");
        }
    }
}

#[cfg(unix)]
fn tty_bell() -> std::io::Result<()> {
    std::fs::OpenOptions::new().write(true).open("/dev/tty")?.write_all(b"\x07")
}

#[cfg(not(unix))]
fn tty_bell() -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no terminal"))
}

struct PendingGuard;

impl Drop for PendingGuard {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::SeqCst);
    }
}

fn desktop(alert: &Alert) {
    if PENDING.fetch_add(1, Ordering::SeqCst) >= MAX_PENDING_NOTIFICATIONS {
        PENDING.fetch_sub(1, Ordering::SeqCst);
        tracing::warn!(id = alert.id, "desktop notification dropped: earlier ones are still pending");
        return;
    }
    let guard = PendingGuard;
    let (id, title, body, severity) = (alert.id, alert.title.clone(), alert.message.clone(), alert.severity);
    let spawned = std::thread::Builder::new().name("kanjo-notify".into()).spawn(move || {
        let _guard = guard;
        match show(&title, &body, severity) {
            Ok(()) => tracing::debug!(id, "desktop notification shown"),
            Err(e) => tracing::warn!(id, error = %e, "desktop notification failed"),
        }
    });
    if let Err(e) = spawned {
        tracing::warn!(id, error = %e, "cannot start the notification thread");
    }
}

fn show(title: &str, body: &str, severity: Severity) -> Result<(), notify_rust::error::Error> {
    let mut n = notify_rust::Notification::new();
    n.appname("Kanjo").summary(title).body(body);
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        n.urgency(match severity {
            Severity::Critical => notify_rust::Urgency::Critical,
            Severity::Warning => notify_rust::Urgency::Normal,
            Severity::Info => notify_rust::Urgency::Low,
        });
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    let _ = severity;
    n.show().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_channels_do_nothing() {
        let alert = Alert {
            id: 1,
            ts: 0,
            kind: kanjo_core::api::AlertKind::CostSpike,
            severity: Severity::Info,
            params: serde_json::json!({}),
            title: "t".into(),
            message: "m".into(),
            acknowledged_at: None,
        };
        let settings = AlertSettings { desktop_notifications: false, terminal_bell: false, ..AlertSettings::default() };
        deliver(&alert, &settings);
        assert_eq!(PENDING.load(Ordering::SeqCst), 0);
    }
}

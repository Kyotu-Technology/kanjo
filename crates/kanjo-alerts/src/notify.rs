//! Local delivery of alerts: a desktop notification (D-Bus on Linux through a
//! pure-Rust client, the native centre on macOS and Windows) and the terminal
//! bell. Nothing here touches the network; failures are logged, never returned.

use crate::thresholds::MAX_PENDING_NOTIFICATIONS;
use kanjo_core::api::{Alert, Severity};
use kanjo_core::settings::AlertSettings;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

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

fn bell() {
    use std::io::IsTerminal;
    let stderr = std::io::stderr();
    if stderr.is_terminal() {
        if let Err(e) = stderr.lock().write_all(b"\x07") {
            tracing::debug!(error = %e, "terminal bell failed");
        }
        return;
    }
    #[cfg(unix)]
    match std::fs::OpenOptions::new().write(true).open("/dev/tty") {
        Ok(mut tty) => {
            if let Err(e) = tty.write_all(b"\x07") {
                tracing::debug!(error = %e, "terminal bell failed");
            }
        }
        Err(e) => tracing::debug!(error = %e, "no terminal for the bell"),
    }
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

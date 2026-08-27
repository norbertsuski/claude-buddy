use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;

use mac_notification_sys::{Notification, NotificationResponse};
use tauri::Emitter;

use crate::config::{self, Config};
use crate::watcher::alerts::{Alert, AlertKind};
use crate::watcher::watch::now_ms;

/// Emitted when a notification could not be shown, so the widget can flash
/// instead of the alert being lost.
pub const FLASH_EVENT: &str = "ui://flash";

/// How many notifications may be waiting on a click at once.
///
/// Waiting for a click blocks the sending thread until macOS resolves the
/// notification, and a notification the user simply ignores may never resolve.
/// Past this many outstanding waiters, alerts are delivered without waiting:
/// still shown, just not clickable through to a session. Eight unanswered
/// alerts means the user is not reading them anyway.
pub const MAX_CLICK_WAITERS: usize = 8;

static OUTSTANDING: AtomicUsize = AtomicUsize::new(0);
static APPLICATION: Once = Once::new();

/// Whether a new notification can afford to wait for a click.
pub fn should_wait_for_click(outstanding: usize) -> bool {
    outstanding < MAX_CLICK_WAITERS
}

/// Whether this alert reaches the user, given their settings.
///
/// The sound is the parent of the three event switches in Settings, so it gates
/// them here too rather than only in the form: an alert is delivered as a
/// notification with a sound, and turning the sound off turns the group off. A
/// config file hand-edited to leave an event armed under a silent parent is
/// answered the same way the form would answer it.
pub fn should_deliver(alert: &Alert, config: &Config, now_ms: i64) -> bool {
    if !config.sound || config.alerts_muted(now_ms) {
        return false;
    }
    match alert.kind {
        AlertKind::NeedsInput => config.alert_needs_input,
        AlertKind::Died => config.alert_died,
        AlertKind::Finished => config.alert_finished,
    }
}

pub fn alert_text(alert: &Alert) -> (String, String) {
    match alert.kind {
        AlertKind::NeedsInput => (
            format!("{} needs you", alert.name),
            alert
                .detail
                .clone()
                .unwrap_or_else(|| "waiting for input".to_string()),
        ),
        AlertKind::Died => (
            format!("{} died", alert.name),
            "the session's process is gone".to_string(),
        ),
        AlertKind::Finished => (
            format!("{} finished", alert.name),
            "the session is idle again".to_string(),
        ),
    }
}

/// Point the notification centre at this app, once per process.
///
/// Under `tauri dev` the binary is not inside a bundle, so there is no
/// identifier to register; borrowing Terminal's is what the Tauri notification
/// plugin did and it keeps notifications working in development.
fn ensure_application(identifier: &str) {
    APPLICATION.call_once(|| {
        let id = if tauri::is_dev() {
            "com.apple.Terminal"
        } else {
            identifier
        };
        let _ = mac_notification_sys::set_application(id);
    });
}

/// Deliver alerts as native notifications.
///
/// Deliberately not `tauri-plugin-notification`: its desktop path spawns
/// `notify_rust::Notification::show()` and discards the result, so it can
/// neither report a delivery failure nor tell us that the user clicked. Both
/// matter here — the flash fallback depends on the first, and click-to-raise on
/// the second.
///
/// Settings are re-read per batch rather than cached, so toggling an alert or
/// muting takes effect immediately without restarting the watcher.
pub fn deliver(app: &tauri::AppHandle, alerts: &[Alert]) {
    if alerts.is_empty() {
        return;
    }

    let config = config::cached();
    let now = now_ms();
    ensure_application(&app.config().identifier);

    for alert in alerts {
        if !should_deliver(alert, &config, now) {
            continue;
        }
        let (title, body) = alert_text(alert);
        let wait = should_wait_for_click(OUTSTANDING.load(Ordering::Relaxed));
        if wait {
            OUTSTANDING.fetch_add(1, Ordering::Relaxed);
        }

        let handle = app.clone();
        let alert = alert.clone();
        let sound = config.sound;

        // One thread per notification: sending blocks until the user resolves
        // it when we are waiting for a click.
        std::thread::spawn(move || {
            let mut options = Notification::new();
            options.wait_for_click(wait);
            if sound {
                options.default_sound();
            }

            let result =
                mac_notification_sys::send_notification(&title, None, &body, Some(&options));

            if wait {
                OUTSTANDING.fetch_sub(1, Ordering::Relaxed);
            }

            match result {
                Ok(NotificationResponse::Click) | Ok(NotificationResponse::ActionButton(_)) => {
                    let _ = crate::bridge::raise::raise_pid(alert.pid);
                }
                Ok(_) => {}
                // The usual cause is denied permission, so fall back to
                // flashing the widget — otherwise a user who declined the
                // prompt gets no signal at all.
                Err(_) => {
                    let _ = handle.emit(FLASH_EVENT, &alert);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::alerts::{Alert, AlertKind};

    fn alert(kind: AlertKind) -> Alert {
        Alert {
            session_id: "id-a".into(),
            pid: 4242,
            name: "api-service-55".into(),
            kind,
            detail: match kind {
                AlertKind::NeedsInput => Some("input needed".into()),
                AlertKind::Died | AlertKind::Finished => None,
            },
        }
    }

    #[test]
    fn a_click_waiter_is_attached_while_there_is_budget() {
        assert!(should_wait_for_click(0));
        assert!(should_wait_for_click(MAX_CLICK_WAITERS - 1));
    }

    #[test]
    fn the_waiter_budget_is_a_hard_cap() {
        // A notification nobody touches parks its thread until macOS resolves
        // it, which may be never. Past the cap, alerts are still delivered —
        // they just cannot be clicked through.
        assert!(!should_wait_for_click(MAX_CLICK_WAITERS));
        assert!(!should_wait_for_click(MAX_CLICK_WAITERS + 1));
    }

    #[test]
    fn defaults_deliver_both_kinds() {
        let config = Config::default();
        assert!(should_deliver(&alert(AlertKind::NeedsInput), &config, 0));
        assert!(should_deliver(&alert(AlertKind::Died), &config, 0));
    }

    #[test]
    fn disabling_needs_input_suppresses_only_that_kind() {
        let mut config = Config::default();
        config.alert_needs_input = false;

        assert!(!should_deliver(&alert(AlertKind::NeedsInput), &config, 0));
        assert!(should_deliver(&alert(AlertKind::Died), &config, 0));
    }

    #[test]
    fn disabling_died_suppresses_only_that_kind() {
        let mut config = Config::default();
        config.alert_died = false;

        assert!(should_deliver(&alert(AlertKind::NeedsInput), &config, 0));
        assert!(!should_deliver(&alert(AlertKind::Died), &config, 0));
    }

    #[test]
    fn an_active_mute_suppresses_everything() {
        let mut config = Config::default();
        config.mute_until_ms = 10_000;

        assert!(!should_deliver(
            &alert(AlertKind::NeedsInput),
            &config,
            9_999
        ));
        assert!(!should_deliver(&alert(AlertKind::Died), &config, 9_999));
    }

    #[test]
    fn an_expired_mute_delivers_again() {
        let mut config = Config::default();
        config.mute_until_ms = 10_000;
        assert!(should_deliver(
            &alert(AlertKind::NeedsInput),
            &config,
            10_000
        ));
    }

    #[test]
    fn needs_input_text_names_the_session_and_its_reason() {
        let (title, body) = alert_text(&alert(AlertKind::NeedsInput));
        assert_eq!(title, "api-service-55 needs you");
        assert_eq!(body, "input needed");
    }

    #[test]
    fn needs_input_text_survives_a_missing_reason() {
        let mut a = alert(AlertKind::NeedsInput);
        a.detail = None;
        let (_, body) = alert_text(&a);
        assert_eq!(body, "waiting for input");
    }

    #[test]
    fn died_text_says_so_plainly() {
        let (title, body) = alert_text(&alert(AlertKind::Died));
        assert_eq!(title, "api-service-55 died");
        assert_eq!(body, "the session's process is gone");
    }

    #[test]
    fn finished_is_off_by_default() {
        let mut a = alert(AlertKind::NeedsInput);
        a.kind = AlertKind::Finished;
        assert!(!should_deliver(&a, &Config::default(), 0));
    }

    #[test]
    fn enabling_finished_delivers_it() {
        let mut config = Config::default();
        config.alert_finished = true;
        let mut a = alert(AlertKind::NeedsInput);
        a.kind = AlertKind::Finished;
        assert!(should_deliver(&a, &config, 0));
    }

    #[test]
    fn finished_text_says_the_turn_is_done() {
        let mut a = alert(AlertKind::NeedsInput);
        a.kind = AlertKind::Finished;
        a.detail = None;
        let (title, body) = alert_text(&a);
        assert_eq!(title, "api-service-55 finished");
        assert_eq!(body, "the session is idle again");
    }
}

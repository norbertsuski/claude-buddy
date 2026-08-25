use tauri::Emitter;
use tauri_plugin_notification::NotificationExt;

use crate::config::{self, Config};
use crate::watcher::alerts::{Alert, AlertKind};
use crate::watcher::watch::now_ms;

/// Emitted when a notification could not be shown, so the widget can flash
/// instead of the alert being lost.
pub const FLASH_EVENT: &str = "ui://flash";

/// Whether this alert reaches the user, given their settings.
pub fn should_deliver(alert: &Alert, config: &Config, now_ms: i64) -> bool {
    if config.alerts_muted(now_ms) {
        return false;
    }
    match alert.kind {
        AlertKind::NeedsInput => config.alert_needs_input,
        AlertKind::Died => config.alert_died,
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
    }
}

/// Deliver alerts as native notifications.
///
/// Settings are re-read per batch rather than cached, so toggling an alert or
/// muting takes effect immediately without restarting the watcher.
pub fn deliver(app: &tauri::AppHandle, alerts: &[Alert]) {
    if alerts.is_empty() {
        return;
    }

    let config = config::cached();
    let now = now_ms();

    for alert in alerts {
        if !should_deliver(alert, &config, now) {
            continue;
        }
        let (title, body) = alert_text(alert);
        let mut builder = app.notification().builder().title(title).body(body);
        if config.sound {
            builder = builder.sound("default");
        }

        // A failed notification must not stop the remaining alerts. The usual
        // cause is denied permission, so fall back to flashing the widget —
        // otherwise a user who declined the prompt gets no signal at all.
        if builder.show().is_err() {
            let _ = app.emit(FLASH_EVENT, alert);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::alerts::{Alert, AlertKind};

    fn alert(kind: AlertKind) -> Alert {
        Alert {
            session_id: "id-a".into(),
            name: "api-service-55".into(),
            kind,
            detail: match kind {
                AlertKind::NeedsInput => Some("input needed".into()),
                AlertKind::Died => None,
            },
        }
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

        assert!(!should_deliver(&alert(AlertKind::NeedsInput), &config, 9_999));
        assert!(!should_deliver(&alert(AlertKind::Died), &config, 9_999));
    }

    #[test]
    fn an_expired_mute_delivers_again() {
        let mut config = Config::default();
        config.mute_until_ms = 10_000;
        assert!(should_deliver(&alert(AlertKind::NeedsInput), &config, 10_000));
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
}

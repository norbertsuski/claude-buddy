use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

/// Whether an update was found, exposed so the tray item can appear.
pub static AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Whether a signing key is configured, and so whether the updater can run.
///
/// The key is the only switch. `tauri.conf.json` ships `pubkey` empty, which
/// keeps `tauri build` happy — it insists the field exists, and equally
/// refuses to bundle updater artifacts for a non-empty key it cannot also
/// sign. An empty key could never verify a download anyway, so rather than
/// checking against a server that will never be trusted, the plugin is simply
/// not registered: `app.updater()` fails and every path here returns quietly.
pub fn is_configured(updater_config: Option<&Value>) -> bool {
    updater_config
        .and_then(|config| config.get("pubkey"))
        .and_then(Value::as_str)
        .is_some_and(|key| !key.trim().is_empty())
}

/// Check once, in the background, and tell the user if there is something newer.
///
/// Deliberately not automatic: replacing a running menu-bar app under the user
/// without asking is the kind of surprise this widget exists to avoid. The
/// check only sets a flag and notifies; installing is a menu item.
pub fn check_on_launch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Ok(updater) = app.updater() else { return };
        match updater.check().await {
            Ok(Some(update)) => {
                AVAILABLE.store(true, Ordering::Relaxed);
                let version = update.version.clone();
                // Sending blocks until the notification centre answers, so it
                // gets a thread rather than a slot in the async runtime.
                std::thread::spawn(move || {
                    let mut options = mac_notification_sys::Notification::new();
                    options.wait_for_click(false);
                    let _ = mac_notification_sys::send_notification(
                        "clawde-buddy update available",
                        None,
                        &format!("version {version} — install it from the tray menu"),
                        Some(&options),
                    );
                });
            }
            // No update, or no reachable manifest. Neither is worth surfacing:
            // a widget that nags about its own update server is worse than one
            // that quietly stays on the version you installed.
            _ => {}
        }
    });
}

/// Download and install, then restart.
pub fn install(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Ok(updater) = app.updater() else { return };
        if let Ok(Some(update)) = updater.check().await {
            if update.download_and_install(|_, _| {}, || {}).await.is_ok() {
                app.restart();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_pubkey_makes_the_updater_live() {
        let config = json!({ "endpoints": ["https://example.test/latest.json"], "pubkey": "abc" });
        assert!(is_configured(Some(&config)));
    }

    #[test]
    fn a_missing_updater_block_leaves_it_off() {
        assert!(!is_configured(None));
    }

    #[test]
    fn an_absent_pubkey_leaves_it_off() {
        let config = json!({ "endpoints": ["https://example.test/latest.json"] });
        assert!(!is_configured(Some(&config)));
    }

    #[test]
    fn a_blank_pubkey_leaves_it_off() {
        // The shipped state: the field is there because `tauri build` demands
        // it, but an empty key could never verify anything.
        assert!(!is_configured(Some(&json!({ "pubkey": "" }))));
        assert!(!is_configured(Some(&json!({ "pubkey": "  \n" }))));
    }
}

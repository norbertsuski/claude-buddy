use std::sync::Mutex;

use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

/// The version the last successful check found newer, or `None` for "nothing
/// known".
///
/// Held so the tray menu can name it. Without this the launch check tells the
/// user "version 0.7.0 — install it from the menu bar" and the menu they then
/// open still says "Check for updates…", as though the check had not happened.
static AVAILABLE: Mutex<Option<String>> = Mutex::new(None);

/// The version waiting to be installed, as far as anyone has checked.
pub fn available() -> Option<String> {
    AVAILABLE
        .lock()
        .expect("available version poisoned")
        .clone()
}

/// Record what a check found. `None` clears — being told you are up to date is
/// itself news, and the menu must stop advertising a version that is now
/// installed.
fn set_available(app: &AppHandle, version: Option<String>) {
    if !record_available(version) {
        return;
    }
    // Menu rebuilds are AppKit calls; both callers of this are on the async
    // runtime.
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || crate::tray::refresh(&handle));
}

/// Write the slot, returning whether that changed anything.
///
/// Separate from [`set_available`] so the bookkeeping is testable without an
/// `AppHandle`, and so an unchanged answer does not rebuild the menu — every
/// launch of an up-to-date copy would otherwise redraw it for nothing.
fn record_available(version: Option<String>) -> bool {
    let mut slot = AVAILABLE.lock().expect("available version poisoned");
    if *slot == version {
        return false;
    }
    *slot = version;
    true
}

/// What the tray's update item should read.
///
/// One item and one handler for both cases: `check_and_install` already
/// installs when there is an update and reports being current when there is
/// not, so this only stops the label misdescribing which of the two is about to
/// happen.
pub fn update_label(available: Option<&str>) -> String {
    match available {
        Some(version) => format!("Install update {version}…"),
        None => "Check for updates…".to_string(),
    }
}

/// Whether a signing key is configured, and so whether the updater can run.
///
/// The key is the only switch. `tauri.conf.json` ships `pubkey` empty, which
/// keeps `tauri build` happy — it insists the field exists, and equally
/// refuses to bundle updater artifacts for a non-empty key it cannot also
/// sign. An empty key could never verify a download anyway, so rather than
/// checking against a server that will never be trusted, the plugin is simply
/// not registered. Both entry points below check this first, because
/// `app.updater()` does not fail quietly when the plugin is missing — it panics
/// on `state()` for a state nobody managed, which on a spawned task is a panic
/// nobody sees.
pub fn is_configured(updater_config: Option<&Value>) -> bool {
    updater_config
        .and_then(|config| config.get("pubkey"))
        .and_then(Value::as_str)
        .is_some_and(|key| !key.trim().is_empty())
}

/// Post a notification, off the calling thread.
///
/// Sending blocks until the notification centre answers, so this never runs on
/// the async runtime or the main thread.
fn tell(app: &AppHandle, title: String, body: String) {
    crate::notify::ensure_application(&app.config().identifier);
    std::thread::spawn(move || {
        let mut options = mac_notification_sys::Notification::new();
        options.wait_for_click(false);
        let _ = mac_notification_sys::send_notification(&title, None, &body, Some(&options));
    });
}

/// Check once, in the background, and tell the user if there is something newer.
///
/// Deliberately not automatic: replacing a running menu-bar app under the user
/// without asking is the kind of surprise this widget exists to avoid. The
/// check only notifies; installing is a menu item.
///
/// Unlike [`check_and_install`], this stays quiet about everything else. Nobody
/// asked for this check, so "you are up to date" and "the manifest was
/// unreachable" are both noise — a widget that nags about its own update server
/// is worse than one that quietly stays on the version you installed.
pub fn check_on_launch(app: AppHandle) {
    if !is_configured(app.config().plugins.0.get("updater")) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let Ok(updater) = app.updater() else { return };
        if let Ok(Some(update)) = updater.check().await {
            set_available(&app, Some(update.version.clone()));
            tell(
                &app,
                "claude-buddy update available".to_string(),
                format!(
                    "version {} — install it from the menu bar",
                    update.version.clone()
                ),
            );
        }
    });
}

/// Check, install if there is anything to install, then restart.
///
/// Every branch reports. This runs because the user chose "Check for updates…"
/// from the menu, and a menu item that silently does nothing three times out of
/// four — already current, unreachable manifest, failed download — is
/// indistinguishable from a broken one. Being current is the *common* answer,
/// so it is the one that most needs saying.
pub fn check_and_install(app: AppHandle) {
    if !is_configured(app.config().plugins.0.get("updater")) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let current = app.package_info().version.to_string();
        let updater = match app.updater() {
            Ok(updater) => updater,
            Err(e) => return tell(&app, "Update check failed".into(), e.to_string()),
        };

        match updater.check().await {
            Ok(Some(update)) => {
                let version = update.version.clone();
                set_available(&app, Some(version.clone()));
                // Before the download, not after: it can take a while on a slow
                // connection, and the app restarts out from under the user when
                // it succeeds. This is the only warning they get.
                tell(
                    &app,
                    format!("Installing claude-buddy {version}"),
                    "downloading — the widget will restart itself".into(),
                );
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(()) => app.restart(),
                    Err(e) => tell(
                        &app,
                        format!("claude-buddy {version} could not be installed"),
                        e.to_string(),
                    ),
                }
            }
            Ok(None) => {
                set_available(&app, None);
                tell(
                    &app,
                    "claude-buddy is up to date".into(),
                    format!("version {current}"),
                )
            }
            Err(e) => tell(&app, "Update check failed".into(), e.to_string()),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The version slot is a global, so each test that touches it puts it back.
    /// `cargo test -- --test-threads=1` is what this crate runs under, so they
    /// cannot interleave.
    fn reset_available() {
        record_available(None);
    }

    #[test]
    fn no_known_version_offers_a_check() {
        assert_eq!(update_label(None), "Check for updates…");
    }

    #[test]
    fn a_known_version_offers_to_install_it_by_name() {
        assert_eq!(update_label(Some("0.7.0")), "Install update 0.7.0…");
    }

    #[test]
    fn the_version_slot_round_trips_and_clears() {
        reset_available();
        assert_eq!(available(), None);

        assert!(record_available(Some("0.7.0".into())));
        assert_eq!(available().as_deref(), Some("0.7.0"));

        // Being told you are current has to clear it, or the menu keeps
        // offering to install a version that is already running.
        assert!(record_available(None));
        assert_eq!(available(), None);
    }

    #[test]
    fn writing_the_same_version_twice_is_not_a_change() {
        reset_available();
        assert!(record_available(Some("0.7.0".into())));
        assert!(!record_available(Some("0.7.0".into())));
        reset_available();
    }

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

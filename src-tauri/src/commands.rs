use std::path::Path;

use tauri::Manager;

use crate::config::{self, Config};

/// Reject settings that would break the widget rather than writing them.
/// A zero paused threshold would mark every session paused instantly.
pub fn validate(config: &Config) -> Result<(), String> {
    if config.paused_threshold_ms <= 0 {
        return Err("paused threshold must be greater than zero".into());
    }
    if !crate::visibility::HIDE_MODES.contains(&config.hide_when.as_str()) {
        return Err(format!("unknown hide mode: {}", config.hide_when));
    }
    Ok(())
}

pub fn persist(path: &Path, config: &Config) -> Result<(), String> {
    validate(config)?;
    config::save(path, config).map_err(|e| format!("could not write config: {e}"))
}

/// Current sessions, for the frontend to fetch on mount.
#[tauri::command]
pub fn get_sessions(
    store: tauri::State<'_, crate::watcher::watch::SnapshotStore>,
) -> Vec<crate::watcher::state::SessionSnapshot> {
    store.get()
}

#[tauri::command]
pub fn get_config() -> Config {
    config::load(&config::config_path())
}

#[tauri::command]
pub fn set_config(app: tauri::AppHandle, config: Config) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;

    persist(&config::config_path(), &config)?;

    // Applying a display choice immediately is the whole point of the setting.
    if let Some(widget) = app.get_webview_window("widget") {
        crate::window::restore_position(&widget);
    }

    let manager = app.autolaunch();
    let result = if config.launch_at_login {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|e| format!("could not update launch at login: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_config_through_the_persist_helper() {
        let path = std::env::temp_dir().join(format!("cb-cmd-{}.json", std::process::id()));
        let mut config = Config::default();
        config.sound = true;
        config.paused_threshold_ms = 5 * 60 * 1000;

        persist(&path, &config).unwrap();

        assert_eq!(crate::config::load(&path), config);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn rejects_a_nonsensical_paused_threshold() {
        let mut config = Config::default();
        config.paused_threshold_ms = 0;
        assert!(validate(&config).is_err());

        config.paused_threshold_ms = -1;
        assert!(validate(&config).is_err());
    }

    #[test]
    fn rejects_an_unknown_hide_mode() {
        let mut config = Config::default();
        config.hide_when = "sometimes".into();
        assert!(validate(&config).is_err());
    }

    #[test]
    fn accepts_every_hide_mode() {
        for mode in crate::visibility::HIDE_MODES {
            let mut config = Config::default();
            config.hide_when = mode.into();
            assert!(validate(&config).is_ok(), "{mode} should be valid");
        }
    }
}

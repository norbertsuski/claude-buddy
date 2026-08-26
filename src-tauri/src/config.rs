use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::watcher::state::PAUSED_THRESHOLD_MS;

/// User settings. Hand-editable JSON: every field has a default so a
/// half-written file still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Vestigial. The view modes were removed in favour of `hide_when`, but the
    /// field is kept so an existing config file still parses and is not silently
    /// rewritten without it.
    pub view_mode: String,
    pub paused_threshold_ms: i64,
    pub alert_needs_input: bool,
    pub alert_died: bool,
    /// Whether finishing a turn interrupts you. Off by default: a finished turn
    /// is the common case, and alerting on it is the noisy choice.
    pub alert_finished: bool,
    pub sound: bool,
    /// Epoch millis until which alerts stay suppressed. Backs "Mute alerts 1h".
    pub mute_until_ms: i64,
    pub launch_at_login: bool,
    /// Whether background jobs and subagents appear at all. They are shown
    /// demoted when enabled, since they belong to a session rather than being
    /// one.
    pub show_background_jobs: bool,
    /// Whether the widget times each animation to the distance it covers and
    /// fades chips in as they appear, rather than giving every change the one
    /// duration tuned for the widest morph. On by default; off restores the
    /// fixed timing for anyone who prefers it.
    pub smooth_status_changes: bool,
    /// Whether the five-hour limit meter appears at the end of the collapsed
    /// row. On by default, but worth being able to turn off: the figure behind
    /// it is a cache Claude Code refreshes only when it fetches usage, so the
    /// meter is absent whenever that cache describes a window that has passed.
    pub show_usage: bool,
    /// When the widget takes itself off screen: `never`, `noSessions` or
    /// `nothingActive`. The tray icon always remains, so a hidden widget is
    /// never unreachable.
    pub hide_when: String,
    /// Display key the widget should appear on, or `None` for the primary
    /// display. Keys come from `list_displays`.
    pub preferred_display: Option<String>,
    /// Widget position keyed by display identifier, so docking and undocking a
    /// monitor does not leave the widget off-screen.
    pub positions: HashMap<String, [f64; 2]>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            view_mode: "dotRow".into(),
            paused_threshold_ms: PAUSED_THRESHOLD_MS,
            alert_needs_input: true,
            alert_died: true,
            alert_finished: false,
            sound: false,
            mute_until_ms: 0,
            launch_at_login: false,
            show_background_jobs: true,
            smooth_status_changes: true,
            show_usage: true,
            hide_when: "noSessions".into(),
            preferred_display: None,
            positions: HashMap::new(),
        }
    }
}

impl Config {
    pub fn alerts_muted(&self, now_ms: i64) -> bool {
        now_ms < self.mute_until_ms
    }
}

/// In-memory copy of the settings file.
///
/// Placement and alert delivery both consult settings on paths that run while
/// the widget is animating; re-reading the file each time put disk I/O on that
/// path for no benefit.
static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<Config>>> = std::sync::OnceLock::new();

fn cache() -> &'static std::sync::Mutex<Option<Config>> {
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

/// Settings, read from disk once and then from memory.
pub fn cached() -> Config {
    let mut slot = cache().lock().expect("config cache poisoned");
    if let Some(config) = slot.as_ref() {
        return config.clone();
    }
    let loaded = load(&config_path());
    *slot = Some(loaded.clone());
    loaded
}

/// Forget the cached copy, so the next read comes from disk.
pub fn invalidate_cache() {
    *cache().lock().expect("config cache poisoned") = None;
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join("Library")
        .join("Application Support")
        .join("com.clawde.buddy")
        .join("config.json")
}

/// Load settings, falling back to defaults for a missing or corrupt file.
/// Never fails: a broken config must not prevent the widget from starting.
pub fn load(path: &Path) -> Config {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, config: &Config) -> std::io::Result<()> {
    *cache().lock().expect("config cache poisoned") = Some(config.clone());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cb-config-{}-{tag}.json", std::process::id()))
    }

    #[test]
    fn saving_refreshes_the_cache() {
        let path = temp_path("cache");
        let mut c = Config::default();
        c.sound = true;
        save(&path, &c).unwrap();

        assert!(cached().sound, "cache should reflect the save");

        invalidate_cache();
        std::fs::remove_file(&path).unwrap();
        // Reading again falls back to the real file, which may not exist here;
        // the point is only that invalidation clears the stored copy.
    }

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.view_mode, "dotRow");
        assert_eq!(c.paused_threshold_ms, crate::watcher::state::PAUSED_THRESHOLD_MS);
        assert!(c.alert_needs_input);
        assert!(c.alert_died);
        assert!(!c.alert_finished);
        assert!(!c.sound);
        assert_eq!(c.mute_until_ms, 0);
        assert!(!c.launch_at_login);
        assert!(c.show_background_jobs);
        assert!(c.smooth_status_changes);
        assert!(c.show_usage);
        assert_eq!(c.hide_when, "noSessions");
        assert_eq!(c.preferred_display, None);
        assert!(c.positions.is_empty());
    }

    #[test]
    fn missing_file_yields_defaults() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load(&path), Config::default());
    }

    #[test]
    fn corrupt_file_yields_defaults_rather_than_panicking() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), Config::default());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn partial_file_fills_unspecified_keys_from_defaults() {
        let path = temp_path("partial");
        std::fs::write(&path, r#"{"sound": true}"#).unwrap();

        let c = load(&path);

        assert!(c.sound);
        assert_eq!(c.view_mode, "dotRow");
        assert_eq!(c.paused_threshold_ms, crate::watcher::state::PAUSED_THRESHOLD_MS);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path("roundtrip");
        let mut c = Config::default();
        c.sound = true;
        c.hide_when = "nothingActive".into();
        c.positions.insert("display-1".into(), [120.0, 44.5]);

        save(&path, &c).unwrap();

        assert_eq!(load(&path), c);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn save_creates_missing_parent_directories() {
        let dir = std::env::temp_dir().join(format!("cb-cfg-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("config.json");

        save(&path, &Config::default()).unwrap();

        assert!(path.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mute_is_active_before_the_deadline_and_expired_after() {
        let mut c = Config::default();
        c.mute_until_ms = 1_000;
        assert!(c.alerts_muted(999));
        assert!(!c.alerts_muted(1_000));
        assert!(!c.alerts_muted(1_001));
    }

    #[test]
    fn default_config_is_never_muted() {
        assert!(!Config::default().alerts_muted(0));
    }

    #[test]
    fn config_path_lands_under_the_bundle_identifier() {
        let p = config_path();
        assert!(p.ends_with("com.clawde.buddy/config.json"), "got {p:?}");
    }
}

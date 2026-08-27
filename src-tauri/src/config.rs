use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// User settings. Hand-editable JSON: every field has a default so a
/// half-written file still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Vestigial. The view modes were removed in favour of `hide_when`, but the
    /// field is kept so an existing config file still parses and is not silently
    /// rewritten without it.
    pub view_mode: String,
    pub alert_needs_input: bool,
    pub alert_died: bool,
    /// Whether finishing a turn interrupts you. Off by default: a finished turn
    /// is the common case, and alerting on it is the noisy choice.
    pub alert_finished: bool,
    /// Whether alerts are delivered at all, and the sound they arrive with. The
    /// parent of the three switches above, in the form and in
    /// `notify::should_deliver` alike: an alert *is* a sound here, so silence
    /// means no alert.
    pub sound: bool,
    /// Epoch millis until which alerts stay suppressed. Backs "Mute alerts 1h".
    pub mute_until_ms: i64,
    pub launch_at_login: bool,
    /// Whether background jobs and subagents appear at all. They are shown
    /// demoted when enabled, since they belong to a session rather than being
    /// one.
    pub show_background_jobs: bool,
    /// Whether the five-hour limit meter appears at the end of the collapsed
    /// row. On by default, but worth being able to turn off: the figure behind
    /// it is a cache Claude Code refreshes only when it fetches usage, so the
    /// meter is absent whenever that cache describes a window that has passed.
    pub show_usage: bool,
    /// When the widget takes itself off screen: `never`, `noSessions` or
    /// `nothingActive`. The tray icon always remains, so a hidden widget is
    /// never unreachable.
    pub hide_when: String,
    /// Whether the user has put the widget away from the tray menu. Outranks
    /// `hide_when` rather than being one of its modes: the policy answers "is
    /// there anything worth showing", and this answers "not now" — for a screen
    /// share or a recording — and has to survive sessions starting and
    /// finishing underneath it.
    pub hidden: bool,
    /// Where the widget lives: `free` to float wherever the user dragged it, or
    /// `notch` to sit in the menu bar flanking a MacBook's notch.
    pub placement: String,
    /// Display key the widget should appear on, or `None` for the primary
    /// display. Keys come from `list_displays`. Ignored under `notch`
    /// placement, which derives its display from where the notch is.
    pub preferred_display: Option<String>,
    /// Widget position keyed by display identifier, so docking and undocking a
    /// monitor does not leave the widget off-screen.
    pub positions: HashMap<String, [f64; 2]>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            view_mode: "dotRow".into(),
            alert_needs_input: true,
            alert_died: true,
            alert_finished: false,
            sound: true,
            mute_until_ms: 0,
            launch_at_login: false,
            show_background_jobs: true,
            show_usage: true,
            hide_when: "noSessions".into(),
            hidden: false,
            placement: "free".into(),
            preferred_display: None,
            positions: HashMap::new(),
        }
    }
}

/// Accepted values for the `placement` setting.
pub const PLACEMENTS: [&str; 2] = ["free", "notch"];

/// `mute_until_ms` for a mute the user has to lift themselves.
///
/// A sentinel rather than an `Option`, so the field keeps one shape in a file
/// people hand-edit and `alerts_muted` stays a single comparison. Deliberately
/// not "now plus a hundred years": that quietly expires, and a mute the user
/// asked to keep until they say otherwise must not.
pub const MUTE_INDEFINITE_MS: i64 = i64::MAX;

/// How long a mute chosen from the tray menu lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteFor {
    Hour,
    EightHours,
    UntilUnmuted,
}

/// The deadline a mute chosen at `now_ms` should be written as.
///
/// Saturating: a clock far enough forward that the addition would wrap has to
/// leave the user muted, never silently unmuted.
pub fn mute_until(now_ms: i64, choice: MuteFor) -> i64 {
    const HOUR_MS: i64 = 60 * 60 * 1000;
    match choice {
        MuteFor::Hour => now_ms.saturating_add(HOUR_MS),
        MuteFor::EightHours => now_ms.saturating_add(8 * HOUR_MS),
        MuteFor::UntilUnmuted => MUTE_INDEFINITE_MS,
    }
}

impl Config {
    pub fn alerts_muted(&self, now_ms: i64) -> bool {
        now_ms < self.mute_until_ms
    }

    /// Whether the widget places itself against the notch.
    ///
    /// Anything other than exactly `notch` reads as free placement. A
    /// hand-edited config must not be able to put the widget somewhere it
    /// cannot be dragged back from, and notch placement suppresses dragging.
    pub fn wants_notch(&self) -> bool {
        self.placement == "notch"
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

/// The bundle identifier the settings directory is named after. Kept in step
/// with `identifier` in `tauri.conf.json`, which is what macOS actually uses.
const BUNDLE_ID: &str = "com.claude.buddy";

/// The identifier shipped up to 0.4.0, before the rename. macOS keys
/// Application Support by the bundle identifier, so changing it makes the OS
/// treat this as an entirely different application: an upgrading user's
/// settings are still under here, and under here only.
///
/// The old spelling is deliberate and must not be "fixed" — it names a
/// directory that already exists on disk on every 0.4.0 machine.
const LEGACY_BUNDLE_ID: &str = "com.clawde.buddy";

fn config_path_for(bundle_id: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join("Library")
        .join("Application Support")
        .join(bundle_id)
        .join("config.json")
}

pub fn config_path() -> PathBuf {
    config_path_for(BUNDLE_ID)
}

/// Where 0.4.0 and earlier kept the settings file.
pub fn legacy_config_path() -> PathBuf {
    config_path_for(LEGACY_BUNDLE_ID)
}

/// One-time carry-over of the pre-rename settings file. Returns whether this
/// call put a file at `current`.
///
/// Copy, rather than reading through to the old path forever. Read-through
/// would mean every load and every save had to know about two locations, and
/// the load-modify-save cycles in `window.rs` would have to decide which one to
/// write back to — a fork that never closes. Copying converges: the moment
/// `current` exists, this is a single `exists` call on every subsequent launch
/// and the old directory can be ignored for good.
///
/// The awkward cases, and what they do here:
///
/// * **Both present.** `current` wins, always, and is not even read. Anyone
///   with a file at the new path has already run a renamed build, so that file
///   is the newer of the two; overwriting it with a stale pre-rename copy would
///   be a worse failure than the one this function exists to prevent.
///   Deliberately `exists` and not "parses": a *corrupt* file at the new path
///   is still the user's file, quite possibly mid-hand-edit, and clobbering it
///   would destroy work that `load` already degrades gracefully around.
///
/// * **The old file is corrupt.** Copied verbatim anyway. The migration's job
///   is to move a file, not to sit in judgement on it, and `load` already owns
///   the corrupt case — it falls back to defaults, exactly as it did before the
///   rename, so this is not a regression for that user. Parsing here would also
///   introduce a second, divergent notion of "valid config" and would silently
///   drop any key this build does not know about.
///
/// * **The copy fails** — unreadable source, read-only or full disk. Swallowed.
///   Nothing is written, `load` returns defaults just as it would have without
///   a migration at all, and the next launch tries again. Settings quietly
///   reverting is the bug being fixed; refusing to start is not an improvement
///   on it.
///
/// The copy goes via a staging file and an atomic rename so a process killed
/// mid-write cannot leave a truncated file at `current` — which, being
/// "present", would block every later attempt and strand the user on defaults
/// permanently.
///
/// The old file is left where it is. Deleting the user's data is not this
/// function's call, and leaving it means a downgrade to 0.4.0 still finds it.
pub fn migrate_config(legacy: &Path, current: &Path) -> bool {
    if current.exists() {
        return false;
    }
    let Ok(bytes) = std::fs::read(legacy) else {
        return false;
    };
    let Some(parent) = current.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    let mut staging = current.as_os_str().to_owned();
    staging.push(".migrating");
    let staging = PathBuf::from(staging);
    if std::fs::write(&staging, &bytes).is_err() {
        let _ = std::fs::remove_file(&staging);
        return false;
    }
    if std::fs::rename(&staging, current).is_err() {
        let _ = std::fs::remove_file(&staging);
        return false;
    }
    true
}

/// Run the migration against the real paths.
///
/// Called once from `run()` before the Tauri builder, which is the only point
/// guaranteed to precede every reader *and* every writer of settings — the
/// load-modify-save in the tray's "Mute alerts 1h" and in `save_position` would
/// otherwise be able to write a defaults-shaped file to the new path and orphan
/// the old one for good.
pub fn migrate_legacy_config() {
    migrate_config(&legacy_config_path(), &config_path());
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

    /// A fresh directory to hang the two bundle-identifier directories off, so
    /// a migration test starts from a known-empty tree.
    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cb-migrate-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The pre-rename path under `root`, with its parent directory created.
    fn legacy_in(root: &Path) -> PathBuf {
        let dir = root.join(LEGACY_BUNDLE_ID);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("config.json")
    }

    /// The post-rename path under `root`. Its parent deliberately does not
    /// exist: on a real upgrade the new directory has never been created.
    fn current_in(root: &Path) -> PathBuf {
        root.join(BUNDLE_ID).join("config.json")
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
    fn a_timed_mute_ends_after_its_span() {
        let now = 1_700_000_000_000;
        assert_eq!(mute_until(now, MuteFor::Hour), now + 3_600_000);
        assert_eq!(mute_until(now, MuteFor::EightHours), now + 8 * 3_600_000);
    }

    #[test]
    fn an_indefinite_mute_never_expires() {
        let c = Config {
            mute_until_ms: mute_until(0, MuteFor::UntilUnmuted),
            ..Config::default()
        };
        assert!(c.alerts_muted(i64::MAX - 1));
    }

    #[test]
    fn a_mute_near_the_end_of_time_does_not_wrap_into_the_past() {
        // Saturating, not wrapping: an addition that overflowed would land in
        // the far past and read as "not muted" the instant it was chosen.
        let now = i64::MAX - 10;
        let c = Config {
            mute_until_ms: mute_until(now, MuteFor::EightHours),
            ..Config::default()
        };
        assert!(c.alerts_muted(now));
    }

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.view_mode, "dotRow");
        assert!(c.alert_needs_input);
        assert!(c.alert_died);
        assert!(!c.alert_finished);
        assert!(c.sound);
        assert_eq!(c.mute_until_ms, 0);
        assert!(!c.launch_at_login);
        assert!(c.show_background_jobs);
        assert!(c.show_usage);
        assert_eq!(c.hide_when, "noSessions");
        assert!(!c.hidden);
        assert_eq!(c.placement, "free");
        assert!(!c.wants_notch());
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
        assert!(p.ends_with("com.claude.buddy/config.json"), "got {p:?}");
    }
    #[test]
    fn only_the_exact_notch_value_places_against_the_notch() {
        // A typo in a hand-edited file falls back to free placement rather than
        // to a mode where the widget cannot be dragged.
        let mut c = Config::default();
        assert!(!c.wants_notch());
        c.placement = "notch".into();
        assert!(c.wants_notch());
        for wrong in ["Notch", "NOTCH", "notched", "", "free"] {
            c.placement = wrong.into();
            assert!(!c.wants_notch(), "{wrong} must not read as notch");
        }
        assert!(PLACEMENTS.contains(&"free") && PLACEMENTS.contains(&"notch"));
    }

    #[test]
    fn legacy_settings_are_copied_when_the_new_path_is_absent() {
        let root = temp_root("copied");
        let legacy = legacy_in(&root);
        let current = current_in(&root);
        // Written raw rather than through `save`, which would also prime the
        // shared cache and leak into the other tests in this module.
        std::fs::write(&legacy, r#"{"hideWhen":"nothingActive","sound":false}"#).unwrap();

        assert!(migrate_config(&legacy, &current));

        let migrated = load(&current);
        assert_eq!(migrated.hide_when, "nothingActive");
        assert!(!migrated.sound);
        assert!(
            current.parent().unwrap().is_dir(),
            "the new settings directory should have been created"
        );
        assert!(legacy.exists(), "the old file is left where it was");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_existing_settings_file_is_never_overwritten_by_the_legacy_one() {
        let root = temp_root("both");
        let legacy = legacy_in(&root);
        let current = current_in(&root);
        std::fs::create_dir_all(current.parent().unwrap()).unwrap();
        std::fs::write(&legacy, r#"{"hideWhen":"nothingActive"}"#).unwrap();
        std::fs::write(&current, r#"{"hideWhen":"never"}"#).unwrap();

        assert!(!migrate_config(&legacy, &current));

        assert_eq!(load(&current).hide_when, "never", "the newer file wins");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_corrupt_new_file_is_left_alone_rather_than_replaced() {
        // Corruption at the new path means the user has run a renamed build and
        // then broken the file by hand. Restoring a pre-rename copy over it
        // would throw away whatever they were in the middle of.
        let root = temp_root("both-corrupt");
        let legacy = legacy_in(&root);
        let current = current_in(&root);
        std::fs::create_dir_all(current.parent().unwrap()).unwrap();
        std::fs::write(&legacy, r#"{"hideWhen":"nothingActive"}"#).unwrap();
        std::fs::write(&current, "{ half-edited").unwrap();

        assert!(!migrate_config(&legacy, &current));

        assert_eq!(std::fs::read_to_string(&current).unwrap(), "{ half-edited");
        assert_eq!(load(&current), Config::default());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn migration_does_nothing_when_neither_file_exists() {
        let root = temp_root("neither");
        let legacy = root.join(LEGACY_BUNDLE_ID).join("config.json");
        let current = current_in(&root);

        assert!(!migrate_config(&legacy, &current));

        assert!(!current.exists(), "nothing should be conjured up");
        assert_eq!(load(&current), Config::default());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_unreadable_legacy_file_leaves_the_new_path_absent() {
        // A directory where the file should be: unreadable without depending on
        // permission bits, which behave differently when the tests run as root.
        let root = temp_root("unreadable");
        let legacy = legacy_in(&root);
        std::fs::create_dir_all(&legacy).unwrap();
        let current = current_in(&root);

        assert!(!migrate_config(&legacy, &current));

        assert!(!current.exists());
        assert_eq!(load(&current), Config::default());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_corrupt_legacy_file_migrates_verbatim_and_still_loads_as_defaults() {
        let root = temp_root("corrupt-legacy");
        let legacy = legacy_in(&root);
        let current = current_in(&root);
        std::fs::write(&legacy, "{ not json").unwrap();

        assert!(migrate_config(&legacy, &current));

        assert_eq!(std::fs::read_to_string(&current).unwrap(), "{ not json");
        assert_eq!(
            load(&current),
            Config::default(),
            "corrupt after the move is still just defaults, never a panic"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn migration_leaves_no_staging_file_behind() {
        let root = temp_root("staging");
        let legacy = legacy_in(&root);
        let current = current_in(&root);
        std::fs::write(&legacy, r#"{"sound":false}"#).unwrap();

        assert!(migrate_config(&legacy, &current));

        let left: Vec<_> = std::fs::read_dir(current.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("config.json")]);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_two_config_paths_differ_only_in_the_bundle_identifier() {
        let legacy = legacy_config_path();
        assert!(
            legacy.ends_with("com.clawde.buddy/config.json"),
            "got {legacy:?}"
        );
        assert_ne!(legacy, config_path());
        assert_eq!(
            legacy.parent().unwrap().parent(),
            config_path().parent().unwrap().parent()
        );
    }
}

//! Launch at login, and the repair the rename made necessary.
//!
//! `tauri-plugin-autostart` names its LaunchAgent after the package, so every
//! build up to 0.4.0 registered `~/Library/LaunchAgents/clawde-buddy.plist`.
//! The renamed build looks for `claude-buddy.plist` and finds nothing, which
//! leaves an upgrading user in the worst of both states: the settings carried
//! over by `config::migrate_config` still say *Launch at login*, the checkbox
//! is still ticked, and nothing is registered under the new name — while the
//! old agent is still there, still pointing at a bundle that has been renamed
//! or thrown away.
//!
//! Nothing reconciles that on its own. `commands::set_config` writes the plist
//! only when settings are *saved*, so the checkbox would have to be toggled off
//! and on again by a user with no reason to suspect anything is wrong.

use std::path::{Path, PathBuf};

use tauri_plugin_autostart::ManagerExt;

/// The LaunchAgent the pre-rename builds registered.
///
/// The old spelling is deliberate and must not be "fixed": it names a file that
/// exists on disk on every 0.4.0 machine that ever ticked the box.
const LEGACY_AGENT: &str = "clawde-buddy.plist";

fn launch_agents_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join("Library")
        .join("LaunchAgents")
}

pub fn legacy_agent_path() -> PathBuf {
    launch_agents_dir().join(LEGACY_AGENT)
}

/// Delete the pre-rename login item. Returns whether this call removed a file.
///
/// Unconditional, and deliberately not gated on the current setting. An agent
/// left behind points at the old bundle, so it either fails silently at every
/// login or — if the old app is still in `/Applications`, which is the likelier
/// case right after an install of the renamed one — quietly launches a second,
/// stale copy of the widget alongside the real one. Neither is something the
/// user asked for, and neither is something they would think to look for.
///
/// A failure is swallowed. This runs during startup, and a login item that
/// could not be tidied is not a reason to refuse to launch.
pub fn remove_legacy_agent(path: &Path) -> bool {
    path.is_file() && std::fs::remove_file(path).is_ok()
}

/// Put launch at login back the way the settings file says it should be, once,
/// at startup.
///
/// `is_enabled` only ever asks about the current name, so on an upgraded
/// machine it answers "no" while the box is ticked — which is precisely the
/// state to repair. Re-enabling is idempotent for everyone else: a user who
/// already has the new agent takes the early return, and a user who never
/// wanted it never reaches `enable`.
pub fn reconcile(app: &tauri::AppHandle, launch_at_login: bool) {
    remove_legacy_agent(&legacy_agent_path());

    if !launch_at_login {
        return;
    }
    let manager = app.autolaunch();
    if manager.is_enabled().unwrap_or(false) {
        return;
    }
    let _ = manager.enable();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory to stand in for `~/Library/LaunchAgents`, matching how
    /// the config tests build theirs — no dev-dependency for the sake of four
    /// filesystem assertions.
    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cb-autostart-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn removes_a_legacy_agent_that_is_there() {
        let plist = temp_root("present").join(LEGACY_AGENT);
        std::fs::write(&plist, "<plist/>").unwrap();

        assert!(remove_legacy_agent(&plist));
        assert!(!plist.exists());
    }

    #[test]
    fn absent_legacy_agent_is_not_an_error() {
        let plist = temp_root("absent").join(LEGACY_AGENT);
        assert!(!remove_legacy_agent(&plist));
    }

    #[test]
    fn a_directory_in_its_place_is_left_alone() {
        // Nothing should be deleted out from under the user on a name match
        // alone; `remove_file` would fail on a directory anyway, but the
        // `is_file` guard is what makes that a decision rather than an accident.
        let impostor = temp_root("impostor").join(LEGACY_AGENT);
        std::fs::create_dir(&impostor).unwrap();

        assert!(!remove_legacy_agent(&impostor));
        assert!(impostor.is_dir());
    }

    #[test]
    fn legacy_agent_path_names_the_old_bundle() {
        let path = legacy_agent_path();
        assert!(
            path.ends_with("Library/LaunchAgents/clawde-buddy.plist"),
            "{path:?}"
        );
    }
}

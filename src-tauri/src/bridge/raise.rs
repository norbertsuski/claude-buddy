use std::path::Path;
use std::sync::Mutex;

use crate::bridge::proc_tree::{bundle_identifier, find_app_bundle, ProcTree, PsProcTree};

pub trait Activator {
    fn activate(&self, bundle_id: &str) -> Result<(), String>;
}

/// Activation via `open -b`.
///
/// Deliberately not AppleScript: `open` needs neither Accessibility nor
/// Automation permission, so a fresh install raises windows without ever
/// prompting the user. Tab-level targeting, which does need Automation, is a
/// later and strictly additive rung.
pub struct OpenActivator;

impl Activator for OpenActivator {
    fn activate(&self, bundle_id: &str) -> Result<(), String> {
        let status = std::process::Command::new("open")
            .args(["-b", bundle_id])
            .status()
            .map_err(|e| format!("could not run open: {e}"))?;

        if status.success() {
            Ok(())
        } else {
            Err(format!("activation refused by open for {bundle_id}"))
        }
    }
}

/// Raise the application hosting `pid`.
///
/// `resolve_id` is injected so tests never read a real bundle from disk.
pub fn raise(
    tree: &dyn ProcTree,
    activator: &dyn Activator,
    resolve_id: &dyn Fn(&Path) -> Option<String>,
    pid: i32,
) -> Result<String, String> {
    let bundle = find_app_bundle(tree, pid)
        .ok_or_else(|| format!("no host application found for pid {pid}"))?;

    let bundle_id = resolve_id(Path::new(&bundle))
        .ok_or_else(|| format!("no bundle identifier in {bundle}"))?;

    activator.activate(&bundle_id)?;
    Ok(bundle_id)
}

/// Bring the window running a session to the front. Returns the bundle
/// identifier that was activated, for display in the popover.
///
/// `async` deliberately: Tauri runs non-async commands on the main thread, and
/// this one spawns `ps` and then `open` and waits on both. On the main thread
/// that stalls the event loop mid-animation.
#[tauri::command]
pub async fn raise_session(pid: i32) -> Result<String, String> {
    raise(
        &PsProcTree::snapshot(),
        &OpenActivator,
        &|path| bundle_identifier(path),
        pid,
    )
}

pub struct RecordingActivator {
    calls: Mutex<Vec<String>>,
    fail: bool,
}

impl RecordingActivator {
    pub fn new() -> Self {
        Self { calls: Mutex::new(Vec::new()), fail: false }
    }

    pub fn failing() -> Self {
        Self { calls: Mutex::new(Vec::new()), fail: true }
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl Default for RecordingActivator {
    fn default() -> Self {
        Self::new()
    }
}

impl Activator for RecordingActivator {
    fn activate(&self, bundle_id: &str) -> Result<(), String> {
        if self.fail {
            return Err(format!("activation refused for {bundle_id}"));
        }
        self.calls.lock().unwrap().push(bundle_id.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::proc_tree::FakeProcTree;

    fn cursor_tree() -> FakeProcTree {
        FakeProcTree::new()
            .with(7952, 7951, "/Users/n/.volta/bin/claude")
            .with(7951, 7447, "claude")
            .with(7447, 6323, "/bin/zsh")
            .with(6323, 5524, "Cursor Helper: terminal pty-host")
            .with(5524, 1, "/Applications/Cursor.app/Contents/MacOS/Cursor")
    }

    fn resolver(id: &'static str) -> impl Fn(&Path) -> Option<String> {
        move |_| Some(id.to_string())
    }

    #[test]
    fn activates_the_bundle_identifier_of_the_host_application() {
        let activator = RecordingActivator::new();

        let outcome = raise(&cursor_tree(), &activator, &resolver("com.todesktop.cursor"), 7952);

        assert_eq!(outcome.as_deref(), Ok("com.todesktop.cursor"));
        assert_eq!(activator.calls(), vec!["com.todesktop.cursor".to_string()]);
    }

    #[test]
    fn errors_when_the_chain_reaches_no_application() {
        let tree = FakeProcTree::new().with(300, 1, "/usr/local/bin/claude");
        let activator = RecordingActivator::new();

        let outcome = raise(&tree, &activator, &resolver("irrelevant"), 300);

        assert!(outcome.is_err());
        assert!(outcome.unwrap_err().contains("no host application"));
        assert!(activator.calls().is_empty(), "must not activate anything");
    }

    #[test]
    fn errors_when_the_bundle_has_no_identifier() {
        let activator = RecordingActivator::new();
        let no_id = |_: &Path| None;

        let outcome = raise(&cursor_tree(), &activator, &no_id, 7952);

        assert!(outcome.unwrap_err().contains("bundle identifier"));
        assert!(activator.calls().is_empty());
    }

    #[test]
    fn propagates_an_activation_failure() {
        let activator = RecordingActivator::failing();

        let outcome = raise(&cursor_tree(), &activator, &resolver("com.todesktop.cursor"), 7952);

        assert!(outcome.unwrap_err().contains("activation refused"));
    }

    #[test]
    fn errors_for_an_unknown_pid_without_activating() {
        let activator = RecordingActivator::new();

        let outcome = raise(&cursor_tree(), &activator, &resolver("x"), 999_999);

        assert!(outcome.is_err());
        assert!(activator.calls().is_empty());
    }
}

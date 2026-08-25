use std::collections::HashMap;
use std::path::Path;

/// Guard against a malformed or cyclic parent chain. No real ancestry from a
/// session to its host application comes close to this depth.
pub const MAX_WALK_DEPTH: usize = 32;

pub trait ProcTree {
    fn parent(&self, pid: i32) -> Option<i32>;
    fn exe(&self, pid: i32) -> Option<String>;
}

/// Walk from a session's pid to the GUI application hosting it, and return that
/// application's bundle path.
///
/// A session's own executable can itself live inside a `.app` — Claude Desktop
/// ships `claude.app` inside its Application Support directory — so the walk
/// continues past the first match and returns the outermost bundle found.
pub fn find_app_bundle(tree: &dyn ProcTree, pid: i32) -> Option<String> {
    let mut current = pid;
    let mut seen = std::collections::HashSet::new();
    let mut outermost = None;

    for _ in 0..MAX_WALK_DEPTH {
        if !seen.insert(current) {
            break;
        }
        if let Some(exe) = tree.exe(current) {
            if let Some(index) = exe.find(".app/") {
                outermost = Some(exe[..index + 4].to_string());
            }
        }
        match tree.parent(current) {
            Some(parent) if parent != current && parent > 1 => current = parent,
            _ => break,
        }
    }

    outermost
}

pub fn bundle_identifier(bundle_path: &Path) -> Option<String> {
    let plist = plist::Value::from_file(bundle_path.join("Contents").join("Info.plist")).ok()?;
    plist
        .as_dictionary()?
        .get("CFBundleIdentifier")?
        .as_string()
        .map(str::to_string)
}

/// One `ps` invocation, indexed. Taken as a snapshot so a single raise cannot
/// see an inconsistent tree mid-walk.
pub struct PsProcTree {
    entries: HashMap<i32, (i32, String)>,
}

impl PsProcTree {
    pub fn snapshot() -> Self {
        let out = std::process::Command::new("ps")
            .args(["-Ao", "pid=,ppid=,comm="])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        Self::parse(&out)
    }

    /// Parse `pid ppid comm` rows. Only the first two fields are whitespace
    /// delimited — the executable path may itself contain spaces, as
    /// `Cursor Helper: terminal pty-host` does.
    pub fn parse(output: &str) -> Self {
        let mut entries = HashMap::new();
        for line in output.lines() {
            let trimmed = line.trim_start();
            let Some((pid_str, rest)) = trimmed.split_once(' ') else {
                continue;
            };
            let Ok(pid) = pid_str.parse::<i32>() else {
                continue;
            };
            let rest = rest.trim_start();
            let Some((ppid_str, exe)) = rest.split_once(' ') else {
                continue;
            };
            let Ok(ppid) = ppid_str.parse::<i32>() else {
                continue;
            };
            entries.insert(pid, (ppid, exe.trim().to_string()));
        }
        Self { entries }
    }
}

impl ProcTree for PsProcTree {
    fn parent(&self, pid: i32) -> Option<i32> {
        self.entries.get(&pid).map(|(ppid, _)| *ppid)
    }

    fn exe(&self, pid: i32) -> Option<String> {
        self.entries.get(&pid).map(|(_, exe)| exe.clone())
    }
}

pub struct FakeProcTree {
    entries: HashMap<i32, (i32, String)>,
}

impl FakeProcTree {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    pub fn with(mut self, pid: i32, ppid: i32, exe: &str) -> Self {
        self.entries.insert(pid, (ppid, exe.to_string()));
        self
    }
}

impl Default for FakeProcTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcTree for FakeProcTree {
    fn parent(&self, pid: i32) -> Option<i32> {
        self.entries.get(&pid).map(|(ppid, _)| *ppid)
    }

    fn exe(&self, pid: i32) -> Option<String> {
        self.entries.get(&pid).map(|(_, exe)| exe.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real ancestry of a cli session running in Cursor's integrated
    /// terminal, captured with `ps -o pid=,ppid=,comm=`.
    fn cursor_tree() -> FakeProcTree {
        FakeProcTree::new()
            .with(7952, 7951, "/Users/n/.volta/tools/image/packages/@anthropic-ai/claude-code/bin/claude")
            .with(7951, 7447, "claude")
            .with(7447, 6323, "/bin/zsh")
            .with(6323, 5524, "Cursor Helper: terminal pty-host")
            .with(5524, 1, "/Applications/Cursor.app/Contents/MacOS/Cursor")
    }

    /// The real ancestry of a claude-desktop session.
    fn desktop_tree() -> FakeProcTree {
        FakeProcTree::new()
            .with(99215, 99213, "/Users/n/Library/Application Support/Claude/claude-code/2.1.237/claude.app/Contents/MacOS/claude")
            .with(99213, 51954, "/Applications/Claude.app/Contents/Helpers/disclaimer")
            .with(51954, 1, "/Applications/Claude.app/Contents/MacOS/Claude")
    }

    #[test]
    fn finds_the_editor_bundle_for_a_cli_session() {
        assert_eq!(
            find_app_bundle(&cursor_tree(), 7952),
            Some("/Applications/Cursor.app".to_string())
        );
    }

    #[test]
    fn finds_the_desktop_bundle_for_a_desktop_session() {
        // The session's own executable lives inside a nested claude.app. The walk
        // must reach the outermost real application, not stop at that one.
        assert_eq!(
            find_app_bundle(&desktop_tree(), 99215),
            Some("/Applications/Claude.app".to_string())
        );
    }

    #[test]
    fn returns_none_for_an_orphan_whose_chain_reaches_no_bundle() {
        let tree = FakeProcTree::new()
            .with(300, 200, "/usr/local/bin/claude")
            .with(200, 1, "/bin/zsh");
        assert_eq!(find_app_bundle(&tree, 300), None);
    }

    #[test]
    fn returns_none_for_an_unknown_pid() {
        assert_eq!(find_app_bundle(&cursor_tree(), 424242), None);
    }

    #[test]
    fn a_parent_cycle_terminates_instead_of_looping() {
        let tree = FakeProcTree::new().with(10, 11, "/bin/a").with(11, 10, "/bin/b");
        assert_eq!(find_app_bundle(&tree, 10), None);
    }

    #[test]
    fn reaching_pid_one_terminates() {
        let tree = FakeProcTree::new().with(2, 1, "/bin/zsh").with(1, 1, "/sbin/launchd");
        assert_eq!(find_app_bundle(&tree, 2), None);
    }

    #[test]
    fn ps_snapshot_knows_our_own_process_and_its_parent() {
        let tree = PsProcTree::snapshot();
        let own = std::process::id() as i32;
        assert!(tree.exe(own).is_some(), "own executable should be known");
        assert!(tree.parent(own).is_some(), "own parent should be known");
    }

    #[test]
    fn ps_snapshot_parses_executables_containing_spaces() {
        // "Cursor Helper: terminal pty-host" would break naive whitespace splitting.
        let tree = PsProcTree::parse("  6323  5524 Cursor Helper: terminal pty-host\n");
        assert_eq!(tree.parent(6323), Some(5524));
        assert_eq!(tree.exe(6323).as_deref(), Some("Cursor Helper: terminal pty-host"));
    }

    #[test]
    fn ps_snapshot_skips_malformed_lines() {
        let tree = PsProcTree::parse("garbage\n  10  1 /bin/zsh\n\n");
        assert_eq!(tree.parent(10), Some(1));
    }

    #[test]
    fn bundle_identifier_reads_cfbundleidentifier() {
        let bundle = std::env::temp_dir().join(format!("cb-bundle-{}.app", std::process::id()));
        let contents = bundle.join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(
            contents.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>com.example.thing</string>
</dict></plist>"#,
        )
        .unwrap();

        assert_eq!(bundle_identifier(&bundle).as_deref(), Some("com.example.thing"));
        std::fs::remove_dir_all(&bundle).unwrap();
    }

    #[test]
    fn bundle_identifier_returns_none_without_a_plist() {
        let bundle = std::env::temp_dir().join(format!("cb-nobundle-{}.app", std::process::id()));
        std::fs::create_dir_all(&bundle).unwrap();
        assert_eq!(bundle_identifier(&bundle), None);
        std::fs::remove_dir_all(&bundle).unwrap();
    }
}

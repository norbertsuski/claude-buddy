use std::collections::HashMap;
use std::path::PathBuf;

use crate::watcher::probes::ActivityProbe;

/// Reads transcript modification times from disk.
pub struct TranscriptActivity {
    projects_dir: PathBuf,
}

impl TranscriptActivity {
    pub fn new(projects_dir: PathBuf) -> Self {
        Self { projects_dir }
    }
}

impl ActivityProbe for TranscriptActivity {
    fn last_activity_ms(&self, cwd: &str, session_id: &str) -> Option<i64> {
        let path = crate::bridge::transcript::find_transcript(&self.projects_dir, cwd, session_id)?;
        let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
        let since_epoch = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(since_epoch.as_millis() as i64)
    }
}

/// Reports nothing, for callers that do not care about activity.
pub struct NoActivity;

impl ActivityProbe for NoActivity {
    fn last_activity_ms(&self, _cwd: &str, _session_id: &str) -> Option<i64> {
        None
    }
}

/// Test double keyed by session id.
pub struct FakeActivity {
    times: HashMap<String, i64>,
}

impl FakeActivity {
    pub fn new() -> Self {
        Self {
            times: HashMap::new(),
        }
    }

    pub fn with(mut self, session_id: &str, at_ms: i64) -> Self {
        self.times.insert(session_id.to_string(), at_ms);
        self
    }
}

impl Default for FakeActivity {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityProbe for FakeActivity {
    fn last_activity_ms(&self, _cwd: &str, session_id: &str) -> Option<i64> {
        self.times.get(session_id).copied()
    }
}

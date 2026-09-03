use std::path::PathBuf;

use buddy_core::watcher::probes::ActivityProbe;

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

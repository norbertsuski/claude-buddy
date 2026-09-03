use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use buddy_core::watcher::probes::WorkProbe;

/// Reads the pending tool call from the session transcript.
///
/// Caches on transcript mtime for the same reason `TranscriptBlocked` does: the
/// watcher reconciles every two seconds and consults this for every statusless
/// session, and a 64KB tail read per session per tick is wasted on a file that
/// has not changed.
pub struct TranscriptWork {
    projects_dir: PathBuf,
    cache: Mutex<HashMap<String, (i64, bool)>>,
}

impl TranscriptWork {
    pub fn new(projects_dir: PathBuf) -> Self {
        Self {
            projects_dir,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn modified_ms(path: &std::path::Path) -> Option<i64> {
        let modified = std::fs::metadata(path).ok()?.modified().ok()?;
        let since_epoch = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(since_epoch.as_millis() as i64)
    }

    fn read(&self, cwd: &str, session_id: &str) -> Option<bool> {
        use crate::bridge::transcript::{
            find_transcript, has_work_in_flight, read_tail, TAIL_BYTES,
        };

        let path = find_transcript(&self.projects_dir, cwd, session_id)?;
        // No mtime means no cache key, so read rather than guess.
        let mtime = Self::modified_ms(&path)?;

        {
            let cache = self.cache.lock().expect("work cache poisoned");
            if let Some((cached_at, answer)) = cache.get(session_id) {
                if *cached_at == mtime {
                    return Some(*answer);
                }
            }
        }

        let answer = read_tail(&path, TAIL_BYTES)
            .ok()
            .map(|bytes| has_work_in_flight(&bytes))?;

        self.cache
            .lock()
            .expect("work cache poisoned")
            .insert(session_id.to_string(), (mtime, answer));

        Some(answer)
    }
}

impl WorkProbe for TranscriptWork {
    fn in_flight(&self, cwd: &str, session_id: &str) -> bool {
        self.read(cwd, session_id).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNNING: &str = concat!(
        r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_bash","name":"Bash"}]}}"#,
        "\n"
    );

    const FINISHED: &str = concat!(
        r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_bash","name":"Bash"}]}}"#,
        "\n",
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_bash"}]}}"#,
        "\n"
    );

    struct Fixture {
        root: PathBuf,
        transcript: PathBuf,
    }

    impl Fixture {
        fn new(tag: &str, body: &str) -> Self {
            let root = std::env::temp_dir().join(format!("cb-work-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let dir = root.join("-Users-n-Code-proj");
            std::fs::create_dir_all(&dir).unwrap();
            let transcript = dir.join("session-1.jsonl");
            std::fs::write(&transcript, body).unwrap();
            Self { root, transcript }
        }

        fn probe(&self) -> TranscriptWork {
            TranscriptWork::new(self.root.clone())
        }

        fn ask(&self, probe: &TranscriptWork) -> bool {
            probe.in_flight("/Users/n/Code/proj", "session-1")
        }

        fn mtime(&self) -> std::time::SystemTime {
            std::fs::metadata(&self.transcript)
                .unwrap()
                .modified()
                .unwrap()
        }

        /// Rewrite the body and move mtime on by a second.
        ///
        /// Writing alone is not enough to prove a cache miss. The cache is keyed
        /// on mtime in whole milliseconds, and two `fs::write` calls in a row
        /// land inside the same millisecond often enough to matter — measured at
        /// roughly one time in four on an SSD, which is exactly the shape of the
        /// intermittent CI failure this replaced. The test is about what a
        /// changed mtime does, so it sets one rather than hoping for one.
        fn rewrite_advancing_mtime(&self, body: &str) {
            let was = self.mtime();
            std::fs::write(&self.transcript, body).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&self.transcript)
                .unwrap()
                .set_modified(was + std::time::Duration::from_secs(1))
                .unwrap();
        }

        /// Rewrite the body while pinning mtime, so a re-read would be visible
        /// in the answer and a cache hit would not.
        fn rewrite_keeping_mtime(&self, body: &str) {
            let was = self.mtime();
            std::fs::write(&self.transcript, body).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&self.transcript)
                .unwrap()
                .set_modified(was)
                .unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_running_tool_call_is_reported() {
        let fixture = Fixture::new("running", RUNNING);
        assert!(fixture.ask(&fixture.probe()));
    }

    #[test]
    fn a_finished_tool_call_is_not_reported() {
        let fixture = Fixture::new("finished", FINISHED);
        assert!(!fixture.ask(&fixture.probe()));
    }

    #[test]
    fn a_missing_transcript_reports_nothing() {
        let probe = TranscriptWork::new(std::env::temp_dir().join("cb-work-missing"));
        assert!(!probe.in_flight("/Users/n/Code/proj", "session-1"));
    }

    #[test]
    fn an_unchanged_transcript_is_answered_from_cache() {
        let fixture = Fixture::new("cache-hit", RUNNING);
        let probe = fixture.probe();
        assert!(fixture.ask(&probe));

        fixture.rewrite_keeping_mtime(FINISHED);
        assert!(fixture.ask(&probe), "same mtime should not be re-read");
    }

    #[test]
    fn a_changed_transcript_is_re_read() {
        let fixture = Fixture::new("cache-miss", RUNNING);
        let probe = fixture.probe();
        assert!(fixture.ask(&probe));

        fixture.rewrite_advancing_mtime(FINISHED);
        assert!(!fixture.ask(&probe), "new mtime should be re-read");
    }
}

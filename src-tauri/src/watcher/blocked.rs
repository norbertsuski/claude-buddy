use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Whether a session is blocked on the user despite never saying so.
///
/// Claude Desktop writes no `status`, `statusUpdatedAt` or `waitingFor` at all,
/// so `state::snapshot` falls back to transcript mtime — which can only
/// distinguish busy from quiet. A session sitting on an unanswered
/// `AskUserQuestion` is quiet, and rendered grey, while it is in fact blocked.
///
/// Injected rather than called directly, matching `PidLiveness`,
/// `ActivityProbe` and `QuestionProbe`, so the state machine stays testable
/// without a transcript on disk.
pub trait BlockedProbe {
    fn pending_prompt(&self, cwd: &str, session_id: &str) -> Option<String>;
}

/// Reads the pending prompt from the session transcript.
///
/// Caches on transcript mtime. The watcher reconciles every two seconds and
/// consults this for every statusless session; without the cache that is a
/// 64KB tail read per session per tick, for a file that has not changed.
pub struct TranscriptBlocked {
    projects_dir: PathBuf,
    cache: Mutex<HashMap<String, (i64, Option<String>)>>,
}

impl TranscriptBlocked {
    pub fn new(projects_dir: PathBuf) -> Self {
        Self { projects_dir, cache: Mutex::new(HashMap::new()) }
    }

    fn modified_ms(path: &std::path::Path) -> Option<i64> {
        let modified = std::fs::metadata(path).ok()?.modified().ok()?;
        let since_epoch = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(since_epoch.as_millis() as i64)
    }
}

impl BlockedProbe for TranscriptBlocked {
    fn pending_prompt(&self, cwd: &str, session_id: &str) -> Option<String> {
        use crate::bridge::transcript::{
            find_transcript, pending_user_prompt, read_tail, TAIL_BYTES,
        };

        let path = find_transcript(&self.projects_dir, cwd, session_id)?;
        // No mtime means no cache key, so read rather than guess.
        let mtime = Self::modified_ms(&path)?;

        {
            let cache = self.cache.lock().expect("blocked cache poisoned");
            if let Some((cached_at, answer)) = cache.get(session_id) {
                if *cached_at == mtime {
                    return answer.clone();
                }
            }
        }

        let answer = read_tail(&path, TAIL_BYTES)
            .ok()
            .and_then(|bytes| pending_user_prompt(&bytes));

        self.cache
            .lock()
            .expect("blocked cache poisoned")
            .insert(session_id.to_string(), (mtime, answer.clone()));

        answer
    }
}

/// Reports nothing, for callers that do not care.
pub struct NoBlocked;

impl BlockedProbe for NoBlocked {
    fn pending_prompt(&self, _cwd: &str, _session_id: &str) -> Option<String> {
        None
    }
}

/// Test double keyed by session id.
pub struct FakeBlocked {
    prompts: HashMap<String, String>,
}

impl FakeBlocked {
    pub fn new() -> Self {
        Self { prompts: HashMap::new() }
    }

    pub fn with(mut self, session_id: &str, label: &str) -> Self {
        self.prompts.insert(session_id.to_string(), label.to_string());
        self
    }
}

impl Default for FakeBlocked {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockedProbe for FakeBlocked {
    fn pending_prompt(&self, _cwd: &str, session_id: &str) -> Option<String> {
        self.prompts.get(session_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PENDING: &str = concat!(
        r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_ask","name":"AskUserQuestion"}]}}"#,
        "\n"
    );

    const ANSWERED: &str = concat!(
        r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_ask","name":"AskUserQuestion"}]}}"#,
        "\n",
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_ask"}]}}"#,
        "\n"
    );

    struct Fixture {
        root: PathBuf,
        transcript: PathBuf,
    }

    impl Fixture {
        fn new(tag: &str, body: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("cb-blocked-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let dir = root.join("-Users-n-Code-proj");
            std::fs::create_dir_all(&dir).unwrap();
            let transcript = dir.join("session-1.jsonl");
            std::fs::write(&transcript, body).unwrap();
            Self { root, transcript }
        }

        fn probe(&self) -> TranscriptBlocked {
            TranscriptBlocked::new(self.root.clone())
        }

        fn ask(&self, probe: &TranscriptBlocked) -> Option<String> {
            probe.pending_prompt("/Users/n/Code/proj", "session-1")
        }

        fn mtime(&self) -> std::time::SystemTime {
            std::fs::metadata(&self.transcript).unwrap().modified().unwrap()
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
    fn a_pending_question_in_the_transcript_is_reported() {
        let fixture = Fixture::new("pending", PENDING);
        assert_eq!(
            fixture.ask(&fixture.probe()).as_deref(),
            Some("question pending")
        );
    }

    #[test]
    fn an_answered_question_is_not_reported() {
        let fixture = Fixture::new("answered", ANSWERED);
        assert_eq!(fixture.ask(&fixture.probe()), None);
    }

    #[test]
    fn a_missing_transcript_reports_nothing() {
        let fixture = Fixture::new("missing", PENDING);
        let probe = fixture.probe();
        assert_eq!(probe.pending_prompt("/Users/n/Code/proj", "no-such-session"), None);
    }

    #[test]
    fn an_unchanged_transcript_is_read_only_once() {
        let fixture = Fixture::new("cache-hit", PENDING);
        let probe = fixture.probe();

        assert_eq!(fixture.ask(&probe).as_deref(), Some("question pending"));

        // The body now says the question was answered. Only a second read of
        // the file could see that, and the pinned mtime must prevent one.
        fixture.rewrite_keeping_mtime(ANSWERED);

        assert_eq!(
            fixture.ask(&probe).as_deref(),
            Some("question pending"),
            "unchanged mtime must be served from the cache, not re-read"
        );
    }

    #[test]
    fn a_touched_transcript_is_read_again() {
        let fixture = Fixture::new("cache-miss", PENDING);
        let probe = fixture.probe();

        assert_eq!(fixture.ask(&probe).as_deref(), Some("question pending"));

        // Same rewrite, this time letting the mtime move forward.
        std::fs::write(&fixture.transcript, ANSWERED).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&fixture.transcript)
            .unwrap()
            .set_modified(fixture.mtime() + std::time::Duration::from_secs(5))
            .unwrap();

        assert_eq!(fixture.ask(&probe), None);
    }

    #[test]
    fn nothing_is_reported_by_the_null_probe() {
        assert_eq!(NoBlocked.pending_prompt("/Users/n/Code/proj", "session-1"), None);
    }

    #[test]
    fn the_fake_answers_only_for_the_session_it_was_given() {
        let probe = FakeBlocked::new().with("a", "question pending");
        assert_eq!(probe.pending_prompt("/x", "a").as_deref(), Some("question pending"));
        assert_eq!(probe.pending_prompt("/x", "b"), None);
    }
}

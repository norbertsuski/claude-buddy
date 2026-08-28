use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// What a session calls itself.
///
/// The registry only ever carries `<folder>-<2 chars>`: `nameSource` is
/// `derived` for every session Claude Code writes, so the row would read as a
/// list of repositories even when three of them are the same repository. The
/// title Claude Code gives a session says what the session is *doing*, and it
/// lives in the transcript rather than the registry.
///
/// Injected rather than called directly, matching `PidLiveness`,
/// `ActivityProbe`, `BlockedProbe`, `QuestionProbe` and `WorkProbe`, so the
/// state machine stays testable without a transcript on disk.
pub trait TitleProbe {
    fn title(&self, cwd: &str, session_id: &str) -> Option<String>;
}

/// Reads the newest `custom-title` record from the session transcript.
///
/// Caches on transcript mtime for the same reason `TranscriptWork` does: the
/// watcher reconciles every two seconds and this is consulted for every
/// session, not just the statusless ones, so an uncached tail read would be the
/// most expensive thing in the loop.
pub struct TranscriptTitle {
    projects_dir: PathBuf,
    cache: Mutex<HashMap<String, (i64, Option<String>)>>,
}

impl TranscriptTitle {
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

    fn read(&self, cwd: &str, session_id: &str) -> Option<String> {
        use crate::bridge::transcript::{
            find_transcript, latest_custom_title, read_tail, TAIL_BYTES,
        };

        let path = find_transcript(&self.projects_dir, cwd, session_id)?;
        // No mtime means no cache key, so read rather than guess.
        let mtime = Self::modified_ms(&path)?;

        {
            let cache = self.cache.lock().expect("title cache poisoned");
            if let Some((cached_at, answer)) = cache.get(session_id) {
                if *cached_at == mtime {
                    return answer.clone();
                }
            }
        }

        // A transcript that exists but has no title is cached too: an untitled
        // session is the common case, and it must not cost a tail read a
        // second.
        let answer = read_tail(&path, TAIL_BYTES)
            .ok()
            .and_then(|bytes| latest_custom_title(&bytes));

        self.cache
            .lock()
            .expect("title cache poisoned")
            .insert(session_id.to_string(), (mtime, answer.clone()));

        answer
    }
}

impl TitleProbe for TranscriptTitle {
    fn title(&self, cwd: &str, session_id: &str) -> Option<String> {
        self.read(cwd, session_id)
    }
}

/// Reports nothing, for callers that do not care.
pub struct NoTitle;

impl TitleProbe for NoTitle {
    fn title(&self, _cwd: &str, _session_id: &str) -> Option<String> {
        None
    }
}

/// Test double keyed by session id.
pub struct FakeTitle {
    titles: HashMap<String, String>,
}

impl FakeTitle {
    pub fn new() -> Self {
        Self {
            titles: HashMap::new(),
        }
    }

    pub fn with(mut self, session_id: &str, title: &str) -> Self {
        self.titles
            .insert(session_id.to_string(), title.to_string());
        self
    }
}

impl Default for FakeTitle {
    fn default() -> Self {
        Self::new()
    }
}

impl TitleProbe for FakeTitle {
    fn title(&self, _cwd: &str, session_id: &str) -> Option<String> {
        self.titles.get(session_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TITLED: &str = concat!(
        r#"{"type":"custom-title","customTitle":"Rebase and conflicts","sessionId":"session-1"}"#,
        "\n"
    );

    const RETITLED: &str = concat!(
        r#"{"type":"custom-title","customTitle":"Rebase and conflicts","sessionId":"session-1"}"#,
        "\n",
        r#"{"type":"custom-title","customTitle":"Ship the release","sessionId":"session-1"}"#,
        "\n"
    );

    const UNTITLED: &str = concat!(r#"{"type":"user","gitBranch":"main"}"#, "\n");

    struct Fixture {
        root: PathBuf,
        transcript: PathBuf,
    }

    impl Fixture {
        fn new(tag: &str, body: &str) -> Self {
            let root = std::env::temp_dir().join(format!("cb-title-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let dir = root.join("-Users-n-Code-proj");
            std::fs::create_dir_all(&dir).unwrap();
            let transcript = dir.join("session-1.jsonl");
            std::fs::write(&transcript, body).unwrap();
            Self { root, transcript }
        }

        fn probe(&self) -> TranscriptTitle {
            TranscriptTitle::new(self.root.clone())
        }

        fn ask(&self, probe: &TranscriptTitle) -> Option<String> {
            probe.title("/Users/n/Code/proj", "session-1")
        }

        fn mtime(&self) -> std::time::SystemTime {
            std::fs::metadata(&self.transcript)
                .unwrap()
                .modified()
                .unwrap()
        }

        /// Rewrite the body and move mtime on by a second. Writing alone is not
        /// enough: the cache is keyed on whole milliseconds and two writes in a
        /// row land inside one often enough to make the test flaky. Same
        /// reasoning as `working.rs`, which learned it from CI.
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
    fn a_titled_session_reports_its_title() {
        let fixture = Fixture::new("titled", TITLED);
        assert_eq!(
            fixture.ask(&fixture.probe()).as_deref(),
            Some("Rebase and conflicts")
        );
    }

    #[test]
    fn the_newest_title_wins() {
        let fixture = Fixture::new("retitled", RETITLED);
        assert_eq!(
            fixture.ask(&fixture.probe()).as_deref(),
            Some("Ship the release")
        );
    }

    #[test]
    fn an_untitled_session_reports_nothing() {
        let fixture = Fixture::new("untitled", UNTITLED);
        assert_eq!(fixture.ask(&fixture.probe()), None);
    }

    #[test]
    fn a_missing_transcript_reports_nothing() {
        let probe = TranscriptTitle::new(std::env::temp_dir().join("cb-title-missing"));
        assert_eq!(probe.title("/Users/n/Code/proj", "session-1"), None);
    }

    #[test]
    fn an_unchanged_transcript_is_answered_from_cache() {
        let fixture = Fixture::new("cache-hit", TITLED);
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe).as_deref(), Some("Rebase and conflicts"));

        fixture.rewrite_keeping_mtime(RETITLED);
        assert_eq!(
            fixture.ask(&probe).as_deref(),
            Some("Rebase and conflicts"),
            "same mtime should not be re-read"
        );
    }

    /// The untitled answer is cached as well, so this proves the absence of a
    /// title is not re-read on every tick either.
    #[test]
    fn an_unchanged_untitled_transcript_is_answered_from_cache() {
        let fixture = Fixture::new("cache-hit-none", UNTITLED);
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe), None);

        fixture.rewrite_keeping_mtime(TITLED);
        assert_eq!(
            fixture.ask(&probe),
            None,
            "same mtime should not be re-read"
        );
    }

    #[test]
    fn a_changed_transcript_is_re_read() {
        let fixture = Fixture::new("cache-miss", TITLED);
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe).as_deref(), Some("Rebase and conflicts"));

        fixture.rewrite_advancing_mtime(RETITLED);
        assert_eq!(
            fixture.ask(&probe).as_deref(),
            Some("Ship the release"),
            "new mtime should be re-read"
        );
    }
}

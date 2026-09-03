use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use buddy_core::watcher::probes::TitleProbe;

/// Largest transcript worth scanning end to end for a title.
///
/// Only reached once per session, and only when the tail had nothing. The
/// figure itself lives in `bridge::transcript`, shared with the task probe.
pub const FULL_SCAN_MAX_BYTES: u64 = crate::bridge::transcript::FULL_SCAN_MAX_BYTES;

/// One session's cached answer.
struct Cached {
    /// Transcript mtime the answer was read at.
    at_ms: i64,
    title: Option<String>,
    /// Whether the whole file has been searched for this session. Once it has,
    /// a tail with no title means the title is simply older than the tail —
    /// not that there is none — so the answer already found stands.
    scanned_whole: bool,
}

/// Reads the newest `custom-title` record from the session transcript.
///
/// Caches on transcript mtime for the same reason `TranscriptWork` does: the
/// watcher reconciles every two seconds and this is consulted for every
/// session, not just the statusless ones, so an uncached tail read would be the
/// most expensive thing in the loop.
///
/// The tail alone is not enough, which the other transcript probes get away
/// with and this one does not. Claude Code appends a `custom-title` when it
/// names a session and again whenever it renames it — usually within the first
/// few exchanges and then never again — so in a session that has run for hours
/// the title sits megabytes behind the end. Measured on a live session: the
/// only `custom-title` record was 1.8MB from the end of a 1.9MB transcript, far
/// outside any tail worth reading every two seconds.
///
/// So the tail is read every time, because a *rename* lands at the end and has
/// to be picked up promptly, and a full scan happens at most once per session —
/// only when the tail yields nothing and the file has not been scanned before.
pub struct TranscriptTitle {
    projects_dir: PathBuf,
    cache: Mutex<HashMap<String, Cached>>,
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

        let (known, scanned_whole) = {
            let cache = self.cache.lock().expect("title cache poisoned");
            match cache.get(session_id) {
                Some(entry) if entry.at_ms == mtime => return entry.title.clone(),
                Some(entry) => (entry.title.clone(), entry.scanned_whole),
                None => (None, false),
            }
        };

        // A rename is appended like any other record, so the tail is what keeps
        // a retitled session current.
        let from_tail = read_tail(&path, TAIL_BYTES)
            .ok()
            .and_then(|bytes| latest_custom_title(&bytes));

        let (title, scanned_whole) = match (from_tail, scanned_whole) {
            (Some(title), _) => (Some(title), scanned_whole),
            // Nothing in the tail and nowhere else looked yet: the title may
            // simply be older than the tail, so pay for one full read.
            (None, false) => (Self::scan_whole(&path), true),
            // Already scanned. The tail having nothing is expected — it has had
            // nothing since the scan — and the answer from that scan stands.
            (None, true) => (known, true),
        };

        // An untitled session is cached too: it is the common case, and it must
        // not cost a read a second, nor a full scan every tick.
        self.cache.lock().expect("title cache poisoned").insert(
            session_id.to_string(),
            Cached {
                at_ms: mtime,
                title: title.clone(),
                scanned_whole,
            },
        );

        title
    }

    /// Search the whole transcript, newest record first.
    fn scan_whole(path: &std::path::Path) -> Option<String> {
        Self::scan_whole_within(path, FULL_SCAN_MAX_BYTES)
    }

    /// The size guard, taken as an argument so a test can exercise it without
    /// writing a file the size of the real limit.
    fn scan_whole_within(path: &std::path::Path, max_bytes: u64) -> Option<String> {
        use crate::bridge::transcript::latest_custom_title;

        if std::fs::metadata(path).ok()?.len() > max_bytes {
            return None;
        }
        latest_custom_title(&std::fs::read(path).ok()?)
    }
}

impl TitleProbe for TranscriptTitle {
    fn title(&self, cwd: &str, session_id: &str) -> Option<String> {
        self.read(cwd, session_id)
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

    /// The case the tail alone gets wrong. Claude Code writes `custom-title`
    /// when it names a session and then not again, so in a long session the
    /// title is buried far behind the end of the file.
    #[test]
    fn a_title_older_than_the_tail_is_still_found() {
        let filler = format!("{}\n", r#"{"type":"user","gitBranch":"main"}"#).repeat(4_000);
        let fixture = Fixture::new("buried", &format!("{TITLED}{filler}"));

        assert!(
            std::fs::metadata(&fixture.transcript).unwrap().len()
                > crate::bridge::transcript::TAIL_BYTES,
            "fixture must be longer than the tail or it proves nothing"
        );
        assert_eq!(
            fixture.ask(&fixture.probe()).as_deref(),
            Some("Rebase and conflicts")
        );
    }

    /// A rename lands at the end of the file, so the tail has to keep winning
    /// over whatever the one full scan turned up.
    #[test]
    fn a_rename_outranks_a_title_found_by_the_full_scan() {
        let filler = format!("{}\n", r#"{"type":"user","gitBranch":"main"}"#).repeat(4_000);
        let fixture = Fixture::new("buried-renamed", &format!("{TITLED}{filler}"));
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe).as_deref(), Some("Rebase and conflicts"));

        fixture.rewrite_advancing_mtime(&format!(
            "{TITLED}{filler}{}",
            r#"{"type":"custom-title","customTitle":"Ship the release","sessionId":"session-1"}"#
        ));

        assert_eq!(fixture.ask(&probe).as_deref(), Some("Ship the release"));
    }

    /// Once the whole file has been searched, later ticks must not keep
    /// searching it: the tail having nothing is the expected steady state.
    #[test]
    fn the_full_scan_is_not_repeated_after_the_file_grows() {
        let filler = format!("{}\n", r#"{"type":"user","gitBranch":"main"}"#).repeat(4_000);
        let fixture = Fixture::new("scanned-once", &format!("{TITLED}{filler}"));
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe).as_deref(), Some("Rebase and conflicts"));

        // The title is gone from the file entirely. A second full scan would
        // find nothing and drop the name off the row; the cached answer stands.
        fixture.rewrite_advancing_mtime(&filler);

        assert_eq!(fixture.ask(&probe).as_deref(), Some("Rebase and conflicts"));
    }

    #[test]
    fn a_transcript_too_large_to_scan_reports_nothing() {
        let filler = format!("{}\n", r#"{"type":"user","gitBranch":"main"}"#).repeat(4_000);
        let fixture = Fixture::new("oversize", &format!("{TITLED}{filler}"));
        // Not a real 32MB file: the guard is what is under test, and writing
        // one would make this the slowest test in the suite.
        assert!(TranscriptTitle::scan_whole_within(&fixture.transcript, 0).is_none());
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

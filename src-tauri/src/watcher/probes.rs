//! The traits `state::snapshot` takes as inputs.
//!
//! Separate from the modules that implement them because the questions are not
//! provider-specific but every answer is: each implementation here reads a
//! Claude Code transcript, whereas the trait is just "can you tell me whether
//! this session is busy". A second agent answers the same questions from its
//! own source.
//!
//! Every trait below is injected into `state::snapshot` rather than called
//! directly, matching `PidLiveness` (`liveness.rs`) and `TaskProbe`
//! (`task.rs`), so the state machine stays testable without a transcript
//! on disk.
//!
//! Each trait is followed by its two doubles — a no-op for callers that do not
//! care, and a `HashMap`/`HashSet`-backed fake for tests — rather than all four
//! traits first and all eight doubles after. These are four unrelated probes,
//! not a hierarchy, so keeping a probe's trait and its doubles together lets a
//! reader take in one probe fully before moving to the next.

use std::collections::{HashMap, HashSet};

/// Last time a session showed any sign of life.
///
/// Needed because only `cli` sessions write `status` to the registry. A
/// `claude-desktop` session has no `status`, no `statusUpdatedAt` and no
/// `updatedAt` at all, so age since `startedAt` is the only thing left — and
/// that ages a session the user is actively working in into `paused`.
///
/// The transcript is appended on every message and every tool result, so its
/// modification time tracks real activity closely.
pub trait ActivityProbe {
    fn last_activity_ms(&self, cwd: &str, session_id: &str) -> Option<i64>;
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

/// Whether a session is blocked on the user despite never saying so.
///
/// Claude Desktop writes no `status`, `statusUpdatedAt` or `waitingFor` at all,
/// so `state::snapshot` falls back to transcript mtime — which can only
/// distinguish busy from quiet. A session sitting on an unanswered
/// `AskUserQuestion` is quiet, and rendered grey, while it is in fact blocked.
pub trait BlockedProbe {
    fn pending_prompt(&self, cwd: &str, session_id: &str) -> Option<String>;
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
        Self {
            prompts: HashMap::new(),
        }
    }

    pub fn with(mut self, session_id: &str, label: &str) -> Self {
        self.prompts
            .insert(session_id.to_string(), label.to_string());
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

/// Whether a session has a tool call still running.
///
/// Claude Desktop writes no `status`, so `state::snapshot` falls back to
/// transcript mtime — and a transcript is silent for as long as a single tool
/// call takes. A build, a test run or a subagent can hold a session quiet for
/// minutes while it is plainly working, which mtime alone reads as `idle`.
pub trait WorkProbe {
    fn in_flight(&self, cwd: &str, session_id: &str) -> bool;
}

/// Reports nothing, for callers that do not care.
pub struct NoWork;

impl WorkProbe for NoWork {
    fn in_flight(&self, _cwd: &str, _session_id: &str) -> bool {
        false
    }
}

/// Test double keyed by session id.
pub struct FakeWork {
    working: HashSet<String>,
}

impl FakeWork {
    pub fn new() -> Self {
        Self {
            working: HashSet::new(),
        }
    }

    pub fn with(mut self, session_id: &str) -> Self {
        self.working.insert(session_id.to_string());
        self
    }
}

impl Default for FakeWork {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkProbe for FakeWork {
    fn in_flight(&self, _cwd: &str, session_id: &str) -> bool {
        self.working.contains(session_id)
    }
}

/// What a session calls itself.
///
/// The registry only ever carries `<folder>-<2 chars>`: `nameSource` is
/// `derived` for every session Claude Code writes, so the row would read as a
/// list of repositories even when three of them are the same repository. The
/// title Claude Code gives a session says what the session is *doing*, and it
/// lives in the transcript rather than the registry.
pub trait TitleProbe {
    fn title(&self, cwd: &str, session_id: &str) -> Option<String>;
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

//! The traits `state::snapshot` takes as inputs.
//!
//! Separate from the modules that implement them because the questions are not
//! provider-specific but every answer is: each implementation here reads a
//! Claude Code transcript, whereas the trait is just "can you tell me whether
//! this session is busy". A second agent answers the same questions from its
//! own source.

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

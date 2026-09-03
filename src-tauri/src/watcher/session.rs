/// One session, in terms the state machine can reason about without knowing
/// which agent produced it.
///
/// This is deliberately the same twelve fields `RegistryFile` carries, because
/// `snapshot()` reads all twelve. The difference is that nothing here is tied
/// to one provider's file format: `RegistryFile` owns the serde spelling of
/// Claude Code's `~/.claude/sessions/<pid>.json`, and a second provider maps
/// its own source onto `RawSession` instead of being forced through that
/// schema.
#[derive(Debug, Clone, PartialEq)]
pub struct RawSession {
    pub pid: i32,
    pub session_id: String,
    pub cwd: String,
    pub started_at: i64,
    /// Process start time as the registry spells it.
    ///
    /// Carried, but deliberately **not** used to tell a live pid from a
    /// recycled one — `PidLiveness` checks identity against `started_at`
    /// instead. Claude Code writes this string in a different timezone than
    /// `ps -o lstart=` prints, so comparing the two marks every live session
    /// dead; `liveness.rs` has the detail and `state.rs` has the regression
    /// test. A provider filling this in should not expect it to be read.
    pub proc_start: Option<String>,
    pub entrypoint: Option<String>,
    /// `interactive`, `bg` or `sdk`.
    pub kind: Option<String>,
    /// Present on background jobs, which belong to a session rather than
    /// being one.
    pub job_id: Option<String>,
    pub name: Option<String>,
    pub status: Option<String>,
    pub status_updated_at: Option<i64>,
    pub waiting_for: Option<String>,
}

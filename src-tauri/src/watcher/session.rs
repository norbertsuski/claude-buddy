use crate::watcher::registry::RegistryFile;

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

impl From<RegistryFile> for RawSession {
    fn from(f: RegistryFile) -> Self {
        Self {
            pid: f.pid,
            session_id: f.session_id,
            cwd: f.cwd,
            started_at: f.started_at,
            proc_start: f.proc_start,
            entrypoint: f.entrypoint,
            kind: f.kind,
            job_id: f.job_id,
            name: f.name,
            status: f.status,
            status_updated_at: f.status_updated_at,
            waiting_for: f.waiting_for,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::registry::RegistryFile;

    fn registry_file() -> RegistryFile {
        RegistryFile {
            pid: 7952,
            session_id: "a1b2c3d4-0000-4000-8000-000000000001".to_string(),
            cwd: "/Users/n/Code/api-service".to_string(),
            started_at: 1_787_637_231_465,
            proc_start: Some("Tue Aug 25 05:53:49 2026".to_string()),
            entrypoint: Some("cli".to_string()),
            kind: Some("interactive".to_string()),
            job_id: Some("job-1".to_string()),
            name: Some("api-service".to_string()),
            status: Some("waiting".to_string()),
            status_updated_at: Some(1_787_637_299_000),
            waiting_for: Some("input".to_string()),
        }
    }

    /// Every field survives the mapping. A field silently dropped here would
    /// present as a state the widget never enters, which is expensive to
    /// diagnose from the UI end.
    #[test]
    fn conversion_preserves_every_field() {
        let raw = RawSession::from(registry_file());

        assert_eq!(raw.pid, 7952);
        assert_eq!(raw.session_id, "a1b2c3d4-0000-4000-8000-000000000001");
        assert_eq!(raw.cwd, "/Users/n/Code/api-service");
        assert_eq!(raw.started_at, 1_787_637_231_465);
        assert_eq!(raw.proc_start.as_deref(), Some("Tue Aug 25 05:53:49 2026"));
        assert_eq!(raw.entrypoint.as_deref(), Some("cli"));
        assert_eq!(raw.kind.as_deref(), Some("interactive"));
        assert_eq!(raw.job_id.as_deref(), Some("job-1"));
        assert_eq!(raw.name.as_deref(), Some("api-service"));
        assert_eq!(raw.status.as_deref(), Some("waiting"));
        assert_eq!(raw.status_updated_at, Some(1_787_637_299_000));
        assert_eq!(raw.waiting_for.as_deref(), Some("input"));
    }

    #[test]
    fn absent_optional_fields_stay_absent() {
        let mut file = registry_file();
        file.proc_start = None;
        file.entrypoint = None;
        file.kind = None;
        file.job_id = None;
        file.name = None;
        file.status = None;
        file.status_updated_at = None;
        file.waiting_for = None;

        let raw = RawSession::from(file);

        assert!(raw.proc_start.is_none());
        assert!(raw.entrypoint.is_none());
        assert!(raw.kind.is_none());
        assert!(raw.job_id.is_none());
        assert!(raw.name.is_none());
        assert!(raw.status.is_none());
        assert!(raw.status_updated_at.is_none());
        assert!(raw.waiting_for.is_none());
    }
}

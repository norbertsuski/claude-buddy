use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::watcher::session::RawSession;

/// One `~/.claude/sessions/<pid>.json` record.
///
/// Only fields claude-buddy consumes are modelled. Unknown fields are ignored
/// so a Claude Code upgrade that adds keys cannot break parsing.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryFile {
    pub pid: i32,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub cwd: String,
    #[serde(rename = "startedAt")]
    pub started_at: i64,
    #[serde(rename = "procStart")]
    pub proc_start: Option<String>,
    pub entrypoint: Option<String>,
    /// `interactive`, `bg` or `sdk`. Only interactive entries are sessions the
    /// user drives; `bg` entries are background jobs and spares.
    pub kind: Option<String>,
    /// Present on background jobs. A `bg` entry with a job id belongs to a
    /// session rather than being one.
    #[serde(rename = "jobId")]
    pub job_id: Option<String>,
    pub name: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "statusUpdatedAt")]
    pub status_updated_at: Option<i64>,
    #[serde(rename = "waitingFor")]
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

/// Parse one registry file. Returns `None` on any malformed input — including a
/// truncated read, which is expected because registry writes are not atomic.
pub fn parse_registry_file(bytes: &[u8]) -> Option<RegistryFile> {
    serde_json::from_slice(bytes).ok()
}

/// Environment variable pointing the watcher at a different registry directory.
///
/// Exists so the widget can be driven from fixtures — the real directory only
/// contains whatever sessions happen to be running, which makes states like a
/// full row of sessions awkward to see.
pub const REGISTRY_DIR_ENV: &str = "CLAUDE_BUDDY_REGISTRY_DIR";

/// The live session registry directory.
pub fn registry_dir() -> PathBuf {
    if let Some(override_dir) = std::env::var_os(REGISTRY_DIR_ENV) {
        return PathBuf::from(override_dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".claude")
        .join("sessions")
}

/// Read every parseable `<pid>.json` in `dir`. Unparseable and non-matching
/// files are skipped silently; a missing directory yields an empty vec.
///
/// This function never writes to `dir`.
pub fn read_registry_dir(dir: &Path) -> Vec<RegistryFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let stem = path.file_stem()?.to_str()?;
            if path.extension()?.to_str()? != "json" || stem.parse::<i32>().is_err() {
                return None;
            }
            parse_registry_file(&std::fs::read(&path).ok()?)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{
      "pid": 7952,
      "sessionId": "a1b2c3d4-0000-4000-8000-000000000001",
      "cwd": "/Users/n/Code/api-service",
      "startedAt": 1787637231465,
      "procStart": "Tue Aug 25 05:53:49 2026",
      "version": "2.1.234",
      "kind": "interactive",
      "entrypoint": "cli",
      "messagingSocketPath": "/tmp/cc-socks/7952.sock",
      "name": "api-service-55",
      "nameSource": "derived",
      "jobId": null,
      "status": "waiting",
      "updatedAt": 1787662267409,
      "statusUpdatedAt": 1787662267409,
      "waitingFor": "input needed"
    }"#;

    // A session that has not reported status yet. Absence is normal, not an error.
    const NO_STATUS: &str = r#"{
      "pid": 99215,
      "sessionId": "a1b2c3d4-0000-4000-8000-000000000002",
      "cwd": "/Users/n/Code/claude-buddy",
      "startedAt": 1787662276356,
      "procStart": "Tue Aug 25 12:51:15 2026",
      "entrypoint": "claude-desktop",
      "name": "claude-buddy-1f"
    }"#;

    #[test]
    fn parses_a_full_record() {
        let f = parse_registry_file(FULL.as_bytes()).expect("should parse");
        assert_eq!(f.pid, 7952);
        assert_eq!(f.name.as_deref(), Some("api-service-55"));
        assert_eq!(f.status.as_deref(), Some("waiting"));
        assert_eq!(f.waiting_for.as_deref(), Some("input needed"));
        assert_eq!(f.status_updated_at, Some(1787662267409));
        assert_eq!(f.proc_start.as_deref(), Some("Tue Aug 25 05:53:49 2026"));
        assert_eq!(f.kind.as_deref(), Some("interactive"));
    }

    #[test]
    fn parses_a_record_with_no_status_fields() {
        let f = parse_registry_file(NO_STATUS.as_bytes()).expect("should parse");
        assert_eq!(f.pid, 99215);
        assert_eq!(f.status, None);
        assert_eq!(f.status_updated_at, None);
        assert_eq!(f.waiting_for, None);
        assert_eq!(f.entrypoint.as_deref(), Some("claude-desktop"));
        assert_eq!(f.kind, None);
    }

    #[test]
    fn returns_none_for_truncated_json() {
        // Registry writes are not atomic; a read can land mid-write.
        let truncated = &FULL.as_bytes()[..FULL.len() / 2];
        assert!(parse_registry_file(truncated).is_none());
    }

    #[test]
    fn returns_none_when_required_fields_are_missing() {
        assert!(parse_registry_file(br#"{"pid": 1}"#).is_none());
    }

    #[test]
    fn reads_only_numeric_json_filenames_from_a_directory() {
        let dir = std::env::temp_dir().join(format!("cb-registry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("7952.json"), FULL).unwrap();
        std::fs::write(dir.join("99215.json"), NO_STATUS).unwrap();
        std::fs::write(dir.join("7952.abcdef.key"), "not json").unwrap();
        std::fs::write(dir.join("garbage.json"), "{ broken").unwrap();

        let mut files = read_registry_dir(&dir);
        files.sort_by_key(|f| f.pid);

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].pid, 7952);
        assert_eq!(files[1].pid, 99215);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_directory_yields_empty_vec() {
        let missing = std::path::Path::new("/nonexistent/claude-buddy/registry");
        assert!(read_registry_dir(missing).is_empty());
    }

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

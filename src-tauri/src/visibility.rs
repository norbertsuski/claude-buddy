use crate::watcher::state::{SessionSnapshot, SessionState};

/// Accepted values for the `hideWhen` setting.
pub const HIDE_MODES: [&str; 3] = ["never", "noSessions", "nothingActive"];

/// Whether the widget should be off screen right now.
///
/// Pure, so the policy is tested without a window server. The caller owns the
/// panel; this only decides.
pub fn should_hide(sessions: &[SessionSnapshot], hide_when: &str) -> bool {
    match hide_when {
        "noSessions" => sessions.is_empty(),
        "nothingActive" => !sessions
            .iter()
            .any(|s| matches!(s.state, SessionState::Waiting | SessionState::Busy)),
        // "never", and anything unrecognised: showing is the safe failure.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(state: SessionState) -> SessionSnapshot {
        SessionSnapshot {
            pid: 1,
            session_id: "a".into(),
            name: "api-service".into(),
            cwd: "/Users/n/Code/api-service".into(),
            entrypoint: "cli".into(),
            state,
            detail: None,
            elapsed_ms: 0,
            uptime_ms: 0,
            status_time_ms: 0,
            started_at_ms: 0,
            background: false,
        }
    }

    #[test]
    fn never_always_shows() {
        assert!(!should_hide(&[], "never"));
        assert!(!should_hide(&[session(SessionState::Idle)], "never"));
        assert!(!should_hide(&[session(SessionState::Waiting)], "never"));
    }

    #[test]
    fn no_sessions_hides_only_an_empty_list() {
        assert!(should_hide(&[], "noSessions"));
        assert!(!should_hide(&[session(SessionState::Idle)], "noSessions"));
        assert!(!should_hide(&[session(SessionState::Paused)], "noSessions"));
    }

    #[test]
    fn nothing_active_hides_a_quiet_list() {
        assert!(should_hide(&[], "nothingActive"));
        assert!(should_hide(&[session(SessionState::Idle)], "nothingActive"));
        assert!(should_hide(&[session(SessionState::Paused)], "nothingActive"));
        assert!(should_hide(&[session(SessionState::Dead)], "nothingActive"));
    }

    #[test]
    fn nothing_active_shows_for_waiting_or_busy() {
        assert!(!should_hide(&[session(SessionState::Waiting)], "nothingActive"));
        assert!(!should_hide(&[session(SessionState::Busy)], "nothingActive"));
    }

    #[test]
    fn an_unrecognised_mode_shows_rather_than_hiding() {
        // A hand-edited config must not be able to make the widget vanish with
        // no way to reason about why.
        assert!(!should_hide(&[], "hologram"));
    }
}

use crate::watcher::state::{SessionSnapshot, SessionState};

/// Accepted values for the `hideWhen` setting.
pub const HIDE_MODES: [&str; 3] = ["never", "noSessions", "nothingActive"];

/// Whether the widget should be off screen right now.
///
/// Pure, so the policy is tested without a window server. The caller owns the
/// panel; this only decides.
///
/// `hidden` is the tray menu's "Hide widget" and wins over every mode,
/// including `never`: it is an explicit instruction from the user, and a
/// widget that reappeared mid-screen-share because a session woke up would be
/// exactly the failure it exists to prevent.
pub fn should_hide(sessions: &[SessionSnapshot], hide_when: &str, hidden: bool) -> bool {
    if hidden {
        return true;
    }
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

    /// `should_hide` with no manual hide in play, which is what every policy
    /// test is about.
    fn auto(sessions: &[SessionSnapshot], hide_when: &str) -> bool {
        should_hide(sessions, hide_when, false)
    }

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
        assert!(!auto(&[], "never"));
        assert!(!auto(&[session(SessionState::Idle)], "never"));
        assert!(!auto(&[session(SessionState::Waiting)], "never"));
    }

    #[test]
    fn no_sessions_hides_only_an_empty_list() {
        assert!(auto(&[], "noSessions"));
        assert!(!auto(&[session(SessionState::Idle)], "noSessions"));
        assert!(!auto(&[session(SessionState::Paused)], "noSessions"));
    }

    #[test]
    fn nothing_active_hides_a_quiet_list() {
        assert!(auto(&[], "nothingActive"));
        assert!(auto(&[session(SessionState::Idle)], "nothingActive"));
        assert!(auto(&[session(SessionState::Paused)], "nothingActive"));
        assert!(auto(&[session(SessionState::Dead)], "nothingActive"));
    }

    #[test]
    fn nothing_active_shows_for_waiting_or_busy() {
        assert!(!auto(&[session(SessionState::Waiting)], "nothingActive"));
        assert!(!auto(&[session(SessionState::Busy)], "nothingActive"));
    }

    #[test]
    fn a_manual_hide_outranks_every_mode() {
        // Including `never`, and including a session actively waiting on the
        // user: the menu item is an instruction, not a preference.
        assert!(should_hide(&[], "never", true));
        assert!(should_hide(
            &[session(SessionState::Waiting)],
            "never",
            true
        ));
        assert!(should_hide(
            &[session(SessionState::Busy)],
            "nothingActive",
            true
        ));
    }

    #[test]
    fn unhiding_hands_the_decision_back_to_the_mode() {
        assert!(!should_hide(&[session(SessionState::Idle)], "never", false));
        assert!(should_hide(&[], "noSessions", false));
    }

    #[test]
    fn an_unrecognised_mode_shows_rather_than_hiding() {
        // A hand-edited config must not be able to make the widget vanish with
        // no way to reason about why.
        assert!(!auto(&[], "hologram"));
    }
}

//! Holding the display awake while an agent is working.
//!
//! The widget already derives "is anything working right now" every tick and
//! does nothing with the answer beyond drawing it. This acts on it: while the
//! user has asked for it and something is busy or waiting on them, macOS is
//! told not to idle-sleep the display.
//!
//! Split the way `visibility` is — a pure policy that answers *whether*, and a
//! holder that does the platform call. The policy is then testable with no
//! window server and no IOKit.

use std::sync::Mutex;

use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};

use crate::watcher::state::{SessionSnapshot, SessionState};

/// Whether the display should be held on right now.
///
/// `Waiting` counts alongside `Busy` on purpose: a session blocked on a
/// permission prompt is the case where a sleeping, locked display costs the
/// user the most — the question is behind it and the run is going nowhere.
///
/// Background jobs need no clause here. `snapshot()` has already filtered the
/// list by the `show_background_jobs` setting, so a subagent the user has
/// chosen not to see is not in `sessions` and cannot hold the display on.
pub fn should_stay_awake(sessions: &[SessionSnapshot], keep_awake: bool) -> bool {
    keep_awake
        && sessions
            .iter()
            .any(|s| matches!(s.state, SessionState::Waiting | SessionState::Busy))
}

/// What `pmset -g assertions` prints while this is held. It has to say who is
/// keeping the display on and why, since that listing is the only place a user
/// can go to find out.
const ASSERTION_NAME: &str = "claude-buddy: agent working";

/// The live assertion, or `None` while the display is free to sleep.
static ASSERTION: Mutex<Option<u32>> = Mutex::new(None);

/// Engage or release the display-sleep assertion, to match `want`.
///
/// Idempotent: creates only on `false -> true`, releases only on
/// `true -> false`. That is what makes it safe to call on every watcher tick —
/// an unchanged answer costs a mutex lock and nothing else.
///
/// There is no release-on-quit counterpart. The kernel drops assertions held by
/// a process that exits, which is also the reason this is an in-process call
/// rather than a spawned `caffeinate`: nothing survives us to leak.
pub fn apply(want: bool) {
    let mut held = ASSERTION.lock().expect("awake assertion poisoned");
    match (want, *held) {
        (true, None) => {
            if let Some(id) = create() {
                *held = Some(id);
            }
        }
        (false, Some(id)) => {
            // Cleared either way. A release that failed leaves an assertion
            // this process can no longer name, and retrying it every tick for
            // the rest of the session would not fix that.
            unsafe { IOPMAssertionRelease(id) };
            *held = None;
        }
        _ => {}
    }
}

fn create() -> Option<u32> {
    let kind = CFString::from_static_string("PreventUserIdleDisplaySleep");
    let name = CFString::new(ASSERTION_NAME);
    let mut id: u32 = 0;
    // SAFETY: both strings outlive the call, and `id` is only read on success.
    let result = unsafe {
        IOPMAssertionCreateWithName(
            kind.as_concrete_TypeRef(),
            ASSERTION_LEVEL_ON,
            name.as_concrete_TypeRef(),
            &mut id,
        )
    };
    (result == 0).then_some(id)
}

/// `kIOPMAssertionLevelOn`.
const ASSERTION_LEVEL_ON: u32 = 255;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: CFStringRef,
        assertion_level: u32,
        assertion_name: CFStringRef,
        assertion_id: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(assertion_id: u32) -> i32;
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
    fn the_setting_off_never_holds_the_display() {
        assert!(!should_stay_awake(&[], false));
        assert!(!should_stay_awake(&[session(SessionState::Busy)], false));
        assert!(!should_stay_awake(&[session(SessionState::Waiting)], false));
    }

    #[test]
    fn a_busy_session_holds_the_display() {
        assert!(should_stay_awake(&[session(SessionState::Busy)], true));
    }

    #[test]
    fn a_waiting_session_holds_the_display() {
        // The prompt is behind the screen that is about to go dark, so this is
        // the case the feature most exists for.
        assert!(should_stay_awake(&[session(SessionState::Waiting)], true));
    }

    #[test]
    fn a_quiet_session_lets_the_display_sleep() {
        assert!(!should_stay_awake(&[session(SessionState::Idle)], true));
        assert!(!should_stay_awake(&[session(SessionState::Paused)], true));
        assert!(!should_stay_awake(&[session(SessionState::Dead)], true));
    }

    #[test]
    fn no_sessions_lets_the_display_sleep() {
        assert!(!should_stay_awake(&[], true));
    }

    #[test]
    fn one_busy_session_among_quiet_ones_is_enough() {
        let sessions = [
            session(SessionState::Idle),
            session(SessionState::Dead),
            session(SessionState::Busy),
        ];
        assert!(should_stay_awake(&sessions, true));
    }
}

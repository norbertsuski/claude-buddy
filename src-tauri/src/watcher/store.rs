//! Where the latest snapshot lives between watcher ticks.
//!
//! Its own module rather than part of `watch.rs`, because the windowing code
//! reads the store but has no business with the loop that fills it.

use crate::watcher::state::SessionSnapshot;

/// The most recent snapshot, readable by the frontend on demand.
///
/// The watcher emits its first snapshot within milliseconds of startup, long
/// before the webview has loaded and subscribed, and the change filter then
/// suppresses every later emission while state stays the same. Without a
/// fetchable copy the UI would sit empty indefinitely.
#[derive(Default)]
pub struct SnapshotStore(std::sync::Mutex<Vec<SessionSnapshot>>);

impl SnapshotStore {
    pub fn set(&self, sessions: Vec<SessionSnapshot>) {
        *self.0.lock().expect("snapshot store poisoned") = sessions;
    }

    pub fn get(&self) -> Vec<SessionSnapshot> {
        self.0.lock().expect("snapshot store poisoned").clone()
    }
}

//! What time it is, in the units everything else here counts in.
//!
//! Its own module because the watcher loop is not the authority on the clock:
//! the tray, the notifier and the usage meter all need it, and half of those
//! are provider-agnostic while `watch.rs` is not.

/// Epoch milliseconds now.
///
/// A clock that cannot be read is treated as the epoch rather than a panic:
/// every caller is either rendering an age or comparing against a deadline, and
/// a widget that draws the wrong duration is better than one that does not draw.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

//! Asking the API what the five-hour window is at, rather than waiting for
//! Claude Code to.
//!
//! `crate::usage` reads a cache Claude Code writes only when it fetches usage
//! itself — in practice when someone opens its `/usage` panel — so the figure
//! on the widget can sit hours behind whatever has actually been spent. This
//! module closes that gap by making the same request Claude Code makes,
//! `GET /api/oauth/usage`, with the OAuth token Claude Code already holds.
//!
//! Three things about that are worth being explicit about, because they set the
//! shape of everything below:
//!
//! * The endpoint is private and undocumented. It can change or disappear in
//!   any Claude Code release, so every step here is fallible and every failure
//!   is silent: the meter simply does not appear. There is no fallback to
//!   Claude Code's own cache of the figure — it is refreshed so rarely that
//!   showing it as if it were current is worse than showing nothing.
//! * The token is borrowed, never managed. An expired access token is a reason
//!   to stop and fall back, not to run the refresh flow — rotating the token
//!   out from under Claude Code, which owns it, is not this app's business.
//! * Reading the token can block on a Keychain dialog the user may never
//!   answer, so none of this runs on the watcher's tick thread. It gets a
//!   thread of its own and publishes into a mutex the tick reads.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::usage::Usage;

/// The request Claude Code makes, and the base it makes it against.
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// How often the window is re-fetched.
///
/// Claude Code will not write its own cache more than once every five minutes,
/// so five minutes is both the resolution the figure actually has and a rate
/// that cannot be called abusive for a meter someone leaves running all day.
const POLL: Duration = Duration::from_secs(300);

/// How long to wait before the first fetch.
///
/// Long enough that launching the app does not put a Keychain prompt on screen
/// while the user is still looking at something else, short enough that the
/// meter is live well within the first window.
const FIRST_FETCH_DELAY: Duration = Duration::from_secs(5);

/// Environment variable holding an OAuth token outright, as Claude Code reads
/// it. Checked first, so a token supplied for this app is never overridden by
/// whatever is on the login Keychain.
const TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// Most recent successful fetch, or `None` until one lands.
///
/// Global rather than threaded through, because the producer is one thread
/// started once at setup and the consumer is the watcher tick, and neither
/// exists in the other's world.
fn live() -> &'static Mutex<Option<Usage>> {
    static LIVE: OnceLock<Mutex<Option<Usage>>> = OnceLock::new();
    LIVE.get_or_init(|| Mutex::new(None))
}

/// Set once the refresher thread is running, so a second `start` is a no-op.
static STARTED: AtomicBool = AtomicBool::new(false);

/// The most recent fetched figure, or `None` if there is none or the window it
/// describes has passed.
///
/// The lapsed check is repeated here rather than trusted from fetch time for
/// the same reason the watcher repeats it against the cache: a window can run
/// out between polls, and a spent figure must not stay on screen.
pub fn latest(now_ms: i64) -> Option<Usage> {
    // A fixture stands in for the whole of this, fetch included: the screenshots
    // are taken against it precisely so the real figure is not in them.
    if fixture_mode() {
        return crate::usage::fixture(now_ms);
    }
    let usage = *live().lock().ok()?;
    usage.filter(|u| u.resets_at_ms > now_ms)
}

/// Start the refresher thread. Idempotent.
pub fn start() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        std::thread::sleep(FIRST_FETCH_DELAY);
        loop {
            // Read per iteration, so hiding the meter stops the requests
            // without a restart and showing it again resumes them. Tied to the
            // meter's own setting: a figure nobody is being shown is not worth
            // a request, and a shown figure is not worth having stale.
            if crate::config::cached().show_usage && !fixture_mode() {
                if let Some(usage) = fetch(crate::clock::now_ms()) {
                    if let Ok(mut slot) = live().lock() {
                        *slot = Some(usage);
                    }
                }
            }
            std::thread::sleep(POLL);
        }
    });
}

/// Whether the meter is pointed at a fixture file.
///
/// The override exists so the documentation screenshots are not taken against
/// the real account, and a live fetch would defeat it by putting the real
/// figure back on screen.
fn fixture_mode() -> bool {
    std::env::var_os(crate::usage::USAGE_FILE_ENV).is_some()
}

/// One request, or `None` on any failure at all.
fn fetch(now_ms: i64) -> Option<Usage> {
    let token = token()?;
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?
        .get(USAGE_URL)
        .bearer_auth(token.trim())
        .header("Content-Type", "application/json")
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: serde_json::Value = response.json().ok()?;
    // The response *is* the utilization object: what Claude Code caches under
    // `cachedUsageUtilization.utilization` is this body, stored verbatim, which
    // is why the fixtures are still in that shape.
    crate::usage::parse_utilization(&body, now_ms)
}

/// An OAuth access token belonging to Claude Code, from wherever it keeps one.
///
/// Sources in the order Claude Code itself prefers them. Any source that yields
/// a token whose expiry has passed is skipped rather than refreshed: see the
/// module docs.
fn token() -> Option<String> {
    if let Some(token) = std::env::var(TOKEN_ENV)
        .ok()
        .filter(|t| !t.trim().is_empty())
    {
        return Some(token);
    }
    if let Some(token) = credentials_file().and_then(|bytes| token_from_json(&bytes)) {
        return Some(token);
    }
    keychain().and_then(|json| token_from_json(json.as_bytes()))
}

/// The credentials file, for installs that keep the token on disk rather than
/// on the Keychain.
fn credentials_file() -> Option<Vec<u8>> {
    let path = dirs::home_dir()?.join(".claude").join(".credentials.json");
    std::fs::read(path).ok()
}

/// The token as the login Keychain holds it.
///
/// Shelled out to `security` rather than linked against the Keychain API: the
/// token then never reaches an argument list, the first read raises the system's
/// own permission dialog, and there is no third crate in the way of a string.
///
/// The account the item is filed under is the current user, which is what
/// Claude Code files it as; the service is tried with and without an account so
/// an item stored either way is found.
fn keychain() -> Option<String> {
    const SERVICE: &str = "Claude Code-credentials";
    let user = std::env::var("USER").ok();
    let attempts: [Vec<&str>; 2] = [
        match user.as_deref() {
            Some(user) => vec!["find-generic-password", "-a", user, "-w", "-s", SERVICE],
            None => vec!["find-generic-password", "-w", "-s", SERVICE],
        },
        vec!["find-generic-password", "-w", "-s", SERVICE],
    ];
    for args in attempts {
        let Ok(output) = std::process::Command::new("security").args(&args).output() else {
            continue;
        };
        if output.status.success() {
            let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Pull a live access token out of a credentials blob.
///
/// The blob is the same shape wherever it comes from: the Keychain item holds
/// the file's contents. An `expiresAt` in the past means the token will be
/// refused, so it is treated as no token at all — one poll skipped, and the
/// next one picks up whatever Claude Code has refreshed to by then.
fn token_from_json(bytes: &[u8]) -> Option<String> {
    let root: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let oauth = root.get("claudeAiOauth")?;
    let token = oauth.get("accessToken")?.as_str()?.trim();
    if token.is_empty() {
        return None;
    }
    // A missing expiry is not treated as expired: it is a field of someone
    // else's file, and its absence says nothing about the token.
    if let Some(expires_at) = oauth.get("expiresAt").and_then(|v| v.as_i64()) {
        if expires_at <= crate::clock::now_ms() {
            return None;
        }
    }
    Some(token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn future_ms() -> i64 {
        crate::clock::now_ms() + 3_600_000
    }

    #[test]
    fn reads_a_live_token() {
        let json = format!(
            r#"{{"claudeAiOauth":{{"accessToken":"tok","expiresAt":{}}}}}"#,
            future_ms()
        );
        assert_eq!(token_from_json(json.as_bytes()).as_deref(), Some("tok"));
    }

    #[test]
    fn accepts_a_token_with_no_expiry() {
        let json = br#"{"claudeAiOauth":{"accessToken":"tok"}}"#;
        assert_eq!(token_from_json(json).as_deref(), Some("tok"));
    }

    #[test]
    fn rejects_an_expired_token() {
        let json = br#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":1}}"#;
        assert_eq!(token_from_json(json), None);
    }

    #[test]
    fn rejects_shapes_that_are_not_credentials() {
        assert_eq!(token_from_json(b"not json"), None);
        assert_eq!(token_from_json(b"{}"), None);
        assert_eq!(token_from_json(br#"{"claudeAiOauth":{}}"#), None);
        assert_eq!(
            token_from_json(br#"{"claudeAiOauth":{"accessToken":""}}"#),
            None
        );
        assert_eq!(
            token_from_json(br#"{"claudeAiOauth":{"accessToken":7}}"#),
            None
        );
    }

    #[test]
    fn a_response_body_parses_as_a_utilization_object() {
        let now = crate::clock::now_ms();
        let body: serde_json::Value = serde_json::from_str(
            r#"{"five_hour":{"utilization":42.4,"resets_at":"2099-01-01T00:00:00Z"},
                "seven_day":{"utilization":10.0,"resets_at":"2099-01-01T00:00:00Z"}}"#,
        )
        .unwrap();
        let usage = crate::usage::parse_utilization(&body, now).unwrap();
        assert_eq!(usage.percent, 42);
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde::Serialize;

use crate::watcher::activity::ActivityProbe;
use crate::watcher::alerts::{diff_alerts, Alert};
use crate::watcher::blocked::BlockedProbe;
use crate::watcher::liveness::PidLiveness;
use crate::watcher::question::QuestionProbe;
use crate::watcher::registry::read_registry_dir;
use crate::watcher::session::RawSession;
use crate::watcher::state::{snapshot, SessionSnapshot, SessionState};
use crate::watcher::task::{TaskProbe, TaskStatus};
use crate::watcher::title::TitleProbe;
use crate::watcher::working::WorkProbe;

/// Reconcile interval. Catches process death and paused-threshold crossings,
/// neither of which changes a file and so neither of which FSEvents reports.
pub const TICK: Duration = Duration::from_secs(2);

/// Tauri event carrying every update to the frontend.
pub const UPDATE_EVENT: &str = "sessions://update";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub sessions: Vec<SessionSnapshot>,
    pub alerts: Vec<Alert>,
    /// Five-hour limit usage, or `None` when there is nothing trustworthy to
    /// show. Absent far more often than present: see `crate::usage`.
    pub usage: Option<crate::usage::Usage>,
}

/// How often the fetched usage figure is picked up.
///
/// Deliberately slower than `TICK`, and unrelated to how often the figure is
/// fetched: this only reads what the refresher thread has published, the figure
/// behind it changes every five minutes at most, and the countdown the widget
/// draws from it runs off an absolute timestamp rather than needing to be
/// re-sent.
pub const USAGE_POLL: Duration = Duration::from_secs(15);

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Identity of a snapshot for change detection: everything the UI renders
/// except the clock-derived fields. Without this, elapsed time alone would make
/// every tick look like a change and the UI would re-render twice a second.
///
/// Tasks are in it by id and status, never by age. A task starting or finishing
/// is a change to draw — on a session whose own state does not move, it is the
/// *only* change — and a task getting a second older is not.
type Fingerprint = (
    String,
    SessionState,
    Option<String>,
    Option<String>,
    Vec<(String, TaskStatus)>,
);

fn fingerprint(sessions: &[SessionSnapshot]) -> Vec<Fingerprint> {
    sessions
        .iter()
        .map(|s| {
            (
                s.session_id.clone(),
                s.state,
                s.detail.clone(),
                // Retitling changes nothing else about a session, so without
                // this the row would keep the name it was first given until
                // something else moved.
                s.title.clone(),
                s.tasks
                    .iter()
                    .map(|t| (t.id.clone(), t.status))
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

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

pub struct WatcherHandle {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl WatcherHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Watch the registry directory and call `on_update` whenever session state
/// changes. Emits once immediately so the UI has data before the first tick.
///
/// This function only ever reads `dir`.
pub fn spawn_watcher(
    dir: PathBuf,
    liveness: Arc<dyn PidLiveness + Send + Sync>,
    activity: Arc<dyn ActivityProbe + Send + Sync>,
    blocked: Arc<dyn BlockedProbe + Send + Sync>,
    work: Arc<dyn WorkProbe + Send + Sync>,
    tasks: Arc<dyn TaskProbe + Send + Sync>,
    titles: Arc<dyn TitleProbe + Send + Sync>,
    question: Arc<dyn QuestionProbe + Send + Sync>,
    on_update: impl Fn(Update) + Send + 'static,
) -> WatcherHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    let join = std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<()>();

        // A missing directory is normal (Claude Code never run). Watching fails,
        // the tick still runs, and the UI shows an empty state.
        let mut fs_watcher = notify::recommended_watcher(move |_res| {
            let _ = tx.send(());
        })
        .ok();
        if let Some(w) = fs_watcher.as_mut() {
            let _ = w.watch(&dir, RecursiveMode::NonRecursive);
        }

        // Session id to the timestamp of the first tick on which it read as
        // dead. Rebuilt each tick from what is still dead, so it cannot grow.
        let mut first_seen_dead: HashMap<String, i64> = HashMap::new();

        let mut previous: Option<Vec<SessionSnapshot>> = None;
        let mut previous_usage: Option<crate::usage::Usage> = None;
        let mut usage: Option<crate::usage::Usage> = None;
        // Zero, not `now`, so the first tick reads rather than waiting a poll.
        let mut usage_read_at: i64 = 0;

        while !stop_thread.load(Ordering::Relaxed) {
            // Read settings per tick, from the cache, so changing the paused
            // threshold or the background-jobs toggle takes effect at once.
            let settings = crate::config::cached();
            let now = now_ms();
            let result = snapshot(
                &read_registry_dir(&dir)
                    .into_iter()
                    .map(RawSession::from)
                    .collect::<Vec<_>>(),
                liveness.as_ref(),
                activity.as_ref(),
                blocked.as_ref(),
                work.as_ref(),
                tasks.as_ref(),
                titles.as_ref(),
                now,
                crate::watcher::state::PAUSED_THRESHOLD_MS,
                settings.show_background_jobs,
                &first_seen_dead,
            );

            first_seen_dead = result
                .dead_now
                .iter()
                .map(|id| {
                    let since = first_seen_dead.get(id).copied().unwrap_or(now);
                    (id.clone(), since)
                })
                .collect();

            let sessions = result.sessions;

            if now - usage_read_at >= USAGE_POLL.as_millis() as i64 {
                usage_read_at = now;
                usage = crate::usage_api::latest(now);
            }
            // Lapsing is checked every tick even between reads: the window can
            // run out mid-interval, and holding a spent figure on screen for up
            // to another poll is the one thing this must not do.
            if usage.is_some_and(|u| u.resets_at_ms <= now) {
                usage = None;
            }

            let changed = previous
                .as_ref()
                .map(|prev| fingerprint(prev) != fingerprint(&sessions))
                .unwrap_or(true)
                || previous_usage != usage;

            if changed {
                let mut alerts = diff_alerts(previous.as_deref(), &sessions);
                crate::watcher::question::enrich_alerts(&mut alerts, &sessions, question.as_ref());
                on_update(Update {
                    sessions: sessions.clone(),
                    alerts,
                    usage,
                });
                previous = Some(sessions);
                previous_usage = usage;
            }

            // Wake on either an FSEvents notification or the reconcile tick,
            // whichever comes first.
            let _ = rx.recv_timeout(TICK);
        }
    });

    WatcherHandle {
        stop,
        join: Some(join),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::sync::Arc;

    use crate::watcher::activity::NoActivity;
    use crate::watcher::blocked::NoBlocked;
    use crate::watcher::liveness::FakeLiveness;
    use crate::watcher::question::{FakeQuestion, NoQuestion};
    use crate::watcher::state::{SessionState, PAUSED_THRESHOLD_MS};
    use crate::watcher::task::{Task, TaskKind, TaskStatus};
    use crate::watcher::tasks::NoTasks;
    use crate::watcher::title::NoTitle;
    use crate::watcher::working::NoWork;

    /// Long enough to cover one reconcile tick plus FSEvents latency.
    const WAIT: Duration = Duration::from_secs(6);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("cb-watch-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write_session(&self, pid: i32, status: Option<&str>) {
            let status_json = match status {
                Some(s) => format!(r#""status": "{s}", "waitingFor": "input needed","#),
                None => String::new(),
            };
            let body = format!(
                r#"{{
                  "pid": {pid},
                  "sessionId": "session-{pid}",
                  "cwd": "/Users/n/Code/proj",
                  "startedAt": {},
                  "entrypoint": "cli",
                  "kind": "interactive",
                  "name": "proj-{pid}",
                  {status_json}
                  "statusUpdatedAt": {}
                }}"#,
                now_ms(),
                now_ms()
            );
            std::fs::write(self.0.join(format!("{pid}.json")), body).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn recv_matching(rx: &mpsc::Receiver<Update>, pred: impl Fn(&Update) -> bool) -> Update {
        let deadline = std::time::Instant::now() + WAIT;
        while std::time::Instant::now() < deadline {
            if let Ok(update) = rx.recv_timeout(Duration::from_millis(500)) {
                if pred(&update) {
                    return update;
                }
            }
        }
        panic!("no matching update within {WAIT:?}");
    }

    /// One tasking session holding one running task.
    fn tasking_snapshot() -> Vec<SessionSnapshot> {
        vec![SessionSnapshot {
            pid: 1,
            session_id: "s".into(),
            name: "proj".into(),
            title: None,
            cwd: "/Users/n/Code/proj".into(),
            entrypoint: "cli".into(),
            state: SessionState::Tasking,
            detail: Some("run tests".into()),
            elapsed_ms: 0,
            uptime_ms: 0,
            status_time_ms: 0,
            started_at_ms: 0,
            background: false,
            tasks: vec![Task {
                id: "t1".into(),
                kind: TaskKind::Shell,
                label: Some("run tests".into()),
                started_at_ms: 0,
                ended_at_ms: None,
                status: TaskStatus::Running,
                output: None,
            }],
        }]
    }

    #[test]
    fn a_task_finishing_re_emits_even_though_nothing_else_moved() {
        // Two snapshots differing only in a task's status are two different
        // things to draw. `fingerprint` ignores the clock on purpose, so
        // without hashing tasks this would be filtered out as unchanged.
        let before = tasking_snapshot();
        let mut after = before.clone();
        after[0].tasks[0].status = TaskStatus::Completed;
        after[0].tasks[0].ended_at_ms = Some(1_000);

        assert_ne!(fingerprint(&before), fingerprint(&after));
    }

    #[test]
    fn a_new_task_appearing_re_emits() {
        let before = tasking_snapshot();
        let mut after = before.clone();
        let mut second = after[0].tasks[0].clone();
        second.id = "t2".into();
        after[0].tasks.push(second);

        assert_ne!(fingerprint(&before), fingerprint(&after));
    }

    #[test]
    fn the_clock_moving_under_an_unchanged_task_does_not_re_emit() {
        // The whole point of the filter: a task getting a second older is not
        // a change, and hashing its age would re-emit every tick.
        let before = tasking_snapshot();
        let mut after = before.clone();
        after[0].elapsed_ms = 90_000;
        after[0].uptime_ms = 90_000;
        after[0].status_time_ms = 90_000;

        assert_eq!(fingerprint(&before), fingerprint(&after));
    }

    #[test]
    fn the_snapshot_store_starts_empty_and_holds_what_it_is_given() {
        let store = SnapshotStore::default();
        assert!(store.get().is_empty());

        let sessions = snapshot(
            &[],
            &FakeLiveness::new(),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            now_ms(),
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;
        store.set(sessions.clone());
        assert_eq!(store.get(), sessions);
    }

    #[test]
    fn emits_an_initial_snapshot_for_existing_sessions() {
        let dir = TempDir::new("initial");
        dir.write_session(4242, None);

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(FakeLiveness::new().with_alive_any_start(4242));
        let handle = spawn_watcher(
            dir.0.clone(),
            liveness,
            Arc::new(NoActivity),
            Arc::new(NoBlocked),
            Arc::new(NoWork),
            Arc::new(NoTasks),
            Arc::new(NoTitle),
            Arc::new(NoQuestion),
            move |u| {
                let _ = tx.send(u);
            },
        );

        let update = recv_matching(&rx, |u| u.sessions.len() == 1);
        assert_eq!(update.sessions[0].pid, 4242);
        // First snapshot is the baseline, so it alerts about nothing.
        assert!(update.alerts.is_empty());

        handle.stop();
    }

    #[test]
    fn a_new_registry_file_produces_a_larger_snapshot() {
        let dir = TempDir::new("appear");
        dir.write_session(4242, None);

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(
            FakeLiveness::new()
                .with_alive_any_start(4242)
                .with_alive_any_start(4343),
        );
        let handle = spawn_watcher(
            dir.0.clone(),
            liveness,
            Arc::new(NoActivity),
            Arc::new(NoBlocked),
            Arc::new(NoWork),
            Arc::new(NoTasks),
            Arc::new(NoTitle),
            Arc::new(NoQuestion),
            move |u| {
                let _ = tx.send(u);
            },
        );

        recv_matching(&rx, |u| u.sessions.len() == 1);
        dir.write_session(4343, None);

        let update = recv_matching(&rx, |u| u.sessions.len() == 2);
        assert!(update.sessions.iter().any(|s| s.pid == 4343));

        handle.stop();
    }

    #[test]
    fn a_session_turning_waiting_produces_an_alert() {
        let dir = TempDir::new("waiting");
        dir.write_session(4242, Some("busy"));

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(FakeLiveness::new().with_alive_any_start(4242));
        let handle = spawn_watcher(
            dir.0.clone(),
            liveness,
            Arc::new(NoActivity),
            Arc::new(NoBlocked),
            Arc::new(NoWork),
            Arc::new(NoTasks),
            Arc::new(NoTitle),
            Arc::new(NoQuestion),
            move |u| {
                let _ = tx.send(u);
            },
        );

        recv_matching(&rx, |u| {
            u.sessions.iter().any(|s| s.state == SessionState::Busy)
        });
        dir.write_session(4242, Some("waiting"));

        let update = recv_matching(&rx, |u| !u.alerts.is_empty());
        assert_eq!(update.alerts[0].session_id, "session-4242");

        handle.stop();
    }

    #[test]
    fn a_needs_input_alert_carries_the_transcript_question() {
        let dir = TempDir::new("question");
        // Start busy so the first snapshot is a baseline, then flip to waiting.
        dir.write_session(4242, Some("busy"));

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(FakeLiveness::new().with_alive_any_start(4242));
        let handle = spawn_watcher(
            dir.0.clone(),
            liveness,
            Arc::new(NoActivity),
            Arc::new(NoBlocked),
            Arc::new(NoWork),
            Arc::new(NoTasks),
            Arc::new(NoTitle),
            Arc::new(FakeQuestion::new().with("session-4242", "Shall I delete the branch?")),
            move |u| {
                let _ = tx.send(u);
            },
        );

        recv_matching(&rx, |u| {
            u.sessions.iter().any(|s| s.state == SessionState::Busy)
        });
        dir.write_session(4242, Some("waiting"));

        let update = recv_matching(&rx, |u| !u.alerts.is_empty());
        handle.stop();

        assert_eq!(update.alerts.len(), 1);
        assert_eq!(
            update.alerts[0].detail.as_deref(),
            Some("Shall I delete the branch?")
        );
    }

    #[test]
    fn a_removed_registry_file_drops_the_session_without_alerting() {
        let dir = TempDir::new("removed");
        dir.write_session(4242, Some("busy"));

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(FakeLiveness::new().with_alive_any_start(4242));
        let handle = spawn_watcher(
            dir.0.clone(),
            liveness,
            Arc::new(NoActivity),
            Arc::new(NoBlocked),
            Arc::new(NoWork),
            Arc::new(NoTasks),
            Arc::new(NoTitle),
            Arc::new(NoQuestion),
            move |u| {
                let _ = tx.send(u);
            },
        );

        recv_matching(&rx, |u| u.sessions.len() == 1);
        std::fs::remove_file(dir.0.join("4242.json")).unwrap();

        let update = recv_matching(&rx, |u| u.sessions.is_empty());
        assert!(update.alerts.is_empty());

        handle.stop();
    }

    #[test]
    fn a_missing_directory_is_not_an_error() {
        let missing = std::env::temp_dir().join("cb-watch-does-not-exist");
        let _ = std::fs::remove_dir_all(&missing);

        let (tx, rx) = mpsc::channel();
        let handle = spawn_watcher(
            missing,
            Arc::new(FakeLiveness::new()),
            Arc::new(NoActivity),
            Arc::new(NoBlocked),
            Arc::new(NoWork),
            Arc::new(NoTasks),
            Arc::new(NoTitle),
            Arc::new(NoQuestion),
            move |u| {
                let _ = tx.send(u);
            },
        );

        let update = recv_matching(&rx, |u| u.sessions.is_empty());
        assert!(update.alerts.is_empty());

        handle.stop();
    }

    #[test]
    fn identical_state_does_not_re_emit() {
        let dir = TempDir::new("dedupe");
        dir.write_session(4242, Some("busy"));

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(FakeLiveness::new().with_alive_any_start(4242));
        let handle = spawn_watcher(
            dir.0.clone(),
            liveness,
            Arc::new(NoActivity),
            Arc::new(NoBlocked),
            Arc::new(NoWork),
            Arc::new(NoTasks),
            Arc::new(NoTitle),
            Arc::new(NoQuestion),
            move |u| {
                let _ = tx.send(u);
            },
        );

        recv_matching(&rx, |u| u.sessions.len() == 1);
        // Two reconcile ticks pass with nothing changing.
        std::thread::sleep(TICK * 2 + Duration::from_millis(500));

        // Elapsed time advances every tick, so re-emission would be constant
        // churn if the loop compared whole snapshots instead of states.
        assert!(rx.try_recv().is_err(), "unchanged state must not re-emit");

        handle.stop();
    }
}

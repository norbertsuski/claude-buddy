//! Ask the task probe what it makes of a real session. `cargo run --example task_probe -- <cwd> <session-id> <started-at-ms>`
//!
//! An example rather than a test: it reads the machine's own transcripts, which
//! no test may depend on. Exists to check a scanner change against real
//! records before shipping it, since a fixture only proves what it encodes.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (cwd, session_id, started) = (&args[1], &args[2], args[3].parse::<i64>().unwrap());
    let projects = std::path::PathBuf::from(std::env::var("CLAUDE_BUDDY_PROJECTS_DIR").unwrap());
    let probe = claude_buddy_lib::watcher::tasks::TranscriptTasks::new(projects);
    let tasks = buddy_core::watcher::task::TaskProbe::tasks(&probe, cwd, session_id, started);
    let running: Vec<_> = tasks
        .iter()
        .filter(|t| t.status == buddy_core::watcher::task::TaskStatus::Running)
        .collect();
    println!(
        "total tasks: {}   still running: {}",
        tasks.len(),
        running.len()
    );
    for t in running.iter().take(8) {
        println!(
            "  RUNNING {:?} {} started_at={}",
            t.kind,
            t.label.as_deref().unwrap_or("-"),
            t.started_at_ms
        );
    }
}

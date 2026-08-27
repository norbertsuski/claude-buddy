use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

/// How much of a transcript to read. Transcripts reach megabytes; the fields
/// clawde-buddy wants are always in the last few records.
pub const TAIL_BYTES: u64 = 65_536;

/// Longest activity string the popover will show on one line.
pub const ACTIVITY_MAX_CHARS: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptDetail {
    pub branch: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub activity: Option<String>,
}

impl TranscriptDetail {
    fn complete(&self) -> bool {
        self.branch.is_some() && self.model.is_some() && self.effort.is_some()
    }
}

/// Extract the newest available value for each field.
///
/// Records are scanned newest-first because different record types carry
/// different fields: an assistant record has model and effort, a user record has
/// only branch, an attachment record has none. Unparseable lines — including the
/// truncated first line a fixed-size tail almost always produces — are skipped.
pub fn detail_from_tail(bytes: &[u8]) -> TranscriptDetail {
    let text = String::from_utf8_lossy(bytes);
    let mut detail = TranscriptDetail::default();

    for line in text.lines().rev() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        if detail.branch.is_none() {
            if let Some(branch) = record.get("gitBranch").and_then(|v| v.as_str()) {
                detail.branch = Some(branch.to_string());
            }
        }
        if detail.model.is_none() {
            if let Some(model) = record
                .get("message")
                .and_then(|m| m.get("model"))
                .and_then(|v| v.as_str())
            {
                detail.model = Some(model.to_string());
            }
        }
        if detail.effort.is_none() {
            if let Some(effort) = record.get("effort").and_then(|v| v.as_str()) {
                detail.effort = Some(effort.to_string());
            }
        }

        if detail.complete() {
            break;
        }
    }

    detail.activity = latest_activity(bytes);
    detail
}

/// Shorten to fit, on a character boundary, with an ellipsis.
fn clip(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= ACTIVITY_MAX_CHARS {
        return text;
    }
    let head: String = text.chars().take(ACTIVITY_MAX_CHARS).collect();
    format!("{head}\u{2026}")
}

/// The content blocks of an assistant record, if this is one.
fn assistant_content(record: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    record
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
}

/// What the session is doing, newest first: the most recent tool use by name,
/// or failing that the most recent thing the assistant said.
///
/// Records are scanned in reverse for the same reason `detail_from_tail` scans
/// in reverse — the tail begins mid-record, and the newest information is at
/// the end.
pub fn latest_activity(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut fallback: Option<String> = None;

    for line in text.lines().rev() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(content) = assistant_content(&record) else {
            continue;
        };

        for block in content.iter().rev() {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("tool_use") => {
                    if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                        return Some(clip(name));
                    }
                }
                Some("text") => {
                    if fallback.is_none() {
                        if let Some(said) = block.get("text").and_then(|t| t.as_str()) {
                            if !said.trim().is_empty() {
                                fallback = Some(clip(said));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fallback
}

/// Each user-blocking tool paired with how it reads in the widget. One table,
/// so a name can never be listed as blocking without a label or the reverse.
const BLOCKING_TOOL_LABELS: [(&str, &str); 2] = [
    ("AskUserQuestion", "question pending"),
    ("ExitPlanMode", "plan approval"),
];

/// Tools that block on a human by definition. A pending call to one of these
/// is proof the session wants the user, not that a tool is slow.
pub const BLOCKING_TOOLS: [&str; 2] = [BLOCKING_TOOL_LABELS[0].0, BLOCKING_TOOL_LABELS[1].0];

/// How a blocking tool reads in the widget, or `None` if it does not block.
fn blocking_tool_label(name: &str) -> Option<&'static str> {
    BLOCKING_TOOL_LABELS
        .iter()
        .find(|(tool, _)| *tool == name)
        .map(|(_, label)| *label)
}

/// The content blocks of any record, assistant or user.
fn message_content(record: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    record
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
}

/// Every `tool_use_id` that has a result somewhere in this tail.
fn answered_tool_uses(records: &[serde_json::Value]) -> std::collections::HashSet<&str> {
    records
        .iter()
        .filter_map(message_content)
        .flatten()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
        .filter_map(|b| b.get("tool_use_id").and_then(|v| v.as_str()))
        .collect()
}

/// The label for a pending user-blocking tool call, if the session has one.
///
/// Claude Desktop never writes `status` to the registry, so a session blocked
/// asking the user a question is indistinguishable, by mtime alone, from one
/// sitting idle. The transcript says otherwise: an `AskUserQuestion` or
/// `ExitPlanMode` call with no `tool_result` for its id can only be waiting on
/// a human. A pending `Bash` proves nothing — it may still be running — which
/// is why only the tools in `BLOCKING_TOOLS` count.
///
/// Unparseable lines are skipped, as in `detail_from_tail`: a fixed-size tail
/// almost always truncates its first record.
pub fn pending_user_prompt(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let records: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    let answered = answered_tool_uses(&records);

    for record in records.iter().rev() {
        let Some(content) = assistant_content(record) else {
            continue;
        };
        for block in content.iter().rev() {
            if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                continue;
            }
            let Some(name) = block.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let Some(label) = blocking_tool_label(name) else {
                continue;
            };
            let id = block.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            if !answered.contains(id) {
                return Some(label.to_string());
            }
        }
    }

    None
}

/// Whether a tool call is still running.
///
/// The busy heuristic for statusless sessions is transcript mtime, and a
/// transcript is only appended when a message or a tool result lands. A single
/// long tool call — a build, a test run, a subagent — writes nothing for
/// minutes, so mtime alone ages an actively working session into `idle`.
/// Measured against a live claude-desktop session, 58% of a twelve-minute
/// working stretch sat inside gaps longer than the busy window.
///
/// An assistant `tool_use` with no `tool_result` for its id is direct evidence
/// the session is mid-turn. Deliberately the mirror image of
/// `pending_user_prompt`: the tools in `BLOCKING_TOOLS` are excluded, because
/// those wait on a human rather than on a machine, and reporting them as busy
/// would bury the one state that needs the user.
///
/// A `tool_use` carrying no id is ignored rather than assumed pending: without
/// an id no result can ever be matched to it, so it would read as in flight
/// forever.
pub fn has_work_in_flight(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    let records: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    let answered = answered_tool_uses(&records);

    records
        .iter()
        .rev()
        .filter_map(assistant_content)
        .flatten()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
        .filter(|b| {
            let name = b.get("name").and_then(|n| n.as_str()).unwrap_or_default();
            !BLOCKING_TOOLS.contains(&name)
        })
        .filter_map(|b| b.get("id").and_then(|v| v.as_str()))
        .any(|id| !answered.contains(id))
}

/// The most recent thing the assistant actually said, ignoring tool uses.
///
/// This is what a waiting session is asking. `latest_activity` prefers the tool
/// name because "Bash" describes what is happening; a pending question is the
/// opposite case, where the prose is the whole point.
pub fn latest_assistant_text(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);

    for line in text.lines().rev() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(content) = assistant_content(&record) else {
            continue;
        };
        for block in content.iter().rev() {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(said) = block.get("text").and_then(|t| t.as_str()) {
                    if !said.trim().is_empty() {
                        return Some(clip(said));
                    }
                }
            }
        }
    }

    None
}

/// Claude Code names project directories after the cwd with separators flattened
/// to dashes: `/Users/n/Code/proj` becomes `-Users-n-Code-proj`.
pub fn project_slug(cwd: &str) -> String {
    cwd.replace(['/', '.'], "-")
}

/// Environment variable pointing at a different transcripts directory, matching
/// `CLAWDE_BUDDY_REGISTRY_DIR`. Lets the widget be driven entirely from
/// fixtures — for tests, and for the screenshots in the README.
pub const PROJECTS_DIR_ENV: &str = "CLAWDE_BUDDY_PROJECTS_DIR";

pub fn projects_dir() -> PathBuf {
    if let Some(override_dir) = std::env::var_os(PROJECTS_DIR_ENV) {
        return PathBuf::from(override_dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".claude")
        .join("projects")
}

/// Locate a session's transcript.
///
/// The slug guess is tried first because it is a single stat. It misses when the
/// session was started in a subdirectory of the project, so a scan of the
/// project directories is the fallback — cheap, since there is one directory per
/// project the user has ever opened.
pub fn find_transcript(projects_dir: &Path, cwd: &str, session_id: &str) -> Option<PathBuf> {
    let filename = format!("{session_id}.jsonl");

    let guess = projects_dir.join(project_slug(cwd)).join(&filename);
    if guess.is_file() {
        return Some(guess);
    }

    std::fs::read_dir(projects_dir).ok()?.find_map(|entry| {
        let candidate = entry.ok()?.path().join(&filename);
        candidate.is_file().then_some(candidate)
    })
}

/// Read at most `max_bytes` from the end of a file.
pub fn read_tail(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))?;

    let mut buf = Vec::with_capacity(max_bytes.min(len) as usize);
    file.take(max_bytes).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Transcript-only fields for one session.
///
/// Returns an all-`None` detail rather than an error when the transcript is
/// missing or unreadable: the popover must still open and show its
/// registry-sourced fields.
///
/// `async` deliberately: non-async commands run on the main thread, and this
/// opens a file and may scan every project directory. It fires on every hover.
#[tauri::command]
pub async fn session_detail(cwd: String, session_id: String) -> TranscriptDetail {
    find_transcript(&projects_dir(), &cwd, &session_id)
        .and_then(|path| read_tail(&path, TAIL_BYTES).ok())
        .map(|bytes| detail_from_tail(&bytes))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape taken from a real transcript: an assistant record carries model and
    /// effort, a user record carries neither, and an attachment record carries
    /// almost nothing. The newest record is last.
    const TAIL: &str = concat!(
        r#"{"type":"assistant","message":{"model":"claude-opus-5"},"effort":"xhigh","gitBranch":"feat/rate-limiting","cwd":"/Users/n/Code/api-service"}"#,
        "\n",
        r#"{"type":"user","message":{"role":"user"},"gitBranch":"feat/rate-limiting","cwd":"/Users/n/Code/api-service"}"#,
        "\n",
        r#"{"type":"attachment","attachment":{"type":"total_tokens_reminder"}}"#,
        "\n"
    );

    const TOOL_TAIL: &str = concat!(
        r#"{"type":"assistant","message":{"model":"claude-opus-5","content":[{"type":"text","text":"Let me check the config."}]}}"#,
        "\n",
        r#"{"type":"assistant","message":{"model":"claude-opus-5","content":[{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#,
        "\n",
    );

    #[test]
    fn latest_activity_reports_the_newest_tool_use() {
        assert_eq!(
            latest_activity(TOOL_TAIL.as_bytes()).as_deref(),
            Some("Bash")
        );
    }

    #[test]
    fn latest_activity_falls_back_to_assistant_text() {
        let tail = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Shall I delete the branch?"}]}}"#;
        assert_eq!(
            latest_activity(tail.as_bytes()).as_deref(),
            Some("Shall I delete the branch?")
        );
    }

    #[test]
    fn latest_activity_truncates_a_long_line() {
        let long = "x".repeat(400);
        let tail = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{long}"}}]}}}}"#
        );
        let out = latest_activity(tail.as_bytes()).unwrap();
        // Counted in chars, not bytes: the ellipsis is three bytes, and the
        // clip is a char-boundary operation.
        let chars = out.chars().count();
        assert!(chars <= ACTIVITY_MAX_CHARS + 1, "got {chars} chars");
        assert!(out.ends_with('\u{2026}'));
    }

    #[test]
    fn latest_activity_skips_a_truncated_leading_line() {
        let tail = format!("{}\n{}", r#"{"type":"assis"#, TOOL_TAIL.trim_end());
        assert_eq!(latest_activity(tail.as_bytes()).as_deref(), Some("Bash"));
    }

    #[test]
    fn latest_activity_reports_nothing_for_a_transcript_with_neither() {
        let tail = r#"{"type":"user","message":{"role":"user"}}"#;
        assert_eq!(latest_activity(tail.as_bytes()), None);
    }

    #[test]
    fn latest_assistant_text_ignores_tool_uses() {
        assert_eq!(
            latest_assistant_text(TOOL_TAIL.as_bytes()).as_deref(),
            Some("Let me check the config.")
        );
    }

    #[test]
    fn detail_from_tail_includes_activity() {
        assert_eq!(
            detail_from_tail(TOOL_TAIL.as_bytes()).activity.as_deref(),
            Some("Bash")
        );
    }

    #[test]
    fn extracts_branch_model_and_effort_from_the_newest_records_that_have_them() {
        let d = detail_from_tail(TAIL.as_bytes());
        assert_eq!(d.branch.as_deref(), Some("feat/rate-limiting"));
        assert_eq!(d.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(d.effort.as_deref(), Some("xhigh"));
    }

    #[test]
    fn prefers_the_newest_value_when_records_disagree() {
        let body = concat!(
            r#"{"gitBranch":"old-branch"}"#,
            "\n",
            r#"{"gitBranch":"new-branch"}"#,
            "\n"
        );
        assert_eq!(
            detail_from_tail(body.as_bytes()).branch.as_deref(),
            Some("new-branch")
        );
    }

    #[test]
    fn a_truncated_first_line_is_skipped_not_fatal() {
        // Reading a fixed tail almost always lands mid-record.
        let body = format!(
            "{}\n{}",
            r#"del":"claude-opus-5"},"gitBranch":"junk"}"#, TAIL
        );
        let d = detail_from_tail(body.as_bytes());
        assert_eq!(d.branch.as_deref(), Some("feat/rate-limiting"));
    }

    #[test]
    fn a_transcript_with_no_assistant_message_yet_yields_no_model() {
        let body = concat!(r#"{"type":"user","gitBranch":"main"}"#, "\n");
        let d = detail_from_tail(body.as_bytes());
        assert_eq!(d.branch.as_deref(), Some("main"));
        assert_eq!(d.model, None);
        assert_eq!(d.effort, None);
    }

    #[test]
    fn empty_input_yields_all_none() {
        assert_eq!(detail_from_tail(b""), TranscriptDetail::default());
    }

    #[test]
    fn entirely_unparseable_input_yields_all_none() {
        assert_eq!(
            detail_from_tail(b"not json at all\nnor this\n"),
            TranscriptDetail::default()
        );
    }

    /// Shape taken from the live claude-desktop transcript: the newest record
    /// is an assistant record whose stop_reason is tool_use and whose only
    /// content block is a call to AskUserQuestion.
    const PENDING_QUESTION_TAIL: &str = concat!(
        r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_old"}]}}"#,
        "\n",
        r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_ask","name":"AskUserQuestion","input":{"questions":[]}}]}}"#,
        "\n",
    );

    #[test]
    fn a_pending_ask_user_question_is_a_pending_prompt() {
        assert_eq!(
            pending_user_prompt(PENDING_QUESTION_TAIL.as_bytes()).as_deref(),
            Some("question pending")
        );
    }

    #[test]
    fn an_answered_ask_user_question_is_not_pending() {
        let tail = format!(
            "{}{}\n",
            PENDING_QUESTION_TAIL,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_ask","content":"answered"}]}}"#
        );
        assert_eq!(pending_user_prompt(tail.as_bytes()), None);
    }

    #[test]
    fn a_pending_bash_call_is_not_a_pending_prompt() {
        // The false-positive guard. A pending Bash means a tool is still
        // running, which is exactly what a timeout heuristic gets wrong.
        let tail = r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_bash","name":"Bash","input":{"command":"cargo build"}}]}}"#;
        assert_eq!(pending_user_prompt(tail.as_bytes()), None);
    }

    #[test]
    fn a_pending_exit_plan_mode_is_a_pending_prompt() {
        let tail = r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_plan","name":"ExitPlanMode","input":{"plan":"do the thing"}}]}}"#;
        assert_eq!(
            pending_user_prompt(tail.as_bytes()).as_deref(),
            Some("plan approval")
        );
    }

    #[test]
    fn pending_user_prompt_skips_a_truncated_leading_line() {
        let tail = format!(
            "{}\n{}",
            r#"e":"tool_use","name":"AskUserQuestion"#, PENDING_QUESTION_TAIL
        );
        assert_eq!(
            pending_user_prompt(tail.as_bytes()).as_deref(),
            Some("question pending")
        );
    }

    #[test]
    fn an_irrelevant_tail_has_no_pending_prompt() {
        assert_eq!(pending_user_prompt(b""), None);
        assert_eq!(pending_user_prompt(TOOL_TAIL.as_bytes()), None);
    }

    #[test]
    fn the_newest_unanswered_blocking_call_wins_over_an_older_answered_one() {
        let tail = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_a","name":"AskUserQuestion"}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_a"}]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_b","name":"ExitPlanMode"}]}}"#,
            "\n",
        );
        assert_eq!(
            pending_user_prompt(tail.as_bytes()).as_deref(),
            Some("plan approval")
        );
    }

    #[test]
    fn an_unanswered_bash_call_is_work_in_flight() {
        // The case mtime cannot see: a long Bash run writes nothing for
        // minutes, and the session reads idle while it is plainly working.
        let tail = r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_bash","name":"Bash","input":{"command":"cargo build"}}]}}"#;
        assert!(has_work_in_flight(tail.as_bytes()));
    }

    #[test]
    fn an_answered_call_is_not_work_in_flight() {
        let tail = format!(
            "{}\n{}",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_bash","name":"Bash"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_bash","content":"done"}]}}"#
        );
        assert!(!has_work_in_flight(tail.as_bytes()));
    }

    #[test]
    fn a_pending_blocking_tool_is_not_work_in_flight() {
        // A question waiting on a human is the opposite of work in flight;
        // reporting it as busy would hide the one state that needs the user.
        assert!(!has_work_in_flight(PENDING_QUESTION_TAIL.as_bytes()));
    }

    #[test]
    fn an_idle_tail_has_no_work_in_flight() {
        assert!(!has_work_in_flight(b""));
        assert!(!has_work_in_flight(TOOL_TAIL.as_bytes()));
    }

    #[test]
    fn work_in_flight_survives_a_truncated_leading_line() {
        let tail = format!(
            "{}\n{}",
            r#"e":"tool_use","name":"Bash"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_b","name":"Bash"}]}}"#
        );
        assert!(has_work_in_flight(tail.as_bytes()));
    }

    #[test]
    fn every_blocking_tool_has_a_label() {
        assert!(BLOCKING_TOOLS.contains(&"AskUserQuestion"));
        assert!(BLOCKING_TOOLS.contains(&"ExitPlanMode"));
        for tool in BLOCKING_TOOLS {
            assert!(blocking_tool_label(tool).is_some(), "{tool} has no label");
        }
        assert_eq!(blocking_tool_label("Bash"), None);
    }

    #[test]
    fn slug_replaces_separators_with_dashes() {
        assert_eq!(
            project_slug("/Users/dev/Documents/Code/clawde-buddy"),
            "-Users-dev-Documents-Code-clawde-buddy"
        );
    }

    #[test]
    fn slug_also_replaces_dots() {
        assert_eq!(
            project_slug("/Users/n/.claude-mem/x"),
            "-Users-n--claude-mem-x"
        );
    }

    #[test]
    fn find_transcript_locates_the_file_via_the_slug() {
        let root = std::env::temp_dir().join(format!("cb-tx-slug-{}", std::process::id()));
        let dir = root.join("-Users-n-Code-proj");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("abc-123.jsonl"), TAIL).unwrap();

        let found = find_transcript(&root, "/Users/n/Code/proj", "abc-123");

        assert_eq!(found, Some(dir.join("abc-123.jsonl")));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn find_transcript_falls_back_to_scanning_when_the_slug_does_not_match() {
        // The session was started in a subdirectory, so the slug guess misses.
        let root = std::env::temp_dir().join(format!("cb-tx-scan-{}", std::process::id()));
        let dir = root.join("-Users-n-Code-somewhere-else");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("abc-123.jsonl"), TAIL).unwrap();

        let found = find_transcript(&root, "/Users/n/Code/proj", "abc-123");

        assert_eq!(found, Some(dir.join("abc-123.jsonl")));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn find_transcript_returns_none_when_absent() {
        let root = std::env::temp_dir().join(format!("cb-tx-none-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(find_transcript(&root, "/Users/n/Code/proj", "nope"), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn read_tail_returns_only_the_last_bytes_of_a_large_file() {
        let path = std::env::temp_dir().join(format!("cb-tail-{}.jsonl", std::process::id()));
        let filler = "x".repeat(200_000);
        std::fs::write(&path, format!("{filler}\n{TAIL}")).unwrap();

        let bytes = read_tail(&path, TAIL_BYTES).unwrap();

        assert!(bytes.len() as u64 <= TAIL_BYTES);
        assert_eq!(
            detail_from_tail(&bytes).branch.as_deref(),
            Some("feat/rate-limiting")
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn read_tail_handles_a_file_smaller_than_the_window() {
        let path = std::env::temp_dir().join(format!("cb-tail-small-{}.jsonl", std::process::id()));
        std::fs::write(&path, TAIL).unwrap();

        let bytes = read_tail(&path, TAIL_BYTES).unwrap();

        assert_eq!(bytes.len(), TAIL.len());
        std::fs::remove_file(&path).unwrap();
    }
}

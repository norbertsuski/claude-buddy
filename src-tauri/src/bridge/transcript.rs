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
        assert_eq!(latest_activity(TOOL_TAIL.as_bytes()).as_deref(), Some("Bash"));
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
        let body = concat!(r#"{"gitBranch":"old-branch"}"#, "\n", r#"{"gitBranch":"new-branch"}"#, "\n");
        assert_eq!(detail_from_tail(body.as_bytes()).branch.as_deref(), Some("new-branch"));
    }

    #[test]
    fn a_truncated_first_line_is_skipped_not_fatal() {
        // Reading a fixed tail almost always lands mid-record.
        let body = format!("{}\n{}", r#"del":"claude-opus-5"},"gitBranch":"junk"}"#, TAIL);
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
        assert_eq!(detail_from_tail(b"not json at all\nnor this\n"), TranscriptDetail::default());
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
        assert_eq!(project_slug("/Users/n/.claude-mem/x"), "-Users-n--claude-mem-x");
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

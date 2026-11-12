//! Reads the session logs agents keep, to tell what an agent is doing in a
//! workspace. tsugi never writes to these files.
//!
//! The formats belong to the agents and can change without notice. A log
//! that cannot be read is skipped; nothing else in tsugi depends on it.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

/// Sessions that were not written to for this long are not reported.
const RECENT: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// How much of the end of a log is read to find the current state.
const TAIL_BYTES: u64 = 1024 * 1024;
const MESSAGE_CHARS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// The agent is in the middle of a turn. This includes waiting for the
    /// user to approve a tool call, which the logs do not distinguish.
    Working,
    /// The agent finished its turn and waits for the user.
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Session {
    /// `claude` or `codex`.
    pub agent: String,
    pub id: String,
    pub state: SessionState,
    /// Unix seconds of the last write to the log.
    pub updated_at: i64,
    pub title: Option<String>,
    /// The last thing the agent said, shortened.
    pub last_message: Option<String>,
}

/// Recent sessions that ran in `workspace`, most recently active first.
/// `home` is the home directory of the user.
pub fn sessions(workspace: &Path, home: &Path) -> Vec<Session> {
    let mut sessions = claude_sessions(workspace, home);
    sessions.extend(codex_sessions(workspace, home));
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions
}

/// Claude Code keeps one directory per working directory, named after the
/// path with every character outside `[A-Za-z0-9]` replaced by `-`, and one
/// JSON Lines file per session in it.
fn claude_sessions(workspace: &Path, home: &Path) -> Vec<Session> {
    let encoded: String = workspace
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let dir = home.join(".claude").join("projects").join(encoded);
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut sessions = Vec::new();
    for path in entries.flatten().map(|entry| entry.path()) {
        if path
            .extension()
            .is_none_or(|extension| extension != "jsonl")
        {
            continue;
        }
        let Some(updated_at) = recent_mtime(&path) else {
            continue;
        };
        let lines = tail_lines(&path);

        let mut state = None;
        let mut last_message = None;
        let mut title = None;
        for entry in lines.iter().rev() {
            match entry["type"].as_str() {
                Some("ai-title") if title.is_none() => {
                    title = entry["aiTitle"].as_str().map(str::to_string);
                }
                Some(kind @ ("user" | "assistant")) => {
                    // Sidechains are subagents; meta entries are written by
                    // local commands and do not start a turn.
                    if entry["isSidechain"] == true || entry["isMeta"] == true {
                        continue;
                    }
                    let message = &entry["message"];
                    if kind == "assistant" && last_message.is_none() {
                        last_message = text_block(&message["content"], "text");
                    }
                    if state.is_none() {
                        state = Some(if kind == "user" {
                            let interrupted = text_block(&message["content"], "text")
                                .or_else(|| message["content"].as_str().map(str::to_string))
                                .is_some_and(|text| text.starts_with("[Request interrupted"));
                            if interrupted {
                                SessionState::Idle
                            } else {
                                SessionState::Working
                            }
                        } else {
                            match message["stop_reason"].as_str() {
                                None | Some("tool_use") => SessionState::Working,
                                Some(_) => SessionState::Idle,
                            }
                        });
                    }
                }
                _ => {}
            }
        }
        let Some(state) = state else {
            continue;
        };
        sessions.push(Session {
            agent: "claude".to_string(),
            id: path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default(),
            state,
            updated_at,
            title,
            last_message,
        });
    }
    sessions
}

/// Codex keeps every session under `~/.codex/sessions/YYYY/MM/DD/`. The
/// first line of a file is a `session_meta` record with the working
/// directory.
fn codex_sessions(workspace: &Path, home: &Path) -> Vec<Session> {
    let mut files = Vec::new();
    collect_jsonl(&home.join(".codex").join("sessions"), 3, &mut files);

    let mut sessions = Vec::new();
    for path in files {
        let Some(updated_at) = recent_mtime(&path) else {
            continue;
        };
        let Some(meta) = first_line(&path) else {
            continue;
        };
        let payload = &meta["payload"];
        let Some(cwd) = payload["cwd"].as_str() else {
            continue;
        };
        let cwd = fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd));
        if cwd != workspace {
            continue;
        }

        let mut state = None;
        let mut last_message = None;
        for entry in tail_lines(&path).iter().rev() {
            let payload = &entry["payload"];
            match (entry["type"].as_str(), payload["type"].as_str()) {
                (Some("event_msg"), Some("task_complete")) => {
                    state.get_or_insert(SessionState::Idle);
                    if last_message.is_none() {
                        last_message = payload["last_agent_message"].as_str().map(shorten);
                    }
                }
                (Some("event_msg"), Some("task_started")) => {
                    state.get_or_insert(SessionState::Working);
                }
                (Some("response_item"), Some("message"))
                    if payload["role"] == "assistant" && last_message.is_none() =>
                {
                    last_message = text_block(&payload["content"], "output_text");
                }
                _ => {}
            }
            if state.is_some() && last_message.is_some() {
                break;
            }
        }
        let Some(state) = state else {
            continue;
        };
        sessions.push(Session {
            agent: "codex".to_string(),
            id: payload["id"].as_str().unwrap_or_default().to_string(),
            state,
            updated_at,
            title: None,
            last_message,
        });
    }
    sessions
}

/// Modification time in Unix seconds, if the file was written recently.
fn recent_mtime(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    if SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default()
        > RECENT
    {
        return None;
    }
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

fn collect_jsonl(dir: &Path, depth: u32, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            if depth > 0 {
                collect_jsonl(&path, depth - 1, files);
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            files.push(path);
        }
    }
}

fn first_line(path: &Path) -> Option<Value> {
    let mut line = String::new();
    BufReader::new(File::open(path).ok()?)
        .read_line(&mut line)
        .ok()?;
    serde_json::from_str(&line).ok()
}

/// The records in the last part of a JSON Lines file, oldest first.
fn tail_lines(path: &Path) -> Vec<Value> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let length = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let start = length.saturating_sub(TAIL_BYTES);
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    text.lines()
        // The first line is cut off unless the whole file was read.
        .skip(if start > 0 { 1 } else { 0 })
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Text of the last content block of the given type.
fn text_block(content: &Value, kind: &str) -> Option<String> {
    content
        .as_array()?
        .iter()
        .rev()
        .find(|block| block["type"] == kind)
        .and_then(|block| block["text"].as_str())
        .map(shorten)
}

fn shorten(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MESSAGE_CHARS {
        return text.to_string();
    }
    let mut short: String = text.chars().take(MESSAGE_CHARS).collect();
    short.push('…');
    short
}

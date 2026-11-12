use std::fs;
use std::path::Path;

use tsugi_core::SessionState;
use tsugi_core::session::sessions;

fn write_lines(path: &Path, lines: &[&str]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, lines.join("\n") + "\n").unwrap();
}

#[test]
fn claude_session_state_follows_the_last_turn() {
    let home = tempfile::tempdir().unwrap();
    let workspace = Path::new("/work/my repo.tsugi/login");
    let dir = home
        .path()
        .join(".claude/projects/-work-my-repo-tsugi-login");

    write_lines(
        &dir.join("idle.jsonl"),
        &[
            r#"{"type":"user","message":{"role":"user","content":"fix the login"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","name":"Edit"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"The login is fixed."}]}}"#,
            r#"{"type":"ai-title","aiTitle":"Fix login"}"#,
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"local command"}}"#,
        ],
    );
    write_lines(
        &dir.join("working.jsonl"),
        &[
            r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"Done with part one."}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":"now part two"}}"#,
            r#"{"type":"assistant","isSidechain":true,"message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"subagent"}]}}"#,
            "not json",
        ],
    );
    write_lines(&dir.join("empty.jsonl"), &[r#"{"type":"mode"}"#]);

    let mut found = sessions(workspace, home.path());
    found.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(found.len(), 2);

    assert_eq!(found[0].agent, "claude");
    assert_eq!(found[0].id, "idle");
    assert_eq!(found[0].state, SessionState::Idle);
    assert_eq!(found[0].title.as_deref(), Some("Fix login"));
    assert_eq!(
        found[0].last_message.as_deref(),
        Some("The login is fixed.")
    );

    assert_eq!(found[1].id, "working");
    assert_eq!(found[1].state, SessionState::Working);
    assert_eq!(
        found[1].last_message.as_deref(),
        Some("Done with part one.")
    );
}

#[test]
fn interrupted_claude_session_is_idle() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".claude/projects/-w");
    write_lines(
        &dir.join("s.jsonl"),
        &[
            r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#,
        ],
    );
    let found = sessions(Path::new("/w"), home.path());
    assert_eq!(found[0].state, SessionState::Idle);
}

#[test]
fn codex_sessions_are_matched_by_working_directory() {
    let home = tempfile::tempdir().unwrap();
    let day = home.path().join(".codex/sessions/2026/11/12");
    write_lines(
        &day.join("rollout-a.jsonl"),
        &[
            r#"{"type":"session_meta","payload":{"id":"aaa","cwd":"/work/repo"}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_started"}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"All tests pass."}}"#,
        ],
    );
    write_lines(
        &day.join("rollout-b.jsonl"),
        &[
            r#"{"type":"session_meta","payload":{"id":"bbb","cwd":"/work/repo"}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"First turn."}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_started"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Looking at the tests."}]}}"#,
        ],
    );
    write_lines(
        &day.join("rollout-c.jsonl"),
        &[
            r#"{"type":"session_meta","payload":{"id":"ccc","cwd":"/work/elsewhere"}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_started"}}"#,
        ],
    );

    let mut found = sessions(Path::new("/work/repo"), home.path());
    found.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].agent, "codex");
    assert_eq!(found[0].state, SessionState::Idle);
    assert_eq!(found[0].last_message.as_deref(), Some("All tests pass."));
    assert_eq!(found[1].state, SessionState::Working);
    assert_eq!(
        found[1].last_message.as_deref(),
        Some("Looking at the tests.")
    );
}

#[test]
fn no_logs_means_no_sessions() {
    let home = tempfile::tempdir().unwrap();
    assert!(sessions(Path::new("/work/repo"), home.path()).is_empty());
}

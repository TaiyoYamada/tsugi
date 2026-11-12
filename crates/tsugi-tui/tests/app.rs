use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tsugi_core::{
    Conflict, Diff, DiffStat, Origin, Session, SessionState, Snapshot, Task, Trigger, Workspace,
    WorkspaceStatus,
};
use tsugi_tui::{App, Focus, Mode, Request, Tab, draw};

fn status(name: &str, task: Option<&str>) -> WorkspaceStatus {
    WorkspaceStatus {
        workspace: Workspace {
            id: format!("id-{name}"),
            name: name.to_string(),
            task_id: task.map(|task| format!("task-{task}")),
            path: PathBuf::from(format!("/work/repo.tsugi/{name}")),
            base: "main".to_string(),
            origin: Origin::Created,
            created_at: 0,
            removed_at: None,
        },
        task: task.map(|task| Task {
            id: format!("task-{task}"),
            title: task.to_string(),
            created_at: 0,
            closed_at: None,
        }),
        branch: Some(format!("refs/heads/tsugi/{name}")),
        head: Some("0123456789abcdef".to_string()),
        is_main: false,
        latest_snapshot: None,
        changes: Some(DiffStat {
            files: 2,
            insertions: 10,
            deletions: 3,
        }),
        sessions: Vec::new(),
    }
}

fn snapshot(seq: u64) -> Snapshot {
    Snapshot {
        id: format!("{seq:040}"),
        workspace_id: "id-login".to_string(),
        seq,
        tree: "tree".to_string(),
        head: Some("0123456789abcdef".to_string()),
        branch: None,
        trigger: Trigger::Watch,
        created_at: 0,
    }
}

fn app() -> App {
    let mut app = App::new();
    app.set_statuses(
        vec![
            status("repo", None),
            status("login", Some("Fix login")),
            status("search", Some("Add search")),
            status("login-2", Some("Fix login")),
        ],
        vec![Conflict {
            workspace: "login".to_string(),
            other: "login-2".to_string(),
            other_is_base: false,
            files: vec!["auth.rs".to_string()],
        }],
    );
    app.snapshots = vec![snapshot(3), snapshot(2), snapshot(1)];
    app
}

fn press(app: &mut App, code: KeyCode) -> Option<Request> {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        assert_eq!(press(app, KeyCode::Char(c)), None);
    }
}

fn screen(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(110, 30)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn names(app: &App) -> Vec<&str> {
    app.statuses
        .iter()
        .map(|status| status.workspace.name.as_str())
        .collect()
}

#[test]
fn workspaces_of_a_task_are_listed_together() {
    let app = app();
    assert_eq!(names(&app), ["login", "login-2", "search", "repo"]);
}

#[test]
fn the_selection_follows_the_workspace_across_reloads() {
    let mut app = app();
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.selected_status().unwrap().workspace.name, "search");

    app.set_statuses(
        vec![status("search", Some("Add search")), status("new", None)],
        Vec::new(),
    );
    assert_eq!(app.selected_status().unwrap().workspace.name, "search");

    // The selected workspace was removed.
    app.set_statuses(vec![status("new", None)], Vec::new());
    assert_eq!(app.selected_status().unwrap().workspace.name, "new");
}

#[test]
fn movement_stays_inside_the_list() {
    let mut app = app();
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(app.selected, 0);
    for _ in 0..10 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.selected, 3);
}

#[test]
fn the_detail_pane_moves_its_own_selection() {
    let mut app = app();
    press(&mut app, KeyCode::Char('2'));
    press(&mut app, KeyCode::Tab);
    assert_eq!((app.tab, app.focus), (Tab::Snapshots, Focus::Detail));
    press(&mut app, KeyCode::Char('j'));
    assert_eq!((app.selected, app.selected_snapshot), (0, 1));
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.focus, Focus::Workspaces);
}

#[test]
fn restore_asks_before_it_overwrites() {
    let mut app = app();
    press(&mut app, KeyCode::Char('2'));
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(press(&mut app, KeyCode::Enter), None);
    assert!(matches!(app.mode, Mode::Confirm { .. }));
    assert!(screen(&app).contains("Restore `login` to snapshot 2?"));

    // Any other key keeps the question open; `n` closes it.
    assert_eq!(press(&mut app, KeyCode::Char('x')), None);
    assert!(matches!(app.mode, Mode::Confirm { .. }));
    assert_eq!(press(&mut app, KeyCode::Char('n')), None);
    assert_eq!(app.mode, Mode::Normal);

    press(&mut app, KeyCode::Char('r'));
    match press(&mut app, KeyCode::Char('y')) {
        Some(Request::Restore(workspace, snapshot)) => {
            assert_eq!(workspace.name, "login");
            assert_eq!(snapshot.seq, 2);
        }
        other => panic!("expected a restore, got {other:?}"),
    }
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn publish_takes_an_optional_message() {
    let mut app = app();
    press(&mut app, KeyCode::Char('p'));
    assert!(screen(&app).contains("Publish `login` to `main`"));
    type_text(&mut app, "Fix the loginx");
    press(&mut app, KeyCode::Backspace);
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        Some(Request::Publish {
            workspace: app.statuses[0].workspace.clone(),
            message: Some("Fix the login".to_string()),
        })
    );

    press(&mut app, KeyCode::Char('p'));
    assert!(matches!(
        press(&mut app, KeyCode::Enter),
        Some(Request::Publish { message: None, .. })
    ));

    press(&mut app, KeyCode::Char('p'));
    type_text(&mut app, "abandoned");
    assert_eq!(press(&mut app, KeyCode::Esc), None);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn letters_typed_into_an_input_are_not_commands() {
    let mut app = app();
    press(&mut app, KeyCode::Char('n'));
    type_text(&mut app, "quick-fix");
    assert!(!app.quit);
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        Some(Request::New {
            name: "quick-fix".to_string()
        })
    );
    // An empty name creates nothing.
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(press(&mut app, KeyCode::Enter), None);
}

#[test]
fn snapshot_needs_no_confirmation_but_remove_sync_and_undo_do() {
    let mut app = app();
    assert!(matches!(
        press(&mut app, KeyCode::Char('s')),
        Some(Request::Snapshot(_))
    ));
    for (key, expected) in [
        ('D', "Remove `login`?"),
        ('y', "Merge `main` into `login`?"),
        ('u', "Undo the last operation?"),
    ] {
        assert_eq!(press(&mut app, KeyCode::Char(key)), None);
        assert!(screen(&app).contains(expected), "{key}");
        assert!(press(&mut app, KeyCode::Char('y')).is_some(), "{key}");
    }
}

#[test]
fn quit_and_help() {
    let mut app = app();
    press(&mut app, KeyCode::Char('?'));
    assert!(screen(&app).contains("publish the workspace to its base branch"));
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.quit, "the first key only closes the help");
    press(&mut app, KeyCode::Char('q'));
    assert!(app.quit);

    let mut app = App::new();
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.quit);
}

#[test]
fn the_screen_shows_tasks_changes_agents_and_conflicts() {
    let mut app = app();
    app.statuses[0].sessions = vec![Session {
        agent: "claude".to_string(),
        id: "s1".to_string(),
        state: SessionState::Working,
        updated_at: 0,
        title: Some("Fix the login form".to_string()),
        last_message: Some("Running the tests now.".to_string()),
    }];
    app.diff = Some(Diff {
        from: "a".to_string(),
        to: "b".to_string(),
        stat: DiffStat::default(),
        files: Vec::new(),
        patch: "diff --git a/auth.rs b/auth.rs\n@@ -1 +1 @@\n-old line\n+new line".to_string(),
    });

    let text = screen(&app);
    for expected in [
        "Fix login",
        "Add search",
        "No task",
        "● login  +10 -3  conflict",
        "login × login-2  auth.rs",
        "+new line",
        "-old line",
    ] {
        assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
    }

    press(&mut app, KeyCode::Char('2'));
    assert!(screen(&app).contains("watch"));

    press(&mut app, KeyCode::Char('3'));
    let text = screen(&app);
    for expected in [
        "/work/repo.tsugi/login",
        "tsugi/login",
        "claude  working",
        "Running the tests now.",
        "Conflicts with login-2: auth.rs",
    ] {
        assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
    }
}

#[test]
fn an_empty_repository_view_does_not_panic() {
    let mut app = App::new();
    assert!(screen(&app).contains("No workspaces."));
    for key in ['j', 'k', 's', 'p', 'D', 'y', 'r', '2', '3'] {
        assert_eq!(press(&mut app, KeyCode::Char(key)), None);
        screen(&app);
    }
    let mut terminal = Terminal::new(TestBackend::new(8, 3)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
}

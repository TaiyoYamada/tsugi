//! The terminal app of tsugi: every workspace of a repository on one screen.
//!
//! The screen is drawn by one thread. A second thread reloads the state of
//! the workspaces every two seconds, a third runs the watcher, and every
//! request that changes the repository runs in a thread of its own, so the screen never waits for
//! git or for a verification command. Each thread opens the repository
//! itself; they coordinate through the repository lock like separate
//! processes do.

mod app;
mod ui;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use tsugi_core::{Conflict, Repo, Result, Trigger, WorkspaceStatus, watch};

pub use app::{App, Focus, Input, Mode, Request, Tab};
pub use ui::draw;

const RELOAD_EVERY: Duration = Duration::from_secs(2);
/// Longest list of snapshots the app shows for a workspace.
const MAX_SNAPSHOTS: usize = 500;

enum Message {
    State(Vec<WorkspaceStatus>, Vec<Conflict>),
    /// A request ended, with a sentence for the footer.
    Finished(String),
    Failed(String),
}

/// Runs the app until the user quits.
pub fn run(repo: Repo, home: Option<&Path>) -> Result<()> {
    let root = repo.root().to_path_buf();
    let (messages, inbox) = mpsc::channel();
    let (wake, wake_up) = mpsc::channel();
    {
        let (root, home, messages) = (root.clone(), home.map(Path::to_path_buf), messages.clone());
        std::thread::spawn(move || reload_loop(&root, home.as_deref(), &messages, &wake_up));
    }

    // While the app is open, workspaces are snapshotted as files change.
    // If a watcher runs already, this one ends at once.
    let stop_watching = Arc::new(AtomicBool::new(false));
    {
        let (root, stop) = (root.clone(), stop_watching.clone());
        std::thread::spawn(move || {
            if let Ok(repo) = Repo::discover(&root) {
                let _ = watch::run(&repo, &stop, &|_| {});
            }
        });
    }

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &repo, &root, &messages, &inbox, &wake);
    ratatui::restore();
    stop_watching.store(true, Ordering::Relaxed);
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    repo: &Repo,
    root: &Path,
    messages: &Sender<Message>,
    inbox: &Receiver<Message>,
    wake: &Sender<()>,
) -> Result<()> {
    let mut app = App::new();
    // Workspace and latest snapshot the detail pane was loaded for.
    let mut loaded: Option<(String, Option<String>)> = None;

    while !app.quit {
        while let Ok(message) = inbox.try_recv() {
            match message {
                Message::State(statuses, conflicts) => app.set_statuses(statuses, conflicts),
                Message::Finished(text) => {
                    app.busy = None;
                    app.message = Some(text);
                    let _ = wake.send(());
                }
                Message::Failed(text) => {
                    app.busy = None;
                    app.message = Some(format!("error: {text}"));
                    let _ = wake.send(());
                }
            }
        }

        let wanted = app.selected_status().map(|status| {
            (
                status.workspace.id.clone(),
                status
                    .latest_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.id.clone()),
            )
        });
        if wanted != loaded {
            if let Err(error) = load_detail(repo, &mut app) {
                app.message = Some(format!("error: {error}"));
            }
            loaded = wanted;
        }

        terminal.draw(|frame| ui::draw(frame, &app))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && let Some(request) = app.handle_key(key)
        {
            if app.busy.is_some() {
                app.message = Some("another request is still running".to_string());
                continue;
            }
            app.busy = Some(request.describe());
            app.message = None;
            let (root, messages) = (root.to_path_buf(), messages.clone());
            std::thread::spawn(move || {
                let message = match execute(&root, request) {
                    Ok(text) => Message::Finished(text),
                    Err(error) => Message::Failed(error.to_string()),
                };
                let _ = messages.send(message);
            });
        }
    }
    Ok(())
}

/// Loads the snapshots and the diff of the selected workspace.
fn load_detail(repo: &Repo, app: &mut App) -> Result<()> {
    app.snapshots.clear();
    app.diff = None;
    let Some(status) = app.selected_status() else {
        return Ok(());
    };
    let mut snapshots = repo.snapshots(&status.workspace)?;
    snapshots.truncate(MAX_SNAPSHOTS);
    // No fork point means the base branch is missing or unrelated.
    let diff = match &status.latest_snapshot {
        Some(latest) => match repo.fork_point(&status.workspace, latest) {
            Ok(from) => Some(repo.diff(&from, &latest.id)?),
            Err(_) => None,
        },
        None => None,
    };
    app.selected_snapshot = app.selected_snapshot.min(snapshots.len().saturating_sub(1));
    app.snapshots = snapshots;
    app.diff = diff;
    Ok(())
}

/// Sends the state of the workspaces every two seconds, and at once when
/// woken. Ends when the app is gone.
fn reload_loop(
    root: &Path,
    home: Option<&Path>,
    messages: &Sender<Message>,
    wake_up: &Receiver<()>,
) {
    loop {
        let state = Repo::discover(root).and_then(|repo| {
            let statuses = repo.poll_status(home)?;
            let conflicts = repo.conflicts_among(&statuses)?;
            Ok((statuses, conflicts))
        });
        let message = match state {
            Ok((statuses, conflicts)) => Message::State(statuses, conflicts),
            Err(error) => Message::Failed(error.to_string()),
        };
        if messages.send(message).is_err() {
            return;
        }
        if let Err(RecvTimeoutError::Disconnected) = wake_up.recv_timeout(RELOAD_EVERY) {
            return;
        }
    }
}

/// Carries out a request and returns a sentence that says what happened.
fn execute(root: &Path, request: Request) -> Result<String> {
    let repo = Repo::discover(root)?;
    let mut text = match request {
        Request::New { name } => {
            let workspace = repo.workspace_new(&name, None, None)?;
            format!(
                "created `{}` at {}",
                workspace.name,
                workspace.path.display()
            )
        }
        Request::Snapshot(workspace) => {
            let outcome = repo.snapshot(&workspace, Trigger::Manual)?;
            if outcome.created {
                format!("snapshot {} of `{}`", outcome.snapshot.seq, workspace.name)
            } else {
                format!(
                    "`{}` has not changed since snapshot {}",
                    workspace.name, outcome.snapshot.seq
                )
            }
        }
        Request::Restore(workspace, snapshot) => {
            let outcome = repo.restore(&workspace, &snapshot)?;
            format!(
                "restored `{}` to snapshot {}; the state before is snapshot {}",
                workspace.name, snapshot.seq, outcome.previous.seq
            )
        }
        Request::Sync(workspace) => {
            repo.workspace_sync(&workspace)?;
            format!("`{}` now sits on `{}`", workspace.name, workspace.base)
        }
        Request::Publish { workspace, message } => {
            let outcome = repo.publish(&workspace, message.as_deref(), true)?;
            format!(
                "published `{}` to `{}`: {} file(s) +{} -{}",
                workspace.name,
                outcome.base,
                outcome.stat.files,
                outcome.stat.insertions,
                outcome.stat.deletions
            )
        }
        Request::Remove(workspace) => {
            repo.workspace_remove(&workspace)?;
            format!("removed `{}`; `u` brings it back", workspace.name)
        }
        Request::Undo => {
            let undone = repo.undo()?;
            format!("undid operation #{} ({})", undone.id, undone.detail.kind())
        }
    };
    for notice in repo.take_notices() {
        text.push_str(&format!(" ({notice})"));
    }
    Ok(text)
}

/// `just now`, `3m ago`, `2h ago`, `5d ago`.
pub fn ago(timestamp: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(timestamp);
    let seconds = (now - timestamp).max(0);
    match seconds {
        0..60 => "just now".to_string(),
        60..3600 => format!("{}m ago", seconds / 60),
        3600..86400 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86400),
    }
}

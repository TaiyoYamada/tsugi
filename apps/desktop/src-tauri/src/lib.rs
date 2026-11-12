//! The desktop app. Everything it does is a call into `tsugi_core`; this
//! file only turns those calls into commands the window can invoke.
//!
//! Every command opens the repository again. That costs a few milliseconds
//! and means the app holds no state that could go stale while agents and
//! the command line tool change the repository underneath it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
use tauri::{Emitter, State};
use ts_rs::TS;
use tsugi_core::{
    Conflict, Diff, Error, Operation, PublishOutcome, Removal, Repo, RestoreOutcome, SessionState,
    Snapshot, SnapshotOutcome, SyncOutcome, Task, Trigger, Workspace, WorkspaceStatus, watch,
};

/// Longest list of snapshots the app shows for a workspace.
const MAX_SNAPSHOTS: usize = 500;

/// A failure, in the same shape the command line tool prints with `--json`.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<Error> for CommandError {
    fn from(error: Error) -> Self {
        CommandError {
            code: error.code().to_string(),
            message: error.to_string(),
        }
    }
}

type CommandResult<T> = Result<T, CommandError>;

/// The result of a command that changed the repository.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Outcome<T> {
    pub value: T,
    /// Things worth telling the user that are not the result itself.
    pub notices: Vec<String>,
}

fn outcome<T>(repo: &Repo, value: T) -> Outcome<T> {
    Outcome {
        value,
        notices: repo.take_notices(),
    }
}

/// Everything the main screen shows.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Overview {
    /// Root of the main worktree.
    pub root: PathBuf,
    pub default_base: String,
    /// True when a watcher takes snapshots as files change, in this app or
    /// started with `tsugi watch`.
    pub watching: bool,
    pub statuses: Vec<WorkspaceStatus>,
    pub conflicts: Vec<Conflict>,
    pub tasks: Vec<Task>,
}

/// What the list of repositories shows for one of them.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct RepoSummary {
    pub workspaces: u32,
    /// Workspaces whose most recently active agent is in the middle of a
    /// turn.
    pub working: u32,
    /// Pairs of workspaces, or of a workspace and its base branch, that
    /// would conflict.
    pub conflicts: u32,
}

/// What the detail pane shows for one workspace.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Detail {
    /// Newest first.
    pub snapshots: Vec<Snapshot>,
    /// What the workspace changed since it left its base branch. `None`
    /// when the base branch is missing or shares no history.
    pub diff: Option<Diff>,
}

/// Watchers this app runs, by repository root. Setting the flag stops one.
#[derive(Default)]
struct Watchers(Arc<Mutex<HashMap<PathBuf, Arc<AtomicBool>>>>);

/// Starts a watcher for the repository unless one is running somewhere.
fn start_watching(watchers: &Watchers, repo: &Repo) -> Result<(), Error> {
    let root = repo.root().to_path_buf();
    let stop = Arc::new(AtomicBool::new(false));
    {
        // Checked and recorded under one lock: the window asks for the same
        // repository from several places at once when it starts.
        let mut running = watchers
            .0
            .lock()
            .expect("no thread panics while holding the lock");
        if running.contains_key(&root) || watch::is_running(repo)? {
            return Ok(());
        }
        running.insert(root.clone(), stop.clone());
    }
    let running = watchers.0.clone();
    std::thread::spawn(move || {
        // The thread opens the repository itself: `Repo` is not shared
        // between threads.
        if let Ok(repo) = Repo::discover(&root) {
            let _ = watch::run(&repo, &stop, &|_| {});
        }
        // The watcher ended, because it was stopped or could not start.
        // Forget it, unless a newer one took its place.
        let mut running = running
            .lock()
            .expect("no thread panics while holding the lock");
        if running
            .get(&root)
            .is_some_and(|current| Arc::ptr_eq(current, &stop))
        {
            running.remove(&root);
        }
    });
    Ok(())
}

/// The directory given on the command line, as in `tsugi-desktop .`.
#[tauri::command]
fn launch_path() -> Option<PathBuf> {
    let path = std::env::args_os().nth(1)?;
    std::path::absolute(path).ok()
}

/// Opens a repository and returns its root. With `watch`, snapshots are
/// taken as its files change for as long as it stays open.
#[tauri::command(async)]
fn open_repo(path: PathBuf, watch: bool, watchers: State<'_, Watchers>) -> CommandResult<PathBuf> {
    let repo = Repo::discover(&path)?;
    if watch {
        start_watching(&watchers, &repo)?;
    }
    Ok(repo.root().to_path_buf())
}

/// Sets tsugi up in a repository and opens it.
#[tauri::command(async)]
fn init_repo(path: PathBuf, watch: bool, watchers: State<'_, Watchers>) -> CommandResult<PathBuf> {
    let repo = Repo::init(&path)?;
    if watch {
        start_watching(&watchers, &repo)?;
    }
    Ok(repo.root().to_path_buf())
}

/// Stops the watcher this app runs for the repository, if it runs one.
/// Called when the repository is taken off the list.
#[tauri::command(async)]
fn close_repo(repo: PathBuf, watchers: State<'_, Watchers>) -> CommandResult<()> {
    let stop = watchers
        .0
        .lock()
        .expect("no thread panics while holding the lock")
        .remove(&repo);
    if let Some(stop) = stop {
        stop.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command(async)]
fn summary(repo: PathBuf) -> CommandResult<RepoSummary> {
    let repo = Repo::discover(&repo)?;
    let statuses = repo.poll_status(std::env::home_dir().as_deref())?;
    let working = statuses
        .iter()
        .filter(|status| {
            status
                .sessions
                .first()
                .is_some_and(|session| session.state == SessionState::Working)
        })
        .count();
    Ok(RepoSummary {
        workspaces: statuses.len() as u32,
        working: working as u32,
        conflicts: repo.conflicts_among(&statuses)?.len() as u32,
    })
}

#[tauri::command(async)]
fn overview(repo: PathBuf) -> CommandResult<Overview> {
    let repo = Repo::discover(&repo)?;
    let statuses = repo.poll_status(std::env::home_dir().as_deref())?;
    Ok(Overview {
        root: repo.root().to_path_buf(),
        default_base: repo.default_base()?,
        watching: watch::is_running(&repo)?,
        conflicts: repo.conflicts_among(&statuses)?,
        tasks: repo.tasks()?,
        statuses,
    })
}

#[tauri::command(async)]
fn detail(repo: PathBuf, workspace: String) -> CommandResult<Detail> {
    let repo = Repo::discover(&repo)?;
    let workspace = repo.workspace(&workspace)?;
    let mut snapshots = repo.snapshots(&workspace)?;
    snapshots.truncate(MAX_SNAPSHOTS);
    let diff = match snapshots.first() {
        Some(latest) => match repo.fork_point(&workspace, latest) {
            Ok(from) => Some(repo.diff(&from, &latest.id)?),
            Err(_) => None,
        },
        None => None,
    };
    Ok(Detail { snapshots, diff })
}

/// The difference between two snapshots, by commit id.
#[tauri::command(async)]
fn diff(repo: PathBuf, from: String, to: String) -> CommandResult<Diff> {
    Ok(Repo::discover(&repo)?.diff(&from, &to)?)
}

#[tauri::command(async)]
fn set_watching(repo: PathBuf, on: bool, watchers: State<'_, Watchers>) -> CommandResult<bool> {
    let repo = Repo::discover(&repo)?;
    if on {
        start_watching(&watchers, &repo)?;
        return Ok(true);
    }
    let stop = watchers
        .0
        .lock()
        .expect("no thread panics while holding the lock")
        .remove(repo.root());
    match stop {
        Some(stop) => {
            stop.store(true, Ordering::Relaxed);
            Ok(false)
        }
        None if watch::is_running(&repo)? => Err(Error::Refused(
            "the watcher was started with `tsugi watch start`; stop it with `tsugi watch stop`"
                .to_string(),
        )
        .into()),
        None => Ok(false),
    }
}

#[tauri::command(async)]
fn task_new(repo: PathBuf, title: String) -> CommandResult<Outcome<Task>> {
    let repo = Repo::discover(&repo)?;
    let task = repo.task_new(&title)?;
    Ok(outcome(&repo, task))
}

#[tauri::command(async)]
fn workspace_new(
    repo: PathBuf,
    name: String,
    task: Option<String>,
    base: Option<String>,
) -> CommandResult<Outcome<Workspace>> {
    let repo = Repo::discover(&repo)?;
    let task = task.map(|id| repo.task(&id)).transpose()?;
    let workspace = repo.workspace_new(&name, task.as_ref(), base.as_deref())?;
    Ok(outcome(&repo, workspace))
}

#[tauri::command(async)]
fn workspace_remove(repo: PathBuf, workspace: String) -> CommandResult<Outcome<Removal>> {
    let repo = Repo::discover(&repo)?;
    let removal = repo.workspace_remove(&repo.workspace(&workspace)?)?;
    Ok(outcome(&repo, removal))
}

#[tauri::command(async)]
fn workspace_sync(repo: PathBuf, workspace: String) -> CommandResult<Outcome<SyncOutcome>> {
    let repo = Repo::discover(&repo)?;
    let synced = repo.workspace_sync(&repo.workspace(&workspace)?)?;
    Ok(outcome(&repo, synced))
}

#[tauri::command(async)]
fn snapshot(repo: PathBuf, workspace: String) -> CommandResult<Outcome<SnapshotOutcome>> {
    let repo = Repo::discover(&repo)?;
    let taken = repo.snapshot(&repo.workspace(&workspace)?, Trigger::Manual)?;
    Ok(outcome(&repo, taken))
}

/// Restores a workspace to one of its snapshots, by commit id.
#[tauri::command(async)]
fn restore(
    repo: PathBuf,
    workspace: String,
    snapshot: String,
) -> CommandResult<Outcome<RestoreOutcome>> {
    let repo = Repo::discover(&repo)?;
    let workspace = repo.workspace(&workspace)?;
    let target = repo
        .snapshots(&workspace)?
        .into_iter()
        .find(|candidate| candidate.id == snapshot)
        .ok_or(Error::SnapshotNotFound(snapshot))?;
    let restored = repo.restore(&workspace, &target)?;
    Ok(outcome(&repo, restored))
}

#[tauri::command(async)]
fn publish(
    repo: PathBuf,
    workspace: String,
    message: Option<String>,
    verify: bool,
) -> CommandResult<Outcome<PublishOutcome>> {
    let repo = Repo::discover(&repo)?;
    let published = repo.publish(&repo.workspace(&workspace)?, message.as_deref(), verify)?;
    Ok(outcome(&repo, published))
}

#[tauri::command(async)]
fn undo(repo: PathBuf) -> CommandResult<Outcome<Operation>> {
    let repo = Repo::discover(&repo)?;
    let undone = repo.undo()?;
    Ok(outcome(&repo, undone))
}

#[tauri::command(async)]
fn operations(repo: PathBuf, limit: u32) -> CommandResult<Vec<Operation>> {
    Ok(Repo::discover(&repo)?.operations(limit)?)
}

/// Registers the commands and their state on a builder. The tests call
/// this with a builder that has no real window behind it.
pub fn with_commands<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .manage(Watchers::default())
        .invoke_handler(tauri::generate_handler![
            launch_path,
            open_repo,
            init_repo,
            close_repo,
            summary,
            overview,
            detail,
            diff,
            set_watching,
            task_new,
            workspace_new,
            workspace_remove,
            workspace_sync,
            snapshot,
            restore,
            publish,
            undo,
            operations,
        ])
}

/// Name of the event that asks the window to show the settings.
const OPEN_SETTINGS: &str = "open-settings";

/// Adds "Settings…" with its usual shortcut to the standard menu, below
/// "About" in the application menu, where macOS apps have it.
fn install_menu(app: &tauri::App) -> tauri::Result<()> {
    let menu = Menu::default(app.handle())?;
    let settings = MenuItem::with_id(app, OPEN_SETTINGS, "Settings…", true, Some("CmdOrCtrl+,"))?;
    if let Some(MenuItemKind::Submenu(first)) = menu.items()?.into_iter().next() {
        first.insert_items(&[&PredefinedMenuItem::separator(app)?, &settings], 1)?;
    }
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        if event.id() == OPEN_SETTINGS {
            // Fails only if the window is gone, and then nobody is asking.
            let _ = app.emit(OPEN_SETTINGS, ());
        }
    });
    Ok(())
}

pub fn run() {
    with_commands(tauri::Builder::default().plugin(tauri_plugin_dialog::init()))
        .setup(|app| Ok(install_menu(app)?))
        .run(tauri::generate_context!())
        .expect("the app starts");
}

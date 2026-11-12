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
use tauri::State;
use ts_rs::TS;
use tsugi_core::{
    Conflict, Diff, Error, Operation, PublishOutcome, Removal, Repo, RestoreOutcome, Snapshot,
    SnapshotOutcome, SyncOutcome, Task, Trigger, Workspace, WorkspaceStatus, watch,
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
struct Watchers(Mutex<HashMap<PathBuf, Arc<AtomicBool>>>);

/// Starts a watcher for the repository unless one is running somewhere.
fn start_watching(watchers: &Watchers, repo: &Repo) -> Result<(), Error> {
    if watch::is_running(repo)? {
        return Ok(());
    }
    let root = repo.root().to_path_buf();
    let stop = Arc::new(AtomicBool::new(false));
    watchers
        .0
        .lock()
        .expect("no thread panics while holding the lock")
        .insert(root.clone(), stop.clone());
    std::thread::spawn(move || {
        // The thread opens the repository itself: `Repo` is not shared
        // between threads.
        if let Ok(repo) = Repo::discover(&root) {
            let _ = watch::run(&repo, &stop, &|_| {});
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

#[tauri::command(async)]
fn open_repo(path: PathBuf, watchers: State<'_, Watchers>) -> CommandResult<PathBuf> {
    let repo = Repo::discover(&path)?;
    start_watching(&watchers, &repo)?;
    Ok(repo.root().to_path_buf())
}

#[tauri::command(async)]
fn init_repo(path: PathBuf, watchers: State<'_, Watchers>) -> CommandResult<PathBuf> {
    let repo = Repo::init(&path)?;
    start_watching(&watchers, &repo)?;
    Ok(repo.root().to_path_buf())
}

#[tauri::command(async)]
fn overview(repo: PathBuf) -> CommandResult<Overview> {
    let repo = Repo::discover(&repo)?;
    let statuses = repo.status(std::env::home_dir().as_deref())?;
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

pub fn run() {
    with_commands(tauri::Builder::default().plugin(tauri_plugin_dialog::init()))
        .run(tauri::generate_context!())
        .expect("the app starts");
}

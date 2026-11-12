//! The watcher: snapshots a workspace shortly after its files change.
//!
//! Safety does not depend on it. Every command that overwrites files takes
//! its own snapshot first; the watcher only makes the history finer.

use std::collections::HashMap;
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::{EventKind, RecursiveMode, Watcher};

use crate::error::{Error, Result};
use crate::model::{Trigger, Workspace};
use crate::repo::Repo;

/// A workspace is snapshotted once its files were quiet for this long.
const QUIET: Duration = Duration::from_millis(1500);
/// ...or once this much time passed since the first change, so that an agent
/// that never pauses is still snapshotted.
const MAX_WAIT: Duration = Duration::from_secs(10);
/// How often new and removed workspaces are looked for.
const REFRESH: Duration = Duration::from_secs(5);
/// How often old snapshots are thinned out and expired.
const GC_EVERY: Duration = Duration::from_secs(60 * 60);
/// Above this many changed paths the watcher stops asking Git whether all
/// of them are ignored and takes the snapshot.
const MAX_PATHS: usize = 256;

struct Watched {
    workspace: Workspace,
    git_dir: PathBuf,
}

struct Pending {
    first: Instant,
    last: Instant,
    paths: Vec<PathBuf>,
    /// A commit, checkout or reset happened.
    head_moved: bool,
}

/// The watcher holds a lock on this file for as long as it runs, so the
/// operating system tells whether one is running, even after a crash.
fn try_lock(repo: &Repo) -> Result<Option<File>> {
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(repo.state_dir().join("watch.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

/// True when a watcher runs for this repository, in any process.
pub fn is_running(repo: &Repo) -> Result<bool> {
    match try_lock(repo)? {
        Some(file) => {
            file.unlock()?;
            Ok(false)
        }
        None => Ok(true),
    }
}

/// Runs the watcher until `stop` is set or tsugi is removed from the
/// repository. `log` receives one line per event worth telling. Fails if a
/// watcher is running already.
pub fn run(repo: &Repo, stop: &AtomicBool, log: &dyn Fn(String)) -> Result<()> {
    // `is_running` takes the lock for an instant to probe; wait that out.
    let deadline = Instant::now() + Duration::from_secs(1);
    let _lock = loop {
        if let Some(file) = try_lock(repo)? {
            break file;
        }
        if Instant::now() >= deadline {
            return Err(Error::Refused("a watcher is already running".to_string()));
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    let (sender, events) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(sender)
        .map_err(|error| Error::Refused(format!("cannot watch files: {error}")))?;
    let mut watched: Vec<Watched> = Vec::new();
    let mut pending: HashMap<String, Pending> = HashMap::new();
    let mut next_refresh = Instant::now();
    let mut next_gc = Instant::now();

    while !stop.load(Ordering::Relaxed) && repo.state_dir().exists() {
        if Instant::now() >= next_gc {
            match repo.gc() {
                Ok(outcome) if outcome.deleted > 0 => {
                    log(format!("deleted {} old snapshot(s)", outcome.deleted));
                }
                Ok(_) => {}
                Err(error) => log(format!("gc failed: {error}")),
            }
            next_gc = Instant::now() + GC_EVERY;
        }
        if Instant::now() >= next_refresh {
            if let Err(error) = refresh(repo, &mut watcher, &mut watched, log) {
                log(format!("refresh failed: {error}"));
            }
            next_refresh = Instant::now() + REFRESH;
        }

        match events.recv_timeout(Duration::from_millis(250)) {
            Ok(Ok(event)) => {
                // Reading a file is an event on Linux; it changes nothing.
                if !matches!(event.kind, EventKind::Access(_)) {
                    for path in event.paths {
                        record(&path, &watched, &mut pending);
                    }
                }
            }
            Ok(Err(error)) => log(format!("watch error: {error}")),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        let now = Instant::now();
        let due: Vec<String> = pending
            .iter()
            .filter(|(_, changes)| {
                now.duration_since(changes.last) >= QUIET
                    || now.duration_since(changes.first) >= MAX_WAIT
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            let changes = pending.remove(&id).expect("the key was just listed");
            let Some(entry) = watched.iter().find(|entry| entry.workspace.id == id) else {
                continue;
            };
            if let Err(error) = snapshot(repo, &entry.workspace, &changes, log) {
                log(format!(
                    "snapshot of `{}` failed: {error}",
                    entry.workspace.name
                ));
            }
        }
    }
    Ok(())
}

/// Starts watching new workspaces and stops watching removed ones.
fn refresh(
    repo: &Repo,
    watcher: &mut notify::RecommendedWatcher,
    watched: &mut Vec<Watched>,
    log: &dyn Fn(String),
) -> Result<()> {
    let first_time = watched.is_empty();
    let current = repo.refresh()?;

    watched.retain(|entry| {
        let exists = current.iter().any(|ws| ws.id == entry.workspace.id);
        if !exists {
            // The directory is gone; the watch usually went with it.
            let _ = watcher.unwatch(&entry.workspace.path);
            log(format!("stopped watching `{}`", entry.workspace.name));
        }
        exists
    });

    for workspace in &current {
        if watched
            .iter()
            .any(|entry| entry.workspace.id == workspace.id)
        {
            continue;
        }
        let git_dir = repo.git_dir(workspace)?;
        let watch = |watcher: &mut notify::RecommendedWatcher, path: &Path| {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .map_err(|error| {
                    Error::Refused(format!("cannot watch {}: {error}", path.display()))
                })
        };
        watch(watcher, &workspace.path)?;
        // Git directories normally live inside the main worktree, which is
        // watched as a workspace. `git init --separate-git-dir` puts them
        // elsewhere.
        if !current.iter().any(|ws| git_dir.starts_with(&ws.path)) {
            watch(watcher, &git_dir)?;
        }
        log(format!("watching `{}`", workspace.name));
        watched.push(Watched {
            workspace: workspace.clone(),
            git_dir,
        });
    }

    if first_time {
        // Whatever changed while no watcher was running.
        repo.status(None)?;
    }
    Ok(())
}

/// Notes that `path` changed, if that can matter to a snapshot.
fn record(path: &Path, watched: &[Watched], pending: &mut HashMap<String, Pending>) {
    // A commit, checkout or reset appends to the reflog of HEAD; a checkout
    // also rewrites HEAD itself.
    for entry in watched {
        if path == entry.git_dir.join("HEAD") || path == entry.git_dir.join("logs").join("HEAD") {
            touch(pending, &entry.workspace).head_moved = true;
            return;
        }
    }
    // Everything else inside a Git directory is Git's own bookkeeping,
    // including the objects and refs tsugi writes for snapshots.
    if path.components().any(|part| part.as_os_str() == ".git") {
        return;
    }
    let inside = watched
        .iter()
        .filter(|entry| path.starts_with(&entry.workspace.path))
        .max_by_key(|entry| entry.workspace.path.components().count());
    if let Some(entry) = inside {
        let changes = touch(pending, &entry.workspace);
        if changes.paths.len() <= MAX_PATHS {
            changes.paths.push(path.to_path_buf());
        }
    }
}

/// The pending changes of a workspace, with the time of the last change set
/// to now.
fn touch<'a>(pending: &'a mut HashMap<String, Pending>, workspace: &Workspace) -> &'a mut Pending {
    let now = Instant::now();
    let changes = pending.entry(workspace.id.clone()).or_insert(Pending {
        first: now,
        last: now,
        paths: Vec::new(),
        head_moved: false,
    });
    changes.last = now;
    changes
}

fn snapshot(
    repo: &Repo,
    workspace: &Workspace,
    changes: &Pending,
    log: &dyn Fn(String),
) -> Result<()> {
    // Builds write thousands of ignored files. Asking Git about the paths is
    // much cheaper than hashing the working tree to find that nothing that
    // counts has changed.
    let only_ignored = !changes.head_moved
        && changes.paths.len() <= MAX_PATHS
        && repo.all_ignored(workspace, &changes.paths)?;
    if only_ignored {
        return Ok(());
    }
    let outcome = repo.snapshot(workspace, Trigger::Watch)?;
    if outcome.created {
        log(format!(
            "snapshot {}@{}",
            workspace.name, outcome.snapshot.seq
        ));
    }
    Ok(())
}

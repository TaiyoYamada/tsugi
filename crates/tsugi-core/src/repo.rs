//! A repository with tsugi set up in it.

use std::cell::RefCell;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::git::{self, Git};
use crate::model::{Origin, Task, Workspace};
use crate::store::{Store, now};

/// `git merge-tree --write-tree` appeared in this version.
const MIN_GIT: (u32, u32) = (2, 38);

const STATE_DIR: &str = "tsugi";
const DATABASE: &str = "state.db";
const META_DEFAULT_BASE: &str = "default_base";

pub struct Repo {
    pub(crate) git: Git,
    pub(crate) main_root: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) store: Store,
    pub(crate) config: Config,
    /// Things the user should hear about that are not the result of the
    /// command itself, such as an interrupted operation that was resolved.
    pub(crate) notices: RefCell<Vec<String>>,
}

/// How long a command that only reads waits for the repository lock before
/// it goes on without it.
const BRIEF_WAIT: Duration = Duration::from_millis(300);

/// Proof that the repository lock is held. Released when dropped.
pub struct RepoLock {
    file: File,
}

impl Drop for RepoLock {
    fn drop(&mut self) {
        // Closing the file is not enough: a git process started by another
        // thread at this moment holds a copy of the descriptor until it
        // calls exec, and the lock would stay taken that long.
        let _ = self.file.unlock();
    }
}

/// A Git worktree as reported by `git worktree list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    /// Full name of the checked-out branch, or `None` when detached.
    pub branch: Option<String>,
}

impl Repo {
    /// Sets tsugi up in the repository that contains `dir`. Doing so again is
    /// harmless.
    pub fn init(dir: &Path) -> Result<Repo> {
        let (major, minor) = git::version()?;
        if (major, minor) < MIN_GIT {
            return Err(Error::GitTooOld {
                required: format!("{}.{}", MIN_GIT.0, MIN_GIT.1),
                found: format!("{major}.{minor}"),
            });
        }
        let common_dir = common_dir(dir)?;
        fs::create_dir_all(common_dir.join(STATE_DIR).join("tmp"))?;
        let repo = Repo::open(common_dir)?;
        if repo.store.meta(META_DEFAULT_BASE)?.is_none() {
            repo.store
                .set_meta(META_DEFAULT_BASE, &repo.detect_default_base()?)?;
        }
        let lock = repo.lock()?;
        repo.settle(&lock)?;
        drop(lock);
        Ok(repo)
    }

    /// Opens the repository that contains `dir`.
    pub fn discover(dir: &Path) -> Result<Repo> {
        let common_dir = common_dir(dir)?;
        if !common_dir.join(STATE_DIR).join(DATABASE).exists() {
            return Err(Error::NotInitialized);
        }
        Repo::open(common_dir)
    }

    fn open(common_dir: PathBuf) -> Result<Repo> {
        let state_dir = common_dir.join(STATE_DIR);
        let main = list_worktrees(&Git::new(&common_dir))?
            .into_iter()
            .next()
            .ok_or(Error::BareRepository)?;
        let store = Store::open(&state_dir.join(DATABASE))?;
        let config = Config::load(&main.path)?;
        Ok(Repo {
            git: Git::new(&main.path),
            main_root: main.path,
            state_dir,
            store,
            config,
            notices: RefCell::new(Vec::new()),
        })
    }

    /// Root of the main worktree.
    pub fn root(&self) -> &Path {
        &self.main_root
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Takes the messages collected while running commands.
    pub fn take_notices(&self) -> Vec<String> {
        self.notices.take()
    }

    pub(crate) fn notice(&self, message: impl Into<String>) {
        self.notices.borrow_mut().push(message.into());
    }

    /// Waits for the repository lock.
    pub(crate) fn lock(&self) -> Result<RepoLock> {
        let file = self.lock_file()?;
        file.lock()?;
        Ok(RepoLock { file })
    }

    /// Takes the repository lock unless another process keeps it for longer
    /// than a moment. Snapshots take a moment; verification, which can take
    /// minutes, runs without the lock.
    pub(crate) fn lock_briefly(&self) -> Result<Option<RepoLock>> {
        let file = self.lock_file()?;
        let deadline = Instant::now() + BRIEF_WAIT;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Some(RepoLock { file })),
                Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(fs::TryLockError::WouldBlock) => return Ok(None),
                Err(fs::TryLockError::Error(error)) => return Err(error.into()),
            }
        }
    }

    fn lock_file(&self) -> Result<File> {
        Ok(File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.state_dir.join("lock"))?)
    }

    /// Brings tsugi's records in line with reality: resolves operations that
    /// an earlier process left unfinished, then adopts new worktrees and
    /// retires workspaces whose worktree is gone.
    pub(crate) fn settle(&self, lock: &RepoLock) -> Result<()> {
        self.recover(lock)?;
        self.reconcile(lock)
    }

    /// `settle` for commands that only read: skipped when another process
    /// is busy with the repository, because that process has settled already.
    pub(crate) fn try_settle(&self) -> Result<()> {
        if let Some(lock) = self.lock_briefly()? {
            self.settle(&lock)?;
        }
        Ok(())
    }

    fn reconcile(&self, _lock: &RepoLock) -> Result<()> {
        let worktrees = self.worktrees()?;
        let mut active = self.workspaces()?;

        for worktree in &worktrees {
            if active.iter().any(|ws| ws.path == worktree.path) {
                continue;
            }
            let wanted = worktree
                .path
                .file_name()
                .map(|name| sanitize_name(&name.to_string_lossy()))
                .unwrap_or_else(|| "workspace".to_string());
            let workspace = Workspace {
                id: self.store.new_id()?,
                name: unique_name(&wanted, &active),
                task_id: None,
                path: worktree.path.clone(),
                base: self.default_base()?,
                origin: Origin::Adopted,
                created_at: now(),
                removed_at: None,
            };
            self.store.insert_workspace(&workspace)?;
            active.push(workspace);
        }

        for workspace in &active {
            if !worktrees.iter().any(|wt| wt.path == workspace.path) {
                self.store
                    .set_workspace_removed(&workspace.id, Some(now()))?;
            }
        }
        Ok(())
    }

    /// Worktrees that exist on disk, the main worktree first. Worktrees tsugi
    /// creates for its own use are left out.
    pub fn worktrees(&self) -> Result<Vec<Worktree>> {
        Ok(list_worktrees(&self.git)?
            .into_iter()
            .filter(|worktree| !worktree.path.starts_with(&self.state_dir))
            .collect())
    }

    /// Branch that workspaces start from and publish to unless told otherwise.
    pub fn default_base(&self) -> Result<String> {
        if let Some(base) = &self.config.workspace.base {
            return Ok(base.clone());
        }
        Ok(self
            .store
            .meta(META_DEFAULT_BASE)?
            .unwrap_or_else(|| "main".to_string()))
    }

    fn detect_default_base(&self) -> Result<String> {
        let remote = self.git.output(
            &[
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD",
            ],
            None,
        )?;
        if remote.success() {
            let name = remote.text();
            let name = name.strip_prefix("origin/").unwrap_or(&name).to_string();
            if self.git.commit(&format!("refs/heads/{name}"))?.is_some() {
                return Ok(name);
            }
        }
        Ok(match self.git.current_branch()? {
            Some(branch) => short_branch(&branch).to_string(),
            None => "main".to_string(),
        })
    }

    // tasks

    pub fn task_new(&self, title: &str) -> Result<Task> {
        let lock = self.lock()?;
        self.settle(&lock)?;
        let task = Task {
            id: self.store.new_id()?,
            title: title.to_string(),
            created_at: now(),
            closed_at: None,
        };
        let op = self.begin(
            &lock,
            crate::model::OpDetail::TaskNew {
                task_id: task.id.clone(),
            },
        )?;
        self.store.insert_task(&task)?;
        op.done(self)?;
        Ok(task)
    }

    pub fn tasks(&self) -> Result<Vec<Task>> {
        self.store.tasks()
    }

    /// Finds a task by id, by a prefix of its id, or by its exact title.
    pub fn task(&self, selector: &str) -> Result<Task> {
        let tasks = self.store.tasks()?;
        tasks
            .iter()
            .find(|task| task.id == selector || task.title == selector)
            .or_else(|| {
                let mut matches = tasks.iter().filter(|task| task.id.starts_with(selector));
                let first = matches.next();
                first.filter(|_| matches.next().is_none())
            })
            .cloned()
            .ok_or_else(|| Error::TaskNotFound(selector.to_string()))
    }

    pub fn task_close(&self, selector: &str) -> Result<Task> {
        let mut task = self.task(selector)?;
        task.closed_at = Some(now());
        self.store.close_task(&task.id, task.closed_at)?;
        Ok(task)
    }

    // workspaces

    /// Workspaces whose worktree exists, oldest first.
    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        Ok(self
            .store
            .workspaces()?
            .into_iter()
            .filter(|workspace| workspace.removed_at.is_none())
            .collect())
    }

    /// Adopts new worktrees, retires workspaces whose worktree is gone, and
    /// returns the workspaces that exist.
    pub fn refresh(&self) -> Result<Vec<Workspace>> {
        self.try_settle()?;
        self.workspaces()
    }

    /// Turns any text, such as the title of a task, into a workspace name
    /// that is valid and not in use.
    pub fn suggest_name(&self, text: &str) -> Result<String> {
        Ok(unique_name(&sanitize_name(text), &self.workspaces()?))
    }

    /// The Git directory of a workspace: `.git` for the main worktree,
    /// `.git/worktrees/<name>` for the others.
    pub fn git_dir(&self, workspace: &Workspace) -> Result<PathBuf> {
        let dir = self
            .git
            .at(&workspace.path)
            .run(&["rev-parse", "--absolute-git-dir"])?;
        Ok(fs::canonicalize(dir)?)
    }

    /// True when every one of `paths`, which are inside the workspace, is
    /// ignored by Git and therefore cannot change a snapshot.
    pub fn all_ignored(&self, workspace: &Workspace, paths: &[PathBuf]) -> Result<bool> {
        let mut input = Vec::new();
        for path in paths {
            input.extend_from_slice(path.to_string_lossy().as_bytes());
            input.push(0);
        }
        let output = self
            .git
            .at(&workspace.path)
            .output(&["check-ignore", "--stdin", "-z"], Some(&input))?;
        // Prints the paths that are ignored; exits with 1 when none is.
        Ok(git::split_nul(&output.stdout).len() == paths.len())
    }

    /// Finds a workspace by name, by id, or by a prefix of its id.
    pub fn workspace(&self, selector: &str) -> Result<Workspace> {
        let active = self.workspaces()?;
        active
            .iter()
            .find(|ws| ws.name == selector || ws.id == selector)
            .or_else(|| {
                let mut matches = active.iter().filter(|ws| ws.id.starts_with(selector));
                let first = matches.next();
                first.filter(|_| matches.next().is_none())
            })
            .cloned()
            .ok_or_else(|| Error::WorkspaceNotFound(selector.to_string()))
    }

    /// The workspace whose worktree contains `dir`.
    pub fn workspace_containing(&self, dir: &Path) -> Result<Workspace> {
        let dir = fs::canonicalize(dir)?;
        self.workspaces()?
            .into_iter()
            .filter(|ws| dir.starts_with(&ws.path))
            .max_by_key(|ws| ws.path.components().count())
            .ok_or_else(|| Error::WorkspaceNotFound(dir.display().to_string()))
    }

    pub(crate) fn worktree_of(&self, workspace: &Workspace) -> Result<Worktree> {
        self.worktrees()?
            .into_iter()
            .find(|worktree| worktree.path == workspace.path)
            .ok_or_else(|| Error::WorkspaceNotFound(workspace.name.clone()))
    }

    pub(crate) fn temp_path(&self, prefix: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        self.state_dir.join("tmp").join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }
}

fn common_dir(dir: &Path) -> Result<PathBuf> {
    let output = Git::new(dir).output(
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        None,
    );
    match output {
        Ok(output) if output.success() => Ok(fs::canonicalize(output.text())?),
        _ => Err(Error::NotARepository(dir.to_path_buf())),
    }
}

fn list_worktrees(git: &Git) -> Result<Vec<Worktree>> {
    let output = git.output(&["worktree", "list", "--porcelain", "-z"], None)?;
    if !output.success() {
        return Err(Error::Git {
            args: "worktree list".to_string(),
            stderr: output.stderr,
        });
    }

    let mut worktrees = Vec::new();
    let text = String::from_utf8_lossy(&output.stdout);
    // Fields end with NUL and records with an empty field; each field is
    // `key` or `key value`.
    for record in text.split("\0\0") {
        let record: Vec<&str> = record.split('\0').collect();
        let value = |key: &str| {
            record
                .iter()
                .find_map(|field| field.strip_prefix(key)?.strip_prefix(' '))
                .map(str::to_string)
        };
        let has = |key: &str| {
            record
                .iter()
                .any(|field| *field == key || field.starts_with(&format!("{key} ")))
        };
        let Some(path) = value("worktree") else {
            continue;
        };
        if has("bare") || has("prunable") {
            continue;
        }
        // A worktree whose directory was deleted by hand stays listed until
        // `git worktree prune`; it cannot be canonicalized.
        let Ok(path) = fs::canonicalize(path) else {
            continue;
        };
        if path.to_str().is_none() {
            return Err(Error::Refused(format!(
                "worktree path is not valid UTF-8: {}",
                path.display()
            )));
        }
        worktrees.push(Worktree {
            path,
            head: value("HEAD").filter(|head| head.bytes().any(|byte| byte != b'0')),
            branch: value("branch"),
        });
    }
    Ok(worktrees)
}

/// `refs/heads/main` → `main`.
pub fn short_branch(full: &str) -> &str {
    full.strip_prefix("refs/heads/").unwrap_or(full)
}

pub(crate) fn validate_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && !name.contains("..")
        && !name.ends_with('.')
        && !name.ends_with(".lock");
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidName(name.to_string()))
    }
}

/// Turns any text into a valid workspace name.
pub(crate) fn sanitize_name(text: &str) -> String {
    let mut name = String::new();
    for c in text.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
            name.push(c);
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    let name = name.trim_matches('-');
    let name: String = name.chars().take(48).collect();
    let name = name.trim_matches('-').to_string();
    if name.is_empty() {
        "workspace".to_string()
    } else {
        name
    }
}

pub(crate) fn unique_name(wanted: &str, taken: &[Workspace]) -> String {
    let free = |name: &str| !taken.iter().any(|ws| ws.name == name);
    if free(wanted) {
        return wanted.to_string();
    }
    (2..)
        .map(|n| format!("{wanted}-{n}"))
        .find(|name| free(name))
        .expect("an unused number exists")
}

/// Ends the process on the spot when the environment asks for it. The tests
/// use this to interrupt an operation at a chosen step. `exit` runs no
/// destructors, so nothing is cleaned up, exactly as after a crash.
pub(crate) fn failpoint(name: &str) {
    if std::env::var("TSUGI_FAILPOINT").is_ok_and(|wanted| wanted == name) {
        std::process::exit(70);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(validate_name("login-fix_2").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("-x").is_err());
        assert!(validate_name("a..b").is_err());
        assert!(validate_name("Upper").is_err());
        assert!(validate_name("x.lock").is_err());
        assert_eq!(sanitize_name("My Repo (copy)"), "my-repo-copy");
        assert_eq!(sanitize_name("日本語"), "workspace");
        assert!(validate_name(&sanitize_name("--Weird  name..")).is_ok());
    }
}

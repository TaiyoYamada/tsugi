//! Creating, removing and syncing workspaces, and reporting their state.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::fscopy::clone_path;
use crate::model::{OpDetail, Origin, Removal, Snapshot, Task, Trigger, Workspace};
use crate::oplog::Op;
use crate::publish::DiffStat;
use crate::repo::{Repo, RepoLock, failpoint, short_branch, validate_name};
use crate::session::{self, Session};
use crate::store::now;

/// Prefix of the branches tsugi creates for workspaces.
const BRANCH_PREFIX: &str = "tsugi/";

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceStatus {
    #[serde(flatten)]
    pub workspace: Workspace,
    pub task: Option<Task>,
    /// Full name of the checked-out branch, or `None` when detached.
    pub branch: Option<String>,
    pub head: Option<String>,
    /// True for the main worktree of the repository.
    pub is_main: bool,
    pub latest_snapshot: Option<Snapshot>,
    /// Size of the difference between the base branch and the latest
    /// snapshot. `None` when there is no snapshot or no base branch.
    pub changes: Option<DiffStat>,
    pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncOutcome {
    pub workspace: Workspace,
    /// Commit the workspace now sits on.
    pub onto: String,
    /// The state of the workspace just before the sync.
    pub previous: Snapshot,
}

impl Repo {
    /// Creates a workspace: a worktree next to the repository on a new branch
    /// that starts at the base branch.
    pub fn workspace_new(
        &self,
        name: &str,
        task: Option<&Task>,
        base: Option<&str>,
    ) -> Result<Workspace> {
        validate_name(name)?;
        let lock = self.lock()?;
        self.settle(&lock)?;

        if self.workspaces()?.iter().any(|ws| ws.name == name) {
            return Err(Error::NameTaken(name.to_string()));
        }
        let base = match base {
            Some(base) => base.to_string(),
            None => self.default_base()?,
        };
        if self.git.commit(&format!("refs/heads/{base}"))?.is_none() {
            return Err(Error::Refused(format!(
                "the base branch `{base}` does not exist"
            )));
        }
        let branch = format!("{BRANCH_PREFIX}{name}");
        if self.git.commit(&format!("refs/heads/{branch}"))?.is_some() {
            return Err(Error::Refused(format!(
                "the branch `{branch}` already exists"
            )));
        }
        let path = self.workspaces_dir().join(name);
        if path.exists() {
            return Err(Error::Refused(format!("{} already exists", path.display())));
        }
        let sources = self.copy_sources()?;

        let workspace = Workspace {
            id: self.store.new_id()?,
            name: name.to_string(),
            task_id: task.map(|task| task.id.clone()),
            path: path.clone(),
            base: base.clone(),
            origin: Origin::Created,
            created_at: now(),
            removed_at: None,
        };
        let detail = OpDetail::WsNew {
            workspace_id: workspace.id.clone(),
            name: name.to_string(),
            path: path.clone(),
            branch: branch.clone(),
        };
        self.with_op(&lock, detail, |_| {
            std::fs::create_dir_all(self.workspaces_dir())?;
            self.git.run(&[
                "worktree",
                "add",
                "--quiet",
                "-b",
                &branch,
                &path.to_string_lossy(),
                &base,
            ])?;
            failpoint("ws_new.after-worktree");
            for relative in &sources {
                let to = path.join(relative);
                if !to.exists() {
                    clone_path(&self.main_root.join(relative), &to)?;
                }
            }
            failpoint("ws_new.after-copy");
            // Last step: recovery treats a workspace without a row as
            // half-created and removes it.
            self.store.insert_workspace(&workspace)?;
            Ok(workspace.clone())
        })
    }

    /// Directory that holds the worktrees tsugi creates: next to the
    /// repository, so tools working in the main worktree do not descend into
    /// them, and on the same volume, so files can be cloned.
    fn workspaces_dir(&self) -> PathBuf {
        let name = self
            .main_root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "repo".to_string());
        self.main_root
            .parent()
            .unwrap_or(&self.main_root)
            .join(format!("{name}.tsugi"))
    }

    /// Paths from `workspace.copy` that exist in the main worktree.
    fn copy_sources(&self) -> Result<Vec<PathBuf>> {
        let mut sources = Vec::new();
        for entry in &self.config.workspace.copy {
            let relative = Path::new(entry);
            let inside = relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)));
            if !inside {
                return Err(Error::Config(format!(
                    "workspace.copy: `{entry}` must be a path inside the repository"
                )));
            }
            if self.main_root.join(relative).symlink_metadata().is_ok() {
                sources.push(relative.to_path_buf());
            }
        }
        Ok(sources)
    }

    /// Removes a workspace. Its content is kept as a snapshot until the
    /// snapshot expires.
    pub fn workspace_remove(&self, workspace: &Workspace) -> Result<Removal> {
        let lock = self.lock()?;
        self.settle(&lock)?;
        if workspace.path == self.main_root {
            return Err(Error::Refused(
                "the main worktree cannot be removed".to_string(),
            ));
        }
        let detail = OpDetail::WsRemove(self.removal_of(workspace)?);
        self.with_op(&lock, detail, |op| {
            self.remove_steps(&lock, workspace, Some(op))
        })
    }

    fn removal_of(&self, workspace: &Workspace) -> Result<Removal> {
        let worktree = self.worktree_of(workspace)?;
        Ok(Removal {
            workspace_id: workspace.id.clone(),
            name: workspace.name.clone(),
            path: workspace.path.clone(),
            branch: worktree.branch,
            head: worktree.head,
            snapshot: None,
            branch_deleted: false,
        })
    }

    /// Snapshot, then remove. `op` is the operation to record progress in,
    /// if the removal is an operation of its own.
    pub(crate) fn remove_steps(
        &self,
        lock: &RepoLock,
        workspace: &Workspace,
        op: Option<&Op>,
    ) -> Result<Removal> {
        let mut removal = self.removal_of(workspace)?;
        let snapshot = self
            .snapshot_locked(lock, workspace, Trigger::PreRemove)?
            .snapshot;
        removal.snapshot = Some(snapshot.id);
        if let Some(op) = op {
            op.record(self, OpDetail::WsRemove(removal.clone()))?;
        }
        failpoint("ws_remove.after-snapshot");
        let removal = self.finish_remove(lock, removal)?;
        if let Some(op) = op {
            op.record(self, OpDetail::WsRemove(removal.clone()))?;
        }
        Ok(removal)
    }

    /// Everything after the snapshot. Safe to run again after a crash.
    pub(crate) fn finish_remove(&self, _lock: &RepoLock, mut removal: Removal) -> Result<Removal> {
        if removal.path.exists() {
            self.git.run(&[
                "worktree",
                "remove",
                "--force",
                &removal.path.to_string_lossy(),
            ])?;
        }
        failpoint("ws_remove.after-worktree");
        self.store
            .set_workspace_removed(&removal.workspace_id, Some(now()))?;

        // A branch tsugi created is deleted only when nothing would be lost:
        // when every commit on it is on the base branch already.
        let workspace = self.store.workspace(&removal.workspace_id)?;
        if let (Some(branch), Some(workspace)) = (&removal.branch, workspace) {
            let short = short_branch(branch);
            if short.starts_with(BRANCH_PREFIX) && self.git.commit(branch)?.is_some() {
                let merged = self
                    .git
                    .output(
                        &[
                            "merge-base",
                            "--is-ancestor",
                            branch,
                            &format!("refs/heads/{}", workspace.base),
                        ],
                        None,
                    )?
                    .success();
                if merged {
                    self.git.run(&["branch", "-D", short])?;
                    removal.branch_deleted = true;
                } else {
                    self.notice(format!(
                        "kept the branch `{short}`: it has commits that are not on `{}`",
                        workspace.base
                    ));
                }
            }
        }
        Ok(removal)
    }

    /// Brings back a removed workspace with the content of its last snapshot.
    pub(crate) fn recreate_workspace(&self, removal: &Removal) -> Result<()> {
        if removal.path.exists() {
            return Err(Error::Refused(format!(
                "{} exists again",
                removal.path.display()
            )));
        }
        if self.workspaces()?.iter().any(|ws| ws.name == removal.name) {
            return Err(Error::NameTaken(removal.name.clone()));
        }
        let head = removal.head.as_deref().ok_or_else(|| {
            Error::Refused("the workspace had no commit to recreate it from".to_string())
        })?;
        let path = removal.path.to_string_lossy();
        match &removal.branch {
            Some(branch) => {
                let short = short_branch(branch);
                if self.git.commit(branch)?.is_none() {
                    self.git.run(&["branch", short, head])?;
                }
                self.git
                    .run(&["worktree", "add", "--quiet", &path, short])?;
            }
            None => {
                self.git
                    .run(&["worktree", "add", "--quiet", "--detach", &path, head])?;
            }
        }
        self.store
            .set_workspace_removed(&removal.workspace_id, None)?;
        if let Some(snapshot) = &removal.snapshot {
            let workspace = self
                .store
                .workspace(&removal.workspace_id)?
                .expect("the workspace row is never deleted");
            let snapshot = self.snapshot_by_id(snapshot)?;
            self.apply_tree(&workspace, &snapshot.tree)?;
        }
        Ok(())
    }

    /// Merges the current base branch into a workspace and moves the
    /// workspace on top of it.
    pub fn workspace_sync(&self, workspace: &Workspace) -> Result<SyncOutcome> {
        let lock = self.lock()?;
        self.settle(&lock)?;
        let base_ref = format!("refs/heads/{}", workspace.base);
        let onto = self.git.commit(&base_ref)?.ok_or_else(|| {
            Error::Refused(format!(
                "the base branch `{}` does not exist",
                workspace.base
            ))
        })?;
        if self.worktree_of(workspace)?.branch.as_deref() == Some(base_ref.as_str()) {
            return Err(Error::Refused(format!(
                "`{}` is on its base branch `{}`; there is nothing to sync with",
                workspace.name, workspace.base
            )));
        }
        let detail = OpDetail::Sync {
            workspace_id: workspace.id.clone(),
            pre: None,
            onto: onto.clone(),
        };
        let previous = self.with_op(&lock, detail, |op| {
            self.sync_steps(&lock, workspace, &onto, Some(op))
        })?;
        Ok(SyncOutcome {
            workspace: workspace.clone(),
            onto,
            previous,
        })
    }

    /// Snapshot, merge `onto` into the snapshot, write the result to the
    /// workspace and move its branch to `onto`. Returns the snapshot.
    pub(crate) fn sync_steps(
        &self,
        lock: &RepoLock,
        workspace: &Workspace,
        onto: &str,
        op: Option<&Op>,
    ) -> Result<Snapshot> {
        let previous = self
            .snapshot_locked(lock, workspace, Trigger::PreSync)?
            .snapshot;
        if let Some(op) = op {
            op.record(
                self,
                OpDetail::Sync {
                    workspace_id: workspace.id.clone(),
                    pre: Some(previous.id.clone()),
                    onto: onto.to_string(),
                },
            )?;
        }
        let merged = self.merge_trees(onto, &previous.id)?;
        failpoint("sync.after-merge");
        self.apply_tree(workspace, &merged)?;
        failpoint("sync.after-files");
        // A mixed reset: the branch and the index move, the files stay.
        self.git
            .at(&workspace.path)
            .run(&["reset", "--quiet", onto])?;
        Ok(previous)
    }

    /// State of every workspace. Takes a snapshot of each workspace first
    /// unless another process is busy with the repository.
    pub fn status(&self, home: Option<&Path>) -> Result<Vec<WorkspaceStatus>> {
        let workspaces = match self.lock_briefly()? {
            Some(lock) => {
                self.settle(&lock)?;
                let workspaces = self.workspaces()?;
                for workspace in &workspaces {
                    self.snapshot_locked(&lock, workspace, Trigger::Auto)?;
                }
                workspaces
            }
            None => self.workspaces()?,
        };
        let worktrees = self.worktrees()?;
        let tasks = self.store.tasks()?;

        let mut statuses = Vec::new();
        for workspace in workspaces {
            let worktree = worktrees.iter().find(|wt| wt.path == workspace.path);
            let latest_snapshot = self.latest_snapshot(&workspace)?;
            let changes = match &latest_snapshot {
                Some(snapshot) => self.changes_since_base(&workspace, snapshot)?,
                None => None,
            };
            statuses.push(WorkspaceStatus {
                task: tasks
                    .iter()
                    .find(|task| Some(&task.id) == workspace.task_id.as_ref())
                    .cloned(),
                branch: worktree.and_then(|wt| wt.branch.clone()),
                head: worktree.and_then(|wt| wt.head.clone()),
                is_main: workspace.path == self.main_root,
                latest_snapshot,
                changes,
                sessions: match home {
                    Some(home) => session::sessions(&workspace.path, home),
                    None => Vec::new(),
                },
                workspace,
            });
        }
        Ok(statuses)
    }
}

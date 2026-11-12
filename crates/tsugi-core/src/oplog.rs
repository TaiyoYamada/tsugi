//! The operation log: every change tsugi makes is recorded before it starts,
//! so that an interrupted change is found and a finished one can be undone.

use std::fs::{self, File, TryLockError};
use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::model::{OpDetail, OpState, Operation, Trigger};
use crate::repo::{Repo, RepoLock};

/// An operation in progress. The file lock it holds tells other processes
/// that the operation is alive; the operating system releases it if this
/// process dies.
pub(crate) struct Op {
    pub id: i64,
    lock_path: PathBuf,
    _file: File,
}

impl Op {
    /// Replaces the recorded facts of the operation.
    pub fn record(&self, repo: &Repo, detail: OpDetail) -> Result<()> {
        repo.store.update_operation_detail(self.id, &detail)
    }

    pub fn done(self, repo: &Repo) -> Result<()> {
        self.done_with_note(repo, None)
    }

    /// Finishes the operation; `note` says what is left for the user to do.
    pub fn done_with_note(self, repo: &Repo, note: Option<&str>) -> Result<()> {
        repo.store.finish_operation(self.id, OpState::Done, note)?;
        let _ = fs::remove_file(&self.lock_path);
        Ok(())
    }
}

impl Repo {
    pub(crate) fn begin(&self, _lock: &RepoLock, detail: OpDetail) -> Result<Op> {
        let id = self.store.insert_operation(&detail)?;
        let lock_path = self.op_lock_path(id);
        fs::create_dir_all(lock_path.parent().expect("inside the state directory"))?;
        let file = File::create(&lock_path)?;
        file.lock()?;
        Ok(Op {
            id,
            lock_path,
            _file: file,
        })
    }

    /// Runs `work` as one operation. If it fails, the operation is resolved
    /// the same way an operation interrupted by a crash would be.
    pub(crate) fn with_op<T>(
        &self,
        lock: &RepoLock,
        detail: OpDetail,
        work: impl FnOnce(&Op) -> Result<T>,
    ) -> Result<T> {
        let op = self.begin(lock, detail)?;
        match work(&op) {
            Ok(value) => {
                op.done(self)?;
                Ok(value)
            }
            Err(error) => {
                self.abandon(lock, op, &error)?;
                Err(error)
            }
        }
    }

    /// Resolves an operation that failed with `error`.
    pub(crate) fn abandon(&self, lock: &RepoLock, op: Op, error: &Error) -> Result<()> {
        let id = op.id;
        let _ = fs::remove_file(&op.lock_path);
        drop(op);
        let operation = self
            .store
            .operation(id)?
            .expect("the operation was just inserted");
        self.resolve_unfinished(lock, &operation, &error.to_string())?;
        Ok(())
    }

    fn op_lock_path(&self, id: i64) -> PathBuf {
        self.state_dir.join("ops").join(format!("{id}.lock"))
    }

    /// Resolves operations whose process died before finishing them.
    pub(crate) fn recover(&self, lock: &RepoLock) -> Result<()> {
        for operation in self.store.started_operations()? {
            let path = self.op_lock_path(operation.id);
            let alive = File::open(&path)
                .is_ok_and(|file| matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
            if alive {
                continue;
            }
            let _ = fs::remove_file(path);
            let outcome =
                self.resolve_unfinished(lock, &operation, "the process was interrupted")?;
            self.notice(format!(
                "operation #{} ({}) did not finish: {outcome}",
                operation.id,
                operation.detail.kind()
            ));
        }
        Ok(())
    }

    /// Brings an unfinished operation to `done` or `failed` and returns a
    /// sentence that says what state things are in.
    fn resolve_unfinished(
        &self,
        lock: &RepoLock,
        operation: &Operation,
        reason: &str,
    ) -> Result<String> {
        let (state, note) = match &operation.detail {
            OpDetail::TaskNew { task_id } => {
                if self.store.tasks()?.iter().any(|task| &task.id == task_id) {
                    (OpState::Done, "the task was created".to_string())
                } else {
                    (OpState::Failed, format!("{reason}; nothing was changed"))
                }
            }
            OpDetail::WsNew {
                workspace_id,
                path,
                branch,
                ..
            } => {
                if self.store.workspace(workspace_id)?.is_some() {
                    (OpState::Done, "the workspace was created".to_string())
                } else {
                    // The directory and the branch did not exist before the
                    // operation, so everything found there is its own.
                    let path_text = path.to_string_lossy();
                    let _ = self
                        .git
                        .output(&["worktree", "remove", "--force", &path_text], None)?;
                    if path.exists() {
                        fs::remove_dir_all(path)?;
                    }
                    self.git.run(&["worktree", "prune"])?;
                    let _ = self.git.output(&["branch", "-D", branch], None)?;
                    (
                        OpState::Failed,
                        format!("{reason}; the half-created workspace was removed"),
                    )
                }
            }
            OpDetail::WsRemove(removal) => {
                if removal.snapshot.is_some() {
                    let removal = self.finish_remove(lock, removal.clone())?;
                    self.store
                        .update_operation_detail(operation.id, &OpDetail::WsRemove(removal))?;
                    (OpState::Done, "the removal was completed".to_string())
                } else {
                    (OpState::Failed, format!("{reason}; nothing was changed"))
                }
            }
            OpDetail::Restore { pre, .. } | OpDetail::Sync { pre, .. } => {
                let note = match pre {
                    Some(pre) => {
                        let snapshot = self.snapshot_by_id(pre)?;
                        format!(
                            "{reason}; files may be partly updated, the state before is snapshot {} of the workspace",
                            snapshot.seq
                        )
                    }
                    None => format!("{reason}; nothing was changed"),
                };
                (OpState::Failed, note)
            }
            OpDetail::Publish(publication) => {
                self.remove_verify_worktree(operation.id)?;
                let tip = self
                    .git
                    .commit(&format!("refs/heads/{}", publication.base))?;
                if tip.as_deref() == Some(publication.commit.as_str()) {
                    (
                        OpState::Done,
                        format!(
                            "`{}` was updated; run `tsugi ws sync` in the workspace to move it onto the published commit",
                            publication.base
                        ),
                    )
                } else {
                    (
                        OpState::Failed,
                        format!("{reason}; `{}` was not changed", publication.base),
                    )
                }
            }
            OpDetail::Undo { of } => (
                OpState::Failed,
                format!("{reason}; operation #{of} may be partly undone"),
            ),
        };
        self.store
            .finish_operation(operation.id, state, Some(&note))?;
        Ok(note)
    }

    /// Operations, newest first.
    pub fn operations(&self, limit: u32) -> Result<Vec<Operation>> {
        self.try_settle()?;
        self.store.operations(limit)
    }

    /// Reverts the most recent finished operation and returns it.
    pub fn undo(&self) -> Result<Operation> {
        let lock = self.lock()?;
        self.settle(&lock)?;
        let target = self
            .store
            .last_undoable_operation()?
            .ok_or(Error::NothingToUndo)?;

        self.with_op(&lock, OpDetail::Undo { of: target.id }, |_| {
            match &target.detail {
                OpDetail::TaskNew { task_id } => {
                    let used = self
                        .store
                        .workspaces()?
                        .iter()
                        .any(|ws| ws.task_id.as_ref() == Some(task_id));
                    if used {
                        return Err(Error::Refused(
                            "the task has workspaces; remove them first".to_string(),
                        ));
                    }
                    self.store.delete_task(task_id)?;
                }
                OpDetail::WsNew { workspace_id, .. } => {
                    let workspace = self.active_workspace(workspace_id)?;
                    self.remove_steps(&lock, &workspace, None)?;
                }
                OpDetail::WsRemove(removal) => self.recreate_workspace(removal)?,
                OpDetail::Restore {
                    workspace_id, pre, ..
                }
                | OpDetail::Sync {
                    workspace_id, pre, ..
                } => {
                    let pre = pre.as_ref().expect("a finished operation has its snapshot");
                    self.restore_for_undo(&lock, workspace_id, pre)?;
                }
                OpDetail::Publish(publication) => {
                    if publication.base_moved {
                        self.move_base_back(publication)?;
                    }
                    // The workspace may have been removed since; the base
                    // branch is what matters.
                    if self.active_workspace(&publication.workspace_id).is_ok() {
                        self.restore_for_undo(&lock, &publication.workspace_id, &publication.pre)?;
                    }
                }
                OpDetail::Undo { .. } => unreachable!("undo operations are never selected"),
            }
            self.store.set_operation_state(target.id, OpState::Undone)
        })?;
        Ok(target)
    }

    fn restore_for_undo(&self, lock: &RepoLock, workspace_id: &str, snapshot: &str) -> Result<()> {
        let workspace = self.active_workspace(workspace_id)?;
        let snapshot = self.snapshot_by_id(snapshot)?;
        self.snapshot_locked(lock, &workspace, Trigger::PreUndo)?;
        self.restore_content(&workspace, &snapshot)?;
        Ok(())
    }

    fn active_workspace(&self, id: &str) -> Result<crate::model::Workspace> {
        self.store
            .workspace(id)?
            .filter(|workspace| workspace.removed_at.is_none())
            .ok_or_else(|| Error::Refused("the workspace no longer exists".to_string()))
    }
}

//! Diffs, merges in memory, conflict detection and publishing.

use std::fs::{self, File};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::fscopy::clone_path;
use crate::git::split_nul;
use crate::model::{OpDetail, Publication, Snapshot, Trigger, Verification, Workspace};
use crate::repo::{Repo, failpoint};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DiffStat {
    pub files: u32,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileChange {
    pub path: String,
    /// `None` for binary files.
    pub insertions: Option<u32>,
    pub deletions: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diff {
    /// Commit or tree the diff starts from.
    pub from: String,
    pub to: String,
    pub stat: DiffStat,
    pub files: Vec<FileChange>,
    /// Unified diff.
    pub patch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    /// Name of a workspace.
    pub workspace: String,
    /// Name of the other workspace, or of the base branch.
    pub other: String,
    /// True when `other` is the base branch of `workspace`.
    pub other_is_base: bool,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishOutcome {
    pub workspace: Workspace,
    pub base: String,
    /// The commit now at the tip of the base branch.
    pub commit: String,
    pub stat: DiffStat,
    pub verifications: Vec<Verification>,
    /// False when the workspace could not be moved onto the new commit
    /// because it changed during verification and now conflicts.
    pub synced: bool,
}

impl Repo {
    /// What a workspace changed relative to its base branch, as of its
    /// latest snapshot.
    pub fn diff_workspace(&self, workspace: &Workspace) -> Result<Diff> {
        let snapshot = self.snapshot(workspace, Trigger::Auto)?.snapshot;
        let base = format!("refs/heads/{}", workspace.base);
        let from = self.git.run(&["merge-base", &base, &snapshot.id])?;
        self.diff(&from, &snapshot.id)
    }

    /// The difference between two commits or trees.
    pub fn diff(&self, from: &str, to: &str) -> Result<Diff> {
        let files = self.file_changes(from, to)?;
        let patch = self.git.run(&[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-renames",
            from,
            to,
        ])?;
        Ok(Diff {
            from: from.to_string(),
            to: to.to_string(),
            stat: stat_of(&files),
            files,
            patch,
        })
    }

    fn file_changes(&self, from: &str, to: &str) -> Result<Vec<FileChange>> {
        let output = self
            .git
            .output(&["diff", "--numstat", "-z", "--no-renames", from, to], None)?;
        if !output.success() {
            return Err(Error::Git {
                args: format!("diff --numstat {from} {to}"),
                stderr: output.stderr,
            });
        }
        Ok(split_nul(&output.stdout)
            .into_iter()
            .filter_map(|record| {
                let record = String::from_utf8_lossy(record);
                let mut fields = record.splitn(3, '\t');
                let insertions = fields.next()?.parse().ok();
                let deletions = fields.next()?.parse().ok();
                Some(FileChange {
                    path: fields.next()?.to_string(),
                    insertions,
                    deletions,
                })
            })
            .collect())
    }

    pub(crate) fn changes_since_base(
        &self,
        workspace: &Workspace,
        snapshot: &Snapshot,
    ) -> Result<Option<DiffStat>> {
        let base = format!("refs/heads/{}", workspace.base);
        let merge_base = self
            .git
            .output(&["merge-base", &base, &snapshot.id], None)?;
        if !merge_base.success() {
            return Ok(None);
        }
        let files = self.file_changes(&merge_base.text(), &snapshot.id)?;
        Ok(Some(stat_of(&files)))
    }

    /// Merges two commits without touching any worktree and returns the
    /// resulting tree.
    pub(crate) fn merge_trees(&self, ours: &str, theirs: &str) -> Result<String> {
        let output = self.git.output(
            &[
                "merge-tree",
                "--write-tree",
                "--name-only",
                "--no-messages",
                "-z",
                ours,
                theirs,
            ],
            None,
        )?;
        let mut fields = split_nul(&output.stdout)
            .into_iter()
            .map(|field| String::from_utf8_lossy(field).to_string());
        match output.code {
            Some(0) => Ok(fields.next().expect("merge-tree prints the tree")),
            // 1 means the merge has conflicts: the tree, then the files.
            Some(1) => Err(Error::Conflict {
                files: fields.skip(1).collect(),
            }),
            _ => Err(Error::Git {
                args: format!("merge-tree {ours} {theirs}"),
                stderr: output.stderr,
            }),
        }
    }

    /// Files that would conflict if two workspaces, or a workspace and its
    /// base branch, were merged now. Nothing is written to any worktree.
    pub fn conflicts(&self) -> Result<Vec<Conflict>> {
        let statuses = self.status(None)?;
        let mut conflicts = Vec::new();

        for (index, status) in statuses.iter().enumerate() {
            let Some(snapshot) = &status.latest_snapshot else {
                continue;
            };
            let base_ref = format!("refs/heads/{}", status.workspace.base);
            if status.branch.as_deref() != Some(base_ref.as_str())
                && let Err(Error::Conflict { files }) = self.merge_trees(&base_ref, &snapshot.id)
            {
                conflicts.push(Conflict {
                    workspace: status.workspace.name.clone(),
                    other: status.workspace.base.clone(),
                    other_is_base: true,
                    files,
                });
            }
            for other in &statuses[index + 1..] {
                let Some(other_snapshot) = &other.latest_snapshot else {
                    continue;
                };
                // Any other failure (for example unrelated histories) means
                // the two cannot be compared, not that they conflict.
                if let Err(Error::Conflict { files }) =
                    self.merge_trees(&snapshot.id, &other_snapshot.id)
                {
                    conflicts.push(Conflict {
                        workspace: status.workspace.name.clone(),
                        other: other.workspace.name.clone(),
                        other_is_base: false,
                        files,
                    });
                }
            }
        }
        Ok(conflicts)
    }

    /// Puts the content of a workspace on its base branch as one commit.
    pub fn publish(
        &self,
        workspace: &Workspace,
        message: Option<&str>,
        verify: bool,
    ) -> Result<PublishOutcome> {
        // Step 1, under the lock: snapshot, merge and build the commit.
        let lock = self.lock()?;
        self.settle(&lock)?;
        let base = workspace.base.clone();
        let base_ref = format!("refs/heads/{base}");
        let old_tip = self
            .git
            .commit(&base_ref)?
            .ok_or_else(|| Error::Refused(format!("the base branch `{base}` does not exist")))?;
        if self.worktree_of(workspace)?.branch.as_deref() == Some(base_ref.as_str()) {
            return Err(Error::Refused(format!(
                "`{}` is on its base branch `{base}`; commit there with git",
                workspace.name
            )));
        }
        let pre = self
            .snapshot_locked(&lock, workspace, Trigger::PrePublish)?
            .snapshot;
        let tree = self.merge_trees(&old_tip, &pre.id)?;
        if tree == self.git.tree_of(&old_tip)? {
            return Err(Error::NothingToPublish(base));
        }
        let message = match message {
            Some(message) => message.to_string(),
            None => match &workspace.task_id {
                Some(task_id) => self.task(task_id)?.title,
                None => format!("Publish {}", workspace.name),
            },
        };
        let mut args = vec!["commit-tree", &tree, "-p", &old_tip, "-F", "-"];
        // `git commit` signs when this is set; a published commit should not
        // be the one unsigned commit on the branch.
        let sign = self
            .git
            .output(&["config", "--type=bool", "--get", "commit.gpgsign"], None)?;
        if sign.text() == "true" {
            args.push("-S");
        }
        let commit = self.git.run_with_stdin(&args, Some(message.as_bytes()))?;
        let files = self.file_changes(&old_tip, &commit)?;
        let mut publication = Publication {
            workspace_id: workspace.id.clone(),
            base: base.clone(),
            pre: pre.id.clone(),
            old_tip: old_tip.clone(),
            commit: commit.clone(),
            base_moved: false,
        };
        let op = self.begin(&lock, OpDetail::Publish(publication.clone()))?;
        // Verification can take minutes. Other commands and the watcher must
        // not wait for it.
        drop(lock);

        // Step 2, without the lock: verify the commit.
        failpoint("publish.before-verify");
        let verifications = if verify {
            match self.verify(op.id, &commit, &tree, workspace) {
                Ok(verifications) => verifications,
                Err(error) => {
                    let lock = self.lock()?;
                    self.abandon(&lock, op, &error)?;
                    return Err(error);
                }
            }
        } else {
            Vec::new()
        };

        // Step 3, under the lock again: move the base branch and the
        // workspace.
        let lock = self.lock()?;
        failpoint("publish.before-move");
        if let Err(error) = self.move_base(&base, &old_tip, &commit) {
            self.abandon(&lock, op, &error)?;
            return Err(error);
        }
        publication.base_moved = true;
        op.record(self, OpDetail::Publish(publication))?;
        failpoint("publish.after-move");

        let synced = match self.sync_steps(&lock, workspace, &commit, None) {
            Ok(_) => true,
            Err(Error::Conflict { files }) => {
                self.notice(format!(
                    "`{}` changed during verification and now conflicts with the published commit in: {}",
                    workspace.name,
                    files.join(", ")
                ));
                false
            }
            Err(error) => {
                op.done_with_note(
                    self,
                    Some("the workspace was not moved onto the published commit"),
                )?;
                return Err(error);
            }
        };
        op.done(self)?;

        Ok(PublishOutcome {
            workspace: workspace.clone(),
            base,
            commit,
            stat: stat_of(&files),
            verifications,
            synced,
        })
    }

    /// Runs the commands of `publish.verify` in a temporary worktree checked
    /// out at `commit`. Results are remembered per tree.
    fn verify(
        &self,
        op_id: i64,
        commit: &str,
        tree: &str,
        source: &Workspace,
    ) -> Result<Vec<Verification>> {
        let dir = self.verify_dir(op_id);
        let log = self
            .state_dir
            .join("logs")
            .join(format!("verify-{op_id}.log"));
        let mut results = Vec::new();
        let mut failure = None;

        for command in &self.config.publish.verify {
            if let Some(known) = self.store.verification(tree, command)?
                && known.passed
            {
                results.push(known);
                continue;
            }
            if !dir.exists() {
                self.git.run(&[
                    "worktree",
                    "add",
                    "--quiet",
                    "--detach",
                    &dir.to_string_lossy(),
                    commit,
                ])?;
                // Dependencies come from the workspace being published: they
                // match its code.
                for entry in &self.config.workspace.copy {
                    let from = source.path.join(entry);
                    if from.symlink_metadata().is_ok() && !dir.join(entry).exists() {
                        clone_path(&from, &dir.join(entry))?;
                    }
                }
                fs::create_dir_all(log.parent().expect("inside the state directory"))?;
            }
            let output = File::options().create(true).append(true).open(&log)?;
            let started = Instant::now();
            let status = Command::new("sh")
                .args(["-c", command])
                .current_dir(&dir)
                .stdin(Stdio::null())
                .stdout(output.try_clone()?)
                .stderr(output)
                .status()?;
            let result = Verification {
                command: command.clone(),
                passed: status.success(),
                exit_code: status.code(),
                duration_ms: started.elapsed().as_millis() as i64,
                cached: false,
            };
            self.store.record_verification(tree, &result)?;
            if !result.passed {
                failure = Some(Error::VerifyFailed {
                    command: command.clone(),
                    code: result.exit_code,
                    log: log.clone(),
                });
                break;
            }
            results.push(result);
        }

        self.remove_verify_worktree(op_id)?;
        match failure {
            Some(error) => Err(error),
            None => {
                let _ = fs::remove_file(log);
                Ok(results)
            }
        }
    }

    fn verify_dir(&self, op_id: i64) -> PathBuf {
        self.state_dir.join("tmp").join(format!("verify-{op_id}"))
    }

    pub(crate) fn remove_verify_worktree(&self, op_id: i64) -> Result<()> {
        let dir = self.verify_dir(op_id);
        if dir.exists() {
            let _ = self.git.output(
                &["worktree", "remove", "--force", &dir.to_string_lossy()],
                None,
            )?;
            if dir.exists() {
                fs::remove_dir_all(&dir)?;
            }
            self.git.run(&["worktree", "prune"])?;
        }
        Ok(())
    }

    /// Moves the base branch from `old` to `new`, which is a child of `old`.
    fn move_base(&self, base: &str, old: &str, new: &str) -> Result<()> {
        let base_ref = format!("refs/heads/{base}");
        let checked_out = self
            .worktrees()?
            .into_iter()
            .find(|worktree| worktree.branch.as_deref() == Some(base_ref.as_str()));
        match checked_out {
            // The files of that worktree have to follow the branch. A
            // fast-forward merge does that and refuses to overwrite
            // uncommitted changes.
            Some(worktree) => {
                let git = self.git.at(&worktree.path);
                if git.commit("HEAD")?.as_deref() != Some(old) {
                    return Err(Error::Refused(format!(
                        "`{base}` moved while the publish was being verified; publish again"
                    )));
                }
                git.run(&["merge", "--ff-only", "--quiet", new])?;
            }
            // The old value makes the update fail if the branch moved.
            None => {
                self.git.run(&["update-ref", &base_ref, new, old])?;
            }
        }
        Ok(())
    }

    /// Undoes `move_base` if the base branch still points to the published
    /// commit.
    pub(crate) fn move_base_back(&self, publication: &Publication) -> Result<()> {
        let base_ref = format!("refs/heads/{}", publication.base);
        if self.git.commit(&base_ref)?.as_deref() != Some(publication.commit.as_str()) {
            return Err(Error::Refused(format!(
                "`{}` has moved since the publish; revert the commit with `git revert {}` instead",
                publication.base, publication.commit
            )));
        }
        let checked_out = self
            .worktrees()?
            .into_iter()
            .find(|worktree| worktree.branch.as_deref() == Some(base_ref.as_str()));
        match checked_out {
            // `--keep` moves the branch and the files and refuses to
            // overwrite uncommitted changes.
            Some(worktree) => {
                self.git.at(&worktree.path).run(&[
                    "reset",
                    "--keep",
                    "--quiet",
                    &publication.old_tip,
                ])?;
            }
            None => {
                self.git.run(&[
                    "update-ref",
                    &base_ref,
                    &publication.old_tip,
                    &publication.commit,
                ])?;
            }
        }
        Ok(())
    }
}

fn stat_of(files: &[FileChange]) -> DiffStat {
    DiffStat {
        files: files.len() as u32,
        insertions: files.iter().filter_map(|file| file.insertions).sum(),
        deletions: files.iter().filter_map(|file| file.deletions).sum(),
    }
}

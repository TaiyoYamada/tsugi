//! Snapshots: the content of a workspace at one moment, stored as a commit.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::git::split_nul;
use crate::model::{OpDetail, Snapshot, Trigger, Workspace};
use crate::repo::{Repo, RepoLock, failpoint};
use crate::store::now;

const IDENTITY_NAME: &str = "tsugi";
const IDENTITY_EMAIL: &str = "tsugi@localhost";

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotOutcome {
    pub snapshot: Snapshot,
    /// False when the workspace had not changed since its latest snapshot.
    pub created: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreOutcome {
    pub workspace: Workspace,
    pub restored: Snapshot,
    /// The state of the workspace just before the restore.
    pub previous: Snapshot,
    /// False when the workspace has a different branch checked out than the
    /// snapshot recorded; then only the files were restored.
    pub branch_moved: bool,
}

/// A temporary index file that describes the working tree of a workspace.
pub(crate) struct TempIndex {
    path: PathBuf,
    pub tree: String,
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) fn latest_ref(workspace_id: &str) -> String {
    format!("refs/tsugi/ws/{workspace_id}/latest")
}

pub(crate) fn snapshot_ref(workspace_id: &str, seq: u64) -> String {
    format!("refs/tsugi/ws/{workspace_id}/snap/{seq:08}")
}

impl Repo {
    /// Takes a snapshot of a workspace unless nothing changed.
    pub fn snapshot(&self, workspace: &Workspace, trigger: Trigger) -> Result<SnapshotOutcome> {
        let lock = self.lock_for(workspace)?;
        self.snapshot_locked(&lock, workspace, trigger)
    }

    pub(crate) fn snapshot_locked(
        &self,
        _lock: &RepoLock,
        workspace: &Workspace,
        trigger: Trigger,
    ) -> Result<SnapshotOutcome> {
        let git = self.git.at(&workspace.path);
        let head = git.commit("HEAD")?;
        let branch = git.current_branch()?;
        let tree = self.build_index(workspace)?.tree.clone();

        let latest = self.latest_snapshot(workspace)?;
        if let Some(latest) = latest.as_ref()
            && latest.tree == tree
            && latest.head == head
        {
            return Ok(SnapshotOutcome {
                snapshot: latest.clone(),
                created: false,
            });
        }

        let seq = latest.as_ref().map_or(1, |latest| latest.seq + 1);
        let created_at = now();
        let message = format!(
            "tsugi snapshot\n\n\
             Tsugi-Workspace: {}\n\
             Tsugi-Seq: {seq}\n\
             Tsugi-Trigger: {}\n\
             Tsugi-Head: {}\n\
             Tsugi-Branch: {}\n",
            workspace.id,
            trigger.as_str(),
            head.as_deref().unwrap_or("none"),
            branch.as_deref().unwrap_or("detached"),
        );
        let date = format!("@{created_at} +0000");
        let mut args = vec!["commit-tree", tree.as_str(), "-F", "-"];
        if let Some(head) = head.as_deref() {
            args.extend(["-p", head]);
        }
        let id = git
            .env("GIT_AUTHOR_NAME", IDENTITY_NAME)
            .env("GIT_AUTHOR_EMAIL", IDENTITY_EMAIL)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_NAME", IDENTITY_NAME)
            .env("GIT_COMMITTER_EMAIL", IDENTITY_EMAIL)
            .env("GIT_COMMITTER_DATE", &date)
            .run_with_stdin(&args, Some(message.as_bytes()))?;
        failpoint("snapshot.after-commit");

        // One transaction: either both refs exist afterwards or neither. The
        // expected old value makes a concurrent writer fail instead of
        // reusing the sequence number.
        let expected = match &latest {
            Some(latest) => latest.id.clone(),
            None => "0".repeat(id.len()),
        };
        let transaction = format!(
            "start\ncreate {} {id}\nupdate {} {id} {expected}\nprepare\ncommit\n",
            snapshot_ref(&workspace.id, seq),
            latest_ref(&workspace.id),
        );
        git.run_with_stdin(&["update-ref", "--stdin"], Some(transaction.as_bytes()))?;

        Ok(SnapshotOutcome {
            snapshot: Snapshot {
                id,
                workspace_id: workspace.id.clone(),
                seq,
                tree,
                head,
                branch,
                trigger,
                created_at,
            },
            created: true,
        })
    }

    /// Writes the working tree of a workspace into a temporary index: every
    /// tracked file as it is on disk, plus the untracked files that are not
    /// ignored and not too large. The index of the worktree is not touched.
    pub(crate) fn build_index(&self, workspace: &Workspace) -> Result<TempIndex> {
        let git = self.git.at(&workspace.path);
        let path = self.temp_path("index");
        // Starting from a copy keeps the cached file stats, so git only
        // reads the files that changed.
        let real = git.run(&["rev-parse", "--path-format=absolute", "--git-path", "index"])?;
        if Path::new(&real).exists() {
            fs::copy(&real, &path)?;
        }
        let mut index = TempIndex {
            path,
            tree: String::new(),
        };
        let git = git.env("GIT_INDEX_FILE", &index.path);
        git.run(&["add", "--update"])?;

        // A file can disappear between listing and adding while an agent is
        // working. Listing again resolves it.
        let mut attempts = 0;
        loop {
            let listed = git.output(&["ls-files", "--others", "--exclude-standard", "-z"], None)?;
            let mut wanted = Vec::new();
            for file in split_nul(&listed.stdout) {
                // Git lists an untracked nested repository as `dir/`.
                if file.ends_with(b"/") {
                    continue;
                }
                let on_disk = workspace.path.join(String::from_utf8_lossy(file).as_ref());
                let too_large = fs::symlink_metadata(on_disk).is_ok_and(|meta| {
                    meta.is_file() && meta.len() > self.config.snapshot.max_file_bytes
                });
                if !too_large {
                    wanted.extend_from_slice(file);
                    wanted.push(0);
                }
            }
            if wanted.is_empty() {
                break;
            }
            let added = git.env("GIT_LITERAL_PATHSPECS", "1").output(
                &["add", "--pathspec-from-file=-", "--pathspec-file-nul"],
                Some(&wanted),
            )?;
            if added.success() {
                break;
            }
            attempts += 1;
            if attempts == 5 {
                return Err(Error::Git {
                    args: "add --pathspec-from-file=-".to_string(),
                    stderr: added.stderr,
                });
            }
        }

        index.tree = git.run(&["write-tree"])?;
        Ok(index)
    }

    /// Makes the files of a workspace identical to `tree`. Ignored files are
    /// left alone. The caller has taken a snapshot.
    pub(crate) fn apply_tree(&self, workspace: &Workspace, tree: &str) -> Result<()> {
        let index = self.build_index(workspace)?;
        self.git
            .at(&workspace.path)
            .env("GIT_INDEX_FILE", &index.path)
            .run(&["read-tree", "--reset", "-u", tree])?;
        Ok(())
    }

    /// Snapshots of a workspace, newest first.
    pub fn snapshots(&self, workspace: &Workspace) -> Result<Vec<Snapshot>> {
        let glob = format!("--glob=refs/tsugi/ws/{}/snap/*", workspace.id);
        let mut snapshots = self.read_snapshots(&[&glob])?;
        snapshots.retain(|snapshot| snapshot.workspace_id == workspace.id);
        snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.seq));
        Ok(snapshots)
    }

    pub fn latest_snapshot(&self, workspace: &Workspace) -> Result<Option<Snapshot>> {
        match self.git.commit(&latest_ref(&workspace.id))? {
            Some(id) => Ok(self.read_snapshots(&[&id])?.into_iter().next()),
            None => Ok(None),
        }
    }

    pub(crate) fn snapshot_by_id(&self, id: &str) -> Result<Snapshot> {
        self.read_snapshots(&[id])?
            .into_iter()
            .next()
            .ok_or_else(|| Error::SnapshotNotFound(id.to_string()))
    }

    /// Resolves `<workspace>@<seq>`, `<workspace>@latest` or `<workspace>`.
    /// The workspace part may be empty when `current` is given.
    pub fn find_snapshot(
        &self,
        selector: &str,
        current: Option<&Workspace>,
    ) -> Result<(Workspace, Snapshot)> {
        let not_found = || Error::SnapshotNotFound(selector.to_string());
        let (name, which) = selector.rsplit_once('@').unwrap_or((selector, "latest"));
        let workspace = match (name, current) {
            ("", Some(current)) => current.clone(),
            ("", None) => return Err(not_found()),
            (name, _) => self.workspace(name)?,
        };
        let reference = match which {
            "latest" => latest_ref(&workspace.id),
            seq => snapshot_ref(&workspace.id, seq.parse().map_err(|_| not_found())?),
        };
        let id = self.git.commit(&reference)?.ok_or_else(not_found)?;
        let snapshot = self.snapshot_by_id(&id)?;
        Ok((workspace, snapshot))
    }

    fn read_snapshots(&self, revisions: &[&str]) -> Result<Vec<Snapshot>> {
        let mut args = vec!["log", "--no-walk", "-z", "--format=%H%x1f%T%x1f%ct%x1f%B"];
        args.extend(revisions);
        let output = self.git.run(&args)?;
        Ok(output.split('\0').filter_map(parse_snapshot).collect())
    }

    /// Puts a workspace back to a snapshot.
    pub fn restore(&self, workspace: &Workspace, target: &Snapshot) -> Result<RestoreOutcome> {
        let lock = self.lock_for(workspace)?;
        let detail = OpDetail::Restore {
            workspace_id: workspace.id.clone(),
            target: target.id.clone(),
            pre: None,
        };
        self.with_op(&lock, detail, |op| {
            let previous = self
                .snapshot_locked(&lock, workspace, Trigger::PreRestore)?
                .snapshot;
            op.record(
                self,
                OpDetail::Restore {
                    workspace_id: workspace.id.clone(),
                    target: target.id.clone(),
                    pre: Some(previous.id.clone()),
                },
            )?;
            failpoint("restore.after-snapshot");
            let branch_moved = self.restore_content(workspace, target)?;
            Ok(RestoreOutcome {
                workspace: workspace.clone(),
                restored: target.clone(),
                previous,
                branch_moved,
            })
        })
    }

    /// Restores files, and the branch if the workspace is still on the branch
    /// the snapshot recorded. The caller has taken a snapshot.
    pub(crate) fn restore_content(&self, workspace: &Workspace, target: &Snapshot) -> Result<bool> {
        self.apply_tree(workspace, &target.tree)?;
        failpoint("restore.after-files");
        let git = self.git.at(&workspace.path);
        match &target.head {
            Some(head) if git.current_branch()? == target.branch => {
                // A mixed reset moves the branch and the index and leaves the
                // files as they were just restored.
                git.run(&["reset", "--quiet", head])?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

fn parse_snapshot(record: &str) -> Option<Snapshot> {
    let mut fields = record.trim_start_matches('\n').splitn(4, '\x1f');
    let id = fields.next()?.to_string();
    let tree = fields.next()?.to_string();
    let created_at = fields.next()?.parse().ok()?;
    let body = fields.next()?;
    let trailer = |key: &str| {
        body.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(": "))
    };
    Some(Snapshot {
        id,
        workspace_id: trailer("Tsugi-Workspace")?.to_string(),
        seq: trailer("Tsugi-Seq")?.parse().ok()?,
        tree,
        head: trailer("Tsugi-Head")
            .filter(|head| *head != "none")
            .map(str::to_string),
        branch: trailer("Tsugi-Branch")
            .filter(|branch| *branch != "detached")
            .map(str::to_string),
        trigger: Trigger::parse(trailer("Tsugi-Trigger")?)?,
        created_at,
    })
}

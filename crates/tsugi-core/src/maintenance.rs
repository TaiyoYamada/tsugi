//! Expiring snapshots and removing tsugi from a repository.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use crate::error::Result;
use crate::repo::Repo;
use crate::snapshot::{latest_ref, snapshot_ref};
use crate::store::now;

/// Temporary files live for milliseconds; one this old belongs to no process.
const STALE_TEMP_FILE: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GcOutcome {
    pub deleted: u32,
    pub kept: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct UninstallOutcome {
    pub refs_deleted: u32,
    /// Worktrees that still exist. They are ordinary Git worktrees and are
    /// left for the user to keep or remove.
    pub worktrees: Vec<PathBuf>,
}

impl Repo {
    /// Deletes snapshots older than `snapshot.keep_days`. The latest snapshot
    /// of a workspace that still exists is always kept.
    pub fn gc(&self) -> Result<GcOutcome> {
        let lock = self.lock()?;
        self.settle(&lock)?;
        let cutoff = now() - i64::from(self.config.snapshot.keep_days) * 24 * 60 * 60;

        let mut deletions = String::new();
        let mut outcome = GcOutcome {
            deleted: 0,
            kept: 0,
        };
        for workspace in self.store.workspaces()? {
            let exists = workspace.removed_at.is_none();
            for (index, snapshot) in self.snapshots(&workspace)?.iter().enumerate() {
                let newest = index == 0;
                if snapshot.created_at >= cutoff || (newest && exists) {
                    outcome.kept += 1;
                    continue;
                }
                deletions.push_str(&format!(
                    "delete {}\n",
                    snapshot_ref(&workspace.id, snapshot.seq)
                ));
                // `latest` points to the newest snapshot and goes with it.
                if newest {
                    deletions.push_str(&format!("delete {}\n", latest_ref(&workspace.id)));
                }
                outcome.deleted += 1;
            }
        }
        if !deletions.is_empty() {
            let transaction = format!("start\n{deletions}prepare\ncommit\n");
            self.git
                .run_with_stdin(&["update-ref", "--stdin"], Some(transaction.as_bytes()))?;
        }

        // A process that was killed leaves its temporary index behind.
        for entry in fs::read_dir(self.state_dir.join("tmp"))? {
            let entry = entry?;
            let age = entry.metadata()?.modified()?.elapsed().unwrap_or_default();
            if entry.file_type()?.is_file() && age > STALE_TEMP_FILE {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(outcome)
    }

    /// Removes everything tsugi wrote: its refs and its directory inside the
    /// Git directory. Worktrees and branches are left alone.
    pub fn uninstall(self) -> Result<UninstallOutcome> {
        let lock = self.lock()?;
        let refs = self
            .git
            .run(&["for-each-ref", "--format=delete %(refname)", "refs/tsugi/"])?;
        let refs_deleted = refs.lines().count() as u32;
        if refs_deleted > 0 {
            let transaction = format!("start\n{refs}\nprepare\ncommit\n");
            self.git
                .run_with_stdin(&["update-ref", "--stdin"], Some(transaction.as_bytes()))?;
        }
        let worktrees = self
            .worktrees()?
            .into_iter()
            .skip(1)
            .map(|worktree| worktree.path)
            .collect();
        let state_dir = self.state_dir.clone();
        drop(lock);
        drop(self);
        fs::remove_dir_all(state_dir)?;
        Ok(UninstallOutcome {
            refs_deleted,
            worktrees,
        })
    }
}

//! Expiring snapshots and removing tsugi from a repository.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use crate::error::Result;
use crate::model::Trigger;
use crate::repo::Repo;
use crate::snapshot::{latest_ref, snapshot_ref};
use crate::store::now;

/// Every automatic snapshot of the last hour is kept...
const KEEP_ALL_FOR: i64 = 60 * 60;
/// ...and of the older ones, one per ten minutes. Both in seconds.
const THIN_TO_ONE_PER: i64 = 10 * 60;

/// Temporary files live for milliseconds; one this old belongs to no process.
const STALE_TEMP_FILE: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GcOutcome {
    pub deleted: u32,
    pub kept: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct UninstallOutcome {
    pub refs_deleted: u32,
    /// Worktrees that still exist. They are ordinary Git worktrees and are
    /// left for the user to keep or remove.
    pub worktrees: Vec<PathBuf>,
}

impl Repo {
    /// Deletes snapshots that are no longer needed.
    ///
    /// - Snapshots older than `snapshot.keep_days` are deleted.
    /// - Of the snapshots taken automatically (`watch`, `auto`) more than an
    ///   hour ago, the last one of every ten minutes is kept. An agent at
    ///   work produces a snapshot every few seconds; that detail matters
    ///   for the last hour, not for last week.
    /// - The latest snapshot of a workspace that still exists is always
    ///   kept, and so is every snapshot taken on request or before an
    ///   operation, until it is older than `snapshot.keep_days`.
    pub fn gc(&self) -> Result<GcOutcome> {
        self.gc_at(now())
    }

    /// `gc` as if the current time were `now` (Unix seconds).
    #[doc(hidden)]
    pub fn gc_at(&self, now: i64) -> Result<GcOutcome> {
        let lock = self.lock()?;
        self.settle(&lock)?;
        let cutoff = now - i64::from(self.config.snapshot.keep_days) * 24 * 60 * 60;

        let mut deletions = String::new();
        let mut outcome = GcOutcome {
            deleted: 0,
            kept: 0,
        };
        for workspace in self.store.workspaces()? {
            let exists = workspace.removed_at.is_none();
            // Ten-minute period of the automatic snapshot kept last. The
            // list is newest first, so the one kept is the last of its
            // period.
            let mut kept_period = None;
            for (index, snapshot) in self.snapshots(&workspace)?.iter().enumerate() {
                let newest = index == 0;
                let automatic = matches!(snapshot.trigger, Trigger::Watch | Trigger::Auto);
                let period = snapshot.created_at / THIN_TO_ONE_PER;
                let keep = if newest && exists {
                    true
                } else if snapshot.created_at < cutoff {
                    false
                } else if !automatic || snapshot.created_at >= now - KEEP_ALL_FOR {
                    true
                } else {
                    kept_period != Some(period)
                };
                if keep {
                    if automatic {
                        kept_period = Some(period);
                    }
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

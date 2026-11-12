//! The data tsugi keeps about a repository.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Task {
    pub id: String,
    pub title: String,
    /// Unix seconds.
    pub created_at: i64,
    pub closed_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Created by `tsugi ws new`.
    Created,
    /// A worktree that already existed and was picked up by tsugi.
    Adopted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub task_id: Option<String>,
    pub path: PathBuf,
    /// Short name of the branch this workspace publishes to.
    pub base: String,
    pub origin: Origin,
    pub created_at: i64,
    pub removed_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum Trigger {
    /// Taken by the watcher after files changed.
    Watch,
    /// Asked for with `tsugi snapshot`.
    Manual,
    /// Taken by a command that needed the current content of the workspace.
    Auto,
    PreRestore,
    PreSync,
    PrePublish,
    PreRemove,
    PreUndo,
}

impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Trigger::Watch => "watch",
            Trigger::Manual => "manual",
            Trigger::Auto => "auto",
            Trigger::PreRestore => "pre-restore",
            Trigger::PreSync => "pre-sync",
            Trigger::PrePublish => "pre-publish",
            Trigger::PreRemove => "pre-remove",
            Trigger::PreUndo => "pre-undo",
        }
    }

    pub fn parse(text: &str) -> Option<Trigger> {
        Some(match text {
            "watch" => Trigger::Watch,
            "manual" => Trigger::Manual,
            "auto" => Trigger::Auto,
            "pre-restore" => Trigger::PreRestore,
            "pre-sync" => Trigger::PreSync,
            "pre-publish" => Trigger::PrePublish,
            "pre-remove" => Trigger::PreRemove,
            "pre-undo" => Trigger::PreUndo,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Snapshot {
    /// Id of the commit object that holds the snapshot.
    pub id: String,
    pub workspace_id: String,
    /// Position of the snapshot in its workspace, starting at 1.
    pub seq: u64,
    pub tree: String,
    /// Commit `HEAD` pointed to, if the branch had any commit.
    pub head: Option<String>,
    /// Full name of the checked-out branch, or `None` when detached.
    pub branch: Option<String>,
    pub trigger: Trigger,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum OpState {
    Started,
    Done,
    Failed,
    Undone,
}

impl OpState {
    pub fn as_str(self) -> &'static str {
        match self {
            OpState::Started => "started",
            OpState::Done => "done",
            OpState::Failed => "failed",
            OpState::Undone => "undone",
        }
    }

    pub fn parse(text: &str) -> OpState {
        match text {
            "started" => OpState::Started,
            "done" => OpState::Done,
            "undone" => OpState::Undone,
            _ => OpState::Failed,
        }
    }
}

/// What an operation did, with every fact undo and recovery need.
/// Fields are filled in as the operation progresses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpDetail {
    TaskNew {
        task_id: String,
    },
    WsNew {
        workspace_id: String,
        name: String,
        path: PathBuf,
        /// Short name of the branch created for the workspace.
        branch: String,
    },
    WsRemove(Removal),
    Restore {
        workspace_id: String,
        target: String,
        /// The `pre-restore` snapshot.
        pre: Option<String>,
    },
    Sync {
        workspace_id: String,
        /// The `pre-sync` snapshot.
        pre: Option<String>,
        onto: String,
    },
    Publish(Publication),
    Undo {
        of: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Removal {
    pub workspace_id: String,
    pub name: String,
    pub path: PathBuf,
    /// Full name of the branch that was checked out.
    pub branch: Option<String>,
    pub head: Option<String>,
    /// The `pre-remove` snapshot.
    pub snapshot: Option<String>,
    pub branch_deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Publication {
    pub workspace_id: String,
    /// Short name of the branch that was published to.
    pub base: String,
    /// The `pre-publish` snapshot.
    pub pre: String,
    pub old_tip: String,
    pub commit: String,
    pub base_moved: bool,
}

impl OpDetail {
    pub fn kind(&self) -> &'static str {
        match self {
            OpDetail::TaskNew { .. } => "task_new",
            OpDetail::WsNew { .. } => "ws_new",
            OpDetail::WsRemove(_) => "ws_remove",
            OpDetail::Restore { .. } => "restore",
            OpDetail::Sync { .. } => "sync",
            OpDetail::Publish(_) => "publish",
            OpDetail::Undo { .. } => "undo",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Operation {
    pub id: i64,
    pub state: OpState,
    pub detail: OpDetail,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// Why the operation failed, or what is left for the user to do.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Verification {
    pub command: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
    /// True when the result was reused from an earlier run on the same tree.
    pub cached: bool,
}

//! tsugi: a layer on top of Git for repositories where several coding agents
//! work in parallel.
//!
//! [`Repo`] is the entry point. Open a repository with [`Repo::discover`] (or
//! set tsugi up with [`Repo::init`]) and call its methods; the command line
//! tool and the apps do nothing else. The design is described in
//! `docs/design.md`.

mod config;
mod error;
mod fscopy;
mod git;
mod maintenance;
mod model;
mod oplog;
mod publish;
mod repo;
pub mod session;
mod snapshot;
mod store;
mod workspace;

pub use config::Config;
pub use error::{Error, Result};
pub use maintenance::{GcOutcome, UninstallOutcome};
pub use model::{
    OpDetail, OpState, Operation, Origin, Publication, Removal, Snapshot, Task, Trigger,
    Verification, Workspace,
};
pub use publish::{Conflict, Diff, DiffStat, FileChange, PublishOutcome};
pub use repo::{Repo, Worktree, short_branch};
pub use session::{Session, SessionState};
pub use snapshot::{RestoreOutcome, SnapshotOutcome};
pub use workspace::{SyncOutcome, WorkspaceStatus};

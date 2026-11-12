use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not inside a git repository: {0}")]
    NotARepository(PathBuf),

    #[error("bare repositories are not supported")]
    BareRepository,

    #[error("tsugi is not set up in this repository; run `tsugi init`")]
    NotInitialized,

    #[error("git {required} or newer is required, found {found}")]
    GitTooOld { required: String, found: String },

    #[error("no workspace matches `{0}`")]
    WorkspaceNotFound(String),

    #[error("no snapshot matches `{0}`")]
    SnapshotNotFound(String),

    #[error("no task matches `{0}`")]
    TaskNotFound(String),

    #[error("the name `{0}` is already in use")]
    NameTaken(String),

    #[error(
        "invalid name `{0}`: use lowercase letters, digits, `.`, `_` and `-`, starting with a letter or digit"
    )]
    InvalidName(String),

    #[error("merge conflict in {} file(s): {}", files.len(), files.join(", "))]
    Conflict { files: Vec<String> },

    #[error("verification failed: `{command}` exited with {}", code.map_or("a signal".to_string(), |c| c.to_string()))]
    VerifyFailed {
        command: String,
        code: Option<i32>,
        log: PathBuf,
    },

    #[error("nothing to publish: the workspace has no changes relative to `{0}`")]
    NothingToPublish(String),

    #[error("nothing to undo")]
    NothingToUndo,

    /// A precondition of the command does not hold. Nothing was changed.
    #[error("{0}")]
    Refused(String),

    #[error("`git {args}` failed: {stderr}")]
    Git { args: String, stderr: String },

    #[error("tsugi.toml: {0}")]
    Config(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Stable identifier used in the JSON output of the CLI.
    pub fn code(&self) -> &'static str {
        match self {
            Error::NotARepository(_) => "not_a_repository",
            Error::BareRepository => "bare_repository",
            Error::NotInitialized => "not_initialized",
            Error::GitTooOld { .. } => "git_too_old",
            Error::WorkspaceNotFound(_) => "workspace_not_found",
            Error::SnapshotNotFound(_) => "snapshot_not_found",
            Error::TaskNotFound(_) => "task_not_found",
            Error::NameTaken(_) => "name_taken",
            Error::InvalidName(_) => "invalid_name",
            Error::Conflict { .. } => "conflict",
            Error::VerifyFailed { .. } => "verify_failed",
            Error::NothingToPublish(_) => "nothing_to_publish",
            Error::NothingToUndo => "nothing_to_undo",
            Error::Refused(_) => "refused",
            Error::Git { .. } => "git_failed",
            Error::Config(_) => "invalid_config",
            Error::Io(_) => "io",
            Error::Sqlite(_) => "database",
            Error::Json(_) => "json",
        }
    }
}

//! Settings read from `tsugi.toml` at the root of the main worktree.

use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub workspace: WorkspaceConfig,
    pub snapshot: SnapshotConfig,
    pub publish: PublishConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceConfig {
    /// Ignored paths cloned from the main worktree into new workspaces.
    pub copy: Vec<String>,
    /// Branch new workspaces start from and publish to.
    pub base: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SnapshotConfig {
    /// Untracked files larger than this are left out of snapshots.
    pub max_file_bytes: u64,
    /// Snapshots older than this many days are deleted by `tsugi gc`.
    pub keep_days: u32,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        SnapshotConfig {
            max_file_bytes: 10 * 1024 * 1024,
            keep_days: 14,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PublishConfig {
    /// Shell commands that must succeed before the base branch moves.
    pub verify: Vec<String>,
}

impl Config {
    pub const FILE_NAME: &'static str = "tsugi.toml";

    pub fn load(root: &Path) -> Result<Config> {
        let path = root.join(Self::FILE_NAME);
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|error| Error::Config(error.to_string()))
    }
}

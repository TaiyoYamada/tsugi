//! The terminal app of tsugi.

use std::path::Path;

use tsugi_core::{Repo, Result};

/// Runs the app until the user quits.
pub fn run(_repo: Repo, _home: Option<&Path>) -> Result<()> {
    Err(tsugi_core::Error::Refused(
        "the terminal app is not available yet; see `tsugi --help` for the commands".to_string(),
    ))
}

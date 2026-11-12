//! A throwaway repository for tests.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use tsugi_core::{Repo, Workspace};

pub struct TestRepo {
    /// Keeps the directory alive; deleted when dropped.
    pub dir: TempDir,
    /// Root of the main worktree.
    pub root: PathBuf,
    pub repo: Repo,
}

impl TestRepo {
    /// A repository with one commit on `main` and tsugi set up.
    pub fn new() -> TestRepo {
        TestRepo::with_config("")
    }

    /// Like `new`, with `tsugi.toml` committed.
    pub fn with_config(config: &str) -> TestRepo {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "--quiet", "--initial-branch=main"]);
        git(&root, &["config", "user.name", "Test"]);
        git(&root, &["config", "user.email", "test@example.com"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        fs::write(root.join("README.md"), "# test\n").unwrap();
        fs::write(root.join(".gitignore"), "ignored/\n*.log\n").unwrap();
        if !config.is_empty() {
            fs::write(root.join("tsugi.toml"), config).unwrap();
        }
        git(&root, &["add", "--all"]);
        git(&root, &["commit", "--quiet", "--message", "init"]);
        let repo = Repo::init(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        TestRepo { dir, root, repo }
    }

    /// The workspace of the main worktree.
    pub fn main(&self) -> Workspace {
        self.repo.workspace_containing(&self.root).unwrap()
    }

    /// Opens the repository again, as a new process would.
    pub fn reopen(&self) -> Repo {
        Repo::discover(&self.root).unwrap()
    }
}

/// Runs git and returns its trimmed stdout. Panics if git fails.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

pub fn write(dir: &Path, file: &str, content: &str) {
    let path = dir.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

pub fn read(dir: &Path, file: &str) -> String {
    fs::read_to_string(dir.join(file)).unwrap()
}

/// Commits everything in a worktree.
pub fn commit_all(dir: &Path, message: &str) -> String {
    git(dir, &["add", "--all"]);
    git(dir, &["commit", "--quiet", "--message", message]);
    git(dir, &["rev-parse", "HEAD"])
}

/// Paths in a tree, sorted.
pub fn tree_files(dir: &Path, tree: &str) -> Vec<String> {
    git(dir, &["ls-tree", "-r", "--name-only", tree])
        .lines()
        .map(str::to_string)
        .collect()
}

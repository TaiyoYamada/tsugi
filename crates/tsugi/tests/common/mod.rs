//! Runs the `tsugi` binary against a throwaway repository.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

pub struct Sandbox {
    pub dir: TempDir,
    /// Root of the main worktree.
    pub root: PathBuf,
}

impl Sandbox {
    /// A repository with one commit on `main` and tsugi set up.
    pub fn new() -> Sandbox {
        Sandbox::with_config("")
    }

    pub fn with_config(config: &str) -> Sandbox {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "--quiet", "--initial-branch=main"]);
        git(&root, &["config", "user.name", "Test"]);
        git(&root, &["config", "user.email", "test@example.com"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        fs::write(root.join("README.md"), "# test\n").unwrap();
        fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        if !config.is_empty() {
            fs::write(root.join("tsugi.toml"), config).unwrap();
        }
        git(&root, &["add", "--all"]);
        git(&root, &["commit", "--quiet", "--message", "init"]);
        let root = fs::canonicalize(root).unwrap();
        let sandbox = Sandbox { dir, root };
        sandbox.ok(&["init"]);
        sandbox
    }

    /// Directory of a workspace created by `tsugi ws new`.
    pub fn workspace(&self, name: &str) -> PathBuf {
        self.root.parent().unwrap().join("repo.tsugi").join(name)
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsugi"));
        command
            .current_dir(&self.root)
            .args(args)
            // The tests must not read the session logs of the person who
            // runs them.
            .env("HOME", self.dir.path())
            .env_remove("TSUGI_FAILPOINT");
        command
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    /// Runs a command that must succeed and returns its stdout.
    pub fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "tsugi {args:?} failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    /// Runs a command with `--json` that must succeed and parses its output.
    pub fn json(&self, args: &[&str]) -> Value {
        let mut all = vec!["--json"];
        all.extend(args);
        serde_json::from_str(&self.ok(&all)).unwrap()
    }

    /// Runs a command that is interrupted at the given step.
    pub fn crash(&self, failpoint: &str, args: &[&str]) {
        let output = self
            .command(args)
            .env("TSUGI_FAILPOINT", failpoint)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(70),
            "tsugi {args:?} did not reach `{failpoint}`: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// The newest operation.
    pub fn last_operation(&self) -> Value {
        self.json(&["op", "log", "-n", "1"])[0].clone()
    }
}

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

pub fn commit_all(dir: &Path, message: &str) -> String {
    git(dir, &["add", "--all"]);
    git(dir, &["commit", "--quiet", "--message", message]);
    git(dir, &["rev-parse", "HEAD"])
}

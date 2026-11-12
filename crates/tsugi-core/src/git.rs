//! Runs the `git` executable. This is the only place tsugi talks to Git.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::error::{Error, Result};

/// Variables Git sets while it runs hooks. Inheriting them would redirect our
/// commands to the repository, worktree or index of the calling Git process.
const INHERITED: [&str; 6] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_PREFIX",
    "GIT_OBJECT_DIRECTORY",
];

#[derive(Debug, Clone)]
pub struct Git {
    dir: PathBuf,
    envs: Vec<(OsString, OsString)>,
}

pub struct Output {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim_end().to_string()
    }
}

impl Git {
    pub fn new(dir: impl Into<PathBuf>) -> Git {
        Git {
            dir: dir.into(),
            envs: Vec::new(),
        }
    }

    pub fn at(&self, dir: impl Into<PathBuf>) -> Git {
        Git {
            dir: dir.into(),
            envs: self.envs.clone(),
        }
    }

    pub fn env(&self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Git {
        let mut git = self.clone();
        git.envs
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        git
    }

    /// Runs git and returns its exit status and output without judging it.
    pub fn output(&self, args: &[&str], stdin: Option<&[u8]>) -> Result<Output> {
        let mut command = Command::new("git");
        command
            .current_dir(&self.dir)
            .args(["-c", "core.quotePath=false"])
            .args(args)
            // Reading commands such as `git status` must not take index.lock
            // away from an agent that is running git in the same worktree.
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for name in INHERITED {
            command.env_remove(name);
        }
        for (key, value) in &self.envs {
            command.env(key, value);
        }

        let mut child = command.spawn()?;
        let output = std::thread::scope(|scope| {
            // Written from a second thread: git may fill its stdout pipe
            // before it has read all of its input.
            if let Some(bytes) = stdin {
                let mut pipe = child.stdin.take().expect("stdin is piped");
                scope.spawn(move || {
                    // A broken pipe means git exited early; its status says why.
                    let _ = pipe.write_all(bytes);
                });
            }
            child.wait_with_output()
        })?;

        Ok(Output {
            code: output.status.code(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
        })
    }

    /// Runs git, fails unless it exits with 0, and returns stdout as text.
    pub fn run(&self, args: &[&str]) -> Result<String> {
        self.run_with_stdin(args, None)
    }

    pub fn run_with_stdin(&self, args: &[&str], stdin: Option<&[u8]>) -> Result<String> {
        let output = self.output(args, stdin)?;
        if !output.success() {
            return Err(Error::Git {
                args: args.join(" "),
                stderr: output.stderr,
            });
        }
        Ok(output.text())
    }

    /// Resolves a revision to a commit id, or `None` if it does not exist.
    pub fn commit(&self, rev: &str) -> Result<Option<String>> {
        let spec = format!("{rev}^{{commit}}");
        let output = self.output(&["rev-parse", "--quiet", "--verify", &spec], None)?;
        Ok(output.success().then(|| output.text()))
    }

    /// The tree of a commit.
    pub fn tree_of(&self, commit: &str) -> Result<String> {
        self.run(&["rev-parse", "--verify", &format!("{commit}^{{tree}}")])
    }

    /// The full name of the checked-out branch, or `None` when detached.
    pub fn current_branch(&self) -> Result<Option<String>> {
        let output = self.output(&["symbolic-ref", "--quiet", "HEAD"], None)?;
        Ok(output.success().then(|| output.text()))
    }
}

/// Splits NUL-separated git output into its non-empty fields.
pub fn split_nul(bytes: &[u8]) -> Vec<&[u8]> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect()
}

/// Installed git version as (major, minor).
pub fn version() -> Result<(u32, u32)> {
    let text = Git::new(".").run(&["version"])?;
    let mut numbers = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u32>().unwrap_or(0));
    Ok((numbers.next().unwrap_or(0), numbers.next().unwrap_or(0)))
}

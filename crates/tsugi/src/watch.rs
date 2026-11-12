//! Starts and stops the watcher as a background process. The watcher
//! itself is `tsugi_core::watch`.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use tsugi_core::watch::is_running;
use tsugi_core::{Repo, Result};

use crate::refused;

fn pid_path(repo: &Repo) -> PathBuf {
    repo.state_dir().join("watch.pid")
}

/// Runs the watcher in this process until it is killed.
pub fn run(repo: &Repo) -> Result<()> {
    fs::write(pid_path(repo), std::process::id().to_string())?;
    tsugi_core::watch::run(repo, &AtomicBool::new(false), &|line| println!("{line}"))
}

/// Starts the watcher in the background. Returns false if one was running.
pub fn start(repo: &Repo, dir: &Path) -> Result<bool> {
    if is_running(repo)? {
        return Ok(false);
    }
    let log = File::create(repo.state_dir().join("watch.log"))?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("-C")
        .arg(dir)
        .args(["watch", "run"])
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // Its own process group: closing the terminal or pressing Ctrl-C there
    // must not stop the watcher.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command.spawn()?;

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if is_running(repo)? {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(refused(format!(
        "the watcher did not start; see {}",
        repo.state_dir().join("watch.log").display()
    )))
}

/// Stops the watcher. Returns false if none was running.
pub fn stop(repo: &Repo) -> Result<bool> {
    if !is_running(repo)? {
        let _ = fs::remove_file(pid_path(repo));
        return Ok(false);
    }
    // Only a watcher started by `tsugi watch` writes its process id.
    let Ok(pid) = fs::read_to_string(pid_path(repo)) else {
        return Err(refused(
            "the watcher runs inside another program, such as the desktop app; stop it there",
        ));
    };
    // The watcher can be stopped at any point: a snapshot becomes visible in
    // one atomic ref update or not at all.
    Command::new("kill").arg(pid.trim()).status()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while is_running(repo)? {
        if Instant::now() >= deadline {
            return Err(refused(format!(
                "the watcher (pid {}) did not stop",
                pid.trim()
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = fs::remove_file(pid_path(repo));
    Ok(true)
}

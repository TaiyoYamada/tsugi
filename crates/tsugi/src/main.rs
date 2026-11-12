//! The `tsugi` command.

mod render;
mod watch;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use serde_json::json;
use tsugi_core::{Error, Repo, Result, Trigger, Workspace};

#[derive(Parser)]
#[command(
    name = "tsugi",
    version,
    about = "A layer on top of Git for repositories where several coding agents work in parallel",
    long_about = None
)]
struct Cli {
    /// Print the result as JSON (see docs/cli.md)
    #[arg(long, global = true)]
    json: bool,

    /// Run as if started in this directory
    #[arg(short = 'C', global = true, value_name = "DIR")]
    dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Set tsugi up in this repository
    Init,
    /// Show every workspace: changes, agent, latest snapshot
    Status,
    /// Manage tasks
    #[command(subcommand)]
    Task(TaskCommand),
    /// Manage workspaces
    #[command(subcommand)]
    Ws(WsCommand),
    /// Take a snapshot of a workspace now
    Snapshot(WorkspaceArg),
    /// List the snapshots of a workspace
    Log {
        #[command(flatten)]
        workspace: WorkspaceArg,
        /// Show at most this many snapshots
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
    },
    /// Show what a workspace changed since it left its base branch
    Diff {
        #[command(flatten)]
        workspace: WorkspaceArg,
        /// Start from this snapshot instead of the base branch
        #[arg(long, value_name = "SNAPSHOT")]
        from: Option<String>,
        /// End at this snapshot instead of the current content
        #[arg(long, value_name = "SNAPSHOT")]
        to: Option<String>,
        /// List the changed files without the patch
        #[arg(long)]
        stat: bool,
    },
    /// Put a workspace back to a snapshot, e.g. `login@12` or `@12`
    Restore {
        #[arg(value_name = "SNAPSHOT")]
        snapshot: String,
    },
    /// List the files on which workspaces would conflict
    Conflicts,
    /// Put the content of a workspace on its base branch as one commit
    Publish {
        #[command(flatten)]
        workspace: WorkspaceArg,
        /// Commit message (default: the title of the task)
        #[arg(short, long)]
        message: Option<String>,
        /// Skip the commands of `publish.verify`
        #[arg(long)]
        no_verify: bool,
    },
    /// Inspect the operation log
    #[command(subcommand)]
    Op(OpCommand),
    /// Revert the most recent operation
    Undo,
    /// Control the process that snapshots workspaces as files change
    #[command(subcommand)]
    Watch(WatchCommand),
    /// Delete snapshots older than `snapshot.keep_days`
    Gc,
    /// Remove everything tsugi wrote to this repository
    Uninstall,
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Create a task
    New { title: String },
    /// List tasks
    Ls,
    /// Mark a task as finished
    Close { task: String },
}

#[derive(Subcommand)]
enum WsCommand {
    /// Create a workspace: a worktree on its own branch
    New {
        /// Name of the workspace (default: derived from the task)
        name: Option<String>,
        /// Task the workspace belongs to
        #[arg(long)]
        task: Option<String>,
        /// Branch to start from and publish to
        #[arg(long)]
        base: Option<String>,
        /// Create this many workspaces for the same task
        #[arg(short = 'n', long, default_value_t = 1)]
        count: u32,
    },
    /// Same as `tsugi status`
    Ls,
    /// Print the directory of a workspace
    Path(WorkspaceArg),
    /// Remove a workspace; its content stays available as a snapshot
    Rm(WorkspaceArg),
    /// Merge the base branch into a workspace
    Sync(WorkspaceArg),
}

#[derive(Subcommand)]
enum OpCommand {
    /// List operations, newest first
    Log {
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: u32,
    },
}

#[derive(Subcommand)]
enum WatchCommand {
    /// Start the watcher in the background
    Start,
    /// Stop the watcher
    Stop,
    /// Tell whether the watcher is running
    Status,
    /// Run the watcher in the foreground
    Run,
}

#[derive(Args)]
struct WorkspaceArg {
    /// Name or id of the workspace (default: the one that contains the
    /// current directory)
    #[arg(value_name = "WORKSPACE")]
    workspace: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json {
                let body =
                    json!({ "error": { "code": error.code(), "message": error.to_string() } });
                println!("{body}");
            } else {
                eprintln!("error: {error}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let dir = match cli.dir {
        Some(dir) => dir,
        None => std::env::current_dir()?,
    };
    let out = Output { json: cli.json };

    let Some(command) = cli.command else {
        let repo = Repo::discover(&dir)?;
        return tsugi_tui::run(repo, home().as_deref());
    };

    if let Command::Init = command {
        let repo = Repo::init(&dir)?;
        let workspaces = repo.workspaces()?;
        out.print(
            &json!({ "root": repo.root(), "workspaces": workspaces }),
            || {
                format!(
                    "tsugi is set up in {} ({} workspace(s) found)\n",
                    repo.root().display(),
                    workspaces.len()
                )
            },
        );
        return Ok(());
    }

    let repo = Repo::discover(&dir)?;
    let result = dispatch(&repo, &dir, command, &out);
    for notice in repo.take_notices() {
        eprintln!("note: {notice}");
    }
    result
}

fn dispatch(repo: &Repo, dir: &Path, command: Command, out: &Output) -> Result<()> {
    let workspace_of = |arg: &WorkspaceArg| -> Result<Workspace> {
        repo.refresh()?;
        match &arg.workspace {
            Some(selector) => repo.workspace(selector),
            None => repo.workspace_containing(dir),
        }
    };

    match command {
        Command::Init => unreachable!("handled before the repository is opened"),

        Command::Status | Command::Ws(WsCommand::Ls) => {
            let statuses = repo.status(home().as_deref())?;
            out.print(&statuses, || render::status(&statuses));
        }

        Command::Task(TaskCommand::New { title }) => {
            let task = repo.task_new(&title)?;
            out.print(&task, || {
                format!("created task {} `{}`\n", task.id, task.title)
            });
        }
        Command::Task(TaskCommand::Ls) => {
            let tasks = repo.tasks()?;
            out.print(&tasks, || render::tasks(&tasks));
        }
        Command::Task(TaskCommand::Close { task }) => {
            let task = repo.task_close(&task)?;
            out.print(&task, || format!("closed task `{}`\n", task.title));
        }

        Command::Ws(WsCommand::New {
            name,
            task,
            base,
            count,
        }) => {
            let task = task.map(|selector| repo.task(&selector)).transpose()?;
            let mut created = Vec::new();
            for index in 1..=count {
                let name = match (&name, &task) {
                    // One workspace keeps the name as given, so that a name
                    // that is taken is an error and not silently changed.
                    (Some(name), _) if count == 1 => name.clone(),
                    (Some(name), _) => repo.suggest_name(&format!("{name}-{index}"))?,
                    (None, Some(task)) => repo.suggest_name(&task.title)?,
                    (None, None) => repo.suggest_name("ws")?,
                };
                created.push(repo.workspace_new(&name, task.as_ref(), base.as_deref())?);
            }
            out.print(&created, || {
                created
                    .iter()
                    .map(|ws| format!("created workspace `{}` at {}\n", ws.name, ws.path.display()))
                    .collect()
            });
        }
        Command::Ws(WsCommand::Path(arg)) => {
            let workspace = workspace_of(&arg)?;
            out.print(&json!({ "path": workspace.path }), || {
                format!("{}\n", workspace.path.display())
            });
        }
        Command::Ws(WsCommand::Rm(arg)) => {
            let workspace = workspace_of(&arg)?;
            let removal = repo.workspace_remove(&workspace)?;
            out.print(&removal, || {
                format!(
                    "removed workspace `{}`; `tsugi undo` brings it back\n",
                    removal.name
                )
            });
        }
        Command::Ws(WsCommand::Sync(arg)) => {
            let workspace = workspace_of(&arg)?;
            let outcome = repo.workspace_sync(&workspace)?;
            out.print(&outcome, || {
                format!(
                    "`{}` now sits on `{}` ({})\n",
                    workspace.name,
                    workspace.base,
                    render::short(&outcome.onto)
                )
            });
        }

        Command::Snapshot(arg) => {
            let workspace = workspace_of(&arg)?;
            let outcome = repo.snapshot(&workspace, Trigger::Manual)?;
            out.print(&outcome, || {
                if outcome.created {
                    format!("snapshot {}@{}\n", workspace.name, outcome.snapshot.seq)
                } else {
                    format!(
                        "nothing changed since {}@{}\n",
                        workspace.name, outcome.snapshot.seq
                    )
                }
            });
        }
        Command::Log { workspace, limit } => {
            let workspace = workspace_of(&workspace)?;
            let mut snapshots = repo.snapshots(&workspace)?;
            snapshots.truncate(limit);
            out.print(&snapshots, || render::snapshots(&workspace, &snapshots));
        }
        Command::Diff {
            workspace,
            from,
            to,
            stat,
        } => {
            let workspace = workspace_of(&workspace)?;
            let to = match to {
                Some(selector) => repo.find_snapshot(&selector, Some(&workspace))?.1,
                None => repo.snapshot(&workspace, Trigger::Auto)?.snapshot,
            };
            let from = match from {
                Some(selector) => repo.find_snapshot(&selector, Some(&workspace))?.1.id,
                None => repo.fork_point(&workspace, &to)?,
            };
            let diff = repo.diff(&from, &to.id)?;
            out.print(&diff, || render::diff(&diff, stat));
        }
        Command::Restore { snapshot } => {
            repo.refresh()?;
            let current = repo.workspace_containing(dir).ok();
            let (workspace, target) = repo.find_snapshot(&snapshot, current.as_ref())?;
            let outcome = repo.restore(&workspace, &target)?;
            out.print(&outcome, || {
                let mut text = format!(
                    "restored `{}` to snapshot {}; the state before is snapshot {}\n",
                    workspace.name, target.seq, outcome.previous.seq
                );
                if !outcome.branch_moved && target.head.is_some() {
                    text.push_str(
                        "the workspace is on another branch than the snapshot was taken on; only files were restored\n",
                    );
                }
                text
            });
        }
        Command::Conflicts => {
            let conflicts = repo.conflicts()?;
            out.print(&conflicts, || render::conflicts(&conflicts));
        }
        Command::Publish {
            workspace,
            message,
            no_verify,
        } => {
            let workspace = workspace_of(&workspace)?;
            let outcome = repo.publish(&workspace, message.as_deref(), !no_verify)?;
            out.print(&outcome, || render::publish(&outcome));
        }

        Command::Op(OpCommand::Log { limit }) => {
            let operations = repo.operations(limit)?;
            out.print(&operations, || render::operations(&operations));
        }
        Command::Undo => {
            let undone = repo.undo()?;
            out.print(&undone, || {
                format!(
                    "undid operation #{} ({})\n",
                    undone.id,
                    undone.detail.kind()
                )
            });
        }

        Command::Watch(WatchCommand::Run) => watch::run(repo)?,
        Command::Watch(WatchCommand::Start) => {
            let started = watch::start(repo, dir)?;
            out.print(&json!({ "running": true, "started": started }), || {
                if started {
                    "the watcher is running\n".to_string()
                } else {
                    "the watcher was running already\n".to_string()
                }
            });
        }
        Command::Watch(WatchCommand::Stop) => {
            let stopped = watch::stop(repo)?;
            out.print(&json!({ "running": false, "stopped": stopped }), || {
                if stopped {
                    "the watcher was stopped\n".to_string()
                } else {
                    "the watcher was not running\n".to_string()
                }
            });
        }
        Command::Watch(WatchCommand::Status) => {
            let running = watch::is_running(repo)?;
            out.print(&json!({ "running": running }), || {
                if running {
                    "the watcher is running\n".to_string()
                } else {
                    "the watcher is not running\n".to_string()
                }
            });
        }

        Command::Gc => {
            let outcome = repo.gc()?;
            out.print(&outcome, || {
                format!(
                    "deleted {} snapshot(s), kept {}\n",
                    outcome.deleted, outcome.kept
                )
            });
        }
        Command::Uninstall => {
            watch::stop(repo)?;
            let outcome = Repo::discover(dir)?.uninstall()?;
            out.print(&outcome, || render::uninstall(&outcome));
        }
    }
    Ok(())
}

struct Output {
    json: bool,
}

impl Output {
    /// Prints `value` as JSON, or the text `human` produces.
    fn print<T: Serialize>(&self, value: &T, human: impl FnOnce() -> String) {
        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(value).expect("results serialize to JSON")
            );
        } else {
            print!("{}", human());
        }
    }
}

/// Home directory of the user, where agents keep their session logs.
fn home() -> Option<PathBuf> {
    std::env::home_dir()
}

/// Reports a failure that is specific to the command line.
fn refused(message: impl Into<String>) -> Error {
    Error::Refused(message.into())
}

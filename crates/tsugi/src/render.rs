//! Output for people. The JSON output is the stable interface; this text can
//! change between versions.

use tsugi_core::{
    Conflict, Diff, OpState, Operation, PublishOutcome, SessionState, Snapshot, Task,
    UninstallOutcome, Workspace, WorkspaceStatus, short_branch,
};
use tsugi_tui::ago;

pub fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}

/// Lays rows out in columns that are as wide as their widest cell.
fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut text = String::new();
    for row in rows {
        let line: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let padding = widths[column] - cell.chars().count();
                format!("{cell}{}", " ".repeat(padding))
            })
            .collect();
        text.push_str(line.join("  ").trim_end());
        text.push('\n');
    }
    text
}

pub fn status(statuses: &[WorkspaceStatus]) -> String {
    let mut rows = vec![vec![
        "WORKSPACE".to_string(),
        "TASK".to_string(),
        "BRANCH".to_string(),
        "CHANGES".to_string(),
        "AGENT".to_string(),
        "SNAPSHOT".to_string(),
    ]];
    for status in statuses {
        let changes = match status.changes {
            Some(stat) if stat.files == 0 => "-".to_string(),
            Some(stat) => format!(
                "{} file(s) +{} -{}",
                stat.files, stat.insertions, stat.deletions
            ),
            None => "?".to_string(),
        };
        let agent = match status.sessions.first() {
            Some(session) => format!(
                "{} {} ({})",
                session.agent,
                match session.state {
                    SessionState::Working => "working",
                    SessionState::Idle => "idle",
                },
                ago(session.updated_at)
            ),
            None => "-".to_string(),
        };
        rows.push(vec![
            status.workspace.name.clone(),
            status
                .task
                .as_ref()
                .map_or("-".to_string(), |task| task.title.clone()),
            status
                .branch
                .as_deref()
                .map_or("(detached)".to_string(), |branch| {
                    short_branch(branch).to_string()
                }),
            changes,
            agent,
            status
                .latest_snapshot
                .as_ref()
                .map_or("-".to_string(), |snapshot| {
                    format!("{} ({})", snapshot.seq, ago(snapshot.created_at))
                }),
        ]);
    }
    table(&rows)
}

pub fn tasks(tasks: &[Task]) -> String {
    if tasks.is_empty() {
        return "no tasks\n".to_string();
    }
    let rows: Vec<Vec<String>> = tasks
        .iter()
        .map(|task| {
            vec![
                task.id.clone(),
                if task.closed_at.is_some() {
                    "closed".to_string()
                } else {
                    "open".to_string()
                },
                task.title.clone(),
            ]
        })
        .collect();
    table(&rows)
}

pub fn snapshots(workspace: &Workspace, snapshots: &[Snapshot]) -> String {
    if snapshots.is_empty() {
        return format!("`{}` has no snapshots\n", workspace.name);
    }
    let rows: Vec<Vec<String>> = snapshots
        .iter()
        .map(|snapshot| {
            vec![
                format!("{}@{}", workspace.name, snapshot.seq),
                ago(snapshot.created_at),
                snapshot.trigger.as_str().to_string(),
                snapshot
                    .head
                    .as_deref()
                    .map_or("-".to_string(), |head| short(head).to_string()),
            ]
        })
        .collect();
    table(&rows)
}

pub fn diff(diff: &Diff, stat_only: bool) -> String {
    if !stat_only {
        return if diff.patch.is_empty() {
            String::new()
        } else {
            format!("{}\n", diff.patch)
        };
    }
    let mut rows: Vec<Vec<String>> = diff
        .files
        .iter()
        .map(|file| {
            vec![
                file.path.clone(),
                match (file.insertions, file.deletions) {
                    (Some(insertions), Some(deletions)) => format!("+{insertions} -{deletions}"),
                    _ => "binary".to_string(),
                },
            ]
        })
        .collect();
    rows.push(vec![
        format!("{} file(s)", diff.stat.files),
        format!("+{} -{}", diff.stat.insertions, diff.stat.deletions),
    ]);
    table(&rows)
}

pub fn conflicts(conflicts: &[Conflict]) -> String {
    if conflicts.is_empty() {
        return "no conflicts\n".to_string();
    }
    let mut text = String::new();
    for conflict in conflicts {
        let other = if conflict.other_is_base {
            format!("its base branch `{}`", conflict.other)
        } else {
            format!("`{}`", conflict.other)
        };
        text.push_str(&format!(
            "`{}` conflicts with {other}:\n",
            conflict.workspace
        ));
        for file in &conflict.files {
            text.push_str(&format!("  {file}\n"));
        }
    }
    text
}

pub fn publish(outcome: &PublishOutcome) -> String {
    let mut text = String::new();
    for verification in &outcome.verifications {
        text.push_str(&format!(
            "verified `{}`{}\n",
            verification.command,
            if verification.cached {
                " (same content as before, not run again)".to_string()
            } else {
                format!(" in {:.1}s", verification.duration_ms as f64 / 1000.0)
            }
        ));
    }
    text.push_str(&format!(
        "published `{}` to `{}` as {}: {} file(s) +{} -{}\n",
        outcome.workspace.name,
        outcome.base,
        short(&outcome.commit),
        outcome.stat.files,
        outcome.stat.insertions,
        outcome.stat.deletions
    ));
    text
}

pub fn operations(operations: &[Operation]) -> String {
    if operations.is_empty() {
        return "no operations\n".to_string();
    }
    let rows: Vec<Vec<String>> = operations
        .iter()
        .map(|operation| {
            vec![
                format!("#{}", operation.id),
                ago(operation.started_at),
                operation.detail.kind().to_string(),
                match operation.state {
                    OpState::Started => "running",
                    OpState::Done => "done",
                    OpState::Failed => "failed",
                    OpState::Undone => "undone",
                }
                .to_string(),
                operation.note.clone().unwrap_or_default(),
            ]
        })
        .collect();
    table(&rows)
}

pub fn uninstall(outcome: &UninstallOutcome) -> String {
    let mut text = format!(
        "removed tsugi from this repository ({} ref(s) deleted)\n",
        outcome.refs_deleted
    );
    if !outcome.worktrees.is_empty() {
        text.push_str("these worktrees are ordinary git worktrees and were left in place:\n");
        for worktree in &outcome.worktrees {
            text.push_str(&format!("  {}\n", worktree.display()));
        }
    }
    text
}

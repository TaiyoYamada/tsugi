//! The state of the app and how keys change it. Nothing here touches the
//! terminal or the repository, so all of it can be tested directly.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tsugi_core::{Conflict, Diff, Snapshot, Workspace, WorkspaceStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Workspaces,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Diff,
    Snapshots,
    Info,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Diff, Tab::Snapshots, Tab::Info];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Diff => "Diff",
            Tab::Snapshots => "Snapshots",
            Tab::Info => "Info",
        }
    }
}

/// Something the user asked for that changes the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    New {
        name: String,
    },
    Snapshot(Workspace),
    Restore(Workspace, Snapshot),
    Sync(Workspace),
    Publish {
        workspace: Workspace,
        message: Option<String>,
    },
    Remove(Workspace),
    Undo,
}

impl Request {
    /// Shown while the request runs.
    pub fn describe(&self) -> String {
        match self {
            Request::New { name } => format!("creating `{name}`"),
            Request::Snapshot(workspace) => format!("taking a snapshot of `{}`", workspace.name),
            Request::Restore(workspace, snapshot) => {
                format!(
                    "restoring `{}` to snapshot {}",
                    workspace.name, snapshot.seq
                )
            }
            Request::Sync(workspace) => format!("syncing `{}`", workspace.name),
            Request::Publish { workspace, .. } => format!("publishing `{}`", workspace.name),
            Request::Remove(workspace) => format!("removing `{}`", workspace.name),
            Request::Undo => "undoing the last operation".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    WorkspaceName,
    PublishMessage(Workspace),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Waiting for `y` before running a request that overwrites something.
    Confirm {
        question: String,
        request: Request,
    },
    /// Typing a line of text.
    Input {
        prompt: String,
        text: String,
        purpose: Input,
    },
    Help,
}

pub struct App {
    pub statuses: Vec<WorkspaceStatus>,
    pub conflicts: Vec<Conflict>,
    /// Index into `statuses`.
    pub selected: usize,
    pub focus: Focus,
    pub tab: Tab,
    /// Snapshots of the selected workspace, newest first.
    pub snapshots: Vec<Snapshot>,
    pub selected_snapshot: usize,
    /// What the selected workspace changed since it left its base branch.
    pub diff: Option<Diff>,
    pub diff_scroll: u16,
    pub mode: Mode,
    /// Result of the last request, or an error.
    pub message: Option<String>,
    /// Description of the request that is running.
    pub busy: Option<String>,
    pub quit: bool,
}

impl App {
    pub fn new() -> App {
        App {
            statuses: Vec::new(),
            conflicts: Vec::new(),
            selected: 0,
            focus: Focus::Workspaces,
            tab: Tab::Diff,
            snapshots: Vec::new(),
            selected_snapshot: 0,
            diff: None,
            diff_scroll: 0,
            mode: Mode::Normal,
            message: None,
            busy: None,
            quit: false,
        }
    }

    pub fn selected_status(&self) -> Option<&WorkspaceStatus> {
        self.statuses.get(self.selected)
    }

    /// Replaces the list of workspaces and keeps the same workspace selected.
    /// Workspaces of the same task are placed next to each other.
    pub fn set_statuses(&mut self, statuses: Vec<WorkspaceStatus>, conflicts: Vec<Conflict>) {
        let selected_id = self
            .selected_status()
            .map(|status| status.workspace.id.clone());
        // Tasks stay in the order of their first workspace, and workspaces
        // without a task come last. The sort is stable.
        let ranks: Vec<usize> = statuses
            .iter()
            .map(|status| task_rank(&statuses, status))
            .collect();
        let mut ranked: Vec<(usize, WorkspaceStatus)> = ranks.into_iter().zip(statuses).collect();
        ranked.sort_by_key(|(rank, _)| *rank);
        let statuses: Vec<WorkspaceStatus> = ranked.into_iter().map(|(_, status)| status).collect();

        self.selected = selected_id
            .and_then(|id| statuses.iter().position(|status| status.workspace.id == id))
            .unwrap_or(0);
        self.statuses = statuses;
        self.conflicts = conflicts;
    }

    /// Names of the workspaces and base branches the workspace conflicts
    /// with.
    pub fn conflicts_of(&self, workspace: &Workspace) -> Vec<&Conflict> {
        self.conflicts
            .iter()
            .filter(|conflict| {
                conflict.workspace == workspace.name
                    || (!conflict.other_is_base && conflict.other == workspace.name)
            })
            .collect()
    }

    /// Applies a key press. Returns a request when the key asks for a change
    /// to the repository that needs no further confirmation.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Request> {
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Help => None,
            Mode::Confirm { question, request } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => Some(request),
                KeyCode::Char('n') | KeyCode::Esc => None,
                _ => {
                    self.mode = Mode::Confirm { question, request };
                    None
                }
            },
            Mode::Input {
                prompt,
                mut text,
                purpose,
            } => match key.code {
                KeyCode::Esc => None,
                KeyCode::Enter => match purpose {
                    Input::WorkspaceName if text.trim().is_empty() => None,
                    Input::WorkspaceName => Some(Request::New {
                        name: text.trim().to_string(),
                    }),
                    Input::PublishMessage(workspace) => Some(Request::Publish {
                        workspace,
                        message: Some(text.trim().to_string()).filter(|text| !text.is_empty()),
                    }),
                },
                code => {
                    match code {
                        KeyCode::Backspace => {
                            text.pop();
                        }
                        KeyCode::Char(c) => text.push(c),
                        _ => {}
                    }
                    self.mode = Mode::Input {
                        prompt,
                        text,
                        purpose,
                    };
                    None
                }
            },
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Option<Request> {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('c') if control => self.quit = true,
            KeyCode::Char('?') => self.mode = Mode::Help,

            KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right
                if self.focus == Focus::Workspaces =>
            {
                self.focus = Focus::Detail;
            }
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left | KeyCode::Esc => {
                self.focus = Focus::Workspaces;
            }
            KeyCode::Char('1') => self.tab = Tab::Diff,
            KeyCode::Char('2') => self.tab = Tab::Snapshots,
            KeyCode::Char('3') => self.tab = Tab::Info,

            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('d') if control => self.move_selection(10),
            KeyCode::Char('u') if control => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),

            KeyCode::Char('n') => {
                self.mode = Mode::Input {
                    prompt: "Name of the new workspace".to_string(),
                    text: String::new(),
                    purpose: Input::WorkspaceName,
                };
            }
            KeyCode::Char('u') => {
                self.mode = Mode::Confirm {
                    question: "Undo the last operation?".to_string(),
                    request: Request::Undo,
                };
            }
            code => return self.handle_workspace_key(code),
        }
        None
    }

    /// Keys that act on the selected workspace.
    fn handle_workspace_key(&mut self, code: KeyCode) -> Option<Request> {
        let status = self.selected_status()?;
        let workspace = status.workspace.clone();
        let name = &workspace.name;
        match code {
            KeyCode::Char('s') => return Some(Request::Snapshot(workspace)),
            KeyCode::Char('y') => {
                self.mode = Mode::Confirm {
                    question: format!("Merge `{}` into `{name}`?", workspace.base),
                    request: Request::Sync(workspace),
                };
            }
            KeyCode::Char('p') => {
                let default = status
                    .task
                    .as_ref()
                    .map_or(format!("Publish {name}"), |task| task.title.clone());
                self.mode = Mode::Input {
                    prompt: format!(
                        "Publish `{name}` to `{}`. Commit message (empty: \"{default}\")",
                        workspace.base
                    ),
                    text: String::new(),
                    purpose: Input::PublishMessage(workspace),
                };
            }
            KeyCode::Char('D') => {
                self.mode = Mode::Confirm {
                    question: format!(
                        "Remove `{name}`? Its content stays available as a snapshot."
                    ),
                    request: Request::Remove(workspace),
                };
            }
            KeyCode::Char('r') | KeyCode::Enter if self.tab == Tab::Snapshots => {
                let snapshot = self.snapshots.get(self.selected_snapshot)?.clone();
                self.mode = Mode::Confirm {
                    question: format!("Restore `{name}` to snapshot {}?", snapshot.seq),
                    request: Request::Restore(workspace, snapshot),
                };
            }
            _ => {}
        }
        None
    }

    /// Moves the selection in the focused pane: the workspace on the left;
    /// on the right the snapshot, or the scroll position of the text.
    fn move_selection(&mut self, by: i32) {
        let moved = |index: usize, length: usize| -> usize {
            let last = length.saturating_sub(1) as i32;
            (index as i32 + by).clamp(0, last) as usize
        };
        match (self.focus, self.tab) {
            (Focus::Workspaces, _) => {
                let before = self.selected;
                self.selected = moved(self.selected, self.statuses.len());
                if self.selected != before {
                    self.diff_scroll = 0;
                    self.selected_snapshot = 0;
                }
            }
            (Focus::Detail, Tab::Snapshots) => {
                self.selected_snapshot = moved(self.selected_snapshot, self.snapshots.len());
            }
            (Focus::Detail, _) => {
                let lines = self
                    .diff
                    .as_ref()
                    .map_or(0, |diff| diff.patch.lines().count());
                self.diff_scroll = moved(self.diff_scroll as usize, lines) as u16;
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        App::new()
    }
}

/// Position of the task of `status` among the tasks, in order of first
/// appearance; workspaces without a task sort after all tasks.
fn task_rank(statuses: &[WorkspaceStatus], status: &WorkspaceStatus) -> usize {
    match &status.workspace.task_id {
        Some(task_id) => statuses
            .iter()
            .position(|other| other.workspace.task_id.as_ref() == Some(task_id))
            .unwrap_or(0),
        None => usize::MAX,
    }
}

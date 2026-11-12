//! Draws the app.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use tsugi_core::{SessionState, WorkspaceStatus, short_branch};

use crate::ago;
use crate::app::{App, Focus, Mode, Tab};

const KEYS: [(&str, &str); 15] = [
    ("j / k", "move down / up"),
    ("h / l, Tab", "switch between the list and the detail"),
    ("1 2 3", "show the diff, the snapshots, the info"),
    ("Ctrl-d / Ctrl-u", "move ten lines"),
    ("n", "new workspace"),
    ("s", "take a snapshot now"),
    ("r, Enter", "restore the selected snapshot"),
    ("y", "sync: merge the base branch into the workspace"),
    ("p", "publish the workspace to its base branch"),
    ("D", "remove the workspace"),
    ("u", "undo the last operation"),
    ("?", "this help"),
    ("q", "quit"),
    ("y / n", "answer a question"),
    ("Esc", "cancel"),
];

pub fn draw(frame: &mut Frame, app: &App) {
    let [body, footer] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(38), Constraint::Fill(1)]).areas(body);
    let conflict_lines = conflict_lines(app);
    // At most half of the column, but tall enough for one line in a border.
    let conflicts_height = (conflict_lines.len() as u16 + 2)
        .min(left.height / 2)
        .max(3);
    let [list, conflicts] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(conflicts_height)]).areas(left);

    draw_workspaces(frame, app, list);
    frame.render_widget(
        Paragraph::new(conflict_lines).block(Block::bordered().title(" Conflicts ")),
        conflicts,
    );
    draw_detail(frame, app, right);
    draw_footer(frame, app, footer);

    match &app.mode {
        Mode::Normal => {}
        Mode::Confirm { question, .. } => {
            popup(
                frame,
                " Confirm ",
                vec![
                    Line::raw(question.as_str()),
                    Line::raw(""),
                    Line::raw("y: yes    n: no"),
                ],
            );
        }
        Mode::Input { prompt, text, .. } => {
            popup(
                frame,
                " Input ",
                vec![
                    Line::raw(prompt.as_str()),
                    Line::raw(""),
                    Line::from(vec![
                        Span::raw("> "),
                        Span::raw(text.as_str()),
                        Span::raw("█"),
                    ]),
                    Line::raw(""),
                    Line::raw("Enter: confirm    Esc: cancel"),
                ],
            );
        }
        Mode::Help => {
            let lines = KEYS
                .iter()
                .map(|(key, meaning)| Line::raw(format!("{key:<18}{meaning}")))
                .collect();
            popup(frame, " Keys ", lines);
        }
    }
}

fn border(focused: bool) -> Style {
    if focused {
        Style::new().fg(Color::Cyan)
    } else {
        Style::new().fg(Color::DarkGray)
    }
}

fn draw_workspaces(frame: &mut Frame, app: &App, area: Rect) {
    let mut items = Vec::new();
    let mut selected_row = None;
    let mut current_task: Option<Option<&str>> = None;

    for (index, status) in app.statuses.iter().enumerate() {
        let task = status.workspace.task_id.as_deref();
        if current_task != Some(task) {
            current_task = Some(task);
            let title = status
                .task
                .as_ref()
                .map_or("No task", |task| task.title.as_str());
            items.push(ListItem::new(Line::styled(
                title.to_string(),
                Style::new().add_modifier(Modifier::BOLD),
            )));
        }
        if index == app.selected {
            selected_row = Some(items.len());
        }
        items.push(ListItem::new(workspace_line(app, status)));
    }

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" Workspaces ")
                .border_style(border(app.focus == Focus::Workspaces)),
        )
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default().with_selected(selected_row);
    frame.render_stateful_widget(list, area, &mut state);
}

fn workspace_line<'a>(app: &App, status: &'a WorkspaceStatus) -> Line<'a> {
    let agent = match status.sessions.first().map(|session| session.state) {
        Some(SessionState::Working) => Span::styled("● ", Style::new().fg(Color::Yellow)),
        Some(SessionState::Idle) => Span::styled("○ ", Style::new().fg(Color::Green)),
        None => Span::raw("  "),
    };
    let mut spans = vec![
        Span::raw("  "),
        agent,
        Span::raw(status.workspace.name.as_str()),
    ];
    if status.is_main {
        spans.push(Span::styled(
            " (main worktree)",
            Style::new().fg(Color::DarkGray),
        ));
    }
    if let Some(stat) = status.changes
        && stat.files > 0
    {
        spans.push(Span::styled(
            format!("  +{}", stat.insertions),
            Style::new().fg(Color::Green),
        ));
        spans.push(Span::styled(
            format!(" -{}", stat.deletions),
            Style::new().fg(Color::Red),
        ));
    }
    if !app.conflicts_of(&status.workspace).is_empty() {
        spans.push(Span::styled("  conflict", Style::new().fg(Color::Red)));
    }
    Line::from(spans)
}

fn conflict_lines(app: &App) -> Vec<Line<'_>> {
    if app.conflicts.is_empty() {
        return vec![Line::styled("none", Style::new().fg(Color::DarkGray))];
    }
    app.conflicts
        .iter()
        .map(|conflict| {
            let other = if conflict.other_is_base {
                format!("{} (base)", conflict.other)
            } else {
                conflict.other.clone()
            };
            Line::from(vec![
                Span::styled(
                    format!("{} × {other}", conflict.workspace),
                    Style::new().fg(Color::Red),
                ),
                Span::raw(format!("  {}", conflict.files.join(", "))),
            ])
        })
        .collect()
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let mut title = vec![Span::raw(" ")];
    for (index, tab) in Tab::ALL.iter().enumerate() {
        let label = format!("{} {}", index + 1, tab.title());
        title.push(if *tab == app.tab {
            Span::styled(
                label,
                Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
        } else {
            Span::styled(label, Style::new().fg(Color::DarkGray))
        });
        title.push(Span::raw("  "));
    }
    let block = Block::bordered()
        .title(Line::from(title))
        .border_style(border(app.focus == Focus::Detail));

    let Some(status) = app.selected_status() else {
        frame.render_widget(Paragraph::new("No workspaces.").block(block), area);
        return;
    };
    match app.tab {
        Tab::Diff => {
            let lines = match &app.diff {
                Some(diff) if diff.patch.is_empty() => vec![Line::raw(format!(
                    "No changes since `{}`.",
                    status.workspace.base
                ))],
                Some(diff) => diff.patch.lines().map(diff_line).collect(),
                None => vec![Line::raw(format!(
                    "Nothing to compare with: the base branch `{}` does not exist.",
                    status.workspace.base
                ))],
            };
            frame.render_widget(
                Paragraph::new(lines)
                    .block(block)
                    .scroll((app.diff_scroll, 0)),
                area,
            );
        }
        Tab::Snapshots => {
            let items: Vec<ListItem> = app
                .snapshots
                .iter()
                .map(|snapshot| {
                    ListItem::new(format!(
                        "{:>5}  {:<9}  {:<12}  {}",
                        snapshot.seq,
                        ago(snapshot.created_at),
                        snapshot.trigger.as_str(),
                        snapshot
                            .head
                            .as_deref()
                            .map_or("-", |head| &head[..8.min(head.len())]),
                    ))
                })
                .collect();
            let list = List::new(items)
                .block(block)
                .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
            let mut state = ListState::default().with_selected(Some(app.selected_snapshot));
            frame.render_stateful_widget(list, area, &mut state);
        }
        Tab::Info => {
            frame.render_widget(
                Paragraph::new(info_lines(app, status))
                    .block(block)
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
    }
}

fn diff_line(line: &str) -> Line<'_> {
    let style = if line.starts_with("+++") || line.starts_with("---") || line.starts_with("diff ") {
        Style::new().add_modifier(Modifier::BOLD)
    } else if line.starts_with('+') {
        Style::new().fg(Color::Green)
    } else if line.starts_with('-') {
        Style::new().fg(Color::Red)
    } else if line.starts_with("@@") {
        Style::new().fg(Color::Cyan)
    } else {
        Style::new()
    };
    // Tabs have no fixed width in a terminal cell grid.
    Line::styled(line.replace('\t', "    "), style)
}

fn info_lines<'a>(app: &App, status: &'a WorkspaceStatus) -> Vec<Line<'a>> {
    let field = |name: &str, value: String| {
        Line::from(vec![
            Span::styled(format!("{name:<10}"), Style::new().fg(Color::DarkGray)),
            Span::raw(value),
        ])
    };
    let mut lines = vec![
        field("Path", status.workspace.path.display().to_string()),
        field(
            "Branch",
            status
                .branch
                .as_deref()
                .map_or("(detached)".to_string(), |branch| {
                    short_branch(branch).to_string()
                }),
        ),
        field("Base", status.workspace.base.clone()),
        field(
            "Task",
            status
                .task
                .as_ref()
                .map_or("-".to_string(), |task| task.title.clone()),
        ),
    ];
    if let Some(stat) = status.changes {
        lines.push(field(
            "Changes",
            format!(
                "{} file(s) +{} -{}",
                stat.files, stat.insertions, stat.deletions
            ),
        ));
    }
    for conflict in app.conflicts_of(&status.workspace) {
        let other = if conflict.workspace == status.workspace.name {
            &conflict.other
        } else {
            &conflict.workspace
        };
        lines.push(Line::styled(
            format!("Conflicts with {other}: {}", conflict.files.join(", ")),
            Style::new().fg(Color::Red),
        ));
    }

    lines.push(Line::raw(""));
    if status.sessions.is_empty() {
        lines.push(Line::styled(
            "No agent session was found for this workspace.",
            Style::new().fg(Color::DarkGray),
        ));
    }
    for session in &status.sessions {
        let state = match session.state {
            SessionState::Working => Span::styled("working", Style::new().fg(Color::Yellow)),
            SessionState::Idle => Span::styled("idle", Style::new().fg(Color::Green)),
        };
        lines.push(Line::from(vec![
            Span::styled(
                session.agent.as_str(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            state,
            Span::raw(format!("  {}", ago(session.updated_at))),
        ]));
        if let Some(title) = &session.title {
            lines.push(Line::raw(title.as_str()));
        }
        if let Some(message) = &session.last_message {
            lines.push(Line::styled(
                message.as_str(),
                Style::new().fg(Color::DarkGray),
            ));
        }
        lines.push(Line::raw(""));
    }
    lines
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let line = if let Some(busy) = &app.busy {
        Line::styled(format!(" {busy}…"), Style::new().fg(Color::Yellow))
    } else if let Some(message) = &app.message {
        Line::raw(format!(" {message}"))
    } else {
        Line::styled(
            " n new  s snapshot  y sync  p publish  D remove  u undo  ? keys  q quit",
            Style::new().fg(Color::DarkGray),
        )
    };
    frame.render_widget(Paragraph::new(line), area);
}

/// A box in the middle of the screen, as tall as its text.
fn popup(frame: &mut Frame, title: &str, lines: Vec<Line>) {
    let area = frame.area();
    let width = (area.width * 2 / 3).clamp(20.min(area.width), 90);
    let inner_width = width.saturating_sub(2).max(1) as usize;
    // Wrapped lines take more than one row.
    let rows: usize = lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(inner_width))
        .sum();
    let height = (rows as u16 + 2).min(area.height);
    let rect = area.centered(Constraint::Length(width), Constraint::Length(height));
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title(title)
                .border_style(Style::new().fg(Color::Cyan)),
        ),
        rect,
    );
}

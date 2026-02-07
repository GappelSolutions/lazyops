mod cicd;
mod help;
mod input;
mod prs;
mod tasks;

use crate::app::{App, InputMode, View};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // If terminal mode is active, render embedded terminal fullscreen
    if app.terminal_mode && app.embedded_terminal.is_some() {
        draw_embedded_terminal(f, app, size);
        return;
    }

    // Main vertical layout: header (3) + content + status bar (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Sprint/project bar
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Status/help bar
        ])
        .split(size);

    // Header bar with view tabs, sprint and project selectors
    tasks::draw_sprint_bar(f, app, chunks[0]);

    // Main content: route based on current view
    match app.current_view {
        View::Tasks => draw_tasks_view(f, app, chunks[1]),
        View::PRs => prs::draw(f, app, chunks[1]),
        View::CICD => cicd::draw(f, app, chunks[1]),
    }

    // Bottom: Status/help bar
    draw_status_bar(f, app, chunks[2]);

    // Overlays (modals, dropdowns, help)
    match app.input_mode {
        InputMode::Help => help::draw_popup(f, app, size),
        InputMode::SprintSelect => input::draw_sprint_dropdown(f, app, size),
        InputMode::ProjectSelect => input::draw_project_dropdown(f, app, size),
        InputMode::EditState => input::draw_state_dropdown(f, app, size),
        InputMode::EditAssignee => input::draw_assignee_dropdown(f, app, size),
        InputMode::Search => input::draw_search_input(f, app, size),
        InputMode::FilterState => input::draw_filter_state_dropdown(f, app, size),
        InputMode::FilterAssignee => input::draw_filter_assignee_dropdown(f, app, size),
        InputMode::CICDSearch => {} // Handled inline in panels
        InputMode::Normal => {}
        InputMode::ReleaseTriggerDialog | InputMode::ApprovalConfirm | InputMode::ConfirmAction => {
        } // Dialogs rendered in cicd module
    }

    // Loading overlay
    if app.loading {
        draw_loading(f, app, size);
    }
}

fn draw_tasks_view(f: &mut Frame, app: &mut App, area: Rect) {
    // Main content: 50/50 horizontal split
    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left: Work items list
    tasks::draw_work_items(f, app, content[0]);

    // Right: Preview pane
    tasks::draw_preview(f, app, content[1]);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let style = if app.status_is_error {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let content = if let Some(msg) = &app.status_message {
        msg.clone()
    } else {
        match app.current_view {
            View::Tasks => {
                // Show active filters or keybindings for Tasks view
                let mut parts: Vec<String> = Vec::new();

                if let Some(state) = &app.filter_state {
                    parts.push(format!("State:{state}"));
                }
                if let Some(assignee) = &app.filter_assignee {
                    parts.push(format!("Assignee:{assignee}"));
                }
                if !app.search_query.is_empty() {
                    let query = &app.search_query;
                    parts.push(format!("Search:\"{query}\""));
                }

                if !parts.is_empty() {
                    let joined = parts.join("  ");
                    format!("Filters: {joined}  │  c:clear  s:state  a:user  f:search")
                } else {
                    match app.input_mode {
                        InputMode::Normal => {
                            match app.focus {
                                crate::app::Focus::WorkItems => {
                                    "j/k:nav  Enter:expand  t:toggle  o:open  s:state  a:user  S:edit  A:assign  f:search  I:sprint  l:preview  r:refresh  ?:help  q:quit".into()
                                }
                                crate::app::Focus::Preview => {
                                    match app.preview_tab {
                                        crate::app::PreviewTab::Details => {
                                            "j/k:scroll  Tab:refs  h:back  o:open  r:refresh  ?:help  q:quit".into()
                                        }
                                        crate::app::PreviewTab::References => {
                                            "j/k:select  ^d/^u:page  Tab:details  h:back  o:open  r:refresh  ?:help  q:quit".into()
                                        }
                                    }
                                }
                            }
                        }
                        InputMode::Search => "Enter:confirm  Esc:cancel".into(),
                        _ => "j/k:select  Enter:confirm  Esc:cancel".into(),
                    }
                }
            }
            View::PRs => {
                // PR view keybindings based on focus and drill-down
                match app.input_mode {
                    InputMode::Normal => {
                        match app.pr_focus {
                            crate::app::PRFocus::Preview => {
                                "j/k:scroll  Tab:switch tab  h:back  o:open  ?:help  q:quit".into()
                            }
                            _ => {
                                match app.pr_drill_down {
                                    crate::app::PRDrillDown::Repos => {
                                        "j/k:nav  f:search  Enter:PRs  o:open  r:refresh  ?:help  q:quit".into()
                                    }
                                    crate::app::PRDrillDown::PRs => {
                                        "j/k:nav  h/l:pane  f:search  Enter:details  Esc:back  o:open  r:refresh  ?:help  q:quit".into()
                                    }
                                }
                            }
                        }
                    }
                    _ => "j/k:select  Enter:confirm  Esc:cancel".into(),
                }
            }
            View::CICD => {
                // CI/CD view keybindings based on focus and drill-down
                match app.input_mode {
                    InputMode::Normal => {
                        match app.cicd_focus {
                            crate::app::CICDFocus::Pipelines => {
                                match app.pipeline_drill_down {
                                    crate::app::PipelineDrillDown::None => {
                                        "j/k:nav  f:search  Enter:runs  p:pin  h/l:panes  o:open  r:refresh  ?:help  q:quit".into()
                                    }
                                    crate::app::PipelineDrillDown::Runs => {
                                        if app.pipeline_runs_limited {
                                            "j/k:nav  f:search  ^d/^u:page  Enter:details  L:all  Esc:back  o:open  ?:help  q:quit".into()
                                        } else {
                                            "j/k:nav  f:search  ^d/^u:page  Enter:details  Esc:back  o:open  ?:help  q:quit".into()
                                        }
                                    }
                                    crate::app::PipelineDrillDown::Tasks => {
                                        "j/k:nav  f:search  ^d/^u:page  Enter:logs  e:edit  Esc:back  o:open  ?:help  q:quit".into()
                                    }
                                }
                            }
                            crate::app::CICDFocus::Releases => {
                                match app.release_drill_down {
                                    crate::app::ReleaseDrillDown::None => {
                                        "j/k:nav  f:search  Enter:releases  T:trigger  p:pin  h/l:panes  o:open  r:refresh  ?:help  q:quit".into()
                                    }
                                    crate::app::ReleaseDrillDown::Items => {
                                        "j/k:nav  f:search  ^d/^u:page  Enter:stages  T:trigger  Esc:back  o:open  ?:help  q:quit".into()
                                    }
                                    crate::app::ReleaseDrillDown::Stages => {
                                        "j/k:nav  f:search  Enter:tasks  a:approve  T:trigger  Esc:back  o:open  ?:help  q:quit".into()
                                    }
                                    crate::app::ReleaseDrillDown::Tasks => {
                                        "j/k:nav  f:search  ^d/^u:page  Enter:logs  T:trigger  Esc:back  o:open  ?:help  q:quit".into()
                                    }
                                }
                            }
                            crate::app::CICDFocus::Preview => {
                                "j/k:scroll  h:back  o:open  ?:help  q:quit".into()
                            }
                        }
                    }
                    _ => "j/k:select  Enter:confirm  Esc:cancel".into(),
                }
            }
        }
    };

    let paragraph = Paragraph::new(content).style(style);
    f.render_widget(paragraph, area);
}

fn draw_loading(f: &mut Frame, app: &App, area: Rect) {
    let spinner = app.spinner_char();
    let message = if app.loading_message.is_empty() {
        "Loading..."
    } else {
        &app.loading_message
    };

    let text = format!(" {spinner} {message} ");
    let width = (text.len() as u16).clamp(20, 50);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = centered_rect(width, 3, area);
    f.render_widget(Clear, inner);
    f.render_widget(block.clone(), inner);

    let text_area = Rect::new(inner.x + 1, inner.y + 1, inner.width - 2, 1);
    let paragraph = Paragraph::new(text)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center);
    f.render_widget(paragraph, text_area);
}

// Helper: create a centered rect
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

// Helper: styled block with focus indicator
pub fn styled_block<'a>(
    title: &'a str,
    focused: bool,
    theme: &'a crate::config::Theme,
) -> Block<'a> {
    let border_color = if focused {
        theme.parse_color(&theme.border_active)
    } else {
        theme.parse_color(&theme.border)
    };

    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" {title} "))
}

/// Draw embedded terminal (nvim log viewer) fullscreen
fn draw_embedded_terminal(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Log Viewer (Ctrl+q to exit) ");

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Resize terminal to match inner area
    let _ = app.resize_terminal(inner.width, inner.height);

    // Get terminal screen content with styles
    if let Some(ref term) = app.embedded_terminal {
        if let Some(screen) = term.get_screen_with_styles() {
            let mut lines: Vec<Line> = Vec::new();

            for row in screen.iter().take(inner.height as usize) {
                let mut spans: Vec<Span> = Vec::new();

                for &(ch, fg, bg, bold) in row.iter().take(inner.width as usize) {
                    let fg_color = vt100_to_ratatui_color(fg);
                    let bg_color = vt100_to_ratatui_color(bg);

                    let mut style = Style::default().fg(fg_color).bg(bg_color);
                    if bold {
                        style = style.add_modifier(Modifier::BOLD);
                    }

                    spans.push(Span::styled(ch.to_string(), style));
                }

                lines.push(Line::from(spans));
            }

            let paragraph = Paragraph::new(lines);
            f.render_widget(paragraph, inner);

            // Render cursor if visible
            if let Some((row, col)) = term.cursor_position() {
                let cursor_x = inner.x + col;
                let cursor_y = inner.y + row;
                if cursor_x < inner.x + inner.width && cursor_y < inner.y + inner.height {
                    f.set_cursor_position((cursor_x, cursor_y));
                }
            }
        }
    }
}

/// Convert vt100 color to ratatui color
fn vt100_to_ratatui_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

use crate::app::{App, CICDFocus, PipelineDrillDown, ReleaseDrillDown};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};

/// Strip ANSI escape sequences and control characters from a string
fn strip_ansi_and_control(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ANSI escape sequence: ESC [ ... letter
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                              // Skip until we hit a letter (the terminator)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            // Also handle ESC followed by other sequences
        } else if c == '\t' {
            // Keep tabs, convert to spaces for consistent width
            result.push_str("    ");
        } else if !c.is_control() {
            result.push(c);
        }
    }
    result
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.cicd_focus == CICDFocus::Preview;

    let border_color = if focused {
        app.config
            .theme
            .parse_color(&app.config.theme.border_active)
    } else {
        app.config.theme.parse_color(&app.config.theme.border)
    };

    // Determine what to show based on drill-down state (not just focus)
    // When in Preview mode, check which drill-down is active
    if app.release_drill_down == ReleaseDrillDown::Tasks {
        // Show release task log
        draw_release_task_log(f, app, area, border_color);
    } else if app.release_drill_down == ReleaseDrillDown::Stages {
        // Show stage preview
        draw_stage_preview(f, app, area, border_color);
    } else if app.release_drill_down == ReleaseDrillDown::Items {
        // Show release preview when viewing release items
        draw_release_preview(f, app, area, border_color);
    } else if app.pipeline_drill_down == PipelineDrillDown::Tasks {
        draw_log_preview(f, app, area, border_color);
    } else if app.pipeline_drill_down == PipelineDrillDown::Runs {
        draw_run_preview(f, app, area, border_color);
    } else {
        // Default preview based on current focus
        draw_default_preview(f, app, area, border_color);
    }
}

fn draw_default_preview(f: &mut Frame, app: &mut App, area: Rect, border_color: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" Preview ");

    let inner = block.inner(area);
    f.render_widget(block, area);

    let content = match app.cicd_focus {
        CICDFocus::Pipelines | CICDFocus::Preview => {
            if let Some(pipeline) = app.pipelines.get(app.selected_pipeline_idx) {
                let status = pipeline.queue_status.as_deref().unwrap_or("unknown");
                let status_icon = match status {
                    "enabled" => "✓ Enabled",
                    "disabled" => "✗ Disabled",
                    "paused" => "⏸ Paused",
                    _ => status,
                };

                format!(
                    "Pipeline: {}\n\
                     ID: {}\n\
                     Status: {}\n\
                     Path: {}\n\n\
                     Press [Enter] to view runs\n\
                     Press [o] to open in browser",
                    pipeline.name, pipeline.id, status_icon, pipeline.path
                )
            } else {
                "Select a pipeline to view details".to_string()
            }
        }
        CICDFocus::Releases => {
            if let Some(release_def) = app.releases.get(app.selected_release_idx) {
                format!(
                    "Release Definition: {}\n\
                     ID: {}\n\
                     Path: {}\n\n\
                     Press [T] to trigger new release\n\
                     Press [o] to open in browser",
                    release_def.name, release_def.id, release_def.path
                )
            } else {
                "Select a release definition to view details".to_string()
            }
        }
    };

    let paragraph = Paragraph::new(content)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White));

    f.render_widget(paragraph, inner);
}

fn draw_run_preview(f: &mut Frame, app: &mut App, area: Rect, border_color: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" Run Details ");

    let inner = block.inner(area);
    f.render_widget(block, area);

    let content = if let Some(run) = app.pipeline_runs.get(app.selected_pipeline_run_idx) {
        let build_num = run.build_number.as_deref().unwrap_or("?");
        let status = run.status.as_deref().unwrap_or("unknown");
        let result = run.result.as_deref().unwrap_or("-");
        let branch = run
            .source_branch
            .as_deref()
            .unwrap_or("")
            .trim_start_matches("refs/heads/");
        let requested_by = run
            .requested_for
            .as_ref()
            .and_then(|u| u.display_name.as_deref())
            .unwrap_or("unknown");

        // Format times
        let start = run.start_time.as_deref().unwrap_or("-");
        let finish = run.finish_time.as_deref().unwrap_or("-");

        // Status with icon
        let (status_icon, _status_color) = match (status, result) {
            ("completed", "succeeded") => ("✓", "green"),
            ("completed", "failed") => ("✗", "red"),
            ("completed", "canceled") => ("⊘", "yellow"),
            ("inProgress", _) => ("⟳", "cyan"),
            ("notStarted", _) => ("○", "gray"),
            _ => ("?", "gray"),
        };

        format!(
            "Build: #{}\n\
             Status: {} {}\n\
             Result: {}\n\
             Branch: {}\n\
             Requested by: {}\n\n\
             Started: {}\n\
             Finished: {}\n\n\
             Press [Enter] to view tasks\n\
             Press [o] to open in browser\n\
             Press [Esc] to go back",
            build_num,
            status_icon,
            status,
            result,
            branch,
            requested_by,
            format_time(start),
            format_time(finish),
        )
    } else {
        "Select a run to view details".to_string()
    };

    let paragraph = Paragraph::new(content)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White));

    f.render_widget(paragraph, inner);
}

fn draw_release_preview(f: &mut Frame, app: &mut App, area: Rect, border_color: Color) {
    let focused = app.cicd_focus == CICDFocus::Preview;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" Release Details ");

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(release) = app.release_list.get(app.selected_release_item_idx) {
        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("Release: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &release.name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(release.id.to_string(), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    release.status.as_deref().unwrap_or("-"),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Created: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    release
                        .created_on
                        .as_deref()
                        .map(format_time)
                        .unwrap_or_else(|| "-".to_string()),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(""),
            Line::styled(
                "── Stages ──",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        // Show environments/stages
        if let Some(envs) = &release.environments {
            for env in envs {
                let (icon, icon_color) = match env.status.as_deref() {
                    Some("succeeded") => ("✓", Color::Green),
                    Some("rejected") | Some("failed") => ("✗", Color::Red),
                    Some("inProgress") => ("⟳", Color::Cyan),
                    Some("canceled") | Some("cancelled") => ("⊘", Color::Yellow),
                    Some("partiallySucceeded") => ("◐", Color::Yellow),
                    Some("notStarted") | Some("scheduled") => ("○", Color::DarkGray),
                    _ => ("○", Color::DarkGray),
                };

                let status_text = env.status.as_deref().unwrap_or("unknown");

                lines.push(Line::from(vec![
                    Span::styled(format!("  {icon} "), Style::default().fg(icon_color)),
                    Span::styled(&env.name, Style::default().fg(Color::White)),
                    Span::raw(": "),
                    Span::styled(status_text, Style::default().fg(icon_color)),
                ]));
            }
        } else {
            lines.push(Line::styled(
                "  Loading stages...",
                Style::default().fg(Color::DarkGray),
            ));
        }

        lines.push(Line::from(""));
        lines.push(Line::styled(
            "Press [o] to open in browser",
            Style::default().fg(Color::DarkGray),
        ));

        let total_lines = lines.len();
        let visible_height = inner.height as usize;

        // Clamp scroll position and update app state
        let max_scroll = total_lines.saturating_sub(visible_height);
        if app.cicd_preview_scroll as usize > max_scroll {
            app.cicd_preview_scroll = max_scroll as u16;
        }
        let scroll = app.cicd_preview_scroll as usize;

        let paragraph = Paragraph::new(lines).scroll((scroll as u16, 0));
        f.render_widget(paragraph, inner);

        // Draw scrollbar if content exceeds visible area
        if total_lines > visible_height && focused {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));
            // Use max_scroll as the scrollable range for accurate positioning
            let mut scrollbar_state = ScrollbarState::new(max_scroll.max(1))
                .position(scroll)
                .viewport_content_length(visible_height);
            f.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
        }
    } else {
        let msg = if app.cicd_loading {
            "Loading release..."
        } else {
            "Select a release to view details"
        };
        let paragraph = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, inner);
    }
}

fn draw_log_preview(f: &mut Frame, app: &mut App, area: Rect, border_color: Color) {
    // Get selected task info for title
    let tasks = app.get_timeline_tasks();
    let task_name = tasks
        .get(app.selected_task_idx)
        .and_then(|t| t.name.as_deref())
        .unwrap_or("Task");

    let title = format!(" Log: {task_name} ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.build_log_lines.is_empty() {
        let msg = if app.cicd_loading {
            "Loading logs..."
        } else {
            "No logs available. Press [Enter] on a task to load logs."
        };
        let paragraph = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, inner);
    } else {
        // Calculate visible area
        let visible_height = inner.height as usize;
        let total_lines = app.build_log_lines.len();

        // Clamp scroll position
        let max_scroll = total_lines.saturating_sub(visible_height);
        if app.log_scroll > max_scroll {
            app.log_scroll = max_scroll;
        }

        // Get visible lines
        let start = app.log_scroll;
        let end = (start + visible_height).min(total_lines);

        // Calculate max width for truncation (inner width minus scrollbar space)
        let max_width = inner.width.saturating_sub(2) as usize;

        let visible_lines: Vec<Line> = app.build_log_lines[start..end]
            .iter()
            .map(|line| {
                // Strip ANSI escape sequences and control characters
                let clean_line = strip_ansi_and_control(line);

                // Truncate to visible width to prevent overflow
                let truncated: String = clean_line.chars().take(max_width).collect();

                // Color code log lines based on content
                let style = if truncated.contains("error")
                    || truncated.contains("Error")
                    || truncated.contains("ERROR")
                {
                    Style::default().fg(Color::Red)
                } else if truncated.contains("warning")
                    || truncated.contains("Warning")
                    || truncated.contains("WARN")
                {
                    Style::default().fg(Color::Yellow)
                } else if truncated.contains("##[section]") {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if truncated.starts_with("##[") {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::styled(truncated, style)
            })
            .collect();

        let paragraph = Paragraph::new(visible_lines);
        f.render_widget(paragraph, inner);

        // Draw scrollbar if needed
        if total_lines > visible_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));
            // Use max_scroll as the scrollable range so position matches correctly
            // At bottom: position == max_scroll, scrollbar at bottom
            let mut scrollbar_state =
                ScrollbarState::new(max_scroll.max(1)).position(app.log_scroll);

            f.render_stateful_widget(
                scrollbar,
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }
    }
}

fn draw_stage_preview(f: &mut Frame, app: &mut App, area: Rect, border_color: Color) {
    let stage_name = app
        .release_stages
        .get(app.selected_release_stage_idx)
        .map(|s| s.name.as_str())
        .unwrap_or("Stage");

    let title = format!(" Stage: {stage_name} ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(stage) = app.release_stages.get(app.selected_release_stage_idx) {
        // Check for pending approval first
        let has_pending_approval = stage
            .pre_deploy_approvals
            .iter()
            .any(|a| a.status.as_deref() == Some("pending"));

        let (icon, icon_color, status_text) = if has_pending_approval {
            (
                "⏳",
                Color::Yellow,
                "pending approval (press 'a' to approve)",
            )
        } else {
            match stage.status.as_deref() {
                Some("succeeded") => ("✓", Color::Green, "succeeded"),
                Some("rejected") | Some("failed") => {
                    ("✗", Color::Red, stage.status.as_deref().unwrap_or("failed"))
                }
                Some("inProgress") => ("⟳", Color::Cyan, "inProgress"),
                Some("canceled") | Some("cancelled") => ("⊘", Color::Yellow, "canceled"),
                Some("partiallySucceeded") => ("◐", Color::Yellow, "partiallySucceeded"),
                Some("notStarted") | Some("scheduled") => (
                    "○",
                    Color::DarkGray,
                    stage.status.as_deref().unwrap_or("notStarted"),
                ),
                _ => (
                    "○",
                    Color::DarkGray,
                    stage.status.as_deref().unwrap_or("unknown"),
                ),
            }
        };

        let lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("Stage: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &stage.name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(stage.id.to_string(), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{icon} {status_text}"),
                    Style::default().fg(icon_color),
                ),
            ]),
            Line::from(""),
            Line::styled(
                "Press [Enter] to view tasks",
                Style::default().fg(Color::DarkGray),
            ),
            Line::styled(
                "Press [Esc] to go back",
                Style::default().fg(Color::DarkGray),
            ),
        ];

        let paragraph = Paragraph::new(lines);
        f.render_widget(paragraph, inner);
    } else {
        let msg = if app.cicd_loading {
            "Loading stage..."
        } else {
            "Select a stage to view details"
        };
        let paragraph = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, inner);
    }
}

fn draw_release_task_log(f: &mut Frame, app: &mut App, area: Rect, border_color: Color) {
    // Get selected task info for title
    let task_name = app
        .release_tasks
        .get(app.selected_release_task_idx)
        .and_then(|t| t.name.as_deref())
        .unwrap_or("Task");

    let title = format!(" Log: {task_name} ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.release_task_logs.is_empty() {
        let msg = if app.cicd_loading {
            "Loading logs..."
        } else {
            "No logs available. Press [Enter] on a task to load logs."
        };
        let paragraph = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, inner);
    } else {
        // Same approach as pipeline logs - no wrapping, manual line slicing
        let visible_height = inner.height as usize;
        let total_lines = app.release_task_logs.len();

        // Clamp scroll position
        let max_scroll = total_lines.saturating_sub(visible_height);
        if app.log_scroll > max_scroll {
            app.log_scroll = max_scroll;
        }

        // Get visible lines
        let start = app.log_scroll;
        let end = (start + visible_height).min(total_lines);

        // Calculate max width for truncation (inner width minus scrollbar space)
        let max_width = inner.width.saturating_sub(2) as usize;

        let visible_lines: Vec<Line> = app.release_task_logs[start..end]
            .iter()
            .map(|line| {
                // Strip ANSI escape sequences and control characters
                let clean_line = strip_ansi_and_control(line);

                // Truncate to visible width to prevent overflow
                let truncated: String = clean_line.chars().take(max_width).collect();

                // Color code log lines based on content
                let style = if truncated.contains("error")
                    || truncated.contains("Error")
                    || truncated.contains("ERROR")
                {
                    Style::default().fg(Color::Red)
                } else if truncated.contains("warning")
                    || truncated.contains("Warning")
                    || truncated.contains("WARN")
                {
                    Style::default().fg(Color::Yellow)
                } else if truncated.contains("##[section]") {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if truncated.starts_with("##[") {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::styled(truncated, style)
            })
            .collect();

        let paragraph = Paragraph::new(visible_lines);
        f.render_widget(paragraph, inner);

        // Draw scrollbar if needed
        if total_lines > visible_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));
            // Use max_scroll as the scrollable range so position matches correctly
            // At bottom: position == max_scroll, scrollbar at bottom
            let mut scrollbar_state =
                ScrollbarState::new(max_scroll.max(1)).position(app.log_scroll);

            f.render_stateful_widget(
                scrollbar,
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }
    }
}

/// Format ISO timestamp to a more readable format
fn format_time(iso: &str) -> String {
    if iso == "-" {
        return "-".to_string();
    }
    // Try to parse and format, or return as-is
    if let Some(dt_part) = iso.split('T').nth(1) {
        if let Some(time) = dt_part.split('.').next() {
            return time.to_string();
        }
    }
    iso.to_string()
}

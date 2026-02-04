use crate::app::{App, CICDFocus, InputMode, ReleaseDrillDown};
use fuzzy_matcher::FuzzyMatcher;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.cicd_focus == CICDFocus::Releases;

    let border_color = if focused {
        app.config.theme.parse_color(&app.config.theme.border_active)
    } else {
        app.config.theme.parse_color(&app.config.theme.border)
    };

    // Check if search input should be shown above this panel
    let show_search = app.input_mode == InputMode::CICDSearch && focused;

    if show_search {
        // Split area: search input (3 lines) + content
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        draw_search_input(f, app, chunks[0]);
        draw_content(f, app, chunks[1], border_color, focused);
    } else {
        draw_content(f, app, area, border_color, focused);
    }
}

fn draw_content(f: &mut Frame, app: &mut App, area: Rect, border_color: Color, focused: bool) {
    match app.release_drill_down {
        ReleaseDrillDown::Tasks => draw_release_tasks(f, app, area, border_color, focused),
        ReleaseDrillDown::Stages => draw_release_stages(f, app, area, border_color, focused),
        ReleaseDrillDown::Items => draw_release_list(f, app, area, border_color, focused),
        ReleaseDrillDown::None => draw_release_definitions(f, app, area, border_color, focused),
    }
}

fn draw_search_input(f: &mut Frame, app: &App, area: Rect) {
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Search (Enter to apply, Esc to cancel) ")
        .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let input = Paragraph::new(app.cicd_search_query.as_str())
        .block(input_block)
        .style(Style::default().fg(Color::White));
    f.render_widget(input, area);

    // Position cursor
    let cursor_x = area.x + 1 + app.cicd_search_query.chars().count() as u16;
    let cursor_y = area.y + 1;
    if cursor_x < area.x + area.width - 1 {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn draw_release_definitions(f: &mut Frame, app: &mut App, area: Rect, border_color: Color, focused: bool) {
    // Collect filtered data to avoid borrow issues
    let search_query = app.cicd_search_query.clone();
    let is_releases_focused = app.cicd_focus == CICDFocus::Releases;
    let selected_idx = app.selected_release_idx;
    let total_count = app.releases.len();
    let pinned = app.pinned_releases.clone();

    // (orig_idx, id, name, is_pinned)
    let mut filtered_data: Vec<(usize, i32, String, bool)> = if search_query.is_empty() || !is_releases_focused {
        app.releases.iter().enumerate()
            .map(|(i, r)| (i, r.id, r.name.clone(), pinned.contains(&r.id)))
            .collect()
    } else {
        app.releases.iter().enumerate()
            .filter(|(_, r)| app.fuzzy_matcher.fuzzy_match(&r.name, &search_query).is_some())
            .map(|(i, r)| (i, r.id, r.name.clone(), pinned.contains(&r.id)))
            .collect()
    };

    // Sort: pinned first, then alphabetically by name
    filtered_data.sort_by(|a, b| {
        match (a.3, b.3) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.2.to_lowercase().cmp(&b.2.to_lowercase()),
        }
    });

    let search_indicator = if !search_query.is_empty() && is_releases_focused {
        format!(" \"{search_query}\"")
    } else {
        String::new()
    };
    let title = format!(" Releases ({}/{}) [l]{} ", filtered_data.len(), total_count, search_indicator);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    if filtered_data.is_empty() {
        let msg = if !search_query.is_empty() && is_releases_focused {
            "No matches. Press Esc to clear."
        } else {
            "No release definitions. Press 'r' to refresh."
        };
        let items = vec![ListItem::new(format!("  {msg}"))];
        let list = List::new(items)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(list, area);
    } else {
        let items: Vec<ListItem> = filtered_data.iter().map(|(orig_idx, _id, name, is_pinned)| {
            let selected = *orig_idx == selected_idx;
            let prefix = if selected && focused { "▸ " } else { "  " };

            let pin_icon = if *is_pinned { "⚑ " } else { "" };

            let style = if selected && focused {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(pin_icon, Style::default().fg(Color::Rgb(220, 180, 80))),
                Span::styled("◆", Style::default().fg(Color::Magenta)),
                Span::raw(" "),
                Span::styled(name.as_str(), style),
            ]))
        }).collect();

        // Find display index for selected item
        let display_idx = filtered_data.iter().position(|(orig_idx, _, _, _)| *orig_idx == selected_idx);
        app.release_list_state.select(display_idx);

        let list = List::new(items).block(block);
        f.render_stateful_widget(list, area, &mut app.release_list_state);
    }
}

fn draw_release_list(f: &mut Frame, app: &mut App, area: Rect, border_color: Color, focused: bool) {
    let release_def_name = app.releases.get(app.selected_release_idx)
        .map(|r| r.name.as_str())
        .unwrap_or("Release");

    // Determine available actions based on selected release
    let action_hints = app.release_list.get(app.selected_release_item_idx)
        .map(|release| {
            let has_active = release.environments.as_ref()
                .map(|envs| envs.iter().any(|e| e.status.as_deref() == Some("inProgress")))
                .unwrap_or(false);
            if has_active || release.status.as_deref() == Some("active") {
                " [C]ancel"
            } else {
                ""
            }
        })
        .unwrap_or("");

    let title = format!(" {} - Releases ({}) [Esc:back]{} ",
        release_def_name, app.release_list.len(), action_hints);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    if app.release_list.is_empty() {
        let msg = "No releases found.";
        let items = vec![ListItem::new(format!("  {msg}"))];
        let list = List::new(items)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(list, area);
    } else {
        let items: Vec<ListItem> = app.release_list.iter().enumerate().map(|(i, release)| {
            let selected = i == app.selected_release_item_idx;
            let prefix = if selected && focused { "▸ " } else { "  " };

            let style = if selected && focused {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            // Build stage indicators from environments
            let mut spans = vec![
                Span::raw(prefix),
                Span::styled(&release.name, style),
                Span::raw("  "),
            ];

            // Add stage status indicators
            if let Some(envs) = &release.environments {
                for env in envs {
                    let (icon, icon_color) = match env.status.as_deref() {
                        Some("succeeded") => ("✓", Color::Green),
                        Some("rejected") | Some("failed") => ("✗", Color::Red),
                        Some("inProgress") => ("⟳", Color::Cyan),
                        Some("canceled") | Some("cancelled") => ("⊘", Color::Yellow),
                        Some("notStarted") | Some("scheduled") => ("○", Color::DarkGray),
                        Some("partiallySucceeded") => ("◐", Color::Yellow),
                        _ => ("○", Color::DarkGray),
                    };

                    // Show abbreviated env name + icon
                    let env_name: String = env.name.chars().take(3).collect();
                    spans.push(Span::styled(format!("{env_name}:"), Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled(icon, Style::default().fg(icon_color)));
                    spans.push(Span::raw(" "));
                }
            }

            ListItem::new(Line::from(spans))
        }).collect();

        app.release_item_list_state.select(Some(app.selected_release_item_idx));
        let list = List::new(items).block(block);
        f.render_stateful_widget(list, area, &mut app.release_item_list_state);
    }
}

fn draw_release_stages(f: &mut Frame, app: &mut App, area: Rect, border_color: Color, focused: bool) {
    let release_name = app.release_list.get(app.selected_release_item_idx)
        .map(|r| r.name.as_str())
        .unwrap_or("Release");

    // Determine available actions based on selected stage status and pending approvals
    let action_hints = app.release_stages.get(app.selected_release_stage_idx)
        .map(|stage| {
            // Check for pending approval first
            let has_pending_approval = stage.pre_deploy_approvals.iter()
                .any(|a| a.status.as_deref() == Some("pending"));

            if has_pending_approval {
                " [C]ancel(reject)"
            } else {
                match stage.status.as_deref() {
                    Some("inProgress") => " [C]ancel",
                    Some("succeeded") | Some("failed") | Some("canceled") | Some("rejected") | Some("notStarted") => " [T]rigger",
                    _ => "",
                }
            }
        })
        .unwrap_or("");

    let title = format!(" {} - Stages ({}) [Esc:back]{} ",
        release_name, app.release_stages.len(), action_hints);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    if app.release_stages.is_empty() {
        let msg = "No stages found.";
        let items = vec![ListItem::new(format!("  {msg}"))];
        let list = List::new(items)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(list, area);
    } else {
        let items: Vec<ListItem> = app.release_stages.iter().enumerate().map(|(i, stage)| {
            let selected = i == app.selected_release_stage_idx;
            let prefix = if selected && focused { "▸ " } else { "  " };

            // Check for pending approval first
            let has_pending_approval = stage.pre_deploy_approvals.iter()
                .any(|a| a.status.as_deref() == Some("pending"));

            let (icon, icon_color) = if has_pending_approval {
                ("⏳", Color::Yellow)  // Pending approval indicator
            } else {
                match stage.status.as_deref() {
                    Some("succeeded") => ("✓", Color::Green),
                    Some("rejected") | Some("failed") => ("✗", Color::Red),
                    Some("inProgress") => ("⟳", Color::Cyan),
                    Some("canceled") | Some("cancelled") => ("⊘", Color::Yellow),
                    Some("notStarted") | Some("scheduled") => ("○", Color::DarkGray),
                    Some("partiallySucceeded") => ("◐", Color::Yellow),
                    _ => ("○", Color::DarkGray),
                }
            };

            let style = if selected && focused {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(" "),
                Span::styled(&stage.name, style),
            ]))
        }).collect();

        app.release_stage_list_state.select(Some(app.selected_release_stage_idx));
        let list = List::new(items).block(block);
        f.render_stateful_widget(list, area, &mut app.release_stage_list_state);
    }
}

fn draw_release_tasks(f: &mut Frame, app: &mut App, area: Rect, border_color: Color, focused: bool) {
    let stage_name = app.release_stages.get(app.selected_release_stage_idx)
        .map(|s| s.name.as_str())
        .unwrap_or("Stage");

    let title = format!(" {} - Tasks ({}) [Esc:back] ", stage_name, app.release_tasks.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    if app.release_tasks.is_empty() {
        let msg = "No tasks found.";
        let items = vec![ListItem::new(format!("  {msg}"))];
        let list = List::new(items)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(list, area);
    } else {
        let items: Vec<ListItem> = app.release_tasks.iter().enumerate().map(|(i, task)| {
            let selected = i == app.selected_release_task_idx;
            let prefix = if selected && focused { "▸ " } else { "  " };

            let (icon, icon_color) = match task.status.as_deref() {
                Some("succeeded") => ("✓", Color::Green),
                Some("failed") => ("✗", Color::Red),
                Some("inProgress") => ("⟳", Color::Cyan),
                Some("skipped") | Some("canceled") => ("⊘", Color::DarkGray),
                _ => ("○", Color::DarkGray),
            };

            let name = task.name.as_deref().unwrap_or("Unknown task");

            let style = if selected && focused {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(" "),
                Span::styled(name, style),
            ]))
        }).collect();

        app.release_task_list_state.select(Some(app.selected_release_task_idx));
        let list = List::new(items).block(block);
        f.render_stateful_widget(list, area, &mut app.release_task_list_state);
    }
}

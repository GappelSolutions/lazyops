use crate::app::{App, CICDFocus, InputMode};
use fuzzy_matcher::FuzzyMatcher;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

/// Task item tuple: (index, name, state, result, has_log)
type TaskItem = (usize, String, Option<String>, Option<String>, bool);

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.cicd_focus == CICDFocus::Pipelines;

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

        // Draw content in remaining space
        match app.pipeline_drill_down {
            crate::app::PipelineDrillDown::Tasks => draw_timeline_tasks(f, app, chunks[1], border_color, focused),
            crate::app::PipelineDrillDown::Runs => draw_pipeline_runs(f, app, chunks[1], border_color, focused),
            crate::app::PipelineDrillDown::None => draw_pipeline_list(f, app, chunks[1], border_color, focused),
        }
    } else {
        // Check which drill-down mode we're in
        match app.pipeline_drill_down {
            crate::app::PipelineDrillDown::Tasks => draw_timeline_tasks(f, app, area, border_color, focused),
            crate::app::PipelineDrillDown::Runs => draw_pipeline_runs(f, app, area, border_color, focused),
            crate::app::PipelineDrillDown::None => draw_pipeline_list(f, app, area, border_color, focused),
        }
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

fn draw_pipeline_list(f: &mut Frame, app: &mut App, area: Rect, border_color: Color, focused: bool) {
    // Collect filtered data to avoid borrow issues
    let search_query = app.cicd_search_query.clone();
    let selected_idx = app.selected_pipeline_idx;
    let total_count = app.pipelines.len();
    let pinned = app.pinned_pipelines.clone();

    // (orig_idx, id, name, queue_status, is_pinned)
    let mut filtered_data: Vec<(usize, i32, String, Option<String>, bool)> = if search_query.is_empty() {
        app.pipelines.iter().enumerate()
            .map(|(i, p)| (i, p.id, p.name.clone(), p.queue_status.clone(), pinned.contains(&p.id)))
            .collect()
    } else {
        app.pipelines.iter().enumerate()
            .filter(|(_, p)| app.fuzzy_matcher.fuzzy_match(&p.name, &search_query).is_some())
            .map(|(i, p)| (i, p.id, p.name.clone(), p.queue_status.clone(), pinned.contains(&p.id)))
            .collect()
    };

    // Sort: pinned first, then alphabetically by name
    filtered_data.sort_by(|a, b| {
        match (a.4, b.4) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.2.to_lowercase().cmp(&b.2.to_lowercase()),
        }
    });

    // Update selected index to match display order
    // Find display position of currently selected item, or select first if not found
    let display_idx = filtered_data.iter().position(|(orig_idx, _, _, _, _)| *orig_idx == selected_idx);
    if display_idx.is_none() && !filtered_data.is_empty() {
        // Selected item not in filtered list, select first item
        app.selected_pipeline_idx = filtered_data[0].0;
    }

    let search_indicator = if !search_query.is_empty() {
        format!(" \"{search_query}\"")
    } else {
        String::new()
    };
    let title = format!(" Pipelines ({}/{}) [h]{} ", filtered_data.len(), total_count, search_indicator);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    if filtered_data.is_empty() {
        let msg = if !search_query.is_empty() {
            "No matches. Press Esc to clear search."
        } else {
            "No pipelines. Press 'r' to refresh."
        };
        let items = vec![ListItem::new(format!("  {msg}"))];
        let list = List::new(items)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(list, area);
    } else {
        let items: Vec<ListItem> = filtered_data.iter().map(|(orig_idx, _id, name, queue_status, is_pinned)| {
            let selected = *orig_idx == selected_idx;
            let prefix = if selected && focused { "▸ " } else { "  " };

            let status_icon = match queue_status.as_deref() {
                Some("enabled") => "●",
                Some("disabled") => "○",
                Some("paused") => "◐",
                _ => "●",
            };

            let pin_icon = if *is_pinned { "⚑ " } else { "" };

            let style = if selected && focused {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(pin_icon, Style::default().fg(Color::Rgb(220, 180, 80))),
                Span::styled(status_icon, Style::default().fg(Color::Green)),
                Span::raw(" "),
                Span::styled(name.as_str(), style),
            ]))
        }).collect();

        // Find display index for selected item
        let display_idx = filtered_data.iter().position(|(orig_idx, _, _, _, _)| *orig_idx == selected_idx);
        app.pipeline_list_state.select(display_idx);

        let list = List::new(items).block(block);
        f.render_stateful_widget(list, area, &mut app.pipeline_list_state);
    }
}

fn draw_pipeline_runs(f: &mut Frame, app: &mut App, area: Rect, border_color: Color, focused: bool) {
    let pipeline_name = app.pipelines.get(app.selected_pipeline_idx)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Pipeline".to_string());

    // Collect filtered data to avoid borrow issues
    let search_query = app.cicd_search_query.clone();
    let selected_idx = app.selected_pipeline_run_idx;
    let total_count = app.pipeline_runs.len();

    #[allow(clippy::type_complexity)]
    let filtered_data: Vec<(usize, Option<String>, Option<String>, Option<String>, Option<String>)> = if search_query.is_empty() {
        app.pipeline_runs.iter().enumerate()
            .map(|(i, r)| (i, r.build_number.clone(), r.source_branch.clone(), r.status.clone(), r.result.clone()))
            .collect()
    } else {
        app.pipeline_runs.iter().enumerate()
            .filter(|(_, r)| {
                let build_match = r.build_number.as_ref()
                    .map(|n| app.fuzzy_matcher.fuzzy_match(n, &search_query).is_some())
                    .unwrap_or(false);
                let branch_match = r.source_branch.as_ref()
                    .map(|b| app.fuzzy_matcher.fuzzy_match(b, &search_query).is_some())
                    .unwrap_or(false);
                build_match || branch_match
            })
            .map(|(i, r)| (i, r.build_number.clone(), r.source_branch.clone(), r.status.clone(), r.result.clone()))
            .collect()
    };

    let search_indicator = if !search_query.is_empty() {
        format!(" \"{search_query}\"")
    } else {
        String::new()
    };
    // Determine available actions based on selected run status
    let action_hints = app.pipeline_runs.get(selected_idx)
        .map(|run| {
            match run.status.as_deref() {
                Some("inProgress") => " [C]ancel",
                Some("completed") => " [T]rigger",
                _ => "",
            }
        })
        .unwrap_or("");

    let load_more = if app.pipeline_runs_limited && total_count >= 10 {
        " L:all"
    } else {
        ""
    };
    let title = format!(" {} - Runs ({}/{}) [Esc:back]{}{}{} ",
        pipeline_name, filtered_data.len(), total_count, action_hints, load_more, search_indicator);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    if filtered_data.is_empty() {
        let msg = if !search_query.is_empty() {
            "No matches. Press Esc to clear."
        } else {
            "No runs found."
        };
        let items = vec![ListItem::new(format!("  {msg}"))];
        let list = List::new(items)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(list, area);
    } else {
        let items: Vec<ListItem> = filtered_data.iter().map(|(orig_idx, build_number, source_branch, status, result)| {
            let selected = *orig_idx == selected_idx;
            let prefix = if selected && focused { "▸ " } else { "  " };

            // Status icon
            let (icon, icon_color) = match (status.as_deref(), result.as_deref()) {
                (Some("completed"), Some("succeeded")) => ("✓", Color::Green),
                (Some("completed"), Some("failed")) => ("✗", Color::Red),
                (Some("completed"), Some("canceled")) => ("⊘", Color::Yellow),
                (Some("inProgress"), _) => ("⟳", Color::Cyan),
                (Some("notStarted"), _) => ("○", Color::DarkGray),
                _ => ("?", Color::DarkGray),
            };

            let build_num = build_number.as_deref().unwrap_or("?");
            let branch = source_branch.as_deref()
                .unwrap_or("")
                .trim_start_matches("refs/heads/");

            let style = if selected && focused {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(" "),
                Span::styled(format!("#{build_num}"), style),
                Span::raw("  "),
                Span::styled(branch, Style::default().fg(Color::DarkGray)),
            ]))
        }).collect();

        // Find display index for selected item
        let display_idx = filtered_data.iter().position(|(orig_idx, _, _, _, _)| *orig_idx == selected_idx);
        app.pipeline_runs_list_state.select(display_idx);

        let list = List::new(items).block(block);
        f.render_stateful_widget(list, area, &mut app.pipeline_runs_list_state);
    }
}

fn draw_timeline_tasks(f: &mut Frame, app: &mut App, area: Rect, border_color: Color, focused: bool) {
    let run_name = app.pipeline_runs.get(app.selected_pipeline_run_idx)
        .and_then(|r| r.build_number.as_ref())
        .map(|n| format!("#{n}"))
        .unwrap_or_else(|| "Run".to_string());

    // Collect task info first to avoid borrow issues
    let selected_task_idx = app.selected_task_idx;
    let search_query = app.cicd_search_query.clone();

    // Use same logic as get_timeline_tasks(): filter to Task type and sort by order
    // The index stored is now the position in this filtered/sorted list, matching events.rs
    let mut tasks: Vec<_> = app.timeline_records.iter()
        .filter(|r| r.record_type.as_deref() == Some("Task"))
        .collect();
    tasks.sort_by_key(|r| r.order.unwrap_or(999));

    let all_task_items: Vec<TaskItem> = tasks.iter()
        .enumerate()
        .map(|(idx, task)| {
            let name = task.name.clone().unwrap_or_else(|| "Unknown task".to_string());
            let state = task.state.clone();
            let result = task.result.clone();
            let has_log = task.log.is_some();
            (idx, name, state, result, has_log)
        })
        .collect();

    // Filter by search query
    let task_items: Vec<_> = if search_query.is_empty() {
        all_task_items
    } else {
        all_task_items.into_iter()
            .filter(|(_, name, _, _, _)| {
                app.fuzzy_matcher.fuzzy_match(name, &search_query).is_some()
            })
            .collect()
    };

    let search_indicator = if !search_query.is_empty() {
        format!(" \"{search_query}\"")
    } else {
        String::new()
    };
    let title = format!(" {} - Tasks ({}) [Esc:back]{} ", run_name, task_items.len(), search_indicator);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    if task_items.is_empty() {
        let msg = if !search_query.is_empty() {
            "No matches. Press Esc to clear."
        } else {
            "No tasks found."
        };
        let items = vec![ListItem::new(format!("  {msg}"))];
        let list = List::new(items)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(list, area);
    } else {
        let items: Vec<ListItem> = task_items.iter().map(|(orig_idx, name, state, result, _has_log)| {
            let selected = *orig_idx == selected_task_idx;
            let prefix = if selected && focused { "▸ " } else { "  " };

            // Status icon based on state and result
            let (icon, icon_color) = match (state.as_deref(), result.as_deref()) {
                (Some("completed"), Some("succeeded")) => ("✓", Color::Green),
                (Some("completed"), Some("failed")) => ("✗", Color::Red),
                (Some("completed"), Some("skipped")) => ("⊘", Color::DarkGray),
                (Some("completed"), Some("canceled")) => ("⊘", Color::Yellow),
                (Some("inProgress"), _) => ("⟳", Color::Cyan),
                (Some("pending"), _) => ("○", Color::DarkGray),
                _ => ("?", Color::DarkGray),
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
                Span::styled(name.as_str(), style),
            ]))
        }).collect();

        // Find display index for selected item
        let display_idx = task_items.iter().position(|(orig_idx, _, _, _, _)| *orig_idx == selected_task_idx);
        app.task_list_state.select(display_idx);

        let list = List::new(items).block(block);
        f.render_stateful_widget(list, area, &mut app.task_list_state);
    }
}

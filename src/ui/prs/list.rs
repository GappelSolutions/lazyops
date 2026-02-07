use crate::app::{App, InputMode, PRDrillDown, PRFocus};
use crate::azure::PullRequest;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.pr_focus.is_list();

    let border_color = if focused {
        app.config
            .theme
            .parse_color(&app.config.theme.border_active)
    } else {
        app.config.theme.parse_color(&app.config.theme.border)
    };

    // Check if search input should be shown above this panel
    let show_search = app.input_mode == InputMode::CICDSearch && focused;

    // When in PR drill-down, show pane tab bar at top
    let show_pane_tabs = app.pr_drill_down == PRDrillDown::PRs;

    let mut constraints: Vec<Constraint> = Vec::new();
    if show_pane_tabs {
        constraints.push(Constraint::Length(3)); // Pane tab bar
    }
    if show_search {
        constraints.push(Constraint::Length(3)); // Search input
    }
    constraints.push(Constraint::Min(0)); // List content

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut chunk_idx = 0;

    if show_pane_tabs {
        draw_pane_tabs(f, app, chunks[chunk_idx], border_color);
        chunk_idx += 1;
    }

    if show_search {
        draw_search_input(f, app, chunks[chunk_idx]);
        chunk_idx += 1;
    }

    let list_area = chunks[chunk_idx];

    match app.pr_drill_down {
        PRDrillDown::Repos => draw_repo_list(f, app, list_area, border_color, focused),
        PRDrillDown::PRs => draw_pr_list(f, app, list_area, border_color, focused),
    }
}

fn draw_pane_tabs(f: &mut Frame, app: &App, area: Rect, border_color: Color) {
    let titles = vec!["Active", "Mine", "Completed", "Abandoned"];
    let active_pane = if app.pr_focus.is_list() {
        app.pr_focus
    } else {
        app.pr_last_list_focus
    };
    let selected = match active_pane {
        PRFocus::Active => 0,
        PRFocus::Mine => 1,
        PRFocus::Completed => 2,
        PRFocus::Abandoned => 3,
        PRFocus::Preview => 0,
    };

    let tabs = Tabs::new(titles)
        .select(selected)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" | ");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" h/l ");

    f.render_widget(tabs.block(block), area);
}

fn draw_search_input(f: &mut Frame, app: &App, area: Rect) {
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Search (Enter to apply, Esc to cancel) ")
        .title_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let input = Paragraph::new(app.pr_search_query.as_str())
        .block(input_block)
        .style(Style::default().fg(Color::White));
    f.render_widget(input, area);

    // Position cursor
    let cursor_x = area.x + 1 + app.pr_search_query.chars().count() as u16;
    let cursor_y = area.y + 1;
    if cursor_x < area.x + area.width - 1 {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn draw_repo_list(f: &mut Frame, app: &mut App, area: Rect, border_color: Color, focused: bool) {
    let search_query = app.pr_search_query.clone();
    let selected_idx = app.selected_repo_idx;
    let total_count = app.repositories.len();

    // (orig_idx, name)
    let filtered_data: Vec<(usize, String)> = if search_query.is_empty() {
        app.repositories
            .iter()
            .enumerate()
            .map(|(i, r)| (i, r.name.clone()))
            .collect()
    } else {
        app.repositories
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                app.fuzzy_matcher
                    .fuzzy_match(&r.name, &search_query)
                    .is_some()
            })
            .map(|(i, r)| (i, r.name.clone()))
            .collect()
    };

    // Update selected index if current selection not in filtered list
    let display_idx = filtered_data
        .iter()
        .position(|(orig_idx, _)| *orig_idx == selected_idx);
    if display_idx.is_none() && !filtered_data.is_empty() {
        app.selected_repo_idx = filtered_data[0].0;
    }

    let search_indicator = if !search_query.is_empty() {
        format!(" \"{search_query}\"")
    } else {
        String::new()
    };
    let title = format!(
        " Repositories ({}/{}) [h]{} ",
        filtered_data.len(),
        total_count,
        search_indicator
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    if filtered_data.is_empty() {
        let msg = if !search_query.is_empty() {
            "No matches. Press Esc to clear search."
        } else {
            "No repositories. Press 'r' to refresh."
        };
        let items = vec![ListItem::new(format!("  {msg}"))];
        let list = List::new(items)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(list, area);
    } else {
        let items: Vec<ListItem> = filtered_data
            .iter()
            .map(|(orig_idx, name)| {
                let selected = *orig_idx == selected_idx;
                let prefix = if selected && focused { "▸ " } else { "  " };

                let style = if selected && focused {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                ListItem::new(Line::from(vec![
                    Span::raw(prefix),
                    Span::styled("●", Style::default().fg(Color::Green)),
                    Span::raw(" "),
                    Span::styled(name.as_str(), style),
                ]))
            })
            .collect();

        let display_idx = filtered_data
            .iter()
            .position(|(orig_idx, _)| *orig_idx == selected_idx);
        app.repo_list_state.select(display_idx);

        let list = List::new(items).block(block);
        f.render_stateful_widget(list, area, &mut app.repo_list_state);
    }
}

fn draw_pr_list(f: &mut Frame, app: &mut App, area: Rect, border_color: Color, focused: bool) {
    let repo_name = app
        .current_repo_name
        .clone()
        .unwrap_or_else(|| "Repo".to_string());
    let search_query = app.pr_search_query.clone();
    let selected_idx = app.selected_pr_idx;
    let total_count = app.pull_requests().len();
    let active_pane = if app.pr_focus.is_list() {
        app.pr_focus
    } else {
        app.pr_last_list_focus
    };
    let filter_label = active_pane.label();

    struct PRRow {
        orig_idx: usize,
        id: i32,
        title: String,
        target: String,
        status_icon: &'static str,
        votes: String,
        is_draft: bool,
        created_by: String,
    }

    let map_pr = |i: usize, pr: &PullRequest| -> PRRow {
        let target = pr
            .target_branch
            .as_deref()
            .map(PullRequest::short_branch)
            .unwrap_or("?")
            .to_string();
        let votes = format_vote_summary(&pr.reviewers);
        let created_by = pr
            .created_by
            .as_ref()
            .map(|c| {
                // Use initials: "Christian Gappel" -> "CG"
                c.display_name
                    .split_whitespace()
                    .filter_map(|w| w.chars().next())
                    .collect::<String>()
            })
            .unwrap_or_default();
        PRRow {
            orig_idx: i,
            id: pr.pull_request_id,
            title: pr.title.clone(),
            target,
            status_icon: pr.status_icon(),
            votes,
            is_draft: pr.is_draft,
            created_by,
        }
    };

    let filtered_data: Vec<PRRow> = if search_query.is_empty() {
        app.pull_requests()
            .iter()
            .enumerate()
            .map(|(i, pr)| map_pr(i, pr))
            .collect()
    } else {
        app.pull_requests()
            .iter()
            .enumerate()
            .filter(|(_, pr)| {
                let title_match = app
                    .fuzzy_matcher
                    .fuzzy_match(&pr.title, &search_query)
                    .is_some();
                let id_match = app
                    .fuzzy_matcher
                    .fuzzy_match(&pr.pull_request_id.to_string(), &search_query)
                    .is_some();
                let branch_match = pr
                    .source_branch
                    .as_ref()
                    .map(|b| app.fuzzy_matcher.fuzzy_match(b, &search_query).is_some())
                    .unwrap_or(false);
                title_match || id_match || branch_match
            })
            .map(|(i, pr)| map_pr(i, pr))
            .collect()
    };

    // Update selected index if current selection not in filtered list
    let display_idx = filtered_data
        .iter()
        .position(|r| r.orig_idx == selected_idx);
    if display_idx.is_none() && !filtered_data.is_empty() {
        app.selected_pr_idx = filtered_data[0].orig_idx;
    }

    let search_indicator = if !search_query.is_empty() {
        format!(" \"{search_query}\"")
    } else {
        String::new()
    };
    let title = format!(
        " PRs - {} ({}/{}) [{}]{} ",
        filter_label,
        filtered_data.len(),
        total_count,
        repo_name,
        search_indicator
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    if filtered_data.is_empty() {
        let msg = if !search_query.is_empty() {
            "No matches. Press Esc to clear search."
        } else {
            "No PRs found."
        };
        let items = vec![ListItem::new(format!("  {msg}"))];
        let list = List::new(items)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(list, area);
    } else {
        let items: Vec<ListItem> = filtered_data
            .iter()
            .map(|row| {
                let selected = row.orig_idx == selected_idx;
                let prefix = if selected && focused { "▸ " } else { "  " };

                let status_color = match row.status_icon {
                    "●" => Color::Green,
                    "✓" => Color::Green,
                    "✗" => Color::Red,
                    "◑" => Color::Yellow,
                    _ => Color::DarkGray,
                };

                let style = if selected && focused {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let draft_indicator = if row.is_draft { " [DRAFT]" } else { "" };

                // Truncate title to fit in available space
                let max_title_len = area.width.saturating_sub(45) as usize;
                let display_title = if row.title.len() > max_title_len {
                    format!("{}...", &row.title[..max_title_len.saturating_sub(3)])
                } else {
                    row.title.clone()
                };

                ListItem::new(Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(row.status_icon, Style::default().fg(status_color)),
                    Span::raw(" "),
                    Span::styled(format!("#{}", row.id), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(display_title, style),
                    Span::styled(draft_indicator, Style::default().fg(Color::Yellow)),
                    Span::raw(" "),
                    Span::styled(&row.votes, Style::default().fg(Color::Magenta)),
                    Span::raw(" "),
                    Span::styled(&row.created_by, Style::default().fg(Color::Yellow)),
                    Span::styled(
                        format!(" → {}", row.target),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();

        let display_idx = filtered_data
            .iter()
            .position(|r| r.orig_idx == selected_idx);
        app.pr_list_state.select(display_idx);

        let list = List::new(items).block(block);
        f.render_stateful_widget(list, area, &mut app.pr_list_state);
    }
}

/// Format reviewer votes into a compact summary
fn format_vote_summary(reviewers: &[crate::azure::PRReviewer]) -> String {
    if reviewers.is_empty() {
        return String::new();
    }
    let icons: Vec<&str> = reviewers
        .iter()
        .map(|r| PullRequest::vote_icon(r.vote))
        .collect();
    format!("[{}]", icons.join(""))
}

use crate::app::{App, PRFocus, PRPreviewTab};
use crate::azure::PullRequest;
use chrono::DateTime;
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Tabs, Wrap,
};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.pr_focus == PRFocus::Preview;

    let border_color = if focused {
        app.config
            .theme
            .parse_color(&app.config.theme.border_active)
    } else {
        app.config.theme.parse_color(&app.config.theme.border)
    };

    // Vertical split: tab bar (3) + content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Draw tab bar
    draw_tab_bar(f, app, chunks[0], border_color);

    // Draw content based on selected tab
    match app.pr_preview_tab {
        PRPreviewTab::Details => draw_details(f, app, chunks[1], border_color),
        PRPreviewTab::Policies => draw_policies(f, app, chunks[1], border_color),
        PRPreviewTab::Threads => draw_threads(f, app, chunks[1], border_color),
    }
}

fn draw_tab_bar(f: &mut Frame, app: &App, area: Rect, border_color: Color) {
    let titles = vec!["Details", "Policies", "Threads"];
    let selected = match app.pr_preview_tab {
        PRPreviewTab::Details => 0,
        PRPreviewTab::Policies => 1,
        PRPreviewTab::Threads => 2,
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
        .title(" Preview [Tab] ");

    f.render_widget(tabs.block(block), area);
}

fn draw_details(f: &mut Frame, app: &mut App, area: Rect, border_color: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Use selected_pr_detail if available, otherwise fall back to list
    let pr = app
        .selected_pr_detail
        .as_ref()
        .or_else(|| app.pull_requests().get(app.selected_pr_idx));

    let content = if let Some(pr) = pr {
        let source = pr
            .source_branch
            .as_deref()
            .map(PullRequest::short_branch)
            .unwrap_or("?");
        let target = pr
            .target_branch
            .as_deref()
            .map(PullRequest::short_branch)
            .unwrap_or("?");

        let status = pr.status.as_deref().unwrap_or("unknown");
        let status_display = match status {
            "active" => "● Active",
            "completed" => "✓ Completed",
            "abandoned" => "✗ Abandoned",
            _ => status,
        };

        let draft = if pr.is_draft { "Yes" } else { "No" };

        let created_by = pr
            .created_by
            .as_ref()
            .map(|c| c.display_name.as_str())
            .unwrap_or("Unknown");

        let date = pr
            .creation_date
            .as_deref()
            .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
            .map(|dt| dt.format("%d.%m.%Y %H:%M").to_string())
            .unwrap_or_else(|| pr.creation_date.as_deref().unwrap_or("Unknown").to_string());

        let merge_status = pr.merge_status.as_deref().unwrap_or("Unknown");
        let merge_display = match merge_status {
            "succeeded" => "✓ Succeeded",
            "conflicts" => "✗ Conflicts",
            "queued" => "◐ Queued",
            "notSet" => "○ Not Set",
            _ => merge_status,
        };

        let auto_complete = if pr.auto_complete_set_by.is_some() {
            let by = pr
                .auto_complete_set_by
                .as_ref()
                .map(|a| a.display_name.as_str())
                .unwrap_or("Unknown");
            format!("Yes (by {by})")
        } else {
            "No".to_string()
        };

        let description = pr.description.as_deref().unwrap_or("No description");

        // Reviewers section
        let mut reviewer_lines = String::new();
        if pr.reviewers.is_empty() {
            reviewer_lines.push_str("  No reviewers");
        } else {
            for reviewer in &pr.reviewers {
                let icon = PullRequest::vote_icon(reviewer.vote);
                let vote_color_label = match reviewer.vote {
                    10 => "Approved",
                    5 => "Approved w/ suggestions",
                    0 => "No vote",
                    -5 => "Waiting for author",
                    -10 => "Rejected",
                    _ => "Unknown",
                };
                let required = if reviewer.is_required.unwrap_or(false) {
                    " (required)"
                } else {
                    ""
                };
                reviewer_lines.push_str(&format!(
                    "  {} {} - {}{}\n",
                    icon, reviewer.display_name, vote_color_label, required
                ));
            }
        }

        format!(
            "Title: {}\n\
             ID: #{}\n\
             \n\
             Branch: {} -> {}\n\
             Status: {}\n\
             Draft: {}\n\
             Merge: {}\n\
             Auto-complete: {}\n\
             \n\
             Created by: {}\n\
             Date: {}\n\
             \n\
             Reviewers:\n\
             {}\n\
             \n\
             Description:\n\
             {}",
            pr.title,
            pr.pull_request_id,
            source,
            target,
            status_display,
            draft,
            merge_display,
            auto_complete,
            created_by,
            date,
            reviewer_lines,
            description,
        )
    } else {
        "Select a PR to view details".to_string()
    };

    let scroll = app.pr_preview_scroll;
    let paragraph = Paragraph::new(content.clone())
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .style(Style::default().fg(Color::White));
    f.render_widget(paragraph, inner);

    // Scrollbar
    let line_count = content.lines().count() as u16;
    if line_count > inner.height {
        let mut scrollbar_state =
            ScrollbarState::new(line_count.saturating_sub(inner.height) as usize)
                .position(scroll as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn draw_policies(f: &mut Frame, app: &mut App, area: Rect, border_color: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" Policies ({}) ", app.pr_policies.len()));

    if app.pr_policies.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);

        let msg = if app.pull_requests().is_empty() {
            "Select a PR to view policies"
        } else {
            "No policy evaluations"
        };
        let paragraph = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, inner);
        return;
    }

    let items: Vec<ListItem> = app
        .pr_policies
        .iter()
        .enumerate()
        .map(|(i, policy)| {
            let status = policy.status.as_deref().unwrap_or("unknown");
            let (icon, icon_color) = match status {
                "approved" => ("✓", Color::Green),
                "rejected" => ("✗", Color::Red),
                "running" => ("◐", Color::Yellow),
                "queued" => ("○", Color::DarkGray),
                "broken" => ("✗", Color::Red),
                _ => ("?", Color::DarkGray),
            };

            let policy_name = policy
                .configuration
                .as_ref()
                .and_then(|c| c.policy_type.as_ref())
                .and_then(|t| t.display_name.as_deref())
                .unwrap_or("Unknown Policy");

            let blocking = policy
                .configuration
                .as_ref()
                .map(|c| {
                    if c.is_blocking {
                        "blocking"
                    } else {
                        "optional"
                    }
                })
                .unwrap_or("unknown");

            let selected = i == app.selected_thread_idx
                && app.pr_focus == PRFocus::Preview
                && app.pr_preview_tab == PRPreviewTab::Policies;
            let prefix = if selected { "▸ " } else { "  " };

            let style = if selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(" "),
                Span::styled(policy_name, style),
                Span::raw(" "),
                Span::styled(
                    format!("({blocking})"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    f.render_stateful_widget(list, area, &mut app.policy_list_state);
}

fn draw_threads(f: &mut Frame, app: &mut App, area: Rect, border_color: Color) {
    // Filter out system/deleted threads
    let visible_threads: Vec<(usize, &crate::azure::PRThread)> = app
        .pr_threads
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            if t.is_deleted {
                return false;
            }
            // Filter out threads that are purely system-generated (no user comments)
            let has_user_comment = t
                .comments
                .iter()
                .any(|c| c.comment_type.as_deref() != Some("system"));
            has_user_comment
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" Threads ({}) ", visible_threads.len()));

    if visible_threads.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);

        let msg = if app.pull_requests().is_empty() {
            "Select a PR to view threads"
        } else {
            "No comment threads"
        };
        let paragraph = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, inner);
        return;
    }

    let focused_on_threads =
        app.pr_focus == PRFocus::Preview && app.pr_preview_tab == PRPreviewTab::Threads;

    let items: Vec<ListItem> = visible_threads
        .iter()
        .enumerate()
        .map(|(display_i, (_, thread))| {
            let status = thread.status.as_deref().unwrap_or("unknown");
            let (icon, icon_color) = match status {
                "active" => ("●", Color::Green),
                "fixed" | "closed" => ("✓", Color::DarkGray),
                "wontFix" => ("○", Color::DarkGray),
                "pending" => ("◐", Color::Yellow),
                "byDesign" => ("✓", Color::Blue),
                _ => ("○", Color::DarkGray),
            };

            // Get first user comment
            let first_comment = thread
                .comments
                .iter()
                .find(|c| c.comment_type.as_deref() != Some("system"));

            let author = first_comment
                .and_then(|c| c.author.as_ref())
                .map(|a| a.display_name.as_str())
                .unwrap_or("Unknown");

            let content = first_comment
                .and_then(|c| c.content.as_deref())
                .unwrap_or("(empty)");

            // Truncate content to fit
            let max_len = area.width.saturating_sub(20) as usize;
            let display_content = if content.len() > max_len {
                format!("{}...", &content[..max_len.saturating_sub(3)])
            } else {
                content.to_string()
            };
            // Remove newlines for single-line display
            let display_content = display_content.replace('\n', " ").replace('\r', "");

            let selected = display_i == app.selected_thread_idx && focused_on_threads;
            let prefix = if selected { "▸ " } else { "  " };

            let style = if selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(" "),
                Span::styled(author, Style::default().fg(Color::Yellow)),
                Span::raw(": "),
                Span::styled(display_content, style),
            ]))
        })
        .collect();

    if focused_on_threads {
        app.thread_list_state.select(Some(app.selected_thread_idx));
    }

    let list = List::new(items).block(block);
    f.render_stateful_widget(list, area, &mut app.thread_list_state);
}

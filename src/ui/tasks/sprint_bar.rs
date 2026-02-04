use crate::app::{App, View};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(35),  // View tabs
            Constraint::Percentage(35),  // Sprint (Tasks) or empty (CI/CD)
            Constraint::Percentage(30),  // Project
        ])
        .split(area);

    // View tabs
    draw_view_tabs(f, app, chunks[0]);

    // Middle section: Sprint selector (Tasks view) or loading status (CI/CD view)
    match app.current_view {
        View::Tasks => draw_sprint_selector(f, app, chunks[1]),
        View::CICD => draw_loading_status(f, app, chunks[1]),
    }

    // Project selector (always visible)
    draw_project_selector(f, app, chunks[2]);
}

fn draw_view_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles = vec!["[1] Tasks", "[2] CI/CD"];
    let selected = match app.current_view {
        View::Tasks => 0,
        View::CICD => 1,
    };

    let tabs = Tabs::new(titles)
        .select(selected)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        )
        .divider(" ");

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" View ");

    f.render_widget(tabs.block(block), area);
}

fn draw_sprint_selector(f: &mut Frame, app: &App, area: Rect) {
    let sprint_name = app.selected_sprint()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "No sprint".into());

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Sprint [I] ");
    let text = Paragraph::new(sprint_name).block(block);
    f.render_widget(text, area);
}

fn draw_project_selector(f: &mut Frame, app: &App, area: Rect) {
    let project_name = app.current_project()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "No project".into());

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Project [P] ");
    let text = Paragraph::new(project_name).block(block);
    f.render_widget(text, area);
}

fn draw_loading_status(f: &mut Frame, app: &App, area: Rect) {
    // Only show loading for manual refreshes, not auto-refresh polling
    let show_loading = app.cicd_loading && !app.release_auto_refresh;

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Status ");

    if show_loading {
        // Simple spinner using frame count
        let spinner_chars = ['◐', '◓', '◑', '◒'];
        let frame = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() / 150) as usize;
        let spinner = spinner_chars[frame % spinner_chars.len()];

        let text = Paragraph::new(format!("{} Loading...", spinner))
            .block(block)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(text, area);
    } else {
        // Show empty block when not loading
        f.render_widget(block, area);
    }
}

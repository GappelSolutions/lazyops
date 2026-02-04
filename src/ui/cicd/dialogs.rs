use ratatui::{
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, List, ListItem},
    Frame,
};

use crate::app::{ReleaseTriggerDialog, DialogCursor};

/// Render the release trigger dialog as a centered popup
pub fn render_release_trigger_dialog(f: &mut Frame, dialog: &ReleaseTriggerDialog) {
    // Calculate centered popup area (60% width, 60% height)
    let area = centered_rect(60, 60, f.area());

    // Clear the background
    f.render_widget(Clear, area);

    // Main dialog block
    let block = Block::default()
        .title(format!(" Trigger Release: {} ", dialog.definition_name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if dialog.loading {
        let loading = Paragraph::new("Loading stages...")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center);
        f.render_widget(loading, inner);
        return;
    }

    // Layout: stages list, buttons
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(5),     // Stages
            Constraint::Length(3),  // Buttons
        ])
        .split(inner);

    // Count enabled stages
    let enabled_count = dialog.stages.iter().filter(|s| s.enabled).count();
    let total_count = dialog.stages.len();

    // Stages list with checkboxes
    let stages_style = Style::default().fg(Color::Yellow);
    let stages_block = Block::default()
        .title(format!(" Stages ({}/{}) - Space:toggle  a:all  n:none ", enabled_count, total_count))
        .borders(Borders::ALL)
        .border_style(stages_style);

    let stage_items: Vec<ListItem> = dialog.stages.iter().enumerate().map(|(i, stage)| {
        let checkbox = if stage.enabled { "[x]" } else { "[ ]" };
        let style = if i == dialog.selected_idx {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if stage.enabled {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        ListItem::new(format!(" {} {}", checkbox, stage.name)).style(style)
    }).collect();

    let stages_list = List::new(stage_items).block(stages_block);
    f.render_widget(stages_list, chunks[0]);

    // Buttons row
    let buttons_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let submit_style = if dialog.cursor == DialogCursor::Submit {
        Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let cancel_style = if dialog.cursor == DialogCursor::Cancel {
        Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red)
    };

    let submit = Paragraph::new(" [Enter] Create Release ")
        .style(submit_style)
        .alignment(Alignment::Center);
    let cancel = Paragraph::new(" [Esc] Cancel ")
        .style(cancel_style)
        .alignment(Alignment::Center);

    f.render_widget(submit, buttons_layout[0]);
    f.render_widget(cancel, buttons_layout[1]);
}

/// Render approval confirmation dialog
pub fn render_approval_dialog(f: &mut Frame, approval_type: &str, stage_name: &str) {
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);

    let title = format!(" {} {} ", approval_type, stage_name);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if approval_type == "Approve" { Color::Green } else { Color::Red }));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = Paragraph::new(vec![
        Line::from(""),
        Line::from(format!("Are you sure you want to {} this stage?", approval_type.to_lowercase())),
        Line::from(""),
        Line::from(Span::styled("[y] Yes  [n] No", Style::default().fg(Color::Yellow))),
    ])
    .alignment(Alignment::Center);

    f.render_widget(text, inner);
}

/// Render confirmation dialog for cancel/retrigger actions
pub fn render_confirm_action_dialog(f: &mut Frame, dialog: &crate::app::ConfirmActionDialog) {
    let area = centered_rect(50, 25, f.area());
    f.render_widget(Clear, area);

    // Determine colors based on action type (destructive = red, constructive = green)
    let (border_color, action_color) = match &dialog.action_type {
        crate::app::ConfirmActionType::CancelPipelineRun { .. } |
        crate::app::ConfirmActionType::CancelRelease { .. } |
        crate::app::ConfirmActionType::CancelReleaseEnvironment { .. } |
        crate::app::ConfirmActionType::RejectApproval { .. } => (Color::Red, Color::Red),
        crate::app::ConfirmActionType::RetriggerPipelineRun { .. } |
        crate::app::ConfirmActionType::RetriggerReleaseEnvironment { .. } => (Color::Green, Color::Green),
    };

    let block = Block::default()
        .title(format!(" {} ", dialog.title()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Layout: description + buttons
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(3),     // Description
            Constraint::Length(3),  // Buttons
        ])
        .split(inner);

    // Description
    let description = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(dialog.description(), Style::default().fg(Color::White))),
        Line::from(""),
    ])
    .alignment(Alignment::Center);
    f.render_widget(description, chunks[0]);

    // Buttons
    let buttons_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let confirm_text = match &dialog.action_type {
        crate::app::ConfirmActionType::CancelPipelineRun { .. } |
        crate::app::ConfirmActionType::CancelRelease { .. } |
        crate::app::ConfirmActionType::CancelReleaseEnvironment { .. } => "[y] Yes, Cancel",
        crate::app::ConfirmActionType::RejectApproval { .. } => "[y] Yes, Reject",
        crate::app::ConfirmActionType::RetriggerPipelineRun { .. } |
        crate::app::ConfirmActionType::RetriggerReleaseEnvironment { .. } => "[y] Yes, Retrigger",
    };

    let confirm = Paragraph::new(confirm_text)
        .style(Style::default().fg(action_color))
        .alignment(Alignment::Center);
    let cancel = Paragraph::new("[n] No")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);

    f.render_widget(confirm, buttons_layout[0]);
    f.render_widget(cancel, buttons_layout[1]);
}

/// Helper to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

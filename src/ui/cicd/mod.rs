mod pipelines;
mod releases;
mod preview;
pub mod dialogs;

use crate::app::App;
use ratatui::prelude::*;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    // Horizontal split: left (50%) + right (50%)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    // Left side: vertical split for Pipelines (50%) + Releases (50%)
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(chunks[0]);

    // Draw panes
    pipelines::draw(f, app, left_chunks[0]);
    releases::draw(f, app, left_chunks[1]);
    preview::draw(f, app, chunks[1]);

    // Draw dialogs on top if active
    if let Some(ref dialog) = app.release_trigger_dialog {
        dialogs::render_release_trigger_dialog(f, dialog);
    }

    if let Some((ref approval_type, ref stage_name)) = app.approval_dialog {
        dialogs::render_approval_dialog(f, approval_type, stage_name);
    }

    if let Some(ref dialog) = app.confirm_action_dialog {
        dialogs::render_confirm_action_dialog(f, dialog);
    }
}

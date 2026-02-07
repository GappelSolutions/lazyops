mod list;
mod preview;

use crate::app::App;
use ratatui::prelude::*;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    // Horizontal split: left (50%) + right (50%)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    list::draw(f, app, chunks[0]);
    preview::draw(f, app, chunks[1]);
}

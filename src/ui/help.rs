use crate::app::{App, View};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

pub fn draw_popup(f: &mut Frame, app: &App, area: Rect) {
    let (title, help_text) = match app.current_view {
        View::Tasks => (
            " Tasks Help - Press ? or Esc to close ",
            r#"
NAVIGATION
  j/k ↑/↓       Move up/down
  h/l           Focus left/right panel
  g/G           Go to top/bottom
  Ctrl+d/u      Page down/up
  Tab           Switch preview tabs
  Enter         Expand/collapse item
  t             Toggle expand all

FILTERS
  f             Search by text
  s             Filter by state
  a             Filter by assignee
  c             Clear all filters

ACTIONS
  o             Open in browser
  S             Edit state
  A             Edit assignee
  p             Pin/unpin item
  y             Copy ticket ID

VIEWS
  1             Tasks view
  2             PRs view
  3             CI/CD view
  I             Select sprint
  P             Select project
  r             Refresh data
  ?             Toggle help
  q             Quit
"#,
        ),
        View::PRs => (
            " PRs Help - Press ? or Esc to close ",
            r#"
NAVIGATION
  j/k ↑/↓       Move up/down
  h/l           Switch pane (Active/Mine/Completed)
  Enter         Drill into PRs / view details
  Esc           Go back / Exit drill-down
  Tab           Switch preview tabs

REPOSITORIES
  Enter         View PRs for repository
  f             Search repositories

PULL REQUESTS
  h/l           Switch pane (Active/Mine/Completed)
  Enter         View PR details
  f             Search PRs
  o             Open in browser

PREVIEW TABS
  Tab           Next tab (Details/Policies/Threads)
  Shift+Tab     Previous tab
  h             Back to list
  j/k           Scroll content

VIEWS
  1             Tasks view
  2             PRs view
  3             CI/CD view
  P             Select project
  r             Refresh data
  ?             Toggle help
  q             Quit
"#,
        ),
        View::CICD => (
            " CI/CD Help - Press ? or Esc to close ",
            r#"
NAVIGATION
  j/k ↑/↓       Move up/down
  h             Focus Pipelines pane
  l             Focus Releases pane
  Enter         Drill into runs/releases
  Esc           Go back / Exit drill-down

PIPELINES
  Enter         View pipeline runs
  p             Pin/unpin pipeline
  o             Open in browser
  w             Toggle live preview (auto-refresh)

RELEASES
  Enter         View releases
  p             Pin/unpin release
  o             Open in browser

LOG VIEWER
  e             Edit/open log in nvim
  Ctrl+q        Exit nvim viewer

ACTIONS
  T             Trigger new release
  a             Approve selected stage
  A             Approve ALL pending stages
  x             Reject stage (TODO)

VIEWS
  1             Tasks view
  2             PRs view
  3             CI/CD view
  P             Select project
  r             Refresh data
  ?             Toggle help
  q             Quit
"#,
        ),
    };

    let block = Block::default().borders(Borders::ALL).title(title);

    let inner = super::centered_rect(50, 30, area);
    f.render_widget(Clear, inner);

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}

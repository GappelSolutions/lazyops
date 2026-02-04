# lazyops

A lazygit-style TUI for Azure DevOps. Browse sprints, manage work items, monitor CI/CD pipelines, and trigger releases - all from your terminal.

**Tasks View**
```
 Tasks                               │ Preview
 ▸ Sprint 42 (current)               │ Details │ References
─────────────────────────────────────┼─────────────────────────────────────────
 * ◐ ⊗ #12847 Fix login redirect    │ #12847 Fix login redirect loop
   ○ ◈ #12845 User profile page     │
     ○ ☑ #12846 Add avatar upload   │ State:    In Progress
   ◐ ◈ #12843 Dark mode support     │ Type:     Bug
     ● ☑ #12844 Theme toggle        │ Assigned: john.doe@company.com
   ○ ⊗ #12841 API timeout errors    │
                                     │ Description:
                                     │ Users are experiencing redirect loops
                                     │ after login when the session cookie...
─────────────────────────────────────┼─────────────────────────────────────────
 [1] Tasks  [2] CI|CD                │ Tags: authentication, urgent
─────────────────────────────────────┴─────────────────────────────────────────
 ↑↓ navigate  o open  s state  ? help          Sprint 42 │ MyProject │ cache: 5m
```

**CI/CD View**
```
 Pipelines                           │ Preview
─────────────────────────────────────┼─────────────────────────────────────────
 * CustomerPortal.CI                 │ Build #2847 — main
   WebAPI.Build                      │─────────────────────────────────────────
   Infrastructure.Deploy             │ ● Build
─────────────────────────────────────│   ● Restore                       00:23
 Releases                            │   ● Build                         01:45
 * CustomerPortal.Release            │   ◐ Test                      running...
   WebAPI.Release                    │   ○ Publish                      pending
                                     │ ○ Deploy
                                     │   ○ Stage                        pending
                                     │   ○ Production                   pending
─────────────────────────────────────┼─────────────────────────────────────────
 [1] Tasks  [2] CI|CD                │ Triggered by: john.doe │ 3 min ago
─────────────────────────────────────┴─────────────────────────────────────────
 ↑↓ navigate  n trigger  x cancel  e logs  ? help     MyProject │ ● 2 in progress
```

## Features

### Work Items (Press `1`)
- **Sprint View** - Browse work items by sprint with hierarchical parent/child display
- **Work Item Details** - View descriptions, state, assignee, tags, estimates
- **References** - See linked PRs, commits, attachments, and child items
- **Quick Actions** - Change state, assignee, pin items, open in browser
- **Filtering** - Search by text, filter by state or assignee

### CI/CD (Press `2`)
- **Pipelines** - Browse pipeline definitions, runs, tasks, and logs
- **Releases** - Browse release definitions, deployments, stages, and tasks
- **Actions** - Trigger pipelines, create releases, approve/reject deployments
- **Cancel/Retrigger** - Stop running builds or redeploy failed stages
- **Live Preview** - Auto-refreshing build progress with task timeline
- **Pinning** - Pin frequently used pipelines and releases

### General
- **Caching** - Fast startup with intelligent caching
- **Customizable** - Themes, keybindings, and settings via config file

## Installation

### Prerequisites

- [Azure CLI](https://docs.microsoft.com/en-us/cli/azure/install-azure-cli) installed and authenticated (`az login`)
- [Azure DevOps extension](https://docs.microsoft.com/en-us/azure/devops/cli/) (`az extension add --name azure-devops`)

### From Source

```bash
git clone https://github.com/yourusername/lazyops
cd lazyops
cargo install --path .
```

### Verify Azure CLI Setup

```bash
az login
az devops configure --defaults organization=https://dev.azure.com/YOUR_ORG
```

## Configuration

Create a config file at `~/.config/lazyops/config.toml`:

```toml
# Default project to load on startup
default_project = "myproject"

# Project configurations
[[projects]]
name = "myproject"
organization = "https://dev.azure.com/myorg"
project = "My Project"
team = "My Team"

[[projects]]
name = "another"
organization = "https://dev.azure.com/myorg"
project = "Another Project"
team = "Another Team"

# Application settings
[settings]
refresh_interval = 300    # Auto-refresh every 5 minutes (0 to disable)
page_jump = 10            # Items to jump with Ctrl+D/U
api_timeout = 30          # API request timeout in seconds
cache_expiry = 3600       # Cache expiry in seconds (1 hour)

# Custom work item states (optional - leave empty for defaults)
# states = ["New", "Active", "Resolved", "Closed"]

# Theme customization (One Dark colors by default)
[theme]
border = "#5c6370"
border_active = "#61afef"
selected_bg = "#2c323c"
text = "#abb2bf"
text_muted = "#5c6370"
highlight = "#61afef"

# Work item type colors
type_bug = "#e06c75"
type_story = "#c678dd"
type_task = "#61afef"
type_feature = "#56b6c2"
type_epic = "#d19a66"

# State colors
state_new = "#61afef"
state_active = "#e5c07b"
state_resolved = "#98c379"
state_closed = "#5c6370"

# Custom keybindings (optional)
[keybindings]
down = "j"
up = "k"
left = "h"
right = "l"
open = "o"
quit = "q"
help = "?"
search = "f"
filter_state = "s"
filter_assignee = "a"
edit_state = "S"
edit_assignee = "A"
select_sprint = "I"
select_project = "P"
```

Config file locations (checked in order):
1. `~/.config/lazyops/config.toml`
2. `~/Library/Application Support/lazyops/config.toml` (macOS)
3. `~/.lazyops.toml`

## Keybindings

### Views

| Key | Action |
|-----|--------|
| `1` | Switch to Tasks view |
| `2` | Switch to CI/CD view |

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up |
| `h` / `l` | Focus left / right panel |
| `g` / `G` | Go to top / bottom |
| `Ctrl+d` / `Ctrl+u` | Page down / up |
| `Tab` | Switch preview tabs (Details / References) |
| `Enter` | Expand / collapse item (or drill down in CI/CD) |
| `Esc` | Go back / exit drill-down |
| `t` | Toggle expand all |

### Filters

| Key | Action |
|-----|--------|
| `f` | Search by text |
| `s` | Filter by state |
| `a` | Filter by assignee |
| `c` | Clear all filters |

### Actions

| Key | Action |
|-----|--------|
| `o` | Open in browser |
| `S` | Edit state |
| `A` | Edit assignee |
| `p` | Pin / unpin item |
| `y` | Copy ticket ID |

### CI/CD Actions

| Key | Action |
|-----|--------|
| `Enter` | Drill into runs/stages/tasks |
| `Esc` | Go back up |
| `n` | Trigger new pipeline run / Create release |
| `x` | Cancel running build / Abandon release |
| `r` | Retrigger / Redeploy |
| `e` | View logs in terminal (nvim) |
| `a` | Approve pending deployment |
| `d` | Reject pending deployment |
| `L` | Load all runs (not just recent 10) |

### Selection

| Key | Action |
|-----|--------|
| `I` | Select sprint |
| `P` | Select project |
| `R` | Refresh data |
| `?` | Toggle help |
| `q` | Quit |

## Usage Tips

### CI/CD View

Press `2` to switch to the CI/CD view:
- **Left panel**: Pipelines (top) and Releases (bottom)
- **Right panel**: Preview with build timeline, logs, or stage details
- Press `Enter` to drill down: Definitions → Runs/Releases → Tasks
- Press `Esc` to go back up
- Press `e` on a task to view full logs in nvim
- Press `n` to trigger a new run or create a release
- Press `x` to cancel, `r` to retrigger

### References Tab

When viewing the References tab:
- Use `j/k` to navigate between linked items
- Press `o` to open the selected reference (PR, commit, attachment)
- Groups: Children, Attachments, Pull Requests, Commits

### Pinned Items

- Press `p` to pin frequently accessed items
- Pinned items appear at the top with a pin icon
- Pins persist across sessions

### Filtering

- Filters can be combined (e.g., filter by state AND assignee)
- Search matches against ticket ID and title
- Press `c` to clear all active filters

### Caching

- Data is cached locally for fast startup
- Cache age shown in status bar
- Press `R` to force refresh from Azure DevOps

## Architecture

```
src/
├── main.rs          # Entry point
├── app.rs           # Application state and logic
├── config.rs        # Configuration loading
├── events.rs        # Keyboard event handling
├── cache.rs         # Local data caching
├── terminal.rs      # Embedded PTY terminal for log viewing
├── azure/
│   ├── client.rs    # Azure DevOps CLI wrapper
│   └── types.rs     # API response types
└── ui/
    ├── mod.rs       # Main UI composition
    ├── input.rs     # Dropdowns and inputs
    ├── help.rs      # Help popup
    ├── tasks/
    │   ├── mod.rs       # Tasks view composition
    │   ├── work_items.rs # Work items list
    │   ├── preview.rs   # Details/References panels
    │   └── sprint_bar.rs # Sprint/Project selectors
    └── cicd/
        ├── mod.rs       # CI/CD view composition
        ├── pipelines.rs # Pipelines panel
        ├── releases.rs  # Releases panel
        ├── preview.rs   # Build timeline/logs preview
        └── dialogs.rs   # Trigger/approval dialogs
```

## License

MIT

## Contributing

Contributions welcome! Please open an issue first to discuss major changes.

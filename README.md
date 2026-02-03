# lazyops

A lazygit-style TUI for Azure DevOps work items. Browse sprints, manage tasks, and track references - all from your terminal.

![lazyops screenshot](docs/screenshot.png)

## Features

- **Sprint View** - Browse work items by sprint with hierarchical parent/child display
- **Work Item Details** - View descriptions, state, assignee, tags, estimates
- **References** - See linked PRs, commits, attachments, and child items
- **Quick Actions** - Change state, assignee, pin items, open in browser
- **Filtering** - Search by text, filter by state or assignee
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

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up |
| `h` / `l` | Focus left / right panel |
| `g` / `G` | Go to top / bottom |
| `Ctrl+d` / `Ctrl+u` | Page down / up |
| `Tab` | Switch preview tabs (Details / References) |
| `Enter` | Expand / collapse item |
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

### Selection

| Key | Action |
|-----|--------|
| `I` | Select sprint |
| `P` | Select project |
| `R` | Refresh data |
| `?` | Toggle help |
| `q` | Quit |

## Usage Tips

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
├── azure/
│   ├── client.rs    # Azure DevOps CLI wrapper
│   └── types.rs     # API response types
└── ui/
    ├── mod.rs       # Main UI composition
    ├── work_items.rs # Work items list
    ├── preview.rs   # Details/References panels
    ├── sprint_bar.rs # Sprint/Project selectors
    ├── input.rs     # Dropdowns and inputs
    └── help.rs      # Help popup
```

## License

MIT

## Contributing

Contributions welcome! Please open an issue first to discuss major changes.

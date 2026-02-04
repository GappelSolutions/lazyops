# lazyops Roadmap

## Done

### Core
- [x] Project setup (Cargo.toml, ratatui, tokio)
- [x] Config loading (`~/.config/lazyops/config.toml`)
- [x] Azure CLI client with WIQL queries
- [x] Caching with configurable expiry
- [x] Multi-project support

### Tasks View (Press `1`)
- [x] Sprint selector with search
- [x] Work item list with parent/child hierarchy
- [x] Preview pane with Details/References tabs
- [x] Edit state, assignee via modal dialogs
- [x] Filter by state, assignee, text search
- [x] Open in browser, copy ticket ID
- [x] Pin frequently used items
- [x] Keyboard navigation (vim-style)

### CI/CD View (Press `2`)
- [x] Pipeline definitions list
- [x] Pipeline runs with drill-down to tasks
- [x] Release definitions list
- [x] Release deployments with stage drill-down
- [x] Build timeline preview with task durations
- [x] Trigger new pipeline runs
- [x] Create releases
- [x] Cancel running builds
- [x] Approve/reject deployments
- [x] View logs in embedded terminal (nvim)
- [x] Retrigger failed stages

---

## In Progress

### Known Issues
- [ ] Relations not loaded in list view (need separate fetch)
- [ ] Story points field not in schema
- [ ] Dead code warnings (cleanup needed)

---

## Planned

### PR View (Press `2` - will shift CI/CD to `3`)

**Phase 1: Foundation**
- [ ] Add PRs as second tab: `[1] Tasks [2] PRs [3] CI/CD`
- [ ] Repository selector in top bar
- [ ] PR list with filter modes (Active, Mine, Completed)
- [ ] PR preview pane (title, branches, reviewers, description)

**Phase 2: PR Creation**
- [ ] Fullscreen create dialog
- [ ] Branch selectors (source/target) with swap
- [ ] Title and description editor
- [ ] Link work items search
- [ ] Options: delete source branch, auto-complete, squash, draft

**Phase 3: PR Actions**
- [ ] Vote dialog (approve, approve with suggestions, wait, reject, reset)
- [ ] Complete dialog (merge, auto-complete, abandon)
- [ ] Mark as draft

**Phase 4: Code Review**
- [ ] PR detail drill-down view
- [ ] Files changed list
- [ ] Comment threads display
- [ ] Add comments and replies
- [ ] Resolve/reactivate threads

**Phase 5: Pipeline Integration**
- [ ] Show build status in PR list/preview
- [ ] Jump to CI/CD view for PR branch
- [ ] Live preview after PR creation

**Post-MVP**
- [ ] Merge conflict resolution (lazygit-style)
- [ ] Commits view
- [ ] Reviewer management (add/remove)
- [ ] Labels and tags

---

### Future Ideas
- [ ] Bulk operations (multi-select)
- [ ] Custom WIQL queries / saved filters
- [ ] Swimlane support
- [ ] Multiple board views
- [ ] Notifications for PR comments
- [ ] Theme customization beyond colors

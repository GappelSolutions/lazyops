use crate::app::{App, Focus, InputMode, View};
use crate::azure::WorkItem;
use crate::ui;
use anyhow::Result;
use arboard::Clipboard;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use std::time::{Duration, Instant};

/// Format ticket content for clipboard (no names, just content)
fn format_ticket_content(item: &WorkItem) -> String {
    let mut content = String::new();

    // Title
    content.push_str(&format!("# #{} {}\n\n", item.id, item.fields.title));

    // Description
    if let Some(desc) = &item.fields.description {
        let plain = html2text::from_read(desc.as_bytes(), 80);
        content.push_str("## Description\n\n");
        content.push_str(&plain);
        content.push_str("\n\n");
    }

    // Tags
    if let Some(tags) = &item.fields.tags {
        content.push_str(&format!("**Tags:** {tags}\n"));
    }

    content
}

pub async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    // Try loading from cache first for instant startup
    let has_cache = app.load_from_cache();

    if has_cache {
        // Cache hit: show data immediately, skip blocking refresh
        if let Some(age) = app.cache_age {
            let mins = age / 60;
            if mins > 0 {
                app.set_status(format!("Cached ({mins}m ago) - press r to refresh"));
            } else {
                app.set_status(format!("Cached ({age}s ago) - press r to refresh"));
            }
        }
    } else {
        // No cache: must load from API (blocking)
        app.set_loading(true, "Loading sprints...");
        terminal.draw(|f| ui::draw(f, app))?;

        if let Err(e) = app.load_sprints().await {
            app.set_error(format!("Failed to load sprints: {e}"));
        } else {
            app.set_loading(true, "Loading work items...");
            terminal.draw(|f| ui::draw(f, app))?;

            if let Err(e) = app.load_work_items().await {
                app.set_error(format!("Failed to load work items: {e}"));
            } else {
                let _ = app.load_users().await;
                app.save_to_cache();
                app.cache_age = Some(0);
            }
        }
        app.set_loading(false, "");
    }

    // Start background relation loader (non-blocking)
    app.start_relations_loader();

    let mut last_refresh = Instant::now();
    let mut last_spinner_tick = Instant::now();
    let refresh_interval = Duration::from_secs(300); // Full refresh every 5 minutes
    let spinner_interval = Duration::from_millis(80);

    loop {
        // Tick spinner during loading
        if app.loading && last_spinner_tick.elapsed() >= spinner_interval {
            app.tick_spinner();
            last_spinner_tick = Instant::now();
        }

        // Poll for loaded relations and titles (non-blocking)
        app.poll_relations();
        app.poll_titles();
        app.poll_cicd();
        app.poll_live_preview();
        app.poll_release_refresh();

        // Start titles loader once some relations have been loaded
        if !app.titles_loader_active && !app.relations_loaded.is_empty() {
            app.start_titles_loader();
        }

        // Clear status messages after 5 seconds
        app.clear_expired_status();

        terminal.draw(|f| ui::draw(f, app))?;

        // Poll for events with short timeout for responsive UI
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if handle_key(app, key).await? {
                    return Ok(());
                }
            }
        }

        // Full data refresh every 5 minutes
        if !app.loading && app.input_mode == InputMode::Normal && last_refresh.elapsed() >= refresh_interval {
            background_full_refresh(app).await;
            last_refresh = Instant::now();
        }
    }
}

/// Full refresh - reloads work items and restarts relation loader
async fn background_full_refresh(app: &mut App) {
    // Cache relations before refresh
    let relations_cache = app.cache_relations();

    if let Some(client) = app.client() {
        if let Some(sprint) = app.selected_sprint() {
            let path = sprint.path.clone();
            if let Ok(items) = client.get_sprint_work_items(&path).await {
                let selected_id = app.selected_work_item().map(|w| w.item.id);

                app.work_items = items;
                app.extract_users_from_work_items();
                app.restore_relations(relations_cache);
                app.rebuild_visible_items();

                if let Some(id) = selected_id {
                    if let Some(pos) = app.visible_items.iter().position(|v| v.item.id == id) {
                        app.work_item_list_state.select(Some(pos));
                    }
                }

                app.cache_age = Some(0);

                // Restart relation loader for any new items
                app.relations_loader_active = false;
                app.start_relations_loader();
            }
        }
    }
}

/// Convert key event to terminal bytes
fn key_to_terminal_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    match key.code {
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+letter -> ASCII control code
            let ctrl_code = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a').wrapping_add(1);
            Some(vec![ctrl_code])
        }
        KeyCode::Char(c) => Some(c.to_string().into_bytes()),
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![127]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![27]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::F(n) => {
            let seq = match n {
                1 => b"\x1bOP".to_vec(),
                2 => b"\x1bOQ".to_vec(),
                3 => b"\x1bOR".to_vec(),
                4 => b"\x1bOS".to_vec(),
                5 => b"\x1b[15~".to_vec(),
                6 => b"\x1b[17~".to_vec(),
                7 => b"\x1b[18~".to_vec(),
                8 => b"\x1b[19~".to_vec(),
                9 => b"\x1b[20~".to_vec(),
                10 => b"\x1b[21~".to_vec(),
                11 => b"\x1b[23~".to_vec(),
                12 => b"\x1b[24~".to_vec(),
                _ => return None,
            };
            Some(seq)
        }
        _ => None,
    }
}

async fn handle_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    // Clear status on any keypress
    app.clear_status();

    // Handle terminal mode - forward keys to embedded terminal
    if app.terminal_mode {
        // Ctrl+q exits terminal mode
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            app.close_embedded_terminal();
            app.set_status("Exited log viewer");
            return Ok(false);
        }

        // Forward key to terminal
        if let Some(data) = key_to_terminal_bytes(&key) {
            let _ = app.send_to_terminal(&data);
        }
        return Ok(false);
    }

    // Handle based on input mode
    match app.input_mode {
        InputMode::Help => {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                    app.input_mode = InputMode::Normal;
                }
                _ => {}
            }
        }

        InputMode::SprintSelect => {
            match key.code {
                KeyCode::Esc => app.input_mode = InputMode::Normal,
                KeyCode::Char('j') | KeyCode::Down => app.dropdown_next(app.sprints.len()),
                KeyCode::Char('k') | KeyCode::Up => app.dropdown_prev(app.sprints.len()),
                KeyCode::Enter => {
                    if let Some(idx) = app.dropdown_list_state.selected() {
                        app.selected_sprint_idx = idx;
                        app.input_mode = InputMode::Normal;
                        app.set_loading(true, "Loading sprint...");
                        if let Err(e) = app.load_work_items().await {
                            app.set_error(format!("Failed to load: {e}"));
                        }
                        app.set_loading(false, "");
                    }
                }
                _ => {}
            }
        }

        InputMode::ProjectSelect => {
            match key.code {
                KeyCode::Esc => app.input_mode = InputMode::Normal,
                KeyCode::Char('j') | KeyCode::Down => app.dropdown_next(app.config.projects.len()),
                KeyCode::Char('k') | KeyCode::Up => app.dropdown_prev(app.config.projects.len()),
                KeyCode::Enter => {
                    if let Some(idx) = app.dropdown_list_state.selected() {
                        app.current_project_idx = idx;
                        app.input_mode = InputMode::Normal;
                        app.set_loading(true, "Loading project...");
                        let _ = app.load_sprints().await;
                        let _ = app.load_work_items().await;
                        let _ = app.load_users().await;
                        app.set_loading(false, "");
                    }
                }
                _ => {}
            }
        }

        InputMode::EditState => {
            let states = app.filtered_edit_states();
            match key.code {
                KeyCode::Esc => {
                    app.filter_input.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Down => app.dropdown_next(states.len()),
                KeyCode::Up => app.dropdown_prev(states.len()),
                KeyCode::Enter => {
                    if let (Some(idx), Some(work_item)) = (app.dropdown_list_state.selected(), app.selected_work_item()) {
                        if let Some(state) = states.get(idx) {
                            let id = work_item.item.id;
                            if let Some(client) = app.client() {
                                match client.update_work_item(id, "state", state).await {
                                    Ok(_) => {
                                        app.set_status(format!("Updated state to {state}"));
                                        let _ = app.load_work_items().await;
                                    }
                                    Err(e) => app.set_error(format!("Failed: {e}")),
                                }
                            }
                        }
                    }
                    app.filter_input.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Backspace => {
                    app.filter_input.pop();
                    app.dropdown_list_state.select(Some(0));
                }
                KeyCode::Char(c) => {
                    app.filter_input.push(c);
                    app.dropdown_list_state.select(Some(0));
                }
                _ => {}
            }
        }

        InputMode::EditAssignee => {
            let assignees = app.filtered_edit_assignees();
            match key.code {
                KeyCode::Esc => {
                    app.filter_input.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Down => app.dropdown_next(assignees.len()),
                KeyCode::Up => app.dropdown_prev(assignees.len()),
                KeyCode::Enter => {
                    if let (Some(idx), Some(work_item)) = (app.dropdown_list_state.selected(), app.selected_work_item()) {
                        if let Some(user) = assignees.get(idx) {
                            let id = work_item.item.id;
                            if let Some(client) = app.client() {
                                match client.update_work_item(id, "assigned-to", &user.unique_name).await {
                                    Ok(_) => {
                                        app.set_status(format!("Assigned to {}", user.display_name));
                                        let _ = app.load_work_items().await;
                                    }
                                    Err(e) => app.set_error(format!("Failed: {e}")),
                                }
                            }
                        }
                    }
                    app.filter_input.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Backspace => {
                    app.filter_input.pop();
                    app.dropdown_list_state.select(Some(0));
                }
                KeyCode::Char(c) => {
                    app.filter_input.push(c);
                    app.dropdown_list_state.select(Some(0));
                }
                _ => {}
            }
        }

        InputMode::Search => {
            match key.code {
                KeyCode::Esc => {
                    app.search_query.clear();
                    app.input_mode = InputMode::Normal;
                    app.rebuild_visible_items();
                }
                KeyCode::Enter => {
                    app.input_mode = InputMode::Normal;
                    // Filter is already applied in rebuild_visible_items
                }
                KeyCode::Backspace => {
                    app.search_query.pop();
                    app.rebuild_visible_items();
                }
                KeyCode::Char(c) => {
                    app.search_query.push(c);
                    app.rebuild_visible_items();
                }
                _ => {}
            }
        }

        InputMode::CICDSearch => {
            match key.code {
                KeyCode::Esc => {
                    app.cicd_search_query.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Enter => {
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Backspace => {
                    app.cicd_search_query.pop();
                    // Reset selection to first matching item based on focus
                    match app.cicd_focus {
                        crate::app::CICDFocus::Pipelines => {
                            match app.pipeline_drill_down {
                                crate::app::PipelineDrillDown::Tasks => app.selected_task_idx = 0,
                                crate::app::PipelineDrillDown::Runs => app.selected_pipeline_run_idx = 0,
                                crate::app::PipelineDrillDown::None => app.select_first_pipeline(),
                            }
                        }
                        crate::app::CICDFocus::Releases => {
                            match app.release_drill_down {
                                crate::app::ReleaseDrillDown::Tasks => app.selected_release_task_idx = 0,
                                crate::app::ReleaseDrillDown::Stages => app.selected_release_stage_idx = 0,
                                crate::app::ReleaseDrillDown::Items => app.selected_release_item_idx = 0,
                                crate::app::ReleaseDrillDown::None => app.select_first_release(),
                            }
                        }
                        _ => {}
                    }
                }
                KeyCode::Char(c) => {
                    app.cicd_search_query.push(c);
                    // Reset selection to first matching item based on focus
                    match app.cicd_focus {
                        crate::app::CICDFocus::Pipelines => {
                            match app.pipeline_drill_down {
                                crate::app::PipelineDrillDown::Tasks => app.selected_task_idx = 0,
                                crate::app::PipelineDrillDown::Runs => app.selected_pipeline_run_idx = 0,
                                crate::app::PipelineDrillDown::None => app.select_first_pipeline(),
                            }
                        }
                        crate::app::CICDFocus::Releases => {
                            match app.release_drill_down {
                                crate::app::ReleaseDrillDown::Tasks => app.selected_release_task_idx = 0,
                                crate::app::ReleaseDrillDown::Stages => app.selected_release_stage_idx = 0,
                                crate::app::ReleaseDrillDown::Items => app.selected_release_item_idx = 0,
                                crate::app::ReleaseDrillDown::None => app.select_first_release(),
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        InputMode::FilterState => {
            let states = app.filtered_states();
            match key.code {
                KeyCode::Esc => {
                    app.filter_input.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Down => app.dropdown_next(states.len()),
                KeyCode::Up => app.dropdown_prev(states.len()),
                KeyCode::Enter => {
                    if let Some(idx) = app.dropdown_list_state.selected() {
                        if let Some(state) = states.get(idx) {
                            if *state == "All" {
                                app.filter_state = None;
                            } else {
                                app.filter_state = Some(state.to_string());
                            }
                        }
                        app.rebuild_visible_items();
                        app.save_to_cache(); // Persist filter
                    }
                    app.filter_input.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Backspace => {
                    app.filter_input.pop();
                    app.dropdown_list_state.select(Some(0));
                }
                KeyCode::Char(c) => {
                    app.filter_input.push(c);
                    app.dropdown_list_state.select(Some(0));
                }
                _ => {}
            }
        }

        InputMode::FilterAssignee => {
            let assignees = app.filtered_assignees();
            match key.code {
                KeyCode::Esc => {
                    app.filter_input.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Down => app.dropdown_next(assignees.len()),
                KeyCode::Up => app.dropdown_prev(assignees.len()),
                KeyCode::Enter => {
                    if let Some(idx) = app.dropdown_list_state.selected() {
                        if let Some(assignee) = assignees.get(idx) {
                            if assignee == "All" {
                                app.filter_assignee = None;
                            } else {
                                app.filter_assignee = Some(assignee.clone());
                            }
                        }
                        app.rebuild_visible_items();
                        app.save_to_cache(); // Persist filter
                    }
                    app.filter_input.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Backspace => {
                    app.filter_input.pop();
                    app.dropdown_list_state.select(Some(0));
                }
                KeyCode::Char(c) => {
                    app.filter_input.push(c);
                    app.dropdown_list_state.select(Some(0));
                }
                _ => {}
            }
        }

        InputMode::ReleaseTriggerDialog => {
            match key.code {
                KeyCode::Esc => {
                    app.release_trigger_dialog = None;
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if let Some(dialog) = &mut app.release_trigger_dialog {
                        if !dialog.stages.is_empty() {
                            dialog.selected_idx = (dialog.selected_idx + 1).min(dialog.stages.len() - 1);
                        }
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if let Some(dialog) = &mut app.release_trigger_dialog {
                        dialog.selected_idx = dialog.selected_idx.saturating_sub(1);
                    }
                }
                KeyCode::Char(' ') => {
                    // Toggle stage enabled/disabled
                    if let Some(dialog) = &mut app.release_trigger_dialog {
                        if let Some(stage) = dialog.stages.get_mut(dialog.selected_idx) {
                            stage.enabled = !stage.enabled;
                        }
                    }
                }
                KeyCode::Char('a') => {
                    // Select all stages
                    if let Some(dialog) = &mut app.release_trigger_dialog {
                        for stage in &mut dialog.stages {
                            stage.enabled = true;
                        }
                    }
                }
                KeyCode::Char('n') => {
                    // Deselect all stages
                    if let Some(dialog) = &mut app.release_trigger_dialog {
                        for stage in &mut dialog.stages {
                            stage.enabled = false;
                        }
                    }
                }
                KeyCode::Enter => {
                    if let Some(dialog) = &app.release_trigger_dialog {
                        let def_id = dialog.definition_id;
                        app.trigger_release(def_id, None);
                        app.release_trigger_dialog = None;
                        app.input_mode = InputMode::Normal;
                    }
                }
                _ => {}
            }
        }

        InputMode::ApprovalConfirm => {
            // Placeholder for approval confirm mode
            match key.code {
                KeyCode::Esc => {
                    app.input_mode = InputMode::Normal;
                }
                _ => {}
            }
        }

        InputMode::Normal => {
            // Check for Ctrl modifiers first
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('d') => {
                        match app.current_view {
                            View::Tasks => {
                                match app.focus {
                                    Focus::WorkItems => app.list_jump_down(),
                                    Focus::Preview => {
                                        if app.preview_tab == crate::app::PreviewTab::References {
                                            app.relations_page_down();
                                        } else {
                                            app.preview_scroll = app.preview_scroll.saturating_add(20).min(app.preview_scroll_max);
                                        }
                                    }
                                }
                            }
                            View::CICD => {
                                // Page down based on focus
                                if app.cicd_focus == crate::app::CICDFocus::Preview {
                                    // Scroll preview pane or logs
                                    if app.release_drill_down == crate::app::ReleaseDrillDown::Tasks && !app.release_task_logs.is_empty() {
                                        app.log_scroll = app.log_scroll.saturating_add(20);
                                    } else {
                                        app.cicd_preview_scroll = app.cicd_preview_scroll.saturating_add(10);
                                    }
                                } else if app.pipeline_drill_down == crate::app::PipelineDrillDown::Tasks {
                                    if !app.build_log_lines.is_empty() {
                                        app.log_scroll = app.log_scroll.saturating_add(20);
                                    } else {
                                        let task_count = app.get_timeline_tasks().len();
                                        if task_count > 0 {
                                            app.selected_task_idx = (app.selected_task_idx + 10).min(task_count - 1);
                                        }
                                    }
                                } else if app.pipeline_drill_down == crate::app::PipelineDrillDown::Runs {
                                    if !app.pipeline_runs.is_empty() {
                                        app.selected_pipeline_run_idx = (app.selected_pipeline_run_idx + 10).min(app.pipeline_runs.len() - 1);
                                    }
                                } else if app.release_drill_down == crate::app::ReleaseDrillDown::Tasks {
                                    if !app.release_tasks.is_empty() {
                                        app.selected_release_task_idx = (app.selected_release_task_idx + 10).min(app.release_tasks.len() - 1);
                                    }
                                } else if app.release_drill_down == crate::app::ReleaseDrillDown::Stages {
                                    if !app.release_stages.is_empty() {
                                        app.selected_release_stage_idx = (app.selected_release_stage_idx + 10).min(app.release_stages.len() - 1);
                                    }
                                } else if app.release_drill_down == crate::app::ReleaseDrillDown::Items
                                    && !app.release_list.is_empty() {
                                        app.selected_release_item_idx = (app.selected_release_item_idx + 10).min(app.release_list.len() - 1);
                                    }
                            }
                        }
                    }
                    KeyCode::Char('u') => {
                        match app.current_view {
                            View::Tasks => {
                                match app.focus {
                                    Focus::WorkItems => app.list_jump_up(),
                                    Focus::Preview => {
                                        if app.preview_tab == crate::app::PreviewTab::References {
                                            app.relations_page_up();
                                        } else {
                                            app.preview_scroll = app.preview_scroll.saturating_sub(20);
                                        }
                                    }
                                }
                            }
                            View::CICD => {
                                // Page up based on focus
                                if app.cicd_focus == crate::app::CICDFocus::Preview {
                                    // Scroll preview pane or logs
                                    if app.release_drill_down == crate::app::ReleaseDrillDown::Tasks && !app.release_task_logs.is_empty() {
                                        app.log_scroll = app.log_scroll.saturating_sub(20);
                                    } else {
                                        app.cicd_preview_scroll = app.cicd_preview_scroll.saturating_sub(10);
                                    }
                                } else if app.pipeline_drill_down == crate::app::PipelineDrillDown::Tasks {
                                    if !app.build_log_lines.is_empty() {
                                        app.log_scroll = app.log_scroll.saturating_sub(20);
                                    } else {
                                        app.selected_task_idx = app.selected_task_idx.saturating_sub(10);
                                    }
                                } else if app.pipeline_drill_down == crate::app::PipelineDrillDown::Runs {
                                    app.selected_pipeline_run_idx = app.selected_pipeline_run_idx.saturating_sub(10);
                                } else if app.release_drill_down == crate::app::ReleaseDrillDown::Tasks {
                                    app.selected_release_task_idx = app.selected_release_task_idx.saturating_sub(10);
                                } else if app.release_drill_down == crate::app::ReleaseDrillDown::Stages {
                                    app.selected_release_stage_idx = app.selected_release_stage_idx.saturating_sub(10);
                                } else if app.release_drill_down == crate::app::ReleaseDrillDown::Items {
                                    app.selected_release_item_idx = app.selected_release_item_idx.saturating_sub(10);
                                }
                            }
                        }
                    }
                    KeyCode::Char('c') => return Ok(true), // Ctrl+C to quit
                    _ => {}
                }
                return Ok(false);
            }

            match key.code {
                // View switching
                KeyCode::Char('1') => {
                    app.current_view = crate::app::View::Tasks;
                    app.set_status("Tasks view");
                }
                KeyCode::Char('2') => {
                    app.current_view = crate::app::View::CICD;
                    // Start background CI/CD loader if not already loaded
                    if app.pipelines.is_empty() && !app.cicd_loading {
                        app.start_cicd_loader();
                    }
                    app.set_status("CI/CD view");
                }

                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('?') => app.input_mode = InputMode::Help,

                // Navigation
                KeyCode::Char('j') | KeyCode::Down => {
                    match app.current_view {
                        View::Tasks => {
                            match app.focus {
                                Focus::WorkItems => app.list_next(),
                                Focus::Preview => {
                                    if app.preview_tab == crate::app::PreviewTab::References {
                                        app.relations_next();
                                    } else {
                                        app.scroll_preview_down();
                                    }
                                }
                            }
                        }
                        View::CICD => {
                            match app.cicd_focus {
                                crate::app::CICDFocus::Pipelines => {
                                    match app.pipeline_drill_down {
                                        crate::app::PipelineDrillDown::Tasks => {
                                            // Navigate tasks
                                            let task_count = app.get_timeline_tasks().len();
                                            if task_count > 0 {
                                                app.selected_task_idx = (app.selected_task_idx + 1).min(task_count - 1);
                                            }
                                        }
                                        crate::app::PipelineDrillDown::Runs => {
                                            // Navigate runs
                                            if !app.pipeline_runs.is_empty() {
                                                app.selected_pipeline_run_idx = (app.selected_pipeline_run_idx + 1).min(app.pipeline_runs.len() - 1);
                                            }
                                        }
                                        crate::app::PipelineDrillDown::None => {
                                            app.pipeline_next();
                                        }
                                    }
                                }
                                crate::app::CICDFocus::Releases => {
                                    match app.release_drill_down {
                                        crate::app::ReleaseDrillDown::Tasks => {
                                            // Navigate tasks
                                            if !app.release_tasks.is_empty() {
                                                app.selected_release_task_idx = (app.selected_release_task_idx + 1).min(app.release_tasks.len() - 1);
                                            }
                                        }
                                        crate::app::ReleaseDrillDown::Stages => {
                                            // Navigate stages
                                            if !app.release_stages.is_empty() {
                                                app.selected_release_stage_idx = (app.selected_release_stage_idx + 1).min(app.release_stages.len() - 1);
                                            }
                                        }
                                        crate::app::ReleaseDrillDown::Items => {
                                            // Navigate release items
                                            if !app.release_list.is_empty() {
                                                app.selected_release_item_idx = (app.selected_release_item_idx + 1).min(app.release_list.len() - 1);
                                            }
                                        }
                                        crate::app::ReleaseDrillDown::None => {
                                            app.release_next();
                                        }
                                    }
                                }
                                crate::app::CICDFocus::Preview => {
                                    // Scroll logs or preview
                                    if (app.pipeline_drill_down == crate::app::PipelineDrillDown::Tasks && !app.build_log_lines.is_empty())
                                        || (app.release_drill_down == crate::app::ReleaseDrillDown::Tasks && !app.release_task_logs.is_empty()) {
                                        app.log_scroll = app.log_scroll.saturating_add(1);
                                    } else {
                                        app.cicd_preview_scroll = app.cicd_preview_scroll.saturating_add(1);
                                    }
                                }
                            }
                        }
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    match app.current_view {
                        View::Tasks => {
                            match app.focus {
                                Focus::WorkItems => app.list_prev(),
                                Focus::Preview => {
                                    if app.preview_tab == crate::app::PreviewTab::References {
                                        app.relations_prev();
                                    } else {
                                        app.scroll_preview_up();
                                    }
                                }
                            }
                        }
                        View::CICD => {
                            match app.cicd_focus {
                                crate::app::CICDFocus::Pipelines => {
                                    match app.pipeline_drill_down {
                                        crate::app::PipelineDrillDown::Tasks => {
                                            // Navigate tasks
                                            if app.selected_task_idx > 0 {
                                                app.selected_task_idx = app.selected_task_idx.saturating_sub(1);
                                            }
                                        }
                                        crate::app::PipelineDrillDown::Runs => {
                                            // Navigate runs
                                            if !app.pipeline_runs.is_empty() {
                                                app.selected_pipeline_run_idx = app.selected_pipeline_run_idx.saturating_sub(1);
                                            }
                                        }
                                        crate::app::PipelineDrillDown::None => {
                                            app.pipeline_prev();
                                        }
                                    }
                                }
                                crate::app::CICDFocus::Releases => {
                                    match app.release_drill_down {
                                        crate::app::ReleaseDrillDown::Tasks => {
                                            // Navigate tasks
                                            app.selected_release_task_idx = app.selected_release_task_idx.saturating_sub(1);
                                        }
                                        crate::app::ReleaseDrillDown::Stages => {
                                            // Navigate stages
                                            app.selected_release_stage_idx = app.selected_release_stage_idx.saturating_sub(1);
                                        }
                                        crate::app::ReleaseDrillDown::Items => {
                                            // Navigate release items
                                            app.selected_release_item_idx = app.selected_release_item_idx.saturating_sub(1);
                                        }
                                        crate::app::ReleaseDrillDown::None => {
                                            app.release_prev();
                                        }
                                    }
                                }
                                crate::app::CICDFocus::Preview => {
                                    // Scroll logs or preview
                                    if (app.pipeline_drill_down == crate::app::PipelineDrillDown::Tasks && !app.build_log_lines.is_empty())
                                        || (app.release_drill_down == crate::app::ReleaseDrillDown::Tasks && !app.release_task_logs.is_empty()) {
                                        app.log_scroll = app.log_scroll.saturating_sub(1);
                                    } else {
                                        app.cicd_preview_scroll = app.cicd_preview_scroll.saturating_sub(1);
                                    }
                                }
                            }
                        }
                    }
                }
                KeyCode::Char('g') => app.list_top(),
                KeyCode::Char('G') => app.list_bottom(),

                // Focus switching (view-aware)
                KeyCode::Char('h') => {
                    match app.current_view {
                        View::Tasks => app.focus = Focus::WorkItems,
                        View::CICD => app.cicd_focus = crate::app::CICDFocus::Pipelines,
                    }
                }
                KeyCode::Char('l') => {
                    match app.current_view {
                        View::Tasks => app.focus = Focus::Preview,
                        View::CICD => app.cicd_focus = crate::app::CICDFocus::Releases,
                    }
                }
                KeyCode::Tab => {
                    app.next_tab();
                    // Reset relations selection when switching tabs
                    app.relations_list_state.select(None);
                }
                KeyCode::BackTab => {
                    app.prev_tab();
                    app.relations_list_state.select(None);
                }

                // Actions
                KeyCode::Enter => {
                    match app.current_view {
                        View::Tasks => {
                            match app.focus {
                                Focus::WorkItems => app.toggle_expand(),
                                Focus::Preview => {
                                    // Open reference in browser (for References tab)
                                }
                            }
                        }
                        View::CICD => {
                            match app.cicd_focus {
                                crate::app::CICDFocus::Pipelines => {
                                    match app.pipeline_drill_down {
                                        crate::app::PipelineDrillDown::None => {
                                            // Drill into pipeline runs
                                            if let Some(pipeline) = app.pipelines.get(app.selected_pipeline_idx) {
                                                let pipeline_id = pipeline.id;
                                                app.pipeline_drill_down = crate::app::PipelineDrillDown::Runs;
                                                app.pipeline_runs.clear();
                                                app.selected_pipeline_run_idx = 0;
                                                // Load runs in background
                                                app.start_pipeline_runs_loader(pipeline_id);
                                            }
                                        }
                                        crate::app::PipelineDrillDown::Runs => {
                                            // Drill into run timeline (stages/jobs/tasks)
                                            if let Some(run) = app.pipeline_runs.get(app.selected_pipeline_run_idx) {
                                                let build_id = run.id;
                                                let is_running = run.status.as_deref() == Some("inProgress");
                                                app.selected_run_id = Some(build_id);  // Store for log loading
                                                app.pipeline_drill_down = crate::app::PipelineDrillDown::Tasks;
                                                app.timeline_records.clear();
                                                app.selected_task_idx = 0;
                                                app.build_log_lines.clear();
                                                app.log_scroll = 0;
                                                // Load timeline in background
                                                app.start_timeline_loader(build_id);
                                                // Auto-start live preview for running builds
                                                if is_running {
                                                    app.start_live_preview(build_id);
                                                }
                                            }
                                        }
                                        crate::app::PipelineDrillDown::Tasks => {
                                            // Load logs for selected task
                                            // Extract log_id first to avoid borrow issues
                                            let log_info = {
                                                let tasks = app.get_timeline_tasks();
                                                tasks.get(app.selected_task_idx)
                                                    .and_then(|task| task.log.as_ref())
                                                    .map(|log| log.id)
                                            };
                                            if let (Some(log_id), Some(build_id)) = (log_info, app.selected_run_id) {
                                                app.build_log_lines.clear();
                                                app.log_scroll = 0;
                                                app.start_log_loader(build_id, log_id);
                                            }
                                        }
                                    }
                                }
                                crate::app::CICDFocus::Releases => {
                                    match app.release_drill_down {
                                        crate::app::ReleaseDrillDown::None => {
                                            // Drill into releases for this definition
                                            if let Some(release_def) = app.releases.get(app.selected_release_idx) {
                                                let def_id = release_def.id;
                                                app.release_drill_down = crate::app::ReleaseDrillDown::Items;
                                                app.release_list.clear();
                                                app.selected_release_item_idx = 0;
                                                app.start_releases_loader(def_id);
                                            }
                                        }
                                        crate::app::ReleaseDrillDown::Items => {
                                            // Drill into stages for selected release
                                            if let Some(release) = app.release_list.get(app.selected_release_item_idx) {
                                                let release_id = release.id;
                                                app.release_drill_down = crate::app::ReleaseDrillDown::Stages;
                                                app.start_release_stages_loader(release_id);
                                                // Start auto-refresh for this release
                                                app.start_release_auto_refresh(release_id);
                                            }
                                        }
                                        crate::app::ReleaseDrillDown::Stages => {
                                            // Drill into tasks for selected stage
                                            if !app.release_stages.is_empty() {
                                                app.load_release_tasks_from_stage(app.selected_release_stage_idx);
                                                if !app.release_tasks.is_empty() {
                                                    app.release_drill_down = crate::app::ReleaseDrillDown::Tasks;
                                                } else {
                                                    app.set_status("No tasks in this stage");
                                                }
                                            }
                                        }
                                        crate::app::ReleaseDrillDown::Tasks => {
                                            // Load logs for selected task
                                            let log_url = app.release_tasks.get(app.selected_release_task_idx)
                                                .and_then(|task| task.log_url.clone());
                                            if let Some(url) = log_url {
                                                app.start_release_task_log_loader(&url);
                                                app.cicd_focus = crate::app::CICDFocus::Preview;
                                            } else {
                                                app.set_status("No log available for this task");
                                            }
                                        }
                                    }
                                }
                                crate::app::CICDFocus::Preview => {}
                            }
                        }
                    }
                }

                // Escape handling
                KeyCode::Esc => {
                    match app.current_view {
                        View::Tasks => {} // No action in Tasks view
                        View::CICD => {
                            if app.cicd_focus == crate::app::CICDFocus::Preview {
                                // Return from preview to correct pane based on drill-down state
                                if app.release_drill_down == crate::app::ReleaseDrillDown::Tasks {
                                    app.cicd_focus = crate::app::CICDFocus::Releases;
                                    app.release_task_logs.clear();
                                } else if app.pipeline_drill_down != crate::app::PipelineDrillDown::None {
                                    app.cicd_focus = crate::app::CICDFocus::Pipelines;
                                } else {
                                    app.cicd_focus = crate::app::CICDFocus::Releases;
                                }
                            } else {
                                // Handle escape based on focus
                                match app.cicd_focus {
                                    crate::app::CICDFocus::Pipelines => {
                                        match app.pipeline_drill_down {
                                            crate::app::PipelineDrillDown::Tasks => {
                                                // Go back to runs list
                                                app.pipeline_drill_down = crate::app::PipelineDrillDown::Runs;
                                                app.timeline_records.clear();
                                                app.build_log_lines.clear();
                                            }
                                            crate::app::PipelineDrillDown::Runs => {
                                                // Go back to pipelines list
                                                app.pipeline_drill_down = crate::app::PipelineDrillDown::None;
                                                app.pipeline_runs.clear();
                                            }
                                            crate::app::PipelineDrillDown::None => {}
                                        }
                                    }
                                    crate::app::CICDFocus::Releases => {
                                        match app.release_drill_down {
                                            crate::app::ReleaseDrillDown::Tasks => {
                                                // Go back to stages list
                                                app.release_drill_down = crate::app::ReleaseDrillDown::Stages;
                                                app.release_tasks.clear();
                                                app.release_task_logs.clear();
                                            }
                                            crate::app::ReleaseDrillDown::Stages => {
                                                // Go back to releases list
                                                app.release_drill_down = crate::app::ReleaseDrillDown::Items;
                                                app.release_stages.clear();
                                                app.stop_release_auto_refresh();
                                            }
                                            crate::app::ReleaseDrillDown::Items => {
                                                // Go back to release definitions list
                                                app.release_drill_down = crate::app::ReleaseDrillDown::None;
                                                app.release_list.clear();
                                            }
                                            crate::app::ReleaseDrillDown::None => {}
                                        }
                                    }
                                    crate::app::CICDFocus::Preview => {}
                                }
                            }
                        }
                    }
                }

                // Task-specific keybindings
                KeyCode::Char('t') => {
                    if app.current_view == View::Tasks {
                        app.toggle_expand_all();
                    }
                }

                KeyCode::Char('p') => {
                    match app.current_view {
                        View::Tasks => app.toggle_pin(),
                        View::CICD => {
                            // Only allow pinning at parent level (not in drill-down)
                            match app.cicd_focus {
                                crate::app::CICDFocus::Pipelines if app.pipeline_drill_down == crate::app::PipelineDrillDown::None => {
                                    app.toggle_pin_pipeline();
                                }
                                crate::app::CICDFocus::Releases if app.release_drill_down == crate::app::ReleaseDrillDown::None => {
                                    app.toggle_pin_release();
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // Modes - Sprint select is Tasks only, Project select works for both
                KeyCode::Char('I') => {
                    if app.current_view == View::Tasks {
                        app.input_mode = InputMode::SprintSelect;
                        app.dropdown_list_state.select(Some(app.selected_sprint_idx));
                    }
                }
                KeyCode::Char('P') => {
                    app.input_mode = InputMode::ProjectSelect;
                    app.dropdown_list_state.select(Some(app.current_project_idx));
                }
                KeyCode::Char('S') => {
                    if app.current_view == View::Tasks && app.selected_work_item().is_some() {
                        app.input_mode = InputMode::EditState;
                        app.dropdown_list_state.select(Some(0));
                    }
                }
                KeyCode::Char('f') => {
                    match app.current_view {
                        View::Tasks => {
                            app.search_query.clear();
                            app.input_mode = InputMode::Search;
                        }
                        View::CICD => {
                            app.cicd_search_query.clear();
                            app.input_mode = InputMode::CICDSearch;
                        }
                    }
                }
                KeyCode::Char('A') => {
                    if app.current_view == View::Tasks && app.selected_work_item().is_some() && !app.users.is_empty() {
                        app.input_mode = InputMode::EditAssignee;
                        app.dropdown_list_state.select(Some(0));
                    } else if app.current_view == View::CICD
                        && app.cicd_focus == crate::app::CICDFocus::Releases
                        && app.release_drill_down == crate::app::ReleaseDrillDown::Stages
                    {
                        // Approve ALL pending stages
                        app.approve_all_pending_stages();
                    }
                }

                // Edit/open log in nvim (CICD view only, when logs are available)
                KeyCode::Char('e') => {
                    if app.current_view == View::CICD
                        && app.pipeline_drill_down == crate::app::PipelineDrillDown::Tasks
                        && !app.build_log_lines.is_empty()
                    {
                        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
                        match app.open_log_viewer(cols, rows) {
                            Ok(_) => app.set_status("Log viewer opened (Ctrl+q to exit)"),
                            Err(e) => app.set_error(format!("Failed to open log viewer: {e}")),
                        }
                    }
                }

                // Cancel pipeline run or release (C key in CICD view)
                KeyCode::Char('C') => {
                    if app.current_view == View::CICD {
                        match app.cicd_focus {
                            crate::app::CICDFocus::Pipelines => {
                                if app.pipeline_drill_down == crate::app::PipelineDrillDown::Runs {
                                    // Cancel pipeline run - only if in progress
                                    if let Some(run) = app.pipeline_runs.get(app.selected_pipeline_run_idx) {
                                        if run.status.as_deref() == Some("inProgress") {
                                            let build_number = run.build_number.clone().unwrap_or_else(|| "?".to_string());
                                            app.confirm_action_dialog = Some(crate::app::ConfirmActionDialog::new(
                                                crate::app::ConfirmActionType::CancelPipelineRun {
                                                    run_id: run.id,
                                                    build_number,
                                                }
                                            ));
                                            app.input_mode = InputMode::ConfirmAction;
                                        } else {
                                            app.set_status("Can only cancel running builds");
                                        }
                                    }
                                }
                            }
                            crate::app::CICDFocus::Releases => {
                                match app.release_drill_down {
                                    crate::app::ReleaseDrillDown::Items => {
                                        // Cancel/abandon entire release
                                        if let Some(release) = app.release_list.get(app.selected_release_item_idx) {
                                            // Check if release has any in-progress environments
                                            let has_active = release.environments.as_ref()
                                                .map(|envs| envs.iter().any(|e| e.status.as_deref() == Some("inProgress")))
                                                .unwrap_or(false);
                                            if has_active || release.status.as_deref() == Some("active") {
                                                app.confirm_action_dialog = Some(crate::app::ConfirmActionDialog::new(
                                                    crate::app::ConfirmActionType::CancelRelease {
                                                        release_id: release.id,
                                                        release_name: release.name.clone(),
                                                    }
                                                ));
                                                app.input_mode = InputMode::ConfirmAction;
                                            } else {
                                                app.set_status("Release is not active");
                                            }
                                        }
                                    }
                                    crate::app::ReleaseDrillDown::Stages => {
                                        // Cancel specific stage or reject pending approval
                                        if let Some(stage) = app.release_stages.get(app.selected_release_stage_idx) {
                                            let release = app.release_list.get(app.selected_release_item_idx);
                                            let release_name = release.map(|r| r.name.clone()).unwrap_or_default();
                                            let release_id = release.map(|r| r.id).unwrap_or(0);

                                            // Check for pending approval first
                                            let pending_approval = stage.pre_deploy_approvals.iter()
                                                .find(|a| a.status.as_deref() == Some("pending"));

                                            if let Some(approval) = pending_approval {
                                                // Reject the pending approval
                                                app.confirm_action_dialog = Some(crate::app::ConfirmActionDialog::new(
                                                    crate::app::ConfirmActionType::RejectApproval {
                                                        approval_id: approval.id,
                                                        release_id,
                                                        environment_name: stage.name.clone(),
                                                    }
                                                ));
                                                app.input_mode = InputMode::ConfirmAction;
                                            } else if stage.status.as_deref() == Some("inProgress") {
                                                // Cancel in-progress stage
                                                app.confirm_action_dialog = Some(crate::app::ConfirmActionDialog::new(
                                                    crate::app::ConfirmActionType::CancelReleaseEnvironment {
                                                        release_id,
                                                        environment_id: stage.id,
                                                        release_name,
                                                        environment_name: stage.name.clone(),
                                                    }
                                                ));
                                                app.input_mode = InputMode::ConfirmAction;
                                            } else {
                                                app.set_status("No action available (not in-progress, no pending approval)");
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // Trigger/Retrigger (T key in CICD view - context-sensitive)
                KeyCode::Char('T') => {
                    if app.current_view == View::CICD {
                        match app.cicd_focus {
                            crate::app::CICDFocus::Pipelines => {
                                if app.pipeline_drill_down == crate::app::PipelineDrillDown::Runs {
                                    // Retrigger pipeline run
                                    if let Some(run) = app.pipeline_runs.get(app.selected_pipeline_run_idx) {
                                        // Can retrigger completed, failed, or canceled runs
                                        if run.status.as_deref() == Some("completed") {
                                            let branch = run.source_branch.clone()
                                                .unwrap_or_else(|| "refs/heads/main".to_string());
                                            let build_number = run.build_number.clone().unwrap_or_else(|| "?".to_string());
                                            let pipeline_id = run.definition.as_ref().map(|d| d.id)
                                                .or(app.current_pipeline_id)
                                                .unwrap_or(0);

                                            app.confirm_action_dialog = Some(crate::app::ConfirmActionDialog::new(
                                                crate::app::ConfirmActionType::RetriggerPipelineRun {
                                                    pipeline_id,
                                                    branch,
                                                    build_number,
                                                }
                                            ));
                                            app.input_mode = InputMode::ConfirmAction;
                                        } else {
                                            app.set_status("Can only retrigger completed builds");
                                        }
                                    }
                                }
                            }
                            crate::app::CICDFocus::Releases => {
                                match app.release_drill_down {
                                    crate::app::ReleaseDrillDown::None => {
                                        // Original behavior: open release trigger dialog for new release
                                        if let Some(release_def) = app.releases.get(app.selected_release_idx) {
                                            let def_id = release_def.id;
                                            let def_name = release_def.name.clone();
                                            app.open_release_trigger_dialog(def_id, def_name);
                                        }
                                    }
                                    crate::app::ReleaseDrillDown::Stages => {
                                        // Retrigger/redeploy specific stage
                                        if let Some(stage) = app.release_stages.get(app.selected_release_stage_idx) {
                                            // Can redeploy if not currently in progress
                                            if stage.status.as_deref() != Some("inProgress") {
                                                let release = app.release_list.get(app.selected_release_item_idx);
                                                let release_name = release.map(|r| r.name.clone()).unwrap_or_default();
                                                let release_id = release.map(|r| r.id).unwrap_or(0);

                                                app.confirm_action_dialog = Some(crate::app::ConfirmActionDialog::new(
                                                    crate::app::ConfirmActionType::RetriggerReleaseEnvironment {
                                                        release_id,
                                                        environment_id: stage.id,
                                                        release_name,
                                                        environment_name: stage.name.clone(),
                                                    }
                                                ));
                                                app.input_mode = InputMode::ConfirmAction;
                                            } else {
                                                app.set_status("Stage is already in progress");
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // Approve stage (CICD view, Stages drill-down) or Filter assignee (Tasks view)
                KeyCode::Char('a') => {
                    match app.current_view {
                        View::Tasks => {
                            app.input_mode = InputMode::FilterAssignee;
                            app.dropdown_list_state.select(Some(0));
                        }
                        View::CICD => {
                            if app.cicd_focus == crate::app::CICDFocus::Releases
                                && app.release_drill_down == crate::app::ReleaseDrillDown::Stages
                            {
                                if let Some(stage) = app.release_stages.get(app.selected_release_stage_idx) {
                                    let env_id = stage.id;
                                    let stage_name = stage.name.clone();
                                    app.approve_stage(env_id, &stage_name);
                                }
                            }
                        }
                    }
                }

                // Load all runs (CICD view, PipelineRuns drill-down)
                KeyCode::Char('L') => {
                    if app.current_view == View::CICD
                        && app.pipeline_drill_down == crate::app::PipelineDrillDown::Runs
                        && app.pipeline_runs_limited
                    {
                        if let Some(pipeline_id) = app.current_pipeline_id {
                            app.start_pipeline_runs_loader_all(pipeline_id);
                            app.set_status("Loading all runs...");
                        }
                    }
                }

                // Toggle live preview (CICD view, when viewing build timeline)
                KeyCode::Char('w') => {
                    if app.current_view == View::CICD
                        && app.pipeline_drill_down == crate::app::PipelineDrillDown::Tasks
                    {
                        if app.live_preview_enabled {
                            app.stop_live_preview();
                            app.set_status("Live preview stopped");
                        } else if let Some(build_id) = app.selected_run_id {
                            app.start_live_preview(build_id);
                            app.set_status("Live preview enabled (auto-refresh every 3s)");
                        }
                    }
                }

                // Open in browser - view-aware
                KeyCode::Char('o') => {
                    match app.current_view {
                        View::Tasks => {
                            // Existing Tasks open logic
                            match app.focus {
                                Focus::WorkItems => {
                                    if let (Some(item), Some(project)) = (app.selected_work_item(), app.current_project()) {
                                        let url = format!(
                                            "{}/_workitems/edit/{}",
                                            project.organization.trim_end_matches('/'),
                                            item.item.id
                                        );
                                        if let Err(e) = open::that(&url) {
                                            app.set_error(format!("Failed to open browser: {e}"));
                                        } else {
                                            app.set_status(format!("Opened #{}", item.item.id));
                                        }
                                    }
                                }
                                Focus::Preview => {
                                    if app.preview_tab == crate::app::PreviewTab::References {
                                        if let Some(relation) = app.selected_relation() {
                                            if let Some(url) = app.get_relation_url(relation) {
                                                if let Err(e) = open::that(&url) {
                                                    app.set_error(format!("Failed to open browser: {e}"));
                                                } else {
                                                    let name = relation.attributes.name.as_deref().unwrap_or("link");
                                                    app.set_status(format!("Opened {name}"));
                                                }
                                            }
                                        } else if let (Some(item), Some(project)) = (app.selected_work_item(), app.current_project()) {
                                            let url = format!(
                                                "{}/_workitems/edit/{}",
                                                project.organization.trim_end_matches('/'),
                                                item.item.id
                                            );
                                            let _ = open::that(&url);
                                        }
                                    } else if let (Some(item), Some(project)) = (app.selected_work_item(), app.current_project()) {
                                        let url = format!(
                                            "{}/_workitems/edit/{}",
                                            project.organization.trim_end_matches('/'),
                                            item.item.id
                                        );
                                        let _ = open::that(&url);
                                    }
                                }
                            }
                        }
                        View::CICD => {
                            // Open pipeline or release in browser - context-aware URLs
                            if let Some(project) = app.current_project() {
                                let org = project.organization.trim_end_matches('/');
                                let proj_encoded = urlencoding::encode(&project.project);

                                let url = match app.cicd_focus {
                                    crate::app::CICDFocus::Pipelines => {
                                        match app.pipeline_drill_down {
                                            crate::app::PipelineDrillDown::None => {
                                                // Open pipeline definition
                                                if let Some(pipeline) = app.pipelines.get(app.selected_pipeline_idx) {
                                                    format!("{}/{}/_build?definitionId={}", org, proj_encoded, pipeline.id)
                                                } else {
                                                    return Ok(false);
                                                }
                                            }
                                            crate::app::PipelineDrillDown::Runs => {
                                                // Open pipeline run results
                                                if let Some(run) = app.pipeline_runs.get(app.selected_pipeline_run_idx) {
                                                    format!("{}/{}/_build/results?buildId={}&view=results", org, proj_encoded, run.id)
                                                } else {
                                                    return Ok(false);
                                                }
                                            }
                                            crate::app::PipelineDrillDown::Tasks => {
                                                // Open pipeline run logs with job/task context
                                                if let Some(run) = app.pipeline_runs.get(app.selected_pipeline_run_idx) {
                                                    // Get selected task's parent job and task IDs for deep linking
                                                    if let Some(task) = app.timeline_records.iter()
                                                        .filter(|r| r.record_type.as_deref() == Some("Task"))
                                                        .nth(app.selected_task_idx)
                                                    {
                                                        if let Some(parent_id) = &task.parent_id {
                                                            format!("{}/{}/_build/results?buildId={}&view=logs&j={}&t={}",
                                                                org, proj_encoded, run.id, parent_id, task.id)
                                                        } else {
                                                            format!("{}/{}/_build/results?buildId={}&view=logs", org, proj_encoded, run.id)
                                                        }
                                                    } else {
                                                        format!("{}/{}/_build/results?buildId={}&view=logs", org, proj_encoded, run.id)
                                                    }
                                                } else {
                                                    return Ok(false);
                                                }
                                            }
                                        }
                                    }
                                    crate::app::CICDFocus::Releases => {
                                        match app.release_drill_down {
                                            crate::app::ReleaseDrillDown::None => {
                                                // Open release definition
                                                if let Some(release_def) = app.releases.get(app.selected_release_idx) {
                                                    format!("{}/{}/_release?_a=releases&view=mine&definitionId={}",
                                                        org, proj_encoded, release_def.id)
                                                } else {
                                                    return Ok(false);
                                                }
                                            }
                                            crate::app::ReleaseDrillDown::Items | crate::app::ReleaseDrillDown::Stages | crate::app::ReleaseDrillDown::Tasks => {
                                                // Open specific release (same URL for items, stages, and tasks)
                                                if let Some(release) = app.release_list.get(app.selected_release_item_idx) {
                                                    format!("{}/{}/_releaseProgress?releaseId={}&_a=release-pipeline-progress",
                                                        org, proj_encoded, release.id)
                                                } else {
                                                    return Ok(false);
                                                }
                                            }
                                        }
                                    }
                                    crate::app::CICDFocus::Preview => {
                                        // From preview, open whatever is being previewed
                                        if app.pipeline_drill_down == crate::app::PipelineDrillDown::Tasks {
                                            if let Some(run) = app.pipeline_runs.get(app.selected_pipeline_run_idx) {
                                                format!("{}/{}/_build/results?buildId={}&view=logs", org, proj_encoded, run.id)
                                            } else {
                                                return Ok(false);
                                            }
                                        } else if app.release_drill_down == crate::app::ReleaseDrillDown::Items {
                                            if let Some(release) = app.release_list.get(app.selected_release_item_idx) {
                                                format!("{}/{}/_releaseProgress?releaseId={}&_a=release-pipeline-progress",
                                                    org, proj_encoded, release.id)
                                            } else {
                                                return Ok(false);
                                            }
                                        } else {
                                            return Ok(false);
                                        }
                                    }
                                };
                                if let Err(e) = open::that(&url) {
                                    app.set_error(format!("Failed to open browser: {e}"));
                                } else {
                                    app.set_status("Opened in browser");
                                }
                            }
                        }
                    }
                }

                // Copy to clipboard - Tasks only
                KeyCode::Char('y') => {
                    if app.current_view == View::Tasks {
                        if let Some(item) = app.selected_work_item() {
                            let id = item.item.id.to_string();
                            if let Ok(mut clipboard) = Clipboard::new() {
                                let _ = clipboard.set_text(&id);
                                app.set_status(format!("Copied #{id} to clipboard"));
                            }
                        }
                    }
                }
                KeyCode::Char('Y') => {
                    if app.current_view == View::Tasks {
                        if let Some(item) = app.selected_work_item() {
                            let content = format_ticket_content(&item.item);
                            if let Ok(mut clipboard) = Clipboard::new() {
                                let _ = clipboard.set_text(&content);
                                app.set_status("Copied ticket content to clipboard");
                            }
                        }
                    }
                }

                // Filters - Tasks only
                KeyCode::Char('s') => {
                    if app.current_view == View::Tasks {
                        app.input_mode = InputMode::FilterState;
                        app.dropdown_list_state.select(Some(0));
                    }
                }
                KeyCode::Char('c') => {
                    if app.current_view == View::Tasks && app.has_active_filters() {
                        app.clear_filters();
                        app.save_to_cache();
                        app.set_status("Filters cleared");
                    }
                }

                // Refresh
                KeyCode::Char('r') => {
                    match app.current_view {
                        View::Tasks => {
                            app.set_loading(true, "Refreshing...");
                            // Cache relations before refresh
                            let relations_cache = app.cache_relations();

                            if let Err(e) = app.load_sprints().await {
                                app.set_error(format!("Failed: {e}"));
                            } else if let Err(e) = app.load_work_items().await {
                                app.set_error(format!("Failed: {e}"));
                            } else {
                                let _ = app.load_users().await;
                                // Restore relations after refresh
                                app.restore_relations(relations_cache);
                                app.save_to_cache();
                                app.set_status("Refreshed & cached");
                                // Restart relation loader for any new items
                                app.relations_loader_active = false;
                                app.start_relations_loader();
                            }
                            app.set_loading(false, "");
                        }
                        View::CICD => {
                            // Context-aware refresh based on current focus and drill-down
                            match app.cicd_focus {
                                crate::app::CICDFocus::Pipelines => {
                                    match app.pipeline_drill_down {
                                        crate::app::PipelineDrillDown::None => {
                                            app.force_refresh_cicd();
                                            app.set_status("Refreshing pipelines...");
                                        }
                                        crate::app::PipelineDrillDown::Runs => {
                                            if let Some(pipeline_id) = app.current_pipeline_id {
                                                app.force_refresh_pipeline_runs(pipeline_id);
                                                app.set_status("Refreshing runs...");
                                            }
                                        }
                                        crate::app::PipelineDrillDown::Tasks => {
                                            if let Some(run) = app.pipeline_runs.get(app.selected_pipeline_run_idx) {
                                                let build_id = run.id;
                                                app.force_refresh_timeline(build_id);
                                                app.set_status("Refreshing tasks...");
                                            }
                                        }
                                    }
                                }
                                crate::app::CICDFocus::Releases => {
                                    match app.release_drill_down {
                                        crate::app::ReleaseDrillDown::None => {
                                            app.force_refresh_cicd();
                                            app.set_status("Refreshing releases...");
                                        }
                                        crate::app::ReleaseDrillDown::Items => {
                                            if let Some(def_id) = app.current_release_def_id {
                                                app.force_refresh_releases(def_id);
                                                app.set_status("Refreshing release items...");
                                            }
                                        }
                                        crate::app::ReleaseDrillDown::Stages => {
                                            // Refresh stages by reloading release detail
                                            if let Some(release) = app.release_list.get(app.selected_release_item_idx) {
                                                app.start_release_stages_loader(release.id);
                                                app.set_status("Refreshing stages...");
                                            }
                                        }
                                        crate::app::ReleaseDrillDown::Tasks => {
                                            // Refresh tasks from current stage
                                            if !app.release_stages.is_empty() {
                                                app.load_release_tasks_from_stage(app.selected_release_stage_idx);
                                                app.set_status("Refreshing tasks...");
                                            }
                                        }
                                    }
                                }
                                crate::app::CICDFocus::Preview => {
                                    // Refresh top-level from preview
                                    app.force_refresh_cicd();
                                    app.set_status("Refreshing CI/CD...");
                                }
                            }
                        }
                    }
                }

                _ => {}
            }
        }

        InputMode::ConfirmAction => {
            match key.code {
                KeyCode::Esc | KeyCode::Char('n') => {
                    app.confirm_action_dialog = None;
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Char('y') | KeyCode::Enter => {
                    if let Some(dialog) = app.confirm_action_dialog.take() {
                        app.execute_confirmed_action(dialog.action_type);
                    }
                    app.input_mode = InputMode::Normal;
                }
                _ => {}
            }
        }
    }

    Ok(false)
}

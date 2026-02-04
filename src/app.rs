use crate::azure::{AzureCli, WorkItem, Sprint, User, WorkItemRelation, Pipeline, PipelineRun, ReleaseDefinition, Release, TimelineRecord};
use crate::cache::{self, CacheEntry, CICDCacheEntry};
use crate::config::Config;
use crate::terminal::EmbeddedTerminal;
use anyhow::Result;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::widgets::ListState;
use std::collections::HashSet;
use tokio::sync::mpsc;

/// Result type for background CI/CD loading
pub enum CICDLoadResult {
    Pipelines(Vec<Pipeline>),
    ReleaseDefinitions(Vec<ReleaseDefinition>),
    PipelineRuns(Vec<PipelineRun>),
    Releases(Vec<Release>),
    ReleaseDetail(usize, Release),  // (index in release_list, release with environments)
    ReleaseStages(Vec<crate::azure::ReleaseEnvironment>),  // Stages for selected release
    ReleaseTasks(Vec<crate::azure::ReleaseTask>),          // Tasks for selected stage
    ReleaseTaskLog(Vec<String>),                           // Log for selected task
    Timeline(Vec<TimelineRecord>),
    BuildLog(Vec<String>),
    PendingApprovals(Vec<crate::azure::Approval>),
    ReleaseDefinitionDetail(crate::azure::ReleaseDefinitionDetail),
    ReleaseCreated(Release),
    ApprovalUpdated { approval_id: i32, release_id: i32, status: String },
    PipelineRunCanceled(i32),
    PipelineRunRetriggered(PipelineRun),
    ReleaseCanceled(i32),
    ReleaseEnvironmentCanceled { release_id: i32, environment_name: String },
    ReleaseEnvironmentRedeployed { release_id: i32, environment_name: String },
    TimelineDelta {
        build_id: i32,
        records: Vec<TimelineRecord>,
        change_id: Option<i32>,
    },
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    WorkItems,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    SprintSelect,
    EditState,
    EditAssignee,
    Search,
    Help,
    ProjectSelect,
    FilterState,
    FilterAssignee,
    CICDSearch,  // Fuzzy search in CICD view
    ReleaseTriggerDialog,
    ApprovalConfirm,
    ConfirmAction,  // For cancel/retrigger confirmation dialog
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewTab {
    #[default]
    Details,
    References,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Tasks,
    CICD,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CICDFocus {
    #[default]
    Pipelines,
    Releases,
    Preview,
}

/// Pipeline-specific drill-down state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PipelineDrillDown {
    #[default]
    None,
    Runs,   // Viewing runs for selected pipeline
    Tasks,  // Viewing tasks for selected run
}

/// Release-specific drill-down state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReleaseDrillDown {
    #[default]
    None,
    Items,   // Viewing releases for selected definition
    Stages,  // Viewing stages/environments for selected release
    Tasks,   // Viewing tasks for selected stage
}

/// Dialog cursor position for release trigger dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogCursor {
    Description,
    Stages,
    Submit,
    Cancel,
}

impl Default for DialogCursor {
    fn default() -> Self {
        DialogCursor::Stages
    }
}

/// Stage selection for release trigger dialog
#[derive(Debug, Clone)]
pub struct StageSelection {
    pub id: i32,
    pub name: String,
    pub enabled: bool,
}

/// Release trigger dialog state
#[derive(Debug, Clone)]
pub struct ReleaseTriggerDialog {
    pub definition_id: i32,
    pub definition_name: String,
    pub description: String,
    pub stages: Vec<StageSelection>,
    pub selected_idx: usize,
    pub cursor: DialogCursor,
    pub loading: bool,
}

impl ReleaseTriggerDialog {
    pub fn new(definition_id: i32, definition_name: String) -> Self {
        Self {
            definition_id,
            definition_name,
            description: String::new(),
            stages: Vec::new(),
            selected_idx: 0,
            cursor: DialogCursor::Description,
            loading: true,
        }
    }
}

/// Type of action to confirm
#[derive(Debug, Clone)]
pub enum ConfirmActionType {
    CancelPipelineRun { run_id: i32, build_number: String },
    RetriggerPipelineRun { pipeline_id: i32, branch: String, build_number: String },
    CancelRelease { release_id: i32, release_name: String },
    CancelReleaseEnvironment { release_id: i32, environment_id: i32, release_name: String, environment_name: String },
    RetriggerReleaseEnvironment { release_id: i32, environment_id: i32, release_name: String, environment_name: String },
    RejectApproval { approval_id: i32, release_id: i32, environment_name: String },
}

/// Confirmation dialog state for cancel/retrigger actions
#[derive(Debug, Clone)]
pub struct ConfirmActionDialog {
    pub action_type: ConfirmActionType,
    pub confirmed: bool,
}

impl ConfirmActionDialog {
    pub fn new(action_type: ConfirmActionType) -> Self {
        Self {
            action_type,
            confirmed: false,
        }
    }

    /// Get the title for the dialog based on action type
    pub fn title(&self) -> &'static str {
        match &self.action_type {
            ConfirmActionType::CancelPipelineRun { .. } => "Cancel Pipeline Run?",
            ConfirmActionType::RetriggerPipelineRun { .. } => "Retrigger Pipeline?",
            ConfirmActionType::CancelRelease { .. } => "Abandon Release?",
            ConfirmActionType::CancelReleaseEnvironment { .. } => "Cancel Stage?",
            ConfirmActionType::RetriggerReleaseEnvironment { .. } => "Redeploy Stage?",
            ConfirmActionType::RejectApproval { .. } => "Reject Approval?",
        }
    }

    /// Get description text for the dialog
    pub fn description(&self) -> String {
        match &self.action_type {
            ConfirmActionType::CancelPipelineRun { build_number, .. } =>
                format!("Cancel build #{}?", build_number),
            ConfirmActionType::RetriggerPipelineRun { branch, build_number, .. } =>
                format!("Retrigger #{} on branch '{}'?", build_number, branch),
            ConfirmActionType::CancelRelease { release_name, .. } =>
                format!("Abandon release '{}'?", release_name),
            ConfirmActionType::CancelReleaseEnvironment { environment_name, release_name, .. } =>
                format!("Cancel '{}' stage in '{}'?", environment_name, release_name),
            ConfirmActionType::RetriggerReleaseEnvironment { environment_name, release_name, .. } =>
                format!("Redeploy '{}' stage in '{}'?", environment_name, release_name),
            ConfirmActionType::RejectApproval { environment_name, .. } =>
                format!("Reject approval for '{}'?", environment_name),
        }
    }
}

impl PreviewTab {
    pub fn next(&self) -> Self {
        match self {
            Self::Details => Self::References,
            Self::References => Self::Details,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Details => Self::References,
            Self::References => Self::Details,
        }
    }
}

pub struct App {
    // Config
    pub config: Config,
    pub current_project_idx: usize,

    // UI state
    pub focus: Focus,
    pub input_mode: InputMode,
    pub preview_tab: PreviewTab,

    // View state
    pub current_view: View,

    // CI/CD state
    pub cicd_focus: CICDFocus,
    pub pipeline_drill_down: PipelineDrillDown,
    pub release_drill_down: ReleaseDrillDown,
    pub pipelines: Vec<Pipeline>,
    pub releases: Vec<ReleaseDefinition>,
    pub pipeline_runs: Vec<PipelineRun>,
    pub release_list: Vec<Release>,
    pub release_stages: Vec<crate::azure::ReleaseEnvironment>,  // Stages for selected release
    pub release_tasks: Vec<crate::azure::ReleaseTask>,          // Tasks for selected stage
    pub release_task_logs: Vec<String>,                         // Logs for selected task
    pub selected_pipeline_idx: usize,
    pub selected_release_idx: usize,
    pub selected_pipeline_run_idx: usize,
    pub selected_release_item_idx: usize,
    pub selected_release_stage_idx: usize,
    pub selected_release_task_idx: usize,
    pub pipeline_list_state: ListState,
    pub release_list_state: ListState,
    pub release_item_list_state: ListState,
    pub release_stage_list_state: ListState,
    pub release_task_list_state: ListState,
    pub pipeline_runs_list_state: ListState,
    pub task_list_state: ListState,
    pub cicd_preview_scroll: u16,
    pub cicd_loading: bool,
    pub cicd_search_query: String,  // Fuzzy search for CICD
    pub cicd_rx: Option<mpsc::Receiver<CICDLoadResult>>,
    pub timeline_records: Vec<TimelineRecord>,
    pub selected_task_idx: usize,
    pub build_log_lines: Vec<String>,
    pub log_scroll: usize,
    pub selected_run_id: Option<i32>,
    pub current_pipeline_id: Option<i32>,
    pub current_release_def_id: Option<i32>,
    pub current_log_id: Option<i32>,
    pub pipeline_runs_limited: bool,  // True if showing limited (10) runs
    pub pinned_pipelines: HashSet<i32>,
    pub pinned_releases: HashSet<i32>,

    // Live preview state
    pub live_preview_enabled: bool,
    pub live_preview_build_id: Option<i32>,  // Currently watched build
    pub live_preview_change_id: Option<i32>, // Last changeId for delta polling
    pub live_preview_last_poll: std::time::Instant,
    pub cicd_tx: Option<mpsc::Sender<CICDLoadResult>>,

    // Release auto-refresh
    pub release_auto_refresh: bool,
    pub release_auto_refresh_id: Option<i32>,  // Release ID to auto-refresh
    pub release_last_refresh: std::time::Instant,
    pub pending_select_release_id: Option<i32>,  // Release to select after list reload

    // Approvals
    pub pending_approvals: Vec<crate::azure::Approval>,
    pub pending_approvals_count: usize,
    pub approvals_loading: bool,

    // Release trigger dialog state
    pub release_trigger_dialog: Option<ReleaseTriggerDialog>,
    pub approval_dialog: Option<(String, String)>,  // (approval_type, stage_name)
    pub confirm_action_dialog: Option<ConfirmActionDialog>,

    // Status
    pub status_message: Option<String>,
    pub status_is_error: bool,
    pub status_set_at: Option<std::time::Instant>,
    pub loading: bool,

    // Data
    pub sprints: Vec<Sprint>,
    pub selected_sprint_idx: usize,
    pub work_items: Vec<WorkItem>,
    pub users: Vec<User>,
    pub current_user: Option<String>,

    // Selection state
    pub work_item_list_state: ListState,
    pub sprint_list_state: ListState,
    pub dropdown_list_state: ListState,

    // Scroll
    pub preview_scroll: u16,
    pub preview_scroll_max: u16,
    pub refs_scroll: u16,

    // Input
    pub search_query: String,

    // Filters
    pub filter_state: Option<String>,
    pub filter_assignee: Option<String>,

    // Hierarchy
    pub expanded_items: HashSet<i32>,
    pub pinned_items: HashSet<i32>,
    pub force_collapsed: bool, // When true, don't auto-expand during filtering

    // Flattened work items for display
    pub visible_items: Vec<VisibleWorkItem>,

    // Cache
    pub cache_age: Option<u64>,
    pub cicd_cache_age: Option<u64>,

    // Fuzzy matcher
    pub fuzzy_matcher: SkimMatcherV2,

    // Filter input buffers for fuzzy dropdown filtering
    pub filter_input: String,

    // Loading spinner state (used by ui::draw_loading)
    #[allow(dead_code)]
    pub spinner_frame: usize,

    // Loading message (used by ui::draw_loading)
    #[allow(dead_code)]
    pub loading_message: String,

    // Relations selection (for References tab)
    pub relations_list_state: ListState,

    // IDs of work items with relations already loaded
    pub relations_loaded: HashSet<i32>,

    // Channel for receiving loaded relations from background task
    pub relations_rx: Option<mpsc::Receiver<(i32, Option<Vec<WorkItemRelation>>)>>,
    // Track if background loader is running
    pub relations_loader_active: bool,

    // Cache for PR/commit titles: key is "pr:{id}" or "commit:{hash}"
    pub relation_titles: std::collections::HashMap<String, String>,
    // Channel for receiving titles from background task
    pub titles_rx: Option<mpsc::Receiver<(String, String)>>,
    // Track if title loader is running
    pub titles_loader_active: bool,

    // Embedded terminal for log viewing
    pub embedded_terminal: Option<EmbeddedTerminal>,
    pub terminal_mode: bool,
    pub log_file_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VisibleWorkItem {
    pub item: WorkItem,
    pub depth: usize,
    pub has_children: bool,
    pub is_expanded: bool,
    pub is_pinned: bool,
}

/// Parsed relation info for display
pub struct ParsedRelation {
    pub icon: &'static str,
    pub description: String,
    pub url: Option<String>,
}

impl App {
    pub fn new(config: Config) -> Self {
        let default_idx = config.default_project
            .as_ref()
            .and_then(|name| config.projects.iter().position(|p| &p.name == name))
            .unwrap_or(0);

        Self {
            config,
            current_project_idx: default_idx,
            focus: Focus::WorkItems,
            input_mode: InputMode::Normal,
            preview_tab: PreviewTab::default(),
            current_view: View::default(),
            cicd_focus: CICDFocus::default(),
            pipeline_drill_down: PipelineDrillDown::default(),
            release_drill_down: ReleaseDrillDown::default(),
            pipelines: Vec::new(),
            releases: Vec::new(),
            pipeline_runs: Vec::new(),
            release_list: Vec::new(),
            release_stages: Vec::new(),
            release_tasks: Vec::new(),
            release_task_logs: Vec::new(),
            selected_pipeline_idx: 0,
            selected_release_idx: 0,
            selected_pipeline_run_idx: 0,
            selected_release_item_idx: 0,
            selected_release_stage_idx: 0,
            selected_release_task_idx: 0,
            pipeline_list_state: ListState::default(),
            release_list_state: ListState::default(),
            release_item_list_state: ListState::default(),
            release_stage_list_state: ListState::default(),
            release_task_list_state: ListState::default(),
            pipeline_runs_list_state: ListState::default(),
            task_list_state: ListState::default(),
            cicd_preview_scroll: 0,
            cicd_loading: false,
            cicd_search_query: String::new(),
            cicd_rx: None,
            timeline_records: Vec::new(),
            selected_task_idx: 0,
            build_log_lines: Vec::new(),
            log_scroll: 0,
            selected_run_id: None,
            current_pipeline_id: None,
            current_release_def_id: None,
            current_log_id: None,
            pipeline_runs_limited: false,
            pinned_pipelines: HashSet::new(),
            pinned_releases: HashSet::new(),
            live_preview_enabled: false,
            live_preview_build_id: None,
            live_preview_change_id: None,
            live_preview_last_poll: std::time::Instant::now(),
            cicd_tx: None,
            release_auto_refresh: false,
            release_auto_refresh_id: None,
            release_last_refresh: std::time::Instant::now(),
            pending_select_release_id: None,
            pending_approvals: Vec::new(),
            pending_approvals_count: 0,
            approvals_loading: false,
            release_trigger_dialog: None,
            approval_dialog: None,
            confirm_action_dialog: None,
            status_message: None,
            status_is_error: false,
            status_set_at: None,
            loading: false,
            sprints: Vec::new(),
            selected_sprint_idx: 0,
            work_items: Vec::new(),
            users: Vec::new(),
            current_user: None,
            work_item_list_state: ListState::default(),
            sprint_list_state: ListState::default(),
            dropdown_list_state: ListState::default(),
            preview_scroll: 0,
            preview_scroll_max: 0,
            refs_scroll: 0,
            search_query: String::new(),
            filter_state: None,
            filter_assignee: None,
            expanded_items: HashSet::new(),
            pinned_items: HashSet::new(),
            force_collapsed: false,
            visible_items: Vec::new(),
            cache_age: None,
            cicd_cache_age: None,
            fuzzy_matcher: SkimMatcherV2::default(),
            filter_input: String::new(),
            spinner_frame: 0,
            loading_message: String::new(),
            relations_list_state: ListState::default(),
            relations_loaded: HashSet::new(),
            relations_rx: None,
            relations_loader_active: false,
            relation_titles: std::collections::HashMap::new(),
            titles_rx: None,
            titles_loader_active: false,
            embedded_terminal: None,
            terminal_mode: false,
            log_file_path: None,
        }
    }

    /// Poll for loaded titles from background task (non-blocking)
    pub fn poll_titles(&mut self) {
        let mut results = Vec::new();
        let mut channel_closed = false;

        if let Some(rx) = &mut self.titles_rx {
            loop {
                match rx.try_recv() {
                    Ok(item) => results.push(item),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        channel_closed = true;
                        break;
                    }
                }
            }
        }

        for (key, title) in results {
            self.relation_titles.insert(key, title);
        }

        // Reset flag when loader finishes so it can restart for new relations
        if channel_closed {
            self.titles_loader_active = false;
            self.titles_rx = None;
        }
    }

    /// Get cached title for a relation key
    #[allow(dead_code)]
    pub fn get_relation_title(&self, key: &str) -> Option<&String> {
        self.relation_titles.get(key)
    }

    /// Poll for loaded relations from background task (non-blocking)
    pub fn poll_relations(&mut self) {
        // Collect results first to avoid borrow issues
        let mut results = Vec::new();
        if let Some(rx) = &mut self.relations_rx {
            while let Ok(item) = rx.try_recv() {
                results.push(item);
            }
        }
        // Then apply them
        for (id, relations) in results {
            self.update_work_item_relations(id, relations);
        }
    }

    /// Start background title loader for PRs and commits
    pub fn start_titles_loader(&mut self) {
        if self.titles_loader_active {
            return;
        }

        // Collect all PR IDs and commit hashes that need titles
        let mut pr_ids: Vec<String> = Vec::new();
        let mut commits: Vec<(String, String)> = Vec::new(); // (repo_guid, hash)

        fn collect_from_items(items: &[WorkItem], pr_ids: &mut Vec<String>, commits: &mut Vec<(String, String)>, existing: &std::collections::HashMap<String, String>) {
            for item in items {
                if let Some(relations) = &item.relations {
                    for rel in relations {
                        let name = rel.attributes.name.as_deref().unwrap_or("");
                        let parts: Vec<&str> = rel.url.split("%2F").flat_map(|s| s.split("%2f")).collect();

                        match name {
                            "Pull Request" => {
                                if let Some(pr_id) = parts.last() {
                                    let key = format!("pr:{pr_id}");
                                    if !existing.contains_key(&key) {
                                        pr_ids.push(pr_id.to_string());
                                    }
                                }
                            }
                            "Fixed in Commit" => {
                                if parts.len() >= 2 {
                                    let hash = parts[parts.len() - 1];
                                    let repo = parts[parts.len() - 2];
                                    let key = format!("commit:{hash}");
                                    if !existing.contains_key(&key) {
                                        commits.push((repo.to_string(), hash.to_string()));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                collect_from_items(&item.children, pr_ids, commits, existing);
            }
        }

        collect_from_items(&self.work_items, &mut pr_ids, &mut commits, &self.relation_titles);

        if pr_ids.is_empty() && commits.is_empty() {
            return;
        }

        let (tx, rx) = mpsc::channel(100);
        self.titles_rx = Some(rx);
        self.titles_loader_active = true;

        let client_info = self.current_project().map(|p| (p.organization.clone(), p.project.clone()));

        if let Some((org, project)) = client_info {
            tokio::spawn(async move {
                // Fetch PR titles in parallel
                let pr_futures: Vec<_> = pr_ids.into_iter().map(|pr_id| {
                    let org = org.clone();
                    let tx = tx.clone();
                    async move {
                        let output = tokio::process::Command::new("az")
                            .args(["repos", "pr", "show"])
                            .args(["--id", &pr_id])
                            .args(["--org", &org])
                            .args(["--output", "json"])
                            .output()
                            .await;

                        if let Ok(o) = output {
                            if o.status.success() {
                                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&o.stdout) {
                                    if let Some(title) = json.get("title").and_then(|t| t.as_str()) {
                                        let key = format!("pr:{pr_id}");
                                        let _ = tx.send((key, title.to_string())).await;
                                    }
                                }
                            }
                        }
                    }
                }).collect();

                // Fetch commit messages in parallel
                let commit_futures: Vec<_> = commits.into_iter().map(|(repo_guid, hash)| {
                    let org = org.clone();
                    let project = project.clone();
                    let tx = tx.clone();
                    async move {
                        let output = tokio::process::Command::new("az")
                            .args(["devops", "invoke"])
                            .args(["--area", "git"])
                            .args(["--resource", "commits"])
                            .args(["--route-parameters", &format!("project={project}"), &format!("repositoryId={repo_guid}"), &format!("commitId={hash}")])
                            .args(["--org", &org])
                            .args(["--output", "json"])
                            .output()
                            .await;

                        if let Ok(o) = output {
                            if o.status.success() {
                                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&o.stdout) {
                                    if let Some(comment) = json.get("comment").and_then(|c| c.as_str()) {
                                        let title = comment.lines().next().unwrap_or(comment);
                                        let key = format!("commit:{hash}");
                                        let _ = tx.send((key, title.to_string())).await;
                                    }
                                }
                            }
                        }
                    }
                }).collect();

                // Run all requests in parallel
                futures::future::join_all(pr_futures).await;
                futures::future::join_all(commit_futures).await;
            });
        }
    }

    /// Start background relation loader
    pub fn start_relations_loader(&mut self) {
        if self.relations_loader_active {
            return; // Already running
        }

        let ids = self.get_ids_needing_relations();
        if ids.is_empty() {
            return;
        }

        let (tx, rx) = mpsc::channel(100);
        self.relations_rx = Some(rx);
        self.relations_loader_active = true;

        // Clone what we need for the background task
        let client_info = self.current_project().map(|p| (p.organization.clone(), p.project.clone()));

        if let Some((org, _project)) = client_info {
            tokio::spawn(async move {
                for id in ids {
                    // Create a fresh client for each request
                    let output = tokio::process::Command::new("az")
                        .args(["boards", "work-item", "show"])
                        .args(["--id", &id.to_string()])
                        .args(["--expand", "relations"])
                        .args(["--org", &org])
                        .args(["--output", "json"])
                        .output()
                        .await;

                    let relations = output.ok().and_then(|o| {
                        if o.status.success() {
                            serde_json::from_slice::<WorkItem>(&o.stdout)
                                .ok()
                                .and_then(|wi| wi.relations)
                        } else {
                            None
                        }
                    });

                    // Send result back (ignore error if receiver dropped)
                    if tx.send((id, relations)).await.is_err() {
                        break; // Receiver dropped, stop loading
                    }

                    // Small delay between requests to avoid overwhelming API
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            });
        }
    }

    #[allow(dead_code)] // Used by events.rs
    pub fn set_loading(&mut self, loading: bool, message: &str) {
        self.loading = loading;
        self.loading_message = message.to_string();
    }

    #[allow(dead_code)] // Used by events.rs
    pub fn tick_spinner(&mut self) {
        self.spinner_frame = (self.spinner_frame + 1) % 10;
    }

    #[allow(dead_code)] // Used by ui::draw_loading
    pub fn spinner_char(&self) -> &'static str {
        const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        SPINNER[self.spinner_frame]
    }

    /// Load data from cache if available
    pub fn load_from_cache(&mut self) -> bool {
        let project_name = match self.current_project() {
            Some(p) => p.name.clone(),
            None => return false,
        };

        if let Some(entry) = cache::load(&project_name) {
            let age = entry.age_seconds();
            let sprint_path = entry.sprint_path.clone();
            self.sprints = entry.sprints;
            // Rebuild hierarchy since children field is not serialized
            self.work_items = AzureCli::build_hierarchy(entry.work_items);
            self.cache_age = Some(age);

            // Restore filters and pinned items
            self.filter_state = entry.filter_state;
            self.filter_assignee = entry.filter_assignee;
            self.pinned_items = entry.pinned_items;

            // Extract users from work items
            self.extract_users_from_work_items();

            // Find the sprint that matches cached sprint_path
            self.selected_sprint_idx = self.sprints.iter()
                .position(|s| s.path == sprint_path)
                .or_else(|| self.sprints.iter().position(|s| s.attributes.time_frame.as_deref() == Some("current")))
                .unwrap_or(0);

            self.sprint_list_state.select(Some(self.selected_sprint_idx));
            self.rebuild_visible_items();
            if !self.visible_items.is_empty() {
                self.work_item_list_state.select(Some(0));
            }
            true
        } else {
            false
        }
    }

    /// Save current data to cache
    pub fn save_to_cache(&self) {
        let project_name = match self.current_project() {
            Some(p) => p.name.clone(),
            None => return,
        };
        let sprint_path = self.selected_sprint()
            .map(|s| s.path.as_str())
            .unwrap_or("");

        // Flatten hierarchical work items back to flat list for serialization
        // (children field is #[serde(skip)] so we need flat list with parent_id)
        let flat_items = Self::flatten_work_items(&self.work_items);

        let entry = CacheEntry::new(
            self.sprints.clone(),
            flat_items,
            self.users.clone(),
            sprint_path,
            self.filter_state.clone(),
            self.filter_assignee.clone(),
            self.pinned_items.clone(),
        );
        let _ = cache::save(&project_name, &entry);
    }

    /// Flatten hierarchical work items back to a flat list
    fn flatten_work_items(items: &[WorkItem]) -> Vec<WorkItem> {
        let mut result = Vec::new();
        fn collect(items: &[WorkItem], result: &mut Vec<WorkItem>) {
            for item in items {
                // Clone item without children (they'll be added separately)
                let mut flat_item = item.clone();
                flat_item.children.clear();
                result.push(flat_item);
                // Recursively collect children
                collect(&item.children, result);
            }
        }
        collect(items, &mut result);
        result
    }

    pub fn current_project(&self) -> Option<&crate::config::ProjectConfig> {
        self.config.projects.get(self.current_project_idx)
    }

    pub fn client(&self) -> Option<AzureCli> {
        self.current_project().map(AzureCli::new)
    }

    // CI/CD data loading (kept for potential direct API use, currently using background loaders)
    #[allow(dead_code)]
    pub async fn load_pipelines(&mut self) -> Result<()> {
        let client = self.client().ok_or_else(|| anyhow::anyhow!("No project configured"))?;
        self.pipelines = client.list_pipelines().await?;

        if !self.pipelines.is_empty() && self.pipeline_list_state.selected().is_none() {
            self.pipeline_list_state.select(Some(0));
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn load_release_definitions(&mut self) -> Result<()> {
        let client = self.client().ok_or_else(|| anyhow::anyhow!("No project configured"))?;
        self.releases = client.list_release_definitions().await?;

        if !self.releases.is_empty() && self.release_list_state.selected().is_none() {
            self.release_list_state.select(Some(0));
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn load_pipeline_runs(&mut self, pipeline_id: i32) -> Result<()> {
        let client = self.client().ok_or_else(|| anyhow::anyhow!("No project configured"))?;
        self.pipeline_runs = client.list_pipeline_runs(pipeline_id).await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn load_releases(&mut self, definition_id: Option<i32>) -> Result<()> {
        let client = self.client().ok_or_else(|| anyhow::anyhow!("No project configured"))?;
        self.release_list = client.list_releases(definition_id).await?;
        Ok(())
    }

    /// Load CI/CD data from cache if available and valid
    pub fn load_cicd_from_cache(&mut self) -> bool {
        let project_name = match self.current_project() {
            Some(p) => p.name.clone(),
            None => return false,
        };

        if let Some(entry) = cache::load_cicd(&project_name) {
            let age = entry.age_seconds();
            self.pipelines = entry.pipelines;
            self.releases = entry.release_definitions;
            self.pinned_pipelines = entry.pinned_pipelines;
            self.pinned_releases = entry.pinned_releases;
            self.cicd_cache_age = Some(age);

            // Select first item in sorted order (pinned first, then alphabetical)
            if !self.pipelines.is_empty() {
                let sorted = self.sorted_pipeline_indices();
                if let Some(&first_idx) = sorted.first() {
                    self.selected_pipeline_idx = first_idx;
                    self.pipeline_list_state.select(Some(0));
                }
            }
            if !self.releases.is_empty() {
                let sorted = self.sorted_release_indices();
                if let Some(&first_idx) = sorted.first() {
                    self.selected_release_idx = first_idx;
                    self.release_list_state.select(Some(0));
                }
            }
            true
        } else {
            false
        }
    }

    /// Save CI/CD data to cache
    pub fn save_cicd_to_cache(&self) {
        let project_name = match self.current_project() {
            Some(p) => p.name.clone(),
            None => return,
        };

        let entry = CICDCacheEntry::new(
            self.pipelines.clone(),
            self.releases.clone(),
            self.pinned_pipelines.clone(),
            self.pinned_releases.clone(),
        );
        let _ = cache::save_cicd(&project_name, &entry);
    }

    /// Start background CI/CD data loader (checks cache first)
    pub fn start_cicd_loader(&mut self) {
        if self.cicd_loading {
            return;
        }

        // Try cache first
        if self.load_cicd_from_cache() {
            return; // Cache hit, no need to load from API
        }

        // Extract project info first to avoid borrow issues
        let (org, proj, project_name) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone(), p.name.clone()),
            None => return,
        };

        // Clone pinned sets to preserve them when saving to cache
        let pinned_pipelines = self.pinned_pipelines.clone();
        let pinned_releases = self.pinned_releases.clone();

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);
        self.cicd_loading = true;

        tokio::spawn(async move {
            let mut pipelines_result: Option<Vec<Pipeline>> = None;
            let mut releases_result: Option<Vec<ReleaseDefinition>> = None;

            // Load pipelines
            let output = tokio::process::Command::new("az")
                .args(["pipelines", "list"])
                .args(["--org", &org])
                .args(["--project", &proj])
                .args(["--output", "json"])
                .output()
                .await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(pipelines) = serde_json::from_slice::<Vec<Pipeline>>(&o.stdout) {
                        pipelines_result = Some(pipelines.clone());
                        let _ = tx.send(CICDLoadResult::Pipelines(pipelines)).await;
                    }
                }
            }

            // Load release definitions
            let output = tokio::process::Command::new("az")
                .args(["pipelines", "release", "definition", "list"])
                .args(["--org", &org])
                .args(["--project", &proj])
                .args(["--output", "json"])
                .output()
                .await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(defs) = serde_json::from_slice::<Vec<ReleaseDefinition>>(&o.stdout) {
                        let filtered: Vec<_> = defs.into_iter()
                            .filter(|d| !d.is_deleted && !d.is_disabled)
                            .collect();
                        releases_result = Some(filtered.clone());
                        let _ = tx.send(CICDLoadResult::ReleaseDefinitions(filtered)).await;
                    }
                }
            }

            // Save to cache after loading (preserve pinned items)
            if let (Some(pipelines), Some(releases)) = (pipelines_result, releases_result) {
                let entry = CICDCacheEntry::new(pipelines, releases, pinned_pipelines, pinned_releases);
                let _ = cache::save_cicd(&project_name, &entry);
            }
        });
    }

    /// Poll for CI/CD data from background loader
    pub fn poll_cicd(&mut self) {
        let mut results = Vec::new();
        let mut channel_closed = false;

        if let Some(rx) = &mut self.cicd_rx {
            loop {
                match rx.try_recv() {
                    Ok(item) => results.push(item),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        channel_closed = true;
                        break;
                    }
                }
            }
        }

        for result in results {
            match result {
                CICDLoadResult::Pipelines(pipelines) => {
                    self.pipelines = pipelines;
                    self.cicd_cache_age = Some(0); // Fresh data
                    if !self.pipelines.is_empty() {
                        // Select first item in sorted order (pinned first, then alphabetical)
                        let sorted = self.sorted_pipeline_indices();
                        if let Some(&first_idx) = sorted.first() {
                            self.selected_pipeline_idx = first_idx;
                            self.pipeline_list_state.select(Some(0)); // Display position 0
                        }
                    }
                }
                CICDLoadResult::ReleaseDefinitions(defs) => {
                    self.releases = defs;
                    if !self.releases.is_empty() {
                        // Select first item in sorted order (pinned first, then alphabetical)
                        let sorted = self.sorted_release_indices();
                        if let Some(&first_idx) = sorted.first() {
                            self.selected_release_idx = first_idx;
                            self.release_list_state.select(Some(0)); // Display position 0
                        }
                    }
                }
                CICDLoadResult::PipelineRuns(runs) => {
                    self.pipeline_runs = runs;
                    if !self.pipeline_runs.is_empty() {
                        self.selected_pipeline_run_idx = 0;
                    }
                }
                CICDLoadResult::Releases(releases) => {
                    self.release_list = releases;

                    // Check if we need to select a specific release (e.g., after creating one)
                    if let Some(target_id) = self.pending_select_release_id.take() {
                        if let Some(idx) = self.release_list.iter().position(|r| r.id == target_id) {
                            self.selected_release_item_idx = idx;
                            // Auto-drill into stages for newly created release
                            self.release_drill_down = ReleaseDrillDown::Stages;
                            self.start_release_stages_loader(target_id);
                        } else if !self.release_list.is_empty() {
                            self.selected_release_item_idx = 0;
                        }
                    } else if !self.release_list.is_empty() {
                        self.selected_release_item_idx = 0;
                    }
                }
                CICDLoadResult::ReleaseDetail(idx, release) => {
                    // Update the release at idx with full details including environments
                    if let Some(r) = self.release_list.get_mut(idx) {
                        r.environments = release.environments;
                    }
                }
                CICDLoadResult::ReleaseStages(stages) => {
                    let was_empty = self.release_stages.is_empty();
                    self.release_stages = stages;
                    // Only reset selection if this is first load or current selection is out of bounds
                    if !self.release_stages.is_empty() {
                        if was_empty || self.selected_release_stage_idx >= self.release_stages.len() {
                            self.selected_release_stage_idx = 0;
                        }
                    }
                }
                CICDLoadResult::ReleaseTasks(tasks) => {
                    self.release_tasks = tasks;
                    if !self.release_tasks.is_empty() {
                        self.selected_release_task_idx = 0;
                    }
                }
                CICDLoadResult::ReleaseTaskLog(lines) => {
                    self.release_task_logs = lines;
                    self.log_scroll = 0;
                }
                CICDLoadResult::Timeline(records) => {
                    self.timeline_records = records;
                    self.selected_task_idx = 0;
                }
                CICDLoadResult::TimelineDelta { build_id, records, change_id } => {
                    // Update change_id for next delta poll
                    if self.live_preview_build_id == Some(build_id) {
                        self.live_preview_change_id = change_id;
                    }

                    // Update timeline records
                    if !records.is_empty() {
                        self.timeline_records = records;

                        // Check if build completed - stop live preview
                        let all_complete = self.timeline_records.iter()
                            .filter(|r| r.record_type.as_deref() == Some("Stage"))
                            .all(|r| r.state.as_deref() == Some("completed"));

                        if all_complete {
                            self.stop_live_preview();
                        }
                    }
                }
                CICDLoadResult::BuildLog(lines) => {
                    self.build_log_lines = lines;
                    self.log_scroll = 0;
                }
                CICDLoadResult::PendingApprovals(approvals) => {
                    self.pending_approvals_count = approvals.len();
                    self.pending_approvals = approvals;
                    self.approvals_loading = false;
                }
                CICDLoadResult::ReleaseDefinitionDetail(detail) => {
                    // Update dialog with stage information
                    if let Some(dialog) = &mut self.release_trigger_dialog {
                        dialog.stages = detail.environments.iter()
                            .map(|env| StageSelection {
                                id: env.id,
                                name: env.name.clone().unwrap_or_else(|| format!("Stage {}", env.id)),
                                enabled: true,
                            })
                            .collect();
                        dialog.loading = false;
                    }
                }
                CICDLoadResult::ReleaseCreated(release) => {
                    let release_name = release.name.clone();
                    let release_id = release.id;
                    let def_id = release.release_definition.as_ref().map(|d| d.id).unwrap_or(0);

                    self.set_status(format!("Release {} created", &release_name));

                    // Close dialog
                    self.release_trigger_dialog = None;
                    self.input_mode = InputMode::Normal;

                    // Store the new release ID to select after reload
                    self.pending_select_release_id = Some(release_id);

                    // Navigate to Items level and reload the full releases list from server
                    self.current_release_def_id = Some(def_id);
                    self.release_drill_down = crate::app::ReleaseDrillDown::Items;
                    self.start_releases_loader(def_id);

                    // Enable auto-refresh to see updates
                    self.start_release_auto_refresh(release_id);
                }
                CICDLoadResult::ApprovalUpdated { approval_id: _, release_id, status } => {
                    self.set_status(format!("Approval {} - refreshing...", status));
                    // Reset loading state so we can start a new loader
                    self.cicd_loading = false;
                    // Always refresh release stages after approval action
                    self.start_release_stages_loader(release_id);
                }
                CICDLoadResult::PipelineRunCanceled(run_id) => {
                    self.set_status(format!("Pipeline run #{} canceled", run_id));
                    // Reset loading state so we can start a new loader
                    self.cicd_loading = false;
                    // Refresh runs list
                    if let Some(pipeline_id) = self.current_pipeline_id {
                        self.start_pipeline_runs_loader(pipeline_id);
                    }
                }
                CICDLoadResult::PipelineRunRetriggered(run) => {
                    let build_num = run.build_number.as_deref().unwrap_or("?");
                    self.set_status(format!("New run #{} started", build_num));
                    // Reset loading state so we can start a new loader
                    self.cicd_loading = false;
                    // Refresh runs list
                    if let Some(pipeline_id) = self.current_pipeline_id {
                        self.start_pipeline_runs_loader(pipeline_id);
                    }
                }
                CICDLoadResult::ReleaseCanceled(release_id) => {
                    self.set_status(format!("Release {} abandoned", release_id));
                    // Reset loading state so we can start a new loader
                    self.cicd_loading = false;
                    // Refresh releases list
                    if let Some(def_id) = self.current_release_def_id {
                        self.start_releases_loader(def_id);
                    }
                }
                CICDLoadResult::ReleaseEnvironmentCanceled { release_id, environment_name } => {
                    self.set_status(format!("Stage '{}' canceled", environment_name));
                    // Reset loading state so we can start a new loader
                    self.cicd_loading = false;
                    // Refresh stages
                    self.start_release_stages_loader(release_id);
                }
                CICDLoadResult::ReleaseEnvironmentRedeployed { release_id, environment_name } => {
                    self.set_status(format!("Stage '{}' redeploying", environment_name));
                    // Reset loading state so we can start a new loader
                    self.cicd_loading = false;
                    // Refresh stages
                    self.start_release_stages_loader(release_id);
                }
                CICDLoadResult::Error(msg) => {
                    self.set_error(msg);
                }
            }
        }

        if channel_closed {
            self.cicd_loading = false;
            self.cicd_rx = None;
        }
    }

    /// Force refresh CI/CD data (bypasses cache)
    pub fn force_refresh_cicd(&mut self) {
        if self.cicd_loading {
            return;
        }

        // Extract project info first to avoid borrow issues
        let (org, proj, project_name) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone(), p.name.clone()),
            None => return,
        };

        // Clone pinned sets to preserve them when saving to cache
        let pinned_pipelines = self.pinned_pipelines.clone();
        let pinned_releases = self.pinned_releases.clone();

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);
        self.cicd_loading = true;

        tokio::spawn(async move {
            let mut pipelines_result: Option<Vec<Pipeline>> = None;
            let mut releases_result: Option<Vec<ReleaseDefinition>> = None;

            // Load pipelines
            let output = tokio::process::Command::new("az")
                .args(["pipelines", "list"])
                .args(["--org", &org])
                .args(["--project", &proj])
                .args(["--output", "json"])
                .output()
                .await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(pipelines) = serde_json::from_slice::<Vec<Pipeline>>(&o.stdout) {
                        pipelines_result = Some(pipelines.clone());
                        let _ = tx.send(CICDLoadResult::Pipelines(pipelines)).await;
                    }
                }
            }

            // Load release definitions
            let output = tokio::process::Command::new("az")
                .args(["pipelines", "release", "definition", "list"])
                .args(["--org", &org])
                .args(["--project", &proj])
                .args(["--output", "json"])
                .output()
                .await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(defs) = serde_json::from_slice::<Vec<ReleaseDefinition>>(&o.stdout) {
                        let filtered: Vec<_> = defs.into_iter()
                            .filter(|d| !d.is_deleted && !d.is_disabled)
                            .collect();
                        releases_result = Some(filtered.clone());
                        let _ = tx.send(CICDLoadResult::ReleaseDefinitions(filtered)).await;
                    }
                }
            }

            // Save to cache after loading (preserve pinned items)
            if let (Some(pipelines), Some(releases)) = (pipelines_result, releases_result) {
                let entry = CICDCacheEntry::new(pipelines, releases, pinned_pipelines, pinned_releases);
                let _ = cache::save_cicd(&project_name, &entry);
            }
        });
    }

    /// Start background loader for pipeline runs (limited to 10 by default)
    pub fn start_pipeline_runs_loader(&mut self, pipeline_id: i32) {
        self.start_pipeline_runs_loader_impl(pipeline_id, Some(10), false);
    }

    /// Start background loader for all pipeline runs (no limit)
    pub fn start_pipeline_runs_loader_all(&mut self, pipeline_id: i32) {
        self.start_pipeline_runs_loader_impl(pipeline_id, None, false);
    }

    /// Force refresh pipeline runs (bypass cache)
    pub fn force_refresh_pipeline_runs(&mut self, pipeline_id: i32) {
        self.start_pipeline_runs_loader_impl(pipeline_id, None, true);
    }

    /// Start background loader for pipeline runs with optional limit
    fn start_pipeline_runs_loader_impl(&mut self, pipeline_id: i32, limit: Option<u32>, force: bool) {
        let (org, proj, proj_name) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone(), p.name.clone()),
            None => return,
        };

        // Track current pipeline for "load more"
        self.current_pipeline_id = Some(pipeline_id);
        self.pipeline_runs_limited = limit.is_some();

        // Stale-while-revalidate: use cache immediately, refresh in background if stale
        let needs_fetch = if !force && limit.is_some() {
            if let Some((cached, needs_refresh)) = cache::load_pipeline_runs(&proj_name, pipeline_id) {
                self.pipeline_runs = cached.runs;
                self.cicd_loading = false;
                needs_refresh // Only fetch if stale
            } else {
                true // No cache, must fetch
            }
        } else {
            true // Force refresh or "load all" always fetches
        };

        if !needs_fetch {
            return;
        }

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);
        self.cicd_loading = true;

        tokio::spawn(async move {
            let mut cmd = tokio::process::Command::new("az");
            cmd.args(["pipelines", "runs", "list"])
                .args(["--org", &org])
                .args(["--project", &proj])
                .args(["--pipeline-ids", &pipeline_id.to_string()]);

            if let Some(top) = limit {
                cmd.args(["--top", &top.to_string()]);
            }

            cmd.args(["--output", "json"]);

            let output = cmd.output().await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(runs) = serde_json::from_slice::<Vec<PipelineRun>>(&o.stdout) {
                        // Save to cache
                        let cache_entry = cache::PipelineRunsCacheEntry::new(pipeline_id, runs.clone());
                        let _ = cache::save_pipeline_runs(&proj_name, &cache_entry);
                        let _ = tx.send(CICDLoadResult::PipelineRuns(runs)).await;
                    }
                }
            }
        });
    }

    /// Start background loader for releases
    pub fn start_releases_loader(&mut self, definition_id: i32) {
        self.start_releases_loader_impl(definition_id, false);
    }

    /// Force refresh releases (bypass cache)
    pub fn force_refresh_releases(&mut self, definition_id: i32) {
        self.start_releases_loader_impl(definition_id, true);
    }

    fn start_releases_loader_impl(&mut self, definition_id: i32, force: bool) {
        let (org, proj, proj_name) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone(), p.name.clone()),
            None => return,
        };

        // Store current definition for refresh
        self.current_release_def_id = Some(definition_id);

        // Stale-while-revalidate: use cache immediately, refresh in background if stale
        let needs_fetch = if !force {
            if let Some((cached, needs_refresh)) = cache::load_releases(&proj_name, definition_id) {
                self.release_list = cached.releases;
                self.cicd_loading = false;
                needs_refresh // Only fetch if stale
            } else {
                true // No cache, must fetch
            }
        } else {
            true // Force refresh always fetches
        };

        if !needs_fetch {
            return;
        }

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);
        self.cicd_loading = true;

        tokio::spawn(async move {
            let output = tokio::process::Command::new("az")
                .args(["pipelines", "release", "list"])
                .args(["--org", &org])
                .args(["--project", &proj])
                .args(["--definition-id", &definition_id.to_string()])
                .args(["--output", "json"])
                .output()
                .await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(releases) = serde_json::from_slice::<Vec<Release>>(&o.stdout) {
                        // Save to cache
                        let cache_entry = cache::ReleasesCacheEntry::new(definition_id, releases.clone());
                        let _ = cache::save_releases(&proj_name, &cache_entry);
                        let _ = tx.send(CICDLoadResult::Releases(releases)).await;
                    }
                }
            }
        });
    }

    /// Start background loader for release detail (to get environments/stages)
    pub fn start_release_detail_loader(&mut self, release_idx: usize, release_id: i32) {
        let (org, proj, _proj_name) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone(), p.name.clone()),
            None => return,
        };

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);
        // Don't set cicd_loading - this is a background detail fetch

        tokio::spawn(async move {
            let output = tokio::process::Command::new("az")
                .args(["pipelines", "release", "show"])
                .args(["--org", &org])
                .args(["--project", &proj])
                .args(["--id", &release_id.to_string()])
                .args(["--output", "json"])
                .output()
                .await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(release) = serde_json::from_slice::<Release>(&o.stdout) {
                        let _ = tx.send(CICDLoadResult::ReleaseDetail(release_idx, release)).await;
                    }
                }
            }
        });
    }

    /// Start background loader for release stages (environments with full details)
    pub fn start_release_stages_loader(&mut self, release_id: i32) {
        // Skip if already loading - prevents race condition where new loader
        // overwrites channel before old result arrives
        if self.cicd_loading && self.cicd_rx.is_some() {
            return;
        }

        let (org, proj, _proj_name) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone(), p.name.clone()),
            None => return,
        };

        // Don't clear - keep previous data visible until new data arrives
        // self.release_stages.clear();
        // Only reset selection index when stages are empty (first load)
        if self.release_stages.is_empty() {
            self.selected_release_stage_idx = 0;
        }

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);
        self.cicd_loading = true;

        tokio::spawn(async move {
            let output = tokio::process::Command::new("az")
                .args(["pipelines", "release", "show"])
                .args(["--org", &org])
                .args(["--project", &proj])
                .args(["--id", &release_id.to_string()])
                .args(["--output", "json"])
                .output()
                .await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(release) = serde_json::from_slice::<Release>(&o.stdout) {
                        if let Some(envs) = release.environments {
                            let _ = tx.send(CICDLoadResult::ReleaseStages(envs)).await;
                        }
                    }
                }
            }
        });
    }

    /// Extract tasks from the selected release stage
    pub fn load_release_tasks_from_stage(&mut self, stage_idx: usize) {
        self.release_tasks.clear();
        self.selected_release_task_idx = 0;

        if let Some(stage) = self.release_stages.get(stage_idx) {
            // Get tasks from the first deploy step's first phase's first job
            if let Some(deploy_step) = stage.deploy_steps.first() {
                if let Some(phase) = deploy_step.release_deploy_phases.first() {
                    if let Some(job) = phase.deployment_jobs.first() {
                        self.release_tasks = job.tasks.clone();
                    }
                }
            }
        }
    }

    /// Start background loader for release task log
    pub fn start_release_task_log_loader(&mut self, log_url: &str) {
        let log_url = log_url.to_string();
        self.release_task_logs.clear();
        self.log_scroll = 0;

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);
        self.cicd_loading = true;

        tokio::spawn(async move {
            // Get access token for Azure DevOps
            let token_output = tokio::process::Command::new("az")
                .args(["account", "get-access-token"])
                .args(["--resource", "499b84ac-1321-427f-aa17-267ca6975798"])
                .args(["--query", "accessToken"])
                .args(["-o", "tsv"])
                .output()
                .await;

            if let Ok(token_out) = token_output {
                if token_out.status.success() {
                    let token = String::from_utf8_lossy(&token_out.stdout).trim().to_string();

                    // Fetch log using curl
                    let output = tokio::process::Command::new("curl")
                        .args(["-s", &log_url])
                        .args(["-H", &format!("Authorization: Bearer {token}")])
                        .output()
                        .await;

                    if let Ok(o) = output {
                        if o.status.success() {
                            let log_content = String::from_utf8_lossy(&o.stdout);
                            let lines: Vec<String> = log_content.lines().map(|l| l.to_string()).collect();
                            let _ = tx.send(CICDLoadResult::ReleaseTaskLog(lines)).await;
                        }
                    }
                }
            }
        });
    }

    /// Start background loader for build timeline
    pub fn start_timeline_loader(&mut self, build_id: i32) {
        self.start_timeline_loader_impl(build_id, false);
    }

    /// Force refresh timeline (bypass cache)
    pub fn force_refresh_timeline(&mut self, build_id: i32) {
        self.start_timeline_loader_impl(build_id, true);
    }

    fn start_timeline_loader_impl(&mut self, build_id: i32, force: bool) {
        let (org, proj, proj_name) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone(), p.name.clone()),
            None => return,
        };

        self.selected_run_id = Some(build_id);

        // Stale-while-revalidate: use cache immediately, refresh in background if stale
        let needs_fetch = if !force {
            if let Some((cached, needs_refresh)) = cache::load_timeline(&proj_name, build_id) {
                self.timeline_records = cached.records;
                self.cicd_loading = false;
                needs_refresh // Only fetch if stale
            } else {
                true // No cache, must fetch
            }
        } else {
            true // Force refresh always fetches
        };

        if !needs_fetch {
            return;
        }

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);
        self.cicd_loading = true;

        tokio::spawn(async move {
            let output = tokio::process::Command::new("az")
                .args(["devops", "invoke"])
                .args(["--area", "build"])
                .args(["--resource", "timeline"])
                .args(["--route-parameters", &format!("project={proj}"), &format!("buildId={build_id}")])
                .args(["--org", &org])
                .args(["--output", "json"])
                .output()
                .await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(resp) = serde_json::from_slice::<crate::azure::TimelineResponse>(&o.stdout) {
                        // Save to cache
                        let cache_entry = cache::TimelineCacheEntry::new(build_id, resp.records.clone());
                        let _ = cache::save_timeline(&proj_name, &cache_entry);
                        let _ = tx.send(CICDLoadResult::Timeline(resp.records)).await;
                    }
                }
            }
        });
    }

    /// Start background loader for build log
    pub fn start_log_loader(&mut self, build_id: i32, log_id: i32) {
        self.start_log_loader_impl(build_id, log_id, false);
    }

    /// Force refresh build log (bypass cache)
    pub fn force_refresh_log(&mut self, build_id: i32, log_id: i32) {
        self.start_log_loader_impl(build_id, log_id, true);
    }

    fn start_log_loader_impl(&mut self, build_id: i32, log_id: i32, force: bool) {
        let (org, proj, proj_name) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone(), p.name.clone()),
            None => return,
        };

        // Store for refresh
        self.current_log_id = Some(log_id);

        // Stale-while-revalidate: use cache immediately, refresh in background if stale
        let needs_fetch = if !force {
            if let Some((cached, needs_refresh)) = cache::load_build_log(&proj_name, build_id, log_id) {
                self.build_log_lines = cached.lines;
                self.cicd_loading = false;
                needs_refresh // Only fetch if stale
            } else {
                true // No cache, must fetch
            }
        } else {
            true // Force refresh always fetches
        };

        if !needs_fetch {
            return;
        }

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);
        self.cicd_loading = true;

        tokio::spawn(async move {
            let output = tokio::process::Command::new("az")
                .args(["devops", "invoke"])
                .args(["--area", "build"])
                .args(["--resource", "logs"])
                .args(["--route-parameters", &format!("project={proj}"), &format!("buildId={build_id}"), &format!("logId={log_id}")])
                .args(["--org", &org])
                .args(["--output", "json"])
                .output()
                .await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(resp) = serde_json::from_slice::<crate::azure::BuildLogResponse>(&o.stdout) {
                        // Save to cache
                        let cache_entry = cache::BuildLogCacheEntry::new(build_id, log_id, resp.value.clone());
                        let _ = cache::save_build_log(&proj_name, &cache_entry);
                        let _ = tx.send(CICDLoadResult::BuildLog(resp.value)).await;
                    }
                }
            }
        });
    }

    /// Start watching a build for live updates
    pub fn start_live_preview(&mut self, build_id: i32) {
        self.live_preview_enabled = true;
        self.live_preview_build_id = Some(build_id);
        self.live_preview_change_id = None; // Reset to get full timeline first
        self.live_preview_last_poll = std::time::Instant::now();
    }

    /// Stop live preview
    pub fn stop_live_preview(&mut self) {
        self.live_preview_enabled = false;
        self.live_preview_build_id = None;
        self.live_preview_change_id = None;
    }

    /// Poll for timeline updates (call from event loop)
    pub fn poll_live_preview(&mut self) {
        if !self.live_preview_enabled {
            return;
        }

        let Some(build_id) = self.live_preview_build_id else {
            return;
        };

        // Only poll every 1 second
        if self.live_preview_last_poll.elapsed() < std::time::Duration::from_secs(1) {
            return;
        }

        self.live_preview_last_poll = std::time::Instant::now();

        // Get current project info
        let (org, proj) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone()),
            None => return,
        };

        // Get or create channel
        let tx = if let Some(tx) = &self.cicd_tx {
            tx.clone()
        } else {
            let (tx, rx) = mpsc::channel(10);
            self.cicd_rx = Some(rx);
            self.cicd_tx = Some(tx.clone());
            tx
        };

        let change_id = self.live_preview_change_id;

        tokio::spawn(async move {
            let mut cmd = tokio::process::Command::new("az");
            cmd.args(["devops", "invoke"])
                .args(["--area", "build"])
                .args(["--resource", "timeline"])
                .args(["--route-parameters",
                       &format!("project={}", proj),
                       &format!("buildId={}", build_id)]);

            // Add changeId for delta polling - returns empty if no changes
            if let Some(change_id) = change_id {
                cmd.args(["--query-parameters", &format!("changeId={}", change_id)]);
            }

            cmd.args(["--org", &org])
                .args(["--output", "json"]);

            if let Ok(output) = cmd.output().await {
                if output.status.success() {
                    if let Ok(response) = serde_json::from_slice::<crate::azure::TimelineResponse>(&output.stdout) {
                        // If no records returned and we had a changeId, nothing changed
                        if !response.records.is_empty() || change_id.is_none() {
                            let _ = tx.send(CICDLoadResult::TimelineDelta {
                                build_id,
                                records: response.records,
                                change_id: response.change_id,
                            }).await;
                        }
                    }
                }
            }
        });
    }

    /// Poll for release stage updates (call from event loop)
    pub fn poll_release_refresh(&mut self) {
        if !self.release_auto_refresh {
            return;
        }

        let Some(release_id) = self.release_auto_refresh_id else {
            return;
        };

        // Only poll every 1 second
        if self.release_last_refresh.elapsed() < std::time::Duration::from_secs(1) {
            return;
        }

        self.release_last_refresh = std::time::Instant::now();

        // Refresh stages
        self.start_release_stages_loader(release_id);
    }

    /// Start auto-refresh for a release
    pub fn start_release_auto_refresh(&mut self, release_id: i32) {
        self.release_auto_refresh = true;
        self.release_auto_refresh_id = Some(release_id);
        self.release_last_refresh = std::time::Instant::now();
    }

    /// Stop release auto-refresh
    pub fn stop_release_auto_refresh(&mut self) {
        self.release_auto_refresh = false;
        self.release_auto_refresh_id = None;
    }

    /// Start background loader for pending approvals
    pub fn start_approvals_loader(&mut self) {
        if self.approvals_loading {
            return;
        }
        self.approvals_loading = true;

        let (org, proj) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone()),
            None => return,
        };

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);

        tokio::spawn(async move {
            let output = tokio::process::Command::new("az")
                .args(["devops", "invoke"])
                .args(["--area", "release"])
                .args(["--resource", "approvals"])
                .args(["--route-parameters", &format!("project={proj}")])
                .args(["--query-parameters", "statusFilter=pending"])
                .args(["--org", &org])
                .args(["--output", "json"])
                .output()
                .await;

            if let Ok(o) = output {
                if o.status.success() {
                    if let Ok(resp) = serde_json::from_slice::<crate::azure::ApprovalsResponse>(&o.stdout) {
                        let _ = tx.send(CICDLoadResult::PendingApprovals(resp.value)).await;
                    }
                }
            }
        });
    }

    /// Open release trigger dialog
    pub fn open_release_trigger_dialog(&mut self, definition_id: i32, definition_name: String) {
        // Set dialog with loading state
        self.release_trigger_dialog = Some(ReleaseTriggerDialog::new(definition_id, definition_name.clone()));
        self.input_mode = InputMode::ReleaseTriggerDialog;

        let (org, proj) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone()),
            None => return,
        };

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);

        tokio::spawn(async move {
            let output = tokio::process::Command::new("az")
                .args(["devops", "invoke"])
                .args(["--area", "release"])
                .args(["--resource", "definitions"])
                .args(["--route-parameters", &format!("project={proj}"), &format!("definitionId={definition_id}")])
                .args(["--org", &org])
                .args(["--output", "json"])
                .output()
                .await;

            match output {
                Ok(o) => {
                    if o.status.success() {
                        match serde_json::from_slice::<crate::azure::ReleaseDefinitionDetail>(&o.stdout) {
                            Ok(detail) => {
                                let _ = tx.send(CICDLoadResult::ReleaseDefinitionDetail(detail)).await;
                            }
                            Err(e) => {
                                // Try parsing as wrapped response (some API versions wrap in "value")
                                #[derive(serde::Deserialize)]
                                struct Wrapper { environments: Vec<crate::azure::ReleaseDefinitionEnvironment> }
                                if let Ok(w) = serde_json::from_slice::<Wrapper>(&o.stdout) {
                                    let detail = crate::azure::ReleaseDefinitionDetail {
                                        id: definition_id,
                                        name: Some(definition_name),
                                        environments: w.environments,
                                        artifacts: vec![],
                                    };
                                    let _ = tx.send(CICDLoadResult::ReleaseDefinitionDetail(detail)).await;
                                } else {
                                    eprintln!("Failed to parse release definition: {e}");
                                    eprintln!("Response: {}", String::from_utf8_lossy(&o.stdout));
                                }
                            }
                        }
                    } else {
                        eprintln!("API error: {}", String::from_utf8_lossy(&o.stderr));
                    }
                }
                Err(e) => eprintln!("Command failed: {e}"),
            }
        });
    }

    /// Get tasks from timeline (filtered to type=Task only, sorted by order)
    pub fn get_timeline_tasks(&self) -> Vec<&TimelineRecord> {
        let mut tasks: Vec<_> = self.timeline_records.iter()
            .filter(|r| r.record_type.as_deref() == Some("Task"))
            .collect();
        tasks.sort_by_key(|r| r.order.unwrap_or(999));
        tasks
    }

    /// Trigger a new release
    pub fn trigger_release(&mut self, definition_id: i32, description: Option<String>) {
        let (org, proj) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone()),
            None => {
                self.set_error("No project configured");
                return;
            }
        };

        self.set_status("Creating release...");

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);

        tokio::spawn(async move {
            let mut cmd = tokio::process::Command::new("az");
            cmd.args(["pipelines", "release", "create"])
                .args(["--definition-id", &definition_id.to_string()])
                .args(["--org", &org])
                .args(["--project", &proj])
                .args(["--output", "json"]);

            if let Some(desc) = &description {
                cmd.args(["--description", desc]);
            }

            let output = cmd.output().await;

            match output {
                Ok(o) => {
                    if o.status.success() {
                        match serde_json::from_slice::<crate::azure::Release>(&o.stdout) {
                            Ok(release) => {
                                let _ = tx.send(CICDLoadResult::ReleaseCreated(release)).await;
                            }
                            Err(e) => {
                                // Send error as status
                                let _ = tx.send(CICDLoadResult::Error(format!("Parse error: {e}"))).await;
                            }
                        }
                    } else {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        let _ = tx.send(CICDLoadResult::Error(format!("Release failed: {}", stderr.lines().next().unwrap_or(&stderr)))).await;
                    }
                }
                Err(e) => {
                    let _ = tx.send(CICDLoadResult::Error(format!("Command failed: {e}"))).await;
                }
            }
        });
    }

    /// Approve a release stage by finding its pending approval
    pub fn approve_stage(&mut self, env_id: i32, stage_name: &str) {
        let (org, proj) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone()),
            None => {
                self.set_error("No project configured");
                return;
            }
        };

        self.set_status(format!("Looking for approval for {}...", stage_name));

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);
        let stage_name = stage_name.to_string();

        tokio::spawn(async move {
            // Step 1: Get pending approvals
            let output = tokio::process::Command::new("az")
                .args(["devops", "invoke"])
                .args(["--area", "release"])
                .args(["--resource", "approvals"])
                .args(["--route-parameters", &format!("project={proj}")])
                .args(["--query-parameters", "statusFilter=pending"])
                .args(["--org", &org])
                .args(["--output", "json"])
                .output()
                .await;

            let approvals_response = match output {
                Ok(o) if o.status.success() => {
                    match serde_json::from_slice::<crate::azure::ApprovalsResponse>(&o.stdout) {
                        Ok(resp) => resp,
                        Err(e) => {
                            let _ = tx.send(CICDLoadResult::Error(format!("Parse error: {e}"))).await;
                            return;
                        }
                    }
                }
                Ok(o) => {
                    let _ = tx.send(CICDLoadResult::Error(format!("API error: {}", String::from_utf8_lossy(&o.stderr)))).await;
                    return;
                }
                Err(e) => {
                    let _ = tx.send(CICDLoadResult::Error(format!("Command failed: {e}"))).await;
                    return;
                }
            };

            // Step 2: Find approval for this environment
            let approval = approvals_response.value.iter()
                .find(|a| a.release_environment.as_ref().map(|e| e.id) == Some(env_id));

            let approval_id = match approval {
                Some(a) => a.id,
                None => {
                    let _ = tx.send(CICDLoadResult::Error(format!("No pending approval for {}", stage_name))).await;
                    return;
                }
            };

            // Step 3: Approve it
            let body = serde_json::json!([{
                "id": approval_id,
                "status": "approved",
                "comments": "Approved via lazyops"
            }]);
            let body_str = body.to_string();

            let temp_path = std::env::temp_dir().join(format!("approval_{}.json", approval_id));
            if let Err(e) = tokio::fs::write(&temp_path, &body_str).await {
                let _ = tx.send(CICDLoadResult::Error(format!("Failed to write temp file: {e}"))).await;
                return;
            }

            let approve_output = tokio::process::Command::new("az")
                .args(["devops", "invoke"])
                .args(["--area", "release"])
                .args(["--resource", "approvals"])
                .args(["--route-parameters", &format!("project={proj}")])
                .args(["--http-method", "PATCH"])
                .args(["--in-file", temp_path.to_str().unwrap()])
                .args(["--org", &org])
                .args(["--output", "json"])
                .output()
                .await;

            // Clean up temp file
            let _ = tokio::fs::remove_file(&temp_path).await;

            match approve_output {
                Ok(o) if o.status.success() => {
                    // Get release_id from the approval response if possible
                    let release_id = approval.and_then(|a| a.release.as_ref().map(|r| r.id)).unwrap_or(0);
                    let _ = tx.send(CICDLoadResult::ApprovalUpdated {
                        approval_id,
                        release_id,
                        status: "approved".to_string(),
                    }).await;
                }
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let _ = tx.send(CICDLoadResult::Error(format!("Approval failed: {}", stderr.lines().next().unwrap_or(&stderr)))).await;
                }
                Err(e) => {
                    let _ = tx.send(CICDLoadResult::Error(format!("Command failed: {e}"))).await;
                }
            }
        });
    }

    /// Approve all pending stages for the current release
    pub fn approve_all_pending_stages(&mut self) {
        // Collect all stages with pending approvals
        let pending_env_ids: Vec<i32> = self.release_stages.iter()
            .filter(|stage| {
                stage.pre_deploy_approvals.iter()
                    .any(|a| a.status.as_deref() == Some("pending"))
            })
            .map(|stage| stage.id)
            .collect();

        if pending_env_ids.is_empty() {
            self.set_status("No pending approvals to approve");
            return;
        }

        let (org, proj) = match self.current_project() {
            Some(p) => (p.organization.clone(), p.project.clone()),
            None => {
                self.set_error("No project configured");
                return;
            }
        };

        // Get release_id for refresh after approval
        let release_id = self.release_list.get(self.selected_release_item_idx)
            .map(|r| r.id)
            .unwrap_or(0);

        let count = pending_env_ids.len();
        self.set_status(format!("Approving {} stage(s)...", count));

        let (tx, rx) = mpsc::channel(10);
        self.cicd_rx = Some(rx);

        tokio::spawn(async move {
            // Step 1: Get all pending approvals
            let output = tokio::process::Command::new("az")
                .args(["devops", "invoke"])
                .args(["--area", "release"])
                .args(["--resource", "approvals"])
                .args(["--route-parameters", &format!("project={proj}")])
                .args(["--query-parameters", "statusFilter=pending"])
                .args(["--org", &org])
                .args(["--output", "json"])
                .output()
                .await;

            let approvals_response = match output {
                Ok(o) if o.status.success() => {
                    match serde_json::from_slice::<crate::azure::ApprovalsResponse>(&o.stdout) {
                        Ok(resp) => resp,
                        Err(e) => {
                            let _ = tx.send(CICDLoadResult::Error(format!("Parse error: {e}"))).await;
                            return;
                        }
                    }
                }
                Ok(o) => {
                    let _ = tx.send(CICDLoadResult::Error(format!("API error: {}", String::from_utf8_lossy(&o.stderr)))).await;
                    return;
                }
                Err(e) => {
                    let _ = tx.send(CICDLoadResult::Error(format!("Command failed: {e}"))).await;
                    return;
                }
            };

            // Step 2: Find approvals for our environments
            let approvals_to_approve: Vec<_> = approvals_response.value.iter()
                .filter(|a| {
                    a.release_environment.as_ref()
                        .map(|e| pending_env_ids.contains(&e.id))
                        .unwrap_or(false)
                })
                .collect();

            if approvals_to_approve.is_empty() {
                let _ = tx.send(CICDLoadResult::Error("No matching approvals found".to_string())).await;
                return;
            }

            // Step 3: Approve all at once
            let body: Vec<_> = approvals_to_approve.iter()
                .map(|a| serde_json::json!({
                    "id": a.id,
                    "status": "approved",
                    "comments": "Approved via lazyops"
                }))
                .collect();
            let body_str = serde_json::Value::Array(body).to_string();

            let temp_path = std::env::temp_dir().join("approval_all.json");
            if let Err(e) = tokio::fs::write(&temp_path, &body_str).await {
                let _ = tx.send(CICDLoadResult::Error(format!("Failed to write temp file: {e}"))).await;
                return;
            }

            let approve_output = tokio::process::Command::new("az")
                .args(["devops", "invoke"])
                .args(["--area", "release"])
                .args(["--resource", "approvals"])
                .args(["--route-parameters", &format!("project={proj}")])
                .args(["--http-method", "PATCH"])
                .args(["--in-file", temp_path.to_str().unwrap()])
                .args(["--org", &org])
                .args(["--output", "json"])
                .output()
                .await;

            // Clean up temp file
            let _ = tokio::fs::remove_file(&temp_path).await;

            match approve_output {
                Ok(o) if o.status.success() => {
                    let _ = tx.send(CICDLoadResult::ApprovalUpdated {
                        approval_id: 0,  // Multiple approvals
                        release_id,
                        status: format!("approved {} stage(s)", approvals_to_approve.len()),
                    }).await;
                }
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let _ = tx.send(CICDLoadResult::Error(format!("Approval failed: {}", stderr.lines().next().unwrap_or(&stderr)))).await;
                }
                Err(e) => {
                    let _ = tx.send(CICDLoadResult::Error(format!("Command failed: {e}"))).await;
                }
            }
        });
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
        self.status_is_error = false;
        self.status_set_at = Some(std::time::Instant::now());
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
        self.status_is_error = true;
        self.status_set_at = Some(std::time::Instant::now());
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
        self.status_is_error = false;
        self.status_set_at = None;
    }

    /// Clear status message if it's older than 5 seconds
    pub fn clear_expired_status(&mut self) {
        if let Some(set_at) = self.status_set_at {
            if set_at.elapsed() > std::time::Duration::from_secs(5) {
                self.clear_status();
            }
        }
    }

    /// Execute a confirmed action (cancel/retrigger)
    pub fn execute_confirmed_action(&mut self, action_type: ConfirmActionType) {
        let client_info = self.current_project().map(|p| {
            (p.organization.clone(), p.project.clone())
        });

        let Some((org, project)) = client_info else {
            self.set_error("No project configured");
            return;
        };

        // Get or create the sender channel
        let tx = if let Some(tx) = &self.cicd_tx {
            tx.clone()
        } else {
            let (tx, rx) = tokio::sync::mpsc::channel(10);
            self.cicd_rx = Some(rx);
            self.cicd_tx = Some(tx.clone());
            tx
        };

        self.cicd_loading = true;

        match action_type {
            ConfirmActionType::CancelPipelineRun { run_id, build_number } => {
                self.set_status(format!("Canceling build #{}...", build_number));
                tokio::spawn(async move {
                    let output = tokio::process::Command::new("az")
                        .args(["pipelines", "build", "update"])
                        .args(["--id", &run_id.to_string()])
                        .args(["--status", "cancelling"])
                        .args(["--org", &org])
                        .args(["--project", &project])
                        .args(["--output", "json"])
                        .output()
                        .await;

                    let result = match output {
                        Ok(o) if o.status.success() => CICDLoadResult::PipelineRunCanceled(run_id),
                        Ok(o) => CICDLoadResult::Error(String::from_utf8_lossy(&o.stderr).to_string()),
                        Err(e) => CICDLoadResult::Error(e.to_string()),
                    };
                    let _ = tx.send(result).await;
                });
            }

            ConfirmActionType::RetriggerPipelineRun { pipeline_id, branch, build_number } => {
                self.set_status(format!("Retriggering #{}...", build_number));
                tokio::spawn(async move {
                    let output = tokio::process::Command::new("az")
                        .args(["pipelines", "run"])
                        .args(["--id", &pipeline_id.to_string()])
                        .args(["--branch", &branch])
                        .args(["--org", &org])
                        .args(["--project", &project])
                        .args(["--output", "json"])
                        .output()
                        .await;

                    let result = match output {
                        Ok(o) if o.status.success() => {
                            match serde_json::from_slice(&o.stdout) {
                                Ok(run) => CICDLoadResult::PipelineRunRetriggered(run),
                                Err(e) => CICDLoadResult::Error(format!("Parse error: {}", e)),
                            }
                        }
                        Ok(o) => CICDLoadResult::Error(String::from_utf8_lossy(&o.stderr).to_string()),
                        Err(e) => CICDLoadResult::Error(e.to_string()),
                    };
                    let _ = tx.send(result).await;
                });
            }

            ConfirmActionType::CancelRelease { release_id, release_name } => {
                self.set_status(format!("Abandoning {}...", release_name));
                tokio::spawn(async move {
                    let body = serde_json::json!({"status": "abandoned"});
                    let temp_path = std::env::temp_dir().join(format!("cancel_release_{}.json", release_id));

                    if let Err(e) = tokio::fs::write(&temp_path, body.to_string()).await {
                        let _ = tx.send(CICDLoadResult::Error(e.to_string())).await;
                        return;
                    }

                    let output = tokio::process::Command::new("az")
                        .args(["devops", "invoke"])
                        .args(["--area", "release"])
                        .args(["--resource", "releases"])
                        .args(["--route-parameters", &format!("project={}", project), &format!("releaseId={}", release_id)])
                        .args(["--api-version", "7.1"])
                        .args(["--http-method", "PATCH"])
                        .args(["--in-file", temp_path.to_str().unwrap()])
                        .args(["--org", &org])
                        .args(["--output", "json"])
                        .output()
                        .await;

                    let _ = tokio::fs::remove_file(&temp_path).await;

                    let result = match output {
                        Ok(o) if o.status.success() => CICDLoadResult::ReleaseCanceled(release_id),
                        Ok(o) => CICDLoadResult::Error(String::from_utf8_lossy(&o.stderr).to_string()),
                        Err(e) => CICDLoadResult::Error(e.to_string()),
                    };
                    let _ = tx.send(result).await;
                });
            }

            ConfirmActionType::CancelReleaseEnvironment { release_id, environment_id, environment_name, .. } => {
                self.set_status(format!("Canceling {}...", environment_name));
                tokio::spawn(async move {
                    let body = serde_json::json!({
                        "status": "canceled",
                        "comment": "Canceled from lazyops"
                    });
                    let temp_path = std::env::temp_dir().join(format!("cancel_env_{}_{}.json", release_id, environment_id));

                    if let Err(e) = tokio::fs::write(&temp_path, body.to_string()).await {
                        let _ = tx.send(CICDLoadResult::Error(e.to_string())).await;
                        return;
                    }

                    let output = tokio::process::Command::new("az")
                        .args(["devops", "invoke"])
                        .args(["--area", "release"])
                        .args(["--resource", "environments"])
                        .args(["--route-parameters",
                               &format!("project={}", project),
                               &format!("releaseId={}", release_id),
                               &format!("environmentId={}", environment_id)])
                        .args(["--api-version", "7.1"])
                        .args(["--http-method", "PATCH"])
                        .args(["--in-file", temp_path.to_str().unwrap()])
                        .args(["--org", &org])
                        .args(["--output", "json"])
                        .output()
                        .await;

                    let _ = tokio::fs::remove_file(&temp_path).await;

                    let result = match output {
                        Ok(o) if o.status.success() => CICDLoadResult::ReleaseEnvironmentCanceled {
                            release_id,
                            environment_name
                        },
                        Ok(o) => CICDLoadResult::Error(String::from_utf8_lossy(&o.stderr).to_string()),
                        Err(e) => CICDLoadResult::Error(e.to_string()),
                    };
                    let _ = tx.send(result).await;
                });
            }

            ConfirmActionType::RetriggerReleaseEnvironment { release_id, environment_id, environment_name, .. } => {
                self.set_status(format!("Redeploying {}...", environment_name));
                tokio::spawn(async move {
                    let body = serde_json::json!({
                        "status": "inProgress",
                        "comment": "Redeployed from lazyops"
                    });
                    let temp_path = std::env::temp_dir().join(format!("redeploy_env_{}_{}.json", release_id, environment_id));

                    if let Err(e) = tokio::fs::write(&temp_path, body.to_string()).await {
                        let _ = tx.send(CICDLoadResult::Error(e.to_string())).await;
                        return;
                    }

                    let output = tokio::process::Command::new("az")
                        .args(["devops", "invoke"])
                        .args(["--area", "release"])
                        .args(["--resource", "environments"])
                        .args(["--route-parameters",
                               &format!("project={}", project),
                               &format!("releaseId={}", release_id),
                               &format!("environmentId={}", environment_id)])
                        .args(["--api-version", "7.1"])
                        .args(["--http-method", "PATCH"])
                        .args(["--in-file", temp_path.to_str().unwrap()])
                        .args(["--org", &org])
                        .args(["--output", "json"])
                        .output()
                        .await;

                    let _ = tokio::fs::remove_file(&temp_path).await;

                    let result = match output {
                        Ok(o) if o.status.success() => CICDLoadResult::ReleaseEnvironmentRedeployed {
                            release_id,
                            environment_name
                        },
                        Ok(o) => CICDLoadResult::Error(String::from_utf8_lossy(&o.stderr).to_string()),
                        Err(e) => CICDLoadResult::Error(e.to_string()),
                    };
                    let _ = tx.send(result).await;
                });
            }

            ConfirmActionType::RejectApproval { approval_id, release_id, environment_name } => {
                self.set_status(format!("Rejecting approval for {}...", environment_name));
                tokio::spawn(async move {
                    // Azure DevOps Approvals API expects an array of approval objects with id included
                    let body = serde_json::json!([{
                        "id": approval_id,
                        "status": "rejected",
                        "comments": "Rejected from lazyops"
                    }]);
                    let temp_path = std::env::temp_dir().join(format!("reject_approval_{}.json", approval_id));

                    if let Err(e) = tokio::fs::write(&temp_path, body.to_string()).await {
                        let _ = tx.send(CICDLoadResult::Error(e.to_string())).await;
                        return;
                    }

                    // Use bulk approvals endpoint (no approvalId in route) with array body
                    let output = tokio::process::Command::new("az")
                        .args(["devops", "invoke"])
                        .args(["--area", "release"])
                        .args(["--resource", "approvals"])
                        .args(["--route-parameters",
                               &format!("project={}", project)])
                        .args(["--http-method", "PATCH"])
                        .args(["--api-version", "7.1"])
                        .args(["--in-file", temp_path.to_str().unwrap()])
                        .args(["--org", &org])
                        .args(["--output", "json"])
                        .output()
                        .await;

                    let _ = tokio::fs::remove_file(&temp_path).await;

                    let result = match output {
                        Ok(o) if o.status.success() => CICDLoadResult::ApprovalUpdated {
                            approval_id,
                            release_id,
                            status: "rejected".to_string(),
                        },
                        Ok(o) => CICDLoadResult::Error(String::from_utf8_lossy(&o.stderr).to_string()),
                        Err(e) => CICDLoadResult::Error(e.to_string()),
                    };
                    let _ = tx.send(result).await;
                });
            }
        }
    }

    // Data loading
    pub async fn load_sprints(&mut self) -> Result<()> {
        let client = self.client().ok_or_else(|| anyhow::anyhow!("No project configured"))?;
        self.sprints = client.get_sprints().await?;

        // Select current sprint by default
        self.selected_sprint_idx = self.sprints.iter()
            .position(|s| s.attributes.time_frame.as_deref() == Some("current"))
            .unwrap_or(0);

        self.sprint_list_state.select(Some(self.selected_sprint_idx));
        Ok(())
    }

    pub async fn load_work_items(&mut self) -> Result<()> {
        let client = self.client().ok_or_else(|| anyhow::anyhow!("No project configured"))?;
        let sprint = self.sprints.get(self.selected_sprint_idx);

        if let Some(sprint) = sprint {
            self.work_items = client.get_sprint_work_items(&sprint.path).await?;
            self.extract_users_from_work_items();
            self.rebuild_visible_items();
            if !self.visible_items.is_empty() {
                self.work_item_list_state.select(Some(0));
            }
        }

        Ok(())
    }

    pub async fn load_users(&mut self) -> Result<()> {
        self.current_user = AzureCli::get_current_user().await.ok();
        Ok(())
    }

    /// Extract unique assignees from work items
    pub fn extract_users_from_work_items(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.users.clear();

        fn collect_users(items: &[WorkItem], users: &mut Vec<User>, seen: &mut std::collections::HashSet<String>) {
            for item in items {
                if let Some(assignee) = &item.fields.assigned_to {
                    if seen.insert(assignee.unique_name.clone()) {
                        users.push(User {
                            display_name: assignee.display_name.clone(),
                            unique_name: assignee.unique_name.clone(),
                        });
                    }
                }
                collect_users(&item.children, users, seen);
            }
        }

        collect_users(&self.work_items, &mut self.users, &mut seen);
        // Sort by display name
        self.users.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    }

    // Flatten hierarchy for display with filters
    pub fn rebuild_visible_items(&mut self) {
        self.visible_items.clear();

        let query = self.search_query.to_lowercase();
        let filter_state = self.filter_state.clone();
        let filter_assignee = self.filter_assignee.clone();
        let matcher = &self.fuzzy_matcher;
        let pinned = &self.pinned_items;
        let force_collapsed = self.force_collapsed;

        #[allow(clippy::too_many_arguments)]
        fn flatten(
            items: &[WorkItem],
            visible: &mut Vec<VisibleWorkItem>,
            expanded: &HashSet<i32>,
            pinned: &HashSet<i32>,
            depth: usize,
            query: &str,
            filter_state: &Option<String>,
            filter_assignee: &Option<String>,
            matcher: &SkimMatcherV2,
            force_collapsed: bool,
        ) -> bool {
            let mut has_match = false;

            for item in items {
                // Apply filters
                let state_match = filter_state.as_ref()
                    .map(|s| item.fields.state.eq_ignore_ascii_case(s))
                    .unwrap_or(true);
                let assignee_match = filter_assignee.as_ref()
                    .map(|a| {
                        if a == "Unassigned" {
                            item.fields.assigned_to.is_none()
                        } else {
                            item.fields.assigned_to.as_ref()
                                .map(|at| at.display_name.eq_ignore_ascii_case(a))
                                .unwrap_or(false)
                        }
                    })
                    .unwrap_or(true);

                let has_children = !item.children.is_empty();
                let is_expanded = expanded.contains(&item.id);
                let is_pinned = depth == 0 && pinned.contains(&item.id);

                // Check if any filters/search are active
                let has_active_filter = !query.is_empty() || filter_state.is_some() || filter_assignee.is_some();

                // Only check child matches when filters are active
                let mut child_matches = false;
                if has_children && has_active_filter {
                    let mut temp = Vec::new();
                    child_matches = flatten(&item.children, &mut temp, expanded, pinned, depth + 1, query, filter_state, filter_assignee, matcher, force_collapsed);
                }

                let passes_filter = state_match && assignee_match;

                // Fuzzy search on title and ID
                let passes_search = if query.is_empty() {
                    true
                } else {
                    let title_match = matcher.fuzzy_match(&item.fields.title, query).is_some();
                    let id_match = item.id.to_string().contains(query);
                    let type_match = matcher.fuzzy_match(&item.fields.work_item_type, query).is_some();
                    title_match || id_match || type_match
                };

                // Show if passes all filters, OR if a child matches (during filter mode)
                let should_show = (passes_filter && passes_search) || child_matches;

                if should_show {
                    has_match = true;
                    // Only auto-expand when a child matches during filtering (unless force_collapsed)
                    let show_expanded = is_expanded || (has_active_filter && child_matches && !force_collapsed);
                    visible.push(VisibleWorkItem {
                        item: item.clone(),
                        depth,
                        has_children,
                        is_expanded: show_expanded,
                        is_pinned,
                    });

                    // Only show children if explicitly expanded OR child matches filter
                    if show_expanded && has_children {
                        flatten(&item.children, visible, expanded, pinned, depth + 1, query, filter_state, filter_assignee, matcher, force_collapsed);
                    }
                }
            }

            has_match
        }

        // Process pinned root items first, then non-pinned (keeps children with parents)
        // Collect indices to avoid borrowing issues
        let pinned_indices: Vec<usize> = self.work_items.iter()
            .enumerate()
            .filter(|(_, i)| pinned.contains(&i.id))
            .map(|(idx, _)| idx)
            .collect();
        let non_pinned_indices: Vec<usize> = self.work_items.iter()
            .enumerate()
            .filter(|(_, i)| !pinned.contains(&i.id))
            .map(|(idx, _)| idx)
            .collect();

        // Flatten pinned items first
        for idx in pinned_indices {
            flatten(&self.work_items[idx..idx+1], &mut self.visible_items, &self.expanded_items, pinned, 0, &query, &filter_state, &filter_assignee, matcher, force_collapsed);
        }

        // Then flatten non-pinned items
        for idx in non_pinned_indices {
            flatten(&self.work_items[idx..idx+1], &mut self.visible_items, &self.expanded_items, pinned, 0, &query, &filter_state, &filter_assignee, matcher, force_collapsed);
        }

        // Reset selection if out of bounds
        if let Some(selected) = self.work_item_list_state.selected() {
            if selected >= self.visible_items.len() {
                self.work_item_list_state.select(if self.visible_items.is_empty() { None } else { Some(0) });
            }
        }
    }

    pub fn selected_work_item(&self) -> Option<&VisibleWorkItem> {
        self.work_item_list_state.selected().and_then(|i| self.visible_items.get(i))
    }

    pub fn selected_sprint(&self) -> Option<&Sprint> {
        self.sprints.get(self.selected_sprint_idx)
    }

    /// Toggle pin on currently selected item (or its parent if it's a child)
    pub fn toggle_pin(&mut self) {
        if let Some(vi) = self.selected_work_item() {
            // If it's a child, find and pin/unpin its parent instead
            let id = if vi.depth > 0 {
                vi.item.fields.parent_id.unwrap_or(vi.item.id)
            } else {
                vi.item.id
            };

            if self.pinned_items.contains(&id) {
                self.pinned_items.remove(&id);
            } else {
                self.pinned_items.insert(id);
            }
            self.rebuild_visible_items();
            self.save_to_cache();
        }
    }

    /// Toggle pin on currently selected pipeline
    pub fn toggle_pin_pipeline(&mut self) {
        // Get current visual position before toggling
        let sorted_before = self.sorted_pipeline_indices();
        let visual_pos = sorted_before.iter().position(|&i| i == self.selected_pipeline_idx).unwrap_or(0);

        if let Some(pipeline) = self.pipelines.get(self.selected_pipeline_idx) {
            let id = pipeline.id;
            if self.pinned_pipelines.contains(&id) {
                self.pinned_pipelines.remove(&id);
            } else {
                self.pinned_pipelines.insert(id);
            }
            self.save_cicd_to_cache();
        }

        // Stay at the same visual position (now showing a different item)
        let sorted_after = self.sorted_pipeline_indices();
        if let Some(&new_idx) = sorted_after.get(visual_pos) {
            self.selected_pipeline_idx = new_idx;
            self.pipeline_list_state.select(Some(visual_pos));
        }
    }

    /// Toggle pin on currently selected release definition
    pub fn toggle_pin_release(&mut self) {
        // Get current visual position before toggling
        let sorted_before = self.sorted_release_indices();
        let visual_pos = sorted_before.iter().position(|&i| i == self.selected_release_idx).unwrap_or(0);

        if let Some(release) = self.releases.get(self.selected_release_idx) {
            let id = release.id;
            if self.pinned_releases.contains(&id) {
                self.pinned_releases.remove(&id);
            } else {
                self.pinned_releases.insert(id);
            }
            self.save_cicd_to_cache();
        }

        // Stay at the same visual position (now showing a different item)
        let sorted_after = self.sorted_release_indices();
        if let Some(&new_idx) = sorted_after.get(visual_pos) {
            self.selected_release_idx = new_idx;
            self.release_list_state.select(Some(visual_pos));
        }
    }

    /// Get sorted pipeline indices (pinned first, then alphabetical)
    pub fn sorted_pipeline_indices(&self) -> Vec<usize> {
        let mut indices: Vec<(usize, bool, String)> = self.pipelines.iter()
            .enumerate()
            .filter(|(_, p)| {
                // Apply search filter if active
                if self.cicd_search_query.is_empty() {
                    true
                } else {
                    self.fuzzy_matcher.fuzzy_match(&p.name, &self.cicd_search_query).is_some()
                }
            })
            .map(|(i, p)| (i, self.pinned_pipelines.contains(&p.id), p.name.to_lowercase()))
            .collect();

        indices.sort_by(|a, b| {
            match (a.1, b.1) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.2.cmp(&b.2),
            }
        });

        indices.into_iter().map(|(i, _, _)| i).collect()
    }

    /// Get sorted release indices (pinned first, then alphabetical)
    pub fn sorted_release_indices(&self) -> Vec<usize> {
        let mut indices: Vec<(usize, bool, String)> = self.releases.iter()
            .enumerate()
            .filter(|(_, r)| {
                // Apply search filter if active and releases panel is focused
                if self.cicd_search_query.is_empty() || self.cicd_focus != CICDFocus::Releases {
                    true
                } else {
                    self.fuzzy_matcher.fuzzy_match(&r.name, &self.cicd_search_query).is_some()
                }
            })
            .map(|(i, r)| (i, self.pinned_releases.contains(&r.id), r.name.to_lowercase()))
            .collect();

        indices.sort_by(|a, b| {
            match (a.1, b.1) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.2.cmp(&b.2),
            }
        });

        indices.into_iter().map(|(i, _, _)| i).collect()
    }

    /// Navigate to next pipeline in sorted order
    pub fn pipeline_next(&mut self) {
        let sorted = self.sorted_pipeline_indices();
        if sorted.is_empty() { return; }

        let current_pos = sorted.iter().position(|&i| i == self.selected_pipeline_idx).unwrap_or(0);
        let new_pos = (current_pos + 1).min(sorted.len() - 1);
        self.selected_pipeline_idx = sorted[new_pos];
        self.pipeline_list_state.select(Some(new_pos));
    }

    /// Navigate to previous pipeline in sorted order
    pub fn pipeline_prev(&mut self) {
        let sorted = self.sorted_pipeline_indices();
        if sorted.is_empty() { return; }

        let current_pos = sorted.iter().position(|&i| i == self.selected_pipeline_idx).unwrap_or(0);
        let new_pos = current_pos.saturating_sub(1);
        self.selected_pipeline_idx = sorted[new_pos];
        self.pipeline_list_state.select(Some(new_pos));
    }

    /// Navigate to next release in sorted order
    pub fn release_next(&mut self) {
        let sorted = self.sorted_release_indices();
        if sorted.is_empty() { return; }

        let current_pos = sorted.iter().position(|&i| i == self.selected_release_idx).unwrap_or(0);
        let new_pos = (current_pos + 1).min(sorted.len() - 1);
        self.selected_release_idx = sorted[new_pos];
        self.release_list_state.select(Some(new_pos));
    }

    /// Navigate to previous release in sorted order
    pub fn release_prev(&mut self) {
        let sorted = self.sorted_release_indices();
        if sorted.is_empty() { return; }

        let current_pos = sorted.iter().position(|&i| i == self.selected_release_idx).unwrap_or(0);
        let new_pos = current_pos.saturating_sub(1);
        self.selected_release_idx = sorted[new_pos];
        self.release_list_state.select(Some(new_pos));
    }

    /// Select first pipeline in sorted order (for search reset)
    pub fn select_first_pipeline(&mut self) {
        let sorted = self.sorted_pipeline_indices();
        if let Some(&first_idx) = sorted.first() {
            self.selected_pipeline_idx = first_idx;
            self.pipeline_list_state.select(Some(0));
        }
    }

    /// Select first release in sorted order (for search reset)
    pub fn select_first_release(&mut self) {
        let sorted = self.sorted_release_indices();
        if let Some(&first_idx) = sorted.first() {
            self.selected_release_idx = first_idx;
            self.release_list_state.select(Some(0));
        }
    }

    // Navigation
    pub fn list_next(&mut self) {
        let len = self.visible_items.len();
        if len == 0 { return; }
        // Stop at bottom, don't wrap
        let i = self.work_item_list_state.selected()
            .map(|i| (i + 1).min(len - 1))
            .unwrap_or(0);
        self.work_item_list_state.select(Some(i));
        self.preview_scroll = 0;
    }

    pub fn list_prev(&mut self) {
        let len = self.visible_items.len();
        if len == 0 { return; }
        // Stop at top, don't wrap
        let i = self.work_item_list_state.selected()
            .map(|i| i.saturating_sub(1))
            .unwrap_or(0);
        self.work_item_list_state.select(Some(i));
        self.preview_scroll = 0;
    }

    pub fn list_top(&mut self) {
        if !self.visible_items.is_empty() {
            self.work_item_list_state.select(Some(0));
            self.preview_scroll = 0;
        }
    }

    pub fn list_bottom(&mut self) {
        if !self.visible_items.is_empty() {
            self.work_item_list_state.select(Some(self.visible_items.len() - 1));
            self.preview_scroll = 0;
        }
    }

    pub fn list_jump_down(&mut self) {
        let len = self.visible_items.len();
        if len == 0 { return; }
        let jump = 10; // Jump 10 items
        let i = self.work_item_list_state.selected()
            .map(|i| (i + jump).min(len - 1))
            .unwrap_or(0);
        self.work_item_list_state.select(Some(i));
        self.preview_scroll = 0;
    }

    pub fn list_jump_up(&mut self) {
        let len = self.visible_items.len();
        if len == 0 { return; }
        let jump = 10; // Jump 10 items
        let i = self.work_item_list_state.selected()
            .map(|i| i.saturating_sub(jump))
            .unwrap_or(0);
        self.work_item_list_state.select(Some(i));
        self.preview_scroll = 0;
    }

    pub fn toggle_expand(&mut self) {
        if let Some(item) = self.selected_work_item() {
            let id = item.item.id;
            if item.has_children {
                if self.expanded_items.contains(&id) {
                    self.expanded_items.remove(&id);
                } else {
                    self.expanded_items.insert(id);
                }
                self.rebuild_visible_items();
            }
        }
    }

    /// Expand all items with children
    pub fn expand_all(&mut self) {
        // Remember current selection
        let selected_id = self.selected_work_item().map(|w| w.item.id);

        fn collect_ids(items: &[WorkItem], ids: &mut HashSet<i32>) {
            for item in items {
                if !item.children.is_empty() {
                    ids.insert(item.id);
                    collect_ids(&item.children, ids);
                }
            }
        }
        collect_ids(&self.work_items, &mut self.expanded_items);
        self.rebuild_visible_items();

        // Restore selection
        if let Some(id) = selected_id {
            if let Some(pos) = self.visible_items.iter().position(|v| v.item.id == id) {
                self.work_item_list_state.select(Some(pos));
            }
        }
    }

    /// Collapse all items
    pub fn collapse_all(&mut self) {
        // Remember current selection
        let selected_id = self.selected_work_item().map(|w| w.item.id);

        self.expanded_items.clear();
        self.rebuild_visible_items();

        // Restore selection
        if let Some(id) = selected_id {
            if let Some(pos) = self.visible_items.iter().position(|v| v.item.id == id) {
                self.work_item_list_state.select(Some(pos));
            }
        }
    }

    /// Toggle between all expanded and all collapsed
    pub fn toggle_expand_all(&mut self) {
        // If currently force-collapsed or all collapsed, expand
        // Otherwise collapse
        if self.force_collapsed || self.expanded_items.is_empty() {
            self.force_collapsed = false;
            self.expand_all();
        } else {
            self.force_collapsed = true;
            self.collapse_all();
        }
    }

    pub fn scroll_preview_down(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_add(10).min(self.preview_scroll_max);
    }

    pub fn scroll_preview_up(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_sub(10);
    }

    pub fn next_tab(&mut self) {
        self.preview_tab = self.preview_tab.next();
        self.preview_scroll = 0;
    }

    pub fn prev_tab(&mut self) {
        self.preview_tab = self.preview_tab.prev();
        self.preview_scroll = 0;
    }

    /// Update relations for a work item by ID (in both work_items tree and visible_items)
    pub fn update_work_item_relations(&mut self, id: i32, relations: Option<Vec<crate::azure::WorkItemRelation>>) {
        // Update in hierarchical work_items
        fn update_in_tree(items: &mut [WorkItem], id: i32, relations: &Option<Vec<crate::azure::WorkItemRelation>>) -> bool {
            for item in items.iter_mut() {
                if item.id == id {
                    item.relations = relations.clone();
                    return true;
                }
                if update_in_tree(&mut item.children, id, relations) {
                    return true;
                }
            }
            false
        }
        update_in_tree(&mut self.work_items, id, &relations);

        // Update in visible_items
        for vi in &mut self.visible_items {
            if vi.item.id == id {
                vi.item.relations = relations.clone();
                break;
            }
        }

        // Mark as loaded
        self.relations_loaded.insert(id);
    }

    /// Get all work item IDs that need relations loaded
    pub fn get_ids_needing_relations(&self) -> Vec<i32> {
        fn collect_ids(items: &[WorkItem], loaded: &HashSet<i32>, result: &mut Vec<i32>) {
            for item in items {
                if !loaded.contains(&item.id) {
                    result.push(item.id);
                }
                collect_ids(&item.children, loaded, result);
            }
        }
        let mut ids = Vec::new();
        collect_ids(&self.work_items, &self.relations_loaded, &mut ids);
        ids
    }

    /// Cache all loaded relations before refresh
    pub fn cache_relations(&self) -> std::collections::HashMap<i32, Vec<crate::azure::WorkItemRelation>> {
        fn collect(items: &[WorkItem], cache: &mut std::collections::HashMap<i32, Vec<crate::azure::WorkItemRelation>>) {
            for item in items {
                if let Some(relations) = &item.relations {
                    cache.insert(item.id, relations.clone());
                }
                collect(&item.children, cache);
            }
        }
        let mut cache = std::collections::HashMap::new();
        collect(&self.work_items, &mut cache);
        cache
    }

    /// Restore cached relations after refresh
    pub fn restore_relations(&mut self, cache: std::collections::HashMap<i32, Vec<crate::azure::WorkItemRelation>>) {
        fn restore(items: &mut [WorkItem], cache: &std::collections::HashMap<i32, Vec<crate::azure::WorkItemRelation>>) {
            for item in items.iter_mut() {
                if let Some(relations) = cache.get(&item.id) {
                    item.relations = Some(relations.clone());
                }
                restore(&mut item.children, cache);
            }
        }
        restore(&mut self.work_items, &cache);
        // Also update visible items
        for vi in &mut self.visible_items {
            if let Some(relations) = cache.get(&vi.item.id) {
                vi.item.relations = Some(relations.clone());
            }
        }
    }

    /// Get relations for the selected work item, grouped and sorted
    /// Order: Children, Attachments, PRs, Commits, Other
    pub fn selected_relations(&self) -> Vec<&crate::azure::WorkItemRelation> {
        let Some(vi) = self.selected_work_item() else {
            return Vec::new();
        };
        let Some(relations) = vi.item.relations.as_ref() else {
            return Vec::new();
        };

        // Filter out parent links (Hierarchy-Reverse) only
        let mut refs: Vec<&crate::azure::WorkItemRelation> = relations.iter()
            .filter(|r| r.rel != "System.LinkTypes.Hierarchy-Reverse")
            .collect();

        // Sort by type priority
        refs.sort_by_key(|r| {
            // Check rel type first for special cases
            if r.rel == "System.LinkTypes.Hierarchy-Forward" {
                return 0; // Children
            }
            if r.rel == "AttachedFile" {
                return 1; // Attachments
            }

            let name = r.attributes.name.as_deref().unwrap_or("");
            match name {
                "Child" => 0,
                "Pull Request" => 2,
                "Fixed in Commit" => 3,
                "Branch" => 4,
                _ => 5, // Other
            }
        });

        refs
    }

    /// Navigate relations list
    pub fn relations_next(&mut self) {
        let len = self.selected_relations().len();
        if len == 0 { return; }
        let i = self.relations_list_state.selected()
            .map(|i| (i + 1).min(len - 1))
            .unwrap_or(0);
        self.relations_list_state.select(Some(i));
    }

    pub fn relations_prev(&mut self) {
        let len = self.selected_relations().len();
        if len == 0 { return; }
        let i = self.relations_list_state.selected()
            .map(|i| i.saturating_sub(1))
            .unwrap_or(0);
        self.relations_list_state.select(Some(i));
    }

    pub fn relations_page_down(&mut self) {
        let len = self.selected_relations().len();
        if len == 0 { return; }
        let i = self.relations_list_state.selected()
            .map(|i| (i + 10).min(len - 1))
            .unwrap_or(0);
        self.relations_list_state.select(Some(i));
    }

    pub fn relations_page_up(&mut self) {
        let len = self.selected_relations().len();
        if len == 0 { return; }
        let i = self.relations_list_state.selected()
            .map(|i| i.saturating_sub(10))
            .unwrap_or(0);
        self.relations_list_state.select(Some(i));
    }

    /// Get the selected relation
    pub fn selected_relation(&self) -> Option<&crate::azure::WorkItemRelation> {
        let refs = self.selected_relations();
        self.relations_list_state.selected().and_then(|i| refs.get(i).copied())
    }

    /// Parse a relation into display-friendly format
    pub fn parse_relation(&self, relation: &crate::azure::WorkItemRelation) -> ParsedRelation {
        let project_config = self.current_project();
        let base = project_config.map(|p| p.organization.trim_end_matches('/').to_string());
        let project_name = project_config.map(|p| p.project.replace(' ', "%20"));

        let name = relation.attributes.name.as_deref().unwrap_or("");

        // Parse artifact URL: vstfs:///Git/{Type}/{projectGuid}%2F{repoGuid}%2F{id}
        // We need to extract repoGuid for proper URLs
        let parts: Vec<&str> = relation.url
            .split("%2F")
            .flat_map(|s| s.split("%2f"))
            .collect();

        match name {
            "Pull Request" => {
                // parts: ["vstfs:", "", "", "Git", "PullRequestId", "{projectGuid}", "{repoGuid}", "{prId}"]
                let pr_id = parts.last().unwrap_or(&"?");
                let repo_guid = if parts.len() >= 3 { parts[parts.len() - 2] } else { "" };

                let url = match (&base, &project_name) {
                    (Some(b), Some(p)) if !repo_guid.is_empty() => {
                        Some(format!("{b}/{p}/_git/{repo_guid}/pullrequest/{pr_id}"))
                    }
                    (Some(b), Some(p)) => {
                        Some(format!("{b}/{p}/_git/pullrequest/{pr_id}"))
                    }
                    _ => None,
                };

                // Use cached title if available
                let key = format!("pr:{pr_id}");
                let description = if let Some(title) = self.relation_titles.get(&key) {
                    format!("#{pr_id} {title}")
                } else {
                    format!("PR #{pr_id}")
                };

                ParsedRelation {
                    icon: "⎇",
                    description,
                    url,
                }
            }
            "Fixed in Commit" => {
                // parts: ["vstfs:", "", "", "Git", "Commit", "{projectGuid}", "{repoGuid}", "{commitHash}"]
                let hash = parts.last().unwrap_or(&"?");
                let repo_guid = if parts.len() >= 3 { parts[parts.len() - 2] } else { "" };
                let short_hash = if hash.len() > 7 { &hash[..7] } else { hash };

                let url = match (&base, &project_name) {
                    (Some(b), Some(p)) if !repo_guid.is_empty() => {
                        Some(format!("{b}/{p}/_git/{repo_guid}/commit/{hash}"))
                    }
                    (Some(b), Some(p)) => {
                        Some(format!("{b}/{p}/_git/commit/{hash}"))
                    }
                    _ => None,
                };

                // Use cached title if available
                let key = format!("commit:{hash}");
                let description = if let Some(title) = self.relation_titles.get(&key) {
                    format!("{short_hash} {title}")
                } else {
                    short_hash.to_string()
                };

                ParsedRelation {
                    icon: "●",
                    description,
                    url,
                }
            }
            "Branch" => {
                let branch = parts.last().unwrap_or(&"?");

                ParsedRelation {
                    icon: "⌥",
                    description: format!("Branch {branch}"),
                    url: None,
                }
            }
            "Child" | "" if relation.rel == "System.LinkTypes.Hierarchy-Forward" => {
                // Child work item - extract ID from URL
                let id = relation.url.split('/').next_back().unwrap_or("?");
                let url = match (&base, &project_name) {
                    (Some(b), Some(p)) => Some(format!("{b}/{p}/_workitems/edit/{id}")),
                    (Some(b), None) => Some(format!("{b}/_workitems/edit/{id}")),
                    _ => None,
                };

                // Try to find title from our work items
                let title = self.find_work_item_title(id.parse().unwrap_or(0));
                let description = if let Some(t) = title {
                    format!("#{id} {t}")
                } else {
                    format!("#{id}")
                };

                ParsedRelation {
                    icon: "◇",
                    description,
                    url,
                }
            }
            _ if relation.rel == "AttachedFile" => {
                // For attachments, attributes.name contains the filename (not "AttachedFile")
                // The relation type is identified by rel="AttachedFile"
                let filename = relation.attributes.name.as_deref().unwrap_or("attachment");

                // Build URL: {base_url}?fileName={filename}&download=false
                // URL encode the filename for the query parameter
                let encoded_filename = filename.replace(' ', "%20");
                let url = if relation.url.contains("?") {
                    Some(format!("{}&fileName={}&download=false", relation.url, encoded_filename))
                } else {
                    Some(format!("{}?fileName={}&download=false", relation.url, encoded_filename))
                };

                ParsedRelation {
                    icon: "📎",
                    description: filename.to_string(),
                    url,
                }
            }
            _ => {
                // Work item link - extract ID from URL (last path segment)
                let id = relation.url.split('/').next_back().unwrap_or("?");
                let url = match (&base, &project_name) {
                    (Some(b), Some(p)) => Some(format!("{b}/{p}/_workitems/edit/{id}")),
                    (Some(b), None) => Some(format!("{b}/_workitems/edit/{id}")),
                    _ => None,
                };

                // Try to find title
                let title = self.find_work_item_title(id.parse().unwrap_or(0));
                let description = if let Some(t) = title {
                    format!("#{id} {t}")
                } else {
                    format!("#{id}")
                };

                ParsedRelation {
                    icon: "◆",
                    description,
                    url,
                }
            }
        }
    }

    /// Find title for a work item by ID (from loaded work items)
    fn find_work_item_title(&self, id: i32) -> Option<String> {
        fn search(items: &[WorkItem], id: i32) -> Option<String> {
            for item in items {
                if item.id == id {
                    return Some(item.fields.title.clone());
                }
                if let Some(t) = search(&item.children, id) {
                    return Some(t);
                }
            }
            None
        }
        search(&self.work_items, id)
    }

    /// Get URL for opening a relation in Azure DevOps
    pub fn get_relation_url(&self, relation: &crate::azure::WorkItemRelation) -> Option<String> {
        self.parse_relation(relation).url
    }

    // Dropdown navigation (for sprints, states, users)
    pub fn dropdown_next(&mut self, max: usize) {
        if max == 0 { return; }
        let i = self.dropdown_list_state.selected().map(|i| (i + 1) % max).unwrap_or(0);
        self.dropdown_list_state.select(Some(i));
    }

    pub fn dropdown_prev(&mut self, max: usize) {
        if max == 0 { return; }
        let i = self.dropdown_list_state.selected().map(|i| if i == 0 { max - 1 } else { i - 1 }).unwrap_or(0);
        self.dropdown_list_state.select(Some(i));
    }

    // Filter helpers
    pub fn available_filter_states(&self) -> Vec<&'static str> {
        vec!["All", "New", "In Progress", "Done In Stage", "Done Not Released", "Done", "Tested w/Bugs", "Removed"]
    }

    /// Get filtered states based on fuzzy input
    pub fn filtered_states(&self) -> Vec<&'static str> {
        let states = self.available_filter_states();
        if self.filter_input.is_empty() {
            return states;
        }
        states.into_iter()
            .filter(|s| self.fuzzy_matcher.fuzzy_match(s, &self.filter_input).is_some())
            .collect()
    }

    pub fn available_filter_assignees(&self) -> Vec<String> {
        let mut assignees: Vec<String> = vec!["All".to_string(), "Unassigned".to_string()];
        for user in &self.users {
            if !assignees.contains(&user.display_name) {
                assignees.push(user.display_name.clone());
            }
        }
        assignees
    }

    /// Get filtered assignees based on fuzzy input
    pub fn filtered_assignees(&self) -> Vec<String> {
        let assignees = self.available_filter_assignees();
        if self.filter_input.is_empty() {
            return assignees;
        }
        assignees.into_iter()
            .filter(|a| self.fuzzy_matcher.fuzzy_match(a, &self.filter_input).is_some())
            .collect()
    }

    pub fn clear_filters(&mut self) {
        self.filter_state = None;
        self.filter_assignee = None;
        self.force_collapsed = false;
        self.rebuild_visible_items();
    }

    pub fn has_active_filters(&self) -> bool {
        self.filter_state.is_some() || self.filter_assignee.is_some()
    }

    /// Get filtered edit states based on fuzzy input (for changing state)
    pub fn filtered_edit_states(&self) -> Vec<&'static str> {
        let states = self.selected_work_item()
            .map(|w| w.item.available_states())
            .unwrap_or_default();
        if self.filter_input.is_empty() {
            return states;
        }
        states.into_iter()
            .filter(|s| self.fuzzy_matcher.fuzzy_match(s, &self.filter_input).is_some())
            .collect()
    }

    /// Get filtered edit assignees based on fuzzy input (for changing assignee)
    pub fn filtered_edit_assignees(&self) -> Vec<&crate::azure::User> {
        if self.filter_input.is_empty() {
            return self.users.iter().collect();
        }
        self.users.iter()
            .filter(|u| self.fuzzy_matcher.fuzzy_match(&u.display_name, &self.filter_input).is_some())
            .collect()
    }

    // ========== Embedded Terminal Methods ==========

    /// Open log viewer in nvim with auto-reload
    /// Creates a temp file and spawns nvim with autoread settings
    pub fn open_log_viewer(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        // Create temp log file
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let log_path = format!("/tmp/lazyops-{timestamp}.log");

        // Write current log content to temp file
        if !self.build_log_lines.is_empty() {
            std::fs::write(&log_path, self.build_log_lines.join("\n"))?;
        } else {
            std::fs::write(&log_path, "")?;
        }

        // Spawn embedded terminal with nvim
        let mut terminal = EmbeddedTerminal::new(cols, rows)?;
        terminal.spawn_log_viewer(&log_path)?;

        self.embedded_terminal = Some(terminal);
        self.terminal_mode = true;
        self.log_file_path = Some(log_path);

        Ok(())
    }

    /// Update the log file with new content (for live updates)
    pub fn update_log_file(&self) -> anyhow::Result<()> {
        if let Some(ref path) = self.log_file_path {
            std::fs::write(path, self.build_log_lines.join("\n"))?;
        }
        Ok(())
    }

    /// Close the embedded terminal and cleanup
    pub fn close_embedded_terminal(&mut self) {
        if let Some(ref mut term) = self.embedded_terminal {
            term.stop();
        }
        self.embedded_terminal = None;
        self.terminal_mode = false;

        // Cleanup temp file
        if let Some(ref path) = self.log_file_path {
            let _ = std::fs::remove_file(path);
        }
        self.log_file_path = None;
    }

    /// Send data to the embedded terminal
    pub fn send_to_terminal(&mut self, data: &[u8]) -> anyhow::Result<()> {
        if let Some(ref mut term) = self.embedded_terminal {
            term.write(data)?;
        }
        Ok(())
    }

    /// Resize the embedded terminal
    pub fn resize_terminal(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        if let Some(ref mut term) = self.embedded_terminal {
            term.resize(cols, rows)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure::{Pipeline, ReleaseDefinition};

    fn make_pipeline(id: i32, name: &str) -> Pipeline {
        Pipeline {
            id,
            name: name.to_string(),
            path: String::new(),
            queue_status: None,
            revision: 0,
        }
    }

    fn make_release_def(id: i32, name: &str) -> ReleaseDefinition {
        ReleaseDefinition {
            id,
            name: name.to_string(),
            path: String::new(),
            is_deleted: false,
            is_disabled: false,
        }
    }

    #[test]
    fn test_sorted_pipeline_indices_alphabetical() {
        let config = Config::default();
        let mut app = App::new(config);

        // Add pipelines in non-alphabetical order
        app.pipelines = vec![
            make_pipeline(1, "Zebra"),
            make_pipeline(2, "Alpha"),
            make_pipeline(3, "Beta"),
        ];

        let sorted = app.sorted_pipeline_indices();

        // Should be sorted alphabetically: Alpha (1), Beta (2), Zebra (0)
        assert_eq!(sorted, vec![1, 2, 0], "Pipelines should be sorted alphabetically");

        // First selected should be Alpha (index 1 in original list)
        assert_eq!(sorted[0], 1);
        assert_eq!(app.pipelines[sorted[0]].name, "Alpha");
    }

    #[test]
    fn test_sorted_pipeline_indices_pinned_first() {
        let config = Config::default();
        let mut app = App::new(config);

        // Add pipelines
        app.pipelines = vec![
            make_pipeline(1, "Zebra"),
            make_pipeline(2, "Alpha"),
            make_pipeline(3, "Beta"),
        ];

        // Pin Zebra (id=1)
        app.pinned_pipelines.insert(1);

        let sorted = app.sorted_pipeline_indices();

        // Should be: Zebra (pinned), then Alpha, Beta alphabetically
        assert_eq!(sorted, vec![0, 1, 2], "Pinned should come first, then alphabetical");
        assert_eq!(app.pipelines[sorted[0]].name, "Zebra");
    }

    #[test]
    fn test_sorted_pipeline_indices_multiple_pinned_alphabetical() {
        let config = Config::default();
        let mut app = App::new(config);

        // Add pipelines
        app.pipelines = vec![
            make_pipeline(1, "Zebra"),
            make_pipeline(2, "Alpha"),
            make_pipeline(3, "Beta"),
            make_pipeline(4, "Delta"),
        ];

        // Pin Zebra and Beta
        app.pinned_pipelines.insert(1); // Zebra
        app.pinned_pipelines.insert(3); // Beta

        let sorted = app.sorted_pipeline_indices();

        // Pinned (alphabetically): Beta, Zebra
        // Then unpinned (alphabetically): Alpha, Delta
        assert_eq!(app.pipelines[sorted[0]].name, "Beta");
        assert_eq!(app.pipelines[sorted[1]].name, "Zebra");
        assert_eq!(app.pipelines[sorted[2]].name, "Alpha");
        assert_eq!(app.pipelines[sorted[3]].name, "Delta");
    }

    #[test]
    fn test_sorted_release_indices_alphabetical() {
        let config = Config::default();
        let mut app = App::new(config);

        app.releases = vec![
            make_release_def(1, "Zebra"),
            make_release_def(2, "Alpha"),
            make_release_def(3, "Beta"),
        ];

        let sorted = app.sorted_release_indices();

        assert_eq!(sorted, vec![1, 2, 0], "Releases should be sorted alphabetically");
        assert_eq!(app.releases[sorted[0]].name, "Alpha");
    }

    // Helper to create work items for testing
    fn make_work_item(id: i32, title: &str, state: &str, parent_id: Option<i32>) -> WorkItem {
        use crate::azure::types::{WorkItem, WorkItemFields};
        WorkItem {
            id,
            rev: 1,
            fields: WorkItemFields {
                title: title.to_string(),
                state: state.to_string(),
                work_item_type: "Task".to_string(),
                assigned_to: None,
                iteration_path: None,
                description: None,
                parent_id,
                created_date: None,
                changed_date: None,
                tags: None,
                remaining_work: None,
                original_estimate: None,
                completed_work: None,
            },
            relations: None,
            children: vec![],
            depth: 0,
        }
    }

    fn make_work_item_with_assignee(id: i32, title: &str, state: &str, assignee: &str) -> WorkItem {
        use crate::azure::types::{AssignedTo, WorkItem, WorkItemFields};
        WorkItem {
            id,
            rev: 1,
            fields: WorkItemFields {
                title: title.to_string(),
                state: state.to_string(),
                work_item_type: "Task".to_string(),
                assigned_to: Some(AssignedTo {
                    display_name: assignee.to_string(),
                    unique_name: format!("{}@example.com", assignee.to_lowercase()),
                }),
                iteration_path: None,
                description: None,
                parent_id: None,
                created_date: None,
                changed_date: None,
                tags: None,
                remaining_work: None,
                original_estimate: None,
                completed_work: None,
            },
            relations: None,
            children: vec![],
            depth: 0,
        }
    }

    // Tests for flatten_work_items
    #[test]
    fn test_flatten_work_items_empty() {
        let items: Vec<WorkItem> = vec![];
        let flattened = App::flatten_work_items(&items);
        assert_eq!(flattened.len(), 0, "Empty list should return empty");
    }

    #[test]
    fn test_flatten_work_items_single() {
        let items = vec![make_work_item(1, "Task 1", "Active", None)];
        let flattened = App::flatten_work_items(&items);
        assert_eq!(flattened.len(), 1, "Single item should return single item");
        assert_eq!(flattened[0].id, 1);
    }

    #[test]
    fn test_flatten_work_items_parent_with_children() {
        let mut parent = make_work_item(1, "Parent", "Active", None);
        parent.children = vec![
            make_work_item(2, "Child 1", "Active", Some(1)),
            make_work_item(3, "Child 2", "Active", Some(1)),
        ];
        parent.depth = 0;
        parent.children[0].depth = 1;
        parent.children[1].depth = 1;

        let items = vec![parent];
        let flattened = App::flatten_work_items(&items);

        assert_eq!(flattened.len(), 3, "Should flatten to 3 items");
        assert_eq!(flattened[0].id, 1, "Parent should be first");
        assert_eq!(flattened[1].id, 2, "First child should be second");
        assert_eq!(flattened[2].id, 3, "Second child should be third");
    }

    #[test]
    fn test_flatten_work_items_deep_nesting() {
        let mut grandparent = make_work_item(1, "Grandparent", "Active", None);
        let mut parent = make_work_item(2, "Parent", "Active", Some(1));
        parent.children = vec![
            make_work_item(3, "Child 1", "Active", Some(2)),
            make_work_item(4, "Child 2", "Active", Some(2)),
        ];
        parent.depth = 1;
        parent.children[0].depth = 2;
        parent.children[1].depth = 2;

        grandparent.children = vec![parent];
        grandparent.depth = 0;

        let items = vec![grandparent];
        let flattened = App::flatten_work_items(&items);

        assert_eq!(flattened.len(), 4, "Should flatten all levels");
        assert_eq!(flattened[0].id, 1, "Grandparent first");
        assert_eq!(flattened[1].id, 2, "Parent second");
        assert_eq!(flattened[2].id, 3, "Child 1 third");
        assert_eq!(flattened[3].id, 4, "Child 2 fourth");
    }

    // Tests for rebuild_visible_items with filters
    #[test]
    fn test_rebuild_visible_items_no_filters() {
        let config = Config::default();
        let mut app = App::new(config);

        app.work_items = vec![
            make_work_item(1, "Task 1", "Active", None),
            make_work_item(2, "Task 2", "Closed", None),
            make_work_item(3, "Task 3", "Active", None),
        ];

        app.rebuild_visible_items();

        assert_eq!(app.visible_items.len(), 3, "No filters should show all items");
    }

    #[test]
    fn test_rebuild_visible_items_filter_state() {
        let config = Config::default();
        let mut app = App::new(config);

        app.work_items = vec![
            make_work_item(1, "Task 1", "Active", None),
            make_work_item(2, "Task 2", "Closed", None),
            make_work_item(3, "Task 3", "Active", None),
        ];

        app.filter_state = Some("Active".to_string());
        app.rebuild_visible_items();

        assert_eq!(app.visible_items.len(), 2, "Should filter to Active items only");
        assert_eq!(app.visible_items[0].item.id, 1);
        assert_eq!(app.visible_items[1].item.id, 3);
    }

    #[test]
    fn test_rebuild_visible_items_filter_assignee() {
        let config = Config::default();
        let mut app = App::new(config);

        app.work_items = vec![
            make_work_item_with_assignee(1, "Task 1", "Active", "Alice"),
            make_work_item_with_assignee(2, "Task 2", "Active", "Bob"),
            make_work_item_with_assignee(3, "Task 3", "Active", "Alice"),
        ];

        app.filter_assignee = Some("Alice".to_string());
        app.rebuild_visible_items();

        assert_eq!(app.visible_items.len(), 2, "Should filter to Alice's items only");
        assert_eq!(app.visible_items[0].item.id, 1);
        assert_eq!(app.visible_items[1].item.id, 3);
    }

    #[test]
    fn test_rebuild_visible_items_search_query() {
        let config = Config::default();
        let mut app = App::new(config);

        app.work_items = vec![
            make_work_item(1, "Implement login", "Active", None),
            make_work_item(2, "Fix bug in auth", "Active", None),
            make_work_item(3, "Add tests", "Active", None),
        ];

        app.search_query = "login".to_string();
        app.rebuild_visible_items();

        assert_eq!(app.visible_items.len(), 1, "Should filter by search query");
        assert_eq!(app.visible_items[0].item.id, 1);
    }

    #[test]
    fn test_rebuild_visible_items_pinned_first() {
        let config = Config::default();
        let mut app = App::new(config);

        app.work_items = vec![
            make_work_item(1, "Alpha", "Active", None),
            make_work_item(2, "Beta", "Active", None),
            make_work_item(3, "Gamma", "Active", None),
        ];

        // Pin item 3
        app.pinned_items.insert(3);
        app.rebuild_visible_items();

        assert_eq!(app.visible_items.len(), 3);
        assert_eq!(app.visible_items[0].item.id, 3, "Pinned item should appear first");
    }

    // Tests for cicd_search_query filtering
    #[test]
    fn test_sorted_pipeline_indices_cicd_search() {
        let config = Config::default();
        let mut app = App::new(config);

        app.pipelines = vec![
            make_pipeline(1, "Frontend Build"),
            make_pipeline(2, "Backend Build"),
            make_pipeline(3, "Frontend Deploy"),
        ];

        app.cicd_search_query = "frontend".to_string();
        let sorted = app.sorted_pipeline_indices();

        assert_eq!(sorted.len(), 2, "Should filter to pipelines matching 'frontend'");
        assert!(app.pipelines[sorted[0]].name.to_lowercase().contains("frontend"));
        assert!(app.pipelines[sorted[1]].name.to_lowercase().contains("frontend"));
    }

    #[test]
    fn test_sorted_release_indices_cicd_search() {
        let config = Config::default();
        let mut app = App::new(config);

        app.releases = vec![
            make_release_def(1, "Production Release"),
            make_release_def(2, "Staging Release"),
            make_release_def(3, "Production Hotfix"),
        ];

        // Set focus to Releases for search to be applied
        app.cicd_focus = CICDFocus::Releases;
        app.cicd_search_query = "production".to_string();
        let sorted = app.sorted_release_indices();

        assert_eq!(sorted.len(), 2, "Should filter to releases matching 'production'");
        assert!(app.releases[sorted[0]].name.to_lowercase().contains("production"));
        assert!(app.releases[sorted[1]].name.to_lowercase().contains("production"));
    }

    // Tests for PreviewTab next/prev
    #[test]
    fn test_preview_tab_next() {
        assert_eq!(PreviewTab::Details.next(), PreviewTab::References);
        assert_eq!(PreviewTab::References.next(), PreviewTab::Details);
    }

    #[test]
    fn test_preview_tab_prev() {
        assert_eq!(PreviewTab::Details.prev(), PreviewTab::References);
        assert_eq!(PreviewTab::References.prev(), PreviewTab::Details);
    }

    // Tests for ConfirmActionDialog description
    #[test]
    fn test_confirm_action_dialog_cancel_pipeline() {
        let dialog = ConfirmActionDialog::new(ConfirmActionType::CancelPipelineRun {
            run_id: 123,
            build_number: "20240101.1".to_string(),
        });

        let desc = dialog.description();
        assert!(desc.contains("Cancel"), "Should mention cancel");
        assert!(desc.contains("20240101.1"), "Should include build number");
    }

    #[test]
    fn test_confirm_action_dialog_retrigger_pipeline() {
        let dialog = ConfirmActionDialog::new(ConfirmActionType::RetriggerPipelineRun {
            pipeline_id: 456,
            branch: "main".to_string(),
            build_number: "20240101.2".to_string(),
        });

        let desc = dialog.description();
        assert!(desc.contains("Retrigger") || desc.contains("Re-run"), "Should mention retrigger");
        assert!(desc.contains("20240101.2"), "Should include build number");
    }

    #[test]
    fn test_confirm_action_dialog_cancel_release() {
        let dialog = ConfirmActionDialog::new(ConfirmActionType::CancelRelease {
            release_id: 789,
            release_name: "Release-42".to_string(),
        });

        let desc = dialog.description();
        assert!(desc.contains("Abandon"), "Should mention abandon");
        assert!(desc.contains("Release-42"), "Should include release name");
    }

    #[test]
    fn test_confirm_action_dialog_cancel_release_environment() {
        let dialog = ConfirmActionDialog::new(ConfirmActionType::CancelReleaseEnvironment {
            release_id: 100,
            environment_id: 5,
            release_name: "Release-10".to_string(),
            environment_name: "Production".to_string(),
        });

        let desc = dialog.description();
        assert!(desc.contains("Cancel"), "Should mention cancel");
        assert!(desc.contains("Production"), "Should include environment name");
        assert!(desc.contains("Release-10"), "Should include release name");
    }

    #[test]
    fn test_confirm_action_dialog_retrigger_release_environment() {
        let dialog = ConfirmActionDialog::new(ConfirmActionType::RetriggerReleaseEnvironment {
            release_id: 200,
            environment_id: 10,
            release_name: "Release-20".to_string(),
            environment_name: "Staging".to_string(),
        });

        let desc = dialog.description();
        assert!(desc.contains("Redeploy") || desc.contains("Retrigger"), "Should mention redeploy/retrigger");
        assert!(desc.contains("Staging"), "Should include environment name");
        assert!(desc.contains("Release-20"), "Should include release name");
    }

    #[test]
    fn test_confirm_action_dialog_reject_approval() {
        let dialog = ConfirmActionDialog::new(ConfirmActionType::RejectApproval {
            approval_id: 999,
            release_id: 300,
            environment_name: "Production".to_string(),
        });

        let desc = dialog.description();
        assert!(desc.contains("Reject"), "Should mention reject");
        assert!(desc.contains("Production"), "Should include environment name");
    }
}

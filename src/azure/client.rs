use crate::azure::types::*;
use crate::config::ProjectConfig;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

pub struct AzureCli {
    pub organization: String,
    pub project: String,
    pub team: String,
    pub timeout_secs: u64,
}

impl AzureCli {
    pub fn new(config: &ProjectConfig) -> Self {
        Self {
            organization: config.organization.clone(),
            project: config.project.clone(),
            team: config.team.clone(),
            timeout_secs: 30, // Default timeout
        }
    }

    #[allow(dead_code)] // Public API for custom timeout configuration
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Execute az command and parse JSON output
    async fn exec<T: serde::de::DeserializeOwned>(&self, args: &[&str]) -> Result<T> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let future = Command::new("az")
            .args(args)
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--output", "json"])
            .output();

        let output = tokio::time::timeout(timeout, future)
            .await
            .context("Azure CLI request timed out")?
            .context("Failed to execute az CLI - is Azure CLI installed?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Provide clearer error messages for common issues
            if stderr.contains("login") || stderr.contains("az login") {
                anyhow::bail!("Azure CLI not authenticated. Run: az login");
            }
            if stderr.contains("not found") || stderr.contains("does not exist") {
                anyhow::bail!("Project/team not found. Check config.toml settings.");
            }
            let trimmed = stderr.trim();
            anyhow::bail!("az command failed: {trimmed}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(&stdout).context("Failed to parse az output")
    }

    /// Get iterations (sprints) for the team
    pub async fn get_sprints(&self) -> Result<Vec<Sprint>> {
        self.exec(&["boards", "iteration", "team", "list", "--team", &self.team])
            .await
    }

    /// Get work items for a sprint iteration path
    pub async fn get_sprint_work_items(&self, iteration_path: &str) -> Result<Vec<WorkItem>> {
        let wiql = format!(
            r#"SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType], [System.AssignedTo], [System.Parent], [System.Description], [System.IterationPath], [System.Tags], [Microsoft.VSTS.Scheduling.RemainingWork], [Microsoft.VSTS.Scheduling.OriginalEstimate], [Microsoft.VSTS.Scheduling.CompletedWork] FROM WorkItems WHERE [System.IterationPath] = '{iteration_path}' ORDER BY [System.WorkItemType], [System.Title]"#
        );

        let items = self.query_work_items(&wiql).await?;
        Ok(Self::build_hierarchy(items))
    }

    /// Execute az command WITHOUT project arg (for work-item show which doesn't accept it)
    #[allow(dead_code)] // Used by test_commands binary
    async fn exec_no_project<T: serde::de::DeserializeOwned>(&self, args: &[&str]) -> Result<T> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let future = Command::new("az")
            .args(args)
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output();

        let output = tokio::time::timeout(timeout, future)
            .await
            .context("Azure CLI request timed out")?
            .context("Failed to execute az CLI - is Azure CLI installed?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("login") || stderr.contains("az login") {
                anyhow::bail!("Azure CLI not authenticated. Run: az login");
            }
            let trimmed = stderr.trim();
            anyhow::bail!("az command failed: {trimmed}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(&stdout).context("Failed to parse az output")
    }

    /// Get single work item by ID (with relations)
    #[allow(dead_code)]
    pub async fn get_work_item(&self, id: i32) -> Result<WorkItem> {
        self.exec_no_project(&[
            "boards",
            "work-item",
            "show",
            "--id",
            &id.to_string(),
            "--expand",
            "relations",
        ])
        .await
    }

    /// Query work items by WIQL - returns full work items directly
    async fn query_work_items(&self, wiql: &str) -> Result<Vec<WorkItem>> {
        // WIQL query returns work items with all requested fields directly
        let items: Vec<WorkItem> = self.exec(&["boards", "query", "--wiql", wiql]).await?;

        Ok(items)
    }

    /// Update a work item field
    pub async fn update_work_item(&self, id: i32, field: &str, value: &str) -> Result<WorkItem> {
        let field_arg = match field {
            "state" => "--state",
            "title" => "--title",
            "assigned-to" => "--assigned-to",
            _ => anyhow::bail!("Unknown field: {field}"),
        };

        // work-item update doesn't accept --project
        self.exec_no_project(&[
            "boards",
            "work-item",
            "update",
            "--id",
            &id.to_string(),
            field_arg,
            value,
        ])
        .await
    }

    /// Get team members (kept for API compatibility but users are extracted from work items)
    #[allow(dead_code)]
    pub async fn get_team_members(&self) -> Result<Vec<User>> {
        Ok(vec![]) // Users are now extracted from work items in App
    }

    /// Get current user email from az account
    pub async fn get_current_user() -> Result<String> {
        #[derive(serde::Deserialize)]
        struct Account {
            user: AccountUser,
        }
        #[derive(serde::Deserialize)]
        struct AccountUser {
            name: String,
        }

        let timeout = Duration::from_secs(10);
        let future = Command::new("az")
            .args(["account", "show", "--output", "json"])
            .output();

        let output = tokio::time::timeout(timeout, future)
            .await
            .context("Azure CLI request timed out")?
            .context("Failed to get current user")?;

        let account: Account = serde_json::from_slice(&output.stdout)?;
        Ok(account.user.name)
    }

    /// Build parent-child hierarchy from flat list (preserves original order from WIQL)
    pub fn build_hierarchy(items: Vec<WorkItem>) -> Vec<WorkItem> {
        // Track original order from WIQL response (StackRank ordering)
        let order_map: HashMap<i32, usize> = items
            .iter()
            .enumerate()
            .map(|(idx, item)| (item.id, idx))
            .collect();

        let mut by_id: HashMap<i32, WorkItem> = items.into_iter().map(|i| (i.id, i)).collect();
        let mut children_map: HashMap<i32, Vec<i32>> = HashMap::new();
        let mut root_ids: Vec<i32> = Vec::new();

        for item in by_id.values() {
            if let Some(parent_id) = item.fields.parent_id {
                if by_id.contains_key(&parent_id) {
                    children_map.entry(parent_id).or_default().push(item.id);
                } else {
                    root_ids.push(item.id);
                }
            } else {
                root_ids.push(item.id);
            }
        }

        // Sort by original WIQL order (StackRank)
        root_ids.sort_by_key(|id| order_map.get(id).copied().unwrap_or(usize::MAX));

        // Sort children by original order as well
        for children in children_map.values_mut() {
            children.sort_by_key(|id| order_map.get(id).copied().unwrap_or(usize::MAX));
        }

        fn build_tree(
            id: i32,
            by_id: &mut HashMap<i32, WorkItem>,
            children_map: &HashMap<i32, Vec<i32>>,
            depth: usize,
        ) -> Option<WorkItem> {
            let mut item = by_id.remove(&id)?;
            item.depth = depth;
            if let Some(child_ids) = children_map.get(&id) {
                for &child_id in child_ids {
                    if let Some(child) = build_tree(child_id, by_id, children_map, depth + 1) {
                        item.children.push(child);
                    }
                }
            }
            Some(item)
        }

        root_ids
            .iter()
            .filter_map(|&id| build_tree(id, &mut by_id, &children_map, 0))
            .collect()
    }

    /// List all pipeline definitions
    #[allow(dead_code)]
    pub async fn list_pipelines(&self) -> Result<Vec<Pipeline>> {
        let output = Command::new("az")
            .args(["pipelines", "list"])
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--output", "json"])
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to list pipelines: {err}");
        }

        let pipelines: Vec<Pipeline> = serde_json::from_slice(&output.stdout)?;
        Ok(pipelines)
    }

    /// List runs for a specific pipeline
    #[allow(dead_code)]
    pub async fn list_pipeline_runs(&self, pipeline_id: i32) -> Result<Vec<PipelineRun>> {
        let output = Command::new("az")
            .args(["pipelines", "runs", "list"])
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--pipeline-ids", &pipeline_id.to_string()])
            .args(["--output", "json"])
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to list pipeline runs: {err}");
        }

        let runs: Vec<PipelineRun> = serde_json::from_slice(&output.stdout)?;
        Ok(runs)
    }

    /// Trigger a pipeline run
    #[allow(dead_code)]
    pub async fn trigger_pipeline(&self, pipeline_id: i32, branch: &str) -> Result<PipelineRun> {
        let output = Command::new("az")
            .args(["pipelines", "run"])
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--id", &pipeline_id.to_string()])
            .args(["--branch", branch])
            .args(["--output", "json"])
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to trigger pipeline: {err}");
        }

        let run: PipelineRun = serde_json::from_slice(&output.stdout)?;
        Ok(run)
    }

    /// List all release definitions
    #[allow(dead_code)]
    pub async fn list_release_definitions(&self) -> Result<Vec<ReleaseDefinition>> {
        let output = Command::new("az")
            .args(["pipelines", "release", "definition", "list"])
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--output", "json"])
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to list release definitions: {err}");
        }

        let definitions: Vec<ReleaseDefinition> = serde_json::from_slice(&output.stdout)?;
        // Filter out deleted/disabled
        Ok(definitions
            .into_iter()
            .filter(|d| !d.is_deleted && !d.is_disabled)
            .collect())
    }

    /// List releases (optionally filtered by definition)
    #[allow(dead_code)]
    pub async fn list_releases(&self, definition_id: Option<i32>) -> Result<Vec<Release>> {
        let mut cmd = Command::new("az");
        cmd.args(["pipelines", "release", "list"])
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--output", "json"]);

        if let Some(def_id) = definition_id {
            cmd.args(["--definition-id", &def_id.to_string()]);
        }

        let output = cmd.output().await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to list releases: {err}");
        }

        let releases: Vec<Release> = serde_json::from_slice(&output.stdout)?;
        Ok(releases)
    }

    /// Get release details (includes environments)
    #[allow(dead_code)]
    pub async fn get_release(&self, release_id: i32) -> Result<Release> {
        let output = Command::new("az")
            .args(["pipelines", "release", "show"])
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--id", &release_id.to_string()])
            .args(["--output", "json"])
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to get release: {err}");
        }

        let release: Release = serde_json::from_slice(&output.stdout)?;
        Ok(release)
    }

    /// Get build timeline (jobs, tasks, stages)
    #[allow(dead_code)]
    pub async fn get_build_timeline(&self, build_id: i32) -> Result<Vec<TimelineRecord>> {
        let output = Command::new("az")
            .args(["devops", "invoke"])
            .args(["--area", "build"])
            .args(["--resource", "timeline"])
            .args([
                "--route-parameters",
                &format!("project={}", self.project),
                &format!("buildId={build_id}"),
            ])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to get timeline: {err}");
        }

        let response: TimelineResponse = serde_json::from_slice(&output.stdout)?;
        Ok(response.records)
    }

    /// Get build timeline with optional changeId for efficient delta updates
    /// Returns None if no changes since last changeId (very lightweight call)
    #[allow(dead_code)]
    pub async fn get_build_timeline_delta(
        &self,
        build_id: i32,
        last_change_id: Option<i32>,
    ) -> Result<Option<(Vec<TimelineRecord>, Option<i32>)>> {
        let mut cmd = Command::new("az");
        cmd.args(["devops", "invoke"])
            .args(["--area", "build"])
            .args(["--resource", "timeline"])
            .args([
                "--route-parameters",
                &format!("project={}", self.project),
                &format!("buildId={build_id}"),
            ]);

        // Add changeId for delta polling - returns empty if no changes
        if let Some(change_id) = last_change_id {
            cmd.args(["--query-parameters", &format!("changeId={change_id}")]);
        }

        cmd.args(["--org", &self.organization])
            .args(["--output", "json"]);

        let output = cmd.output().await.context("Failed to get timeline delta")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to get timeline: {stderr}");
        }

        let response: TimelineResponse =
            serde_json::from_slice(&output.stdout).context("Failed to parse timeline response")?;

        // If no records returned and we had a changeId, nothing changed
        if response.records.is_empty() && last_change_id.is_some() {
            return Ok(None);
        }

        Ok(Some((response.records, response.change_id)))
    }

    /// Get build log content
    #[allow(dead_code)]
    pub async fn get_build_log(&self, build_id: i32, log_id: i32) -> Result<Vec<String>> {
        let output = Command::new("az")
            .args(["devops", "invoke"])
            .args(["--area", "build"])
            .args(["--resource", "logs"])
            .args([
                "--route-parameters",
                &format!("project={}", self.project),
                &format!("buildId={build_id}"),
                &format!("logId={log_id}"),
            ])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to get log: {err}");
        }

        let response: BuildLogResponse = serde_json::from_slice(&output.stdout)?;
        Ok(response.value)
    }

    /// Get pending approvals for the current user
    #[allow(dead_code)]
    pub async fn get_pending_approvals(&self) -> Result<Vec<Approval>> {
        let output = Command::new("az")
            .args(["devops", "invoke"])
            .args(["--area", "release"])
            .args(["--resource", "approvals"])
            .args(["--route-parameters", &format!("project={}", self.project)])
            .args([
                "--query-parameters",
                "statusFilter=pending",
                "includeMyGroupApprovals=true",
            ])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output()
            .await
            .context("Failed to execute az devops invoke for approvals")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to get pending approvals: {stderr}");
        }

        let response: ApprovalsResponse =
            serde_json::from_slice(&output.stdout).context("Failed to parse approvals response")?;
        Ok(response.value)
    }

    /// Approve or reject a release approval
    #[allow(dead_code)]
    pub async fn update_approval(
        &self,
        approval_id: i32,
        status: &str,
        comments: Option<&str>,
    ) -> Result<Approval> {
        // Azure DevOps Approvals API expects an array of approval objects with id included
        let body = serde_json::json!([{
            "id": approval_id,
            "status": status,
            "comments": comments.unwrap_or("")
        }]);
        let body_str = serde_json::to_string(&body)?;

        // Write body to temp file since az devops invoke needs --in-file
        let temp_path = std::env::temp_dir().join(format!("approval_{approval_id}.json"));
        tokio::fs::write(&temp_path, &body_str).await?;

        // Use bulk approvals endpoint (no approvalId in route) with array body
        let output = Command::new("az")
            .args(["devops", "invoke"])
            .args(["--area", "release"])
            .args(["--resource", "approvals"])
            .args(["--route-parameters", &format!("project={}", self.project)])
            .args(["--http-method", "PATCH"])
            .args(["--api-version", "7.1"])
            .args(["--in-file", temp_path.to_str().unwrap()])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output()
            .await
            .context("Failed to execute az devops invoke for approval update")?;

        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_path).await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to update approval: {stderr}");
        }

        // Response is an array, extract first element
        let approvals: Vec<Approval> =
            serde_json::from_slice(&output.stdout).context("Failed to parse approval response")?;
        approvals
            .into_iter()
            .next()
            .context("No approval returned in response")
    }

    /// Get release definition details (for trigger dialog)
    #[allow(dead_code)]
    pub async fn get_release_definition_detail(
        &self,
        definition_id: i32,
    ) -> Result<ReleaseDefinitionDetail> {
        let output = Command::new("az")
            .args(["devops", "invoke"])
            .args(["--area", "release"])
            .args(["--resource", "definitions"])
            .args([
                "--route-parameters",
                &format!("project={}", self.project),
                &format!("definitionId={definition_id}"),
            ])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output()
            .await
            .context("Failed to get release definition detail")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to get release definition: {stderr}");
        }

        let detail: ReleaseDefinitionDetail = serde_json::from_slice(&output.stdout)
            .context("Failed to parse release definition detail")?;
        Ok(detail)
    }

    /// Create a new release
    #[allow(dead_code)]
    pub async fn create_release(
        &self,
        definition_id: i32,
        description: Option<&str>,
    ) -> Result<Release> {
        let mut cmd = Command::new("az");
        cmd.args(["pipelines", "release", "create"])
            .args(["--definition-id", &definition_id.to_string()])
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--output", "json"]);

        if let Some(desc) = description {
            cmd.args(["--description", desc]);
        }

        let output = cmd.output().await.context("Failed to create release")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to create release: {stderr}");
        }

        let release: Release =
            serde_json::from_slice(&output.stdout).context("Failed to parse created release")?;
        Ok(release)
    }

    /// Cancel a running pipeline build
    #[allow(dead_code)]
    pub async fn cancel_pipeline_run(&self, run_id: i32) -> Result<()> {
        let output = Command::new("az")
            .args(["pipelines", "build", "update"])
            .args(["--id", &run_id.to_string()])
            .args(["--status", "cancelling"])
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--output", "json"])
            .output()
            .await
            .context("Failed to cancel pipeline run")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to cancel pipeline run: {stderr}");
        }

        Ok(())
    }

    /// Retrigger a pipeline on the same branch (creates new run)
    #[allow(dead_code)]
    pub async fn retrigger_pipeline_run(
        &self,
        pipeline_id: i32,
        branch: &str,
    ) -> Result<PipelineRun> {
        // Reuse the existing trigger_pipeline method
        self.trigger_pipeline(pipeline_id, branch).await
    }

    /// Abandon/cancel a release
    #[allow(dead_code)]
    pub async fn cancel_release(&self, release_id: i32) -> Result<()> {
        let body = serde_json::json!({
            "status": "abandoned"
        });
        let body_str = serde_json::to_string(&body)?;

        // Write body to temp file since az devops invoke needs --in-file
        let temp_path = std::env::temp_dir().join(format!("release_cancel_{release_id}.json"));
        tokio::fs::write(&temp_path, &body_str).await?;

        let output = Command::new("az")
            .args(["devops", "invoke"])
            .args(["--area", "release"])
            .args(["--resource", "releases"])
            .args([
                "--route-parameters",
                &format!("project={}", self.project),
                &format!("releaseId={release_id}"),
            ])
            .args(["--http-method", "PATCH"])
            .args(["--in-file", temp_path.to_str().unwrap()])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output()
            .await
            .context("Failed to cancel release")?;

        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_path).await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to cancel release: {stderr}");
        }

        Ok(())
    }

    /// Cancel a specific release environment/stage
    #[allow(dead_code)]
    pub async fn cancel_release_environment(
        &self,
        release_id: i32,
        environment_id: i32,
    ) -> Result<()> {
        let body = serde_json::json!({
            "status": "canceled",
            "comment": "Canceled from lazyops"
        });
        let body_str = serde_json::to_string(&body)?;

        // Write body to temp file since az devops invoke needs --in-file
        let temp_path =
            std::env::temp_dir().join(format!("env_cancel_{release_id}_{environment_id}.json"));
        tokio::fs::write(&temp_path, &body_str).await?;

        let output = Command::new("az")
            .args(["devops", "invoke"])
            .args(["--area", "release"])
            .args(["--resource", "releases/environments"])
            .args([
                "--route-parameters",
                &format!("project={}", self.project),
                &format!("releaseId={release_id}"),
                &format!("environmentId={environment_id}"),
            ])
            .args(["--http-method", "PATCH"])
            .args(["--in-file", temp_path.to_str().unwrap()])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output()
            .await
            .context("Failed to cancel release environment")?;

        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_path).await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to cancel release environment: {stderr}");
        }

        Ok(())
    }

    /// Redeploy/retrigger a specific release environment/stage
    #[allow(dead_code)]
    pub async fn redeploy_release_environment(
        &self,
        release_id: i32,
        environment_id: i32,
    ) -> Result<()> {
        let body = serde_json::json!({
            "status": "inProgress",
            "comment": "Redeployed from lazyops"
        });
        let body_str = serde_json::to_string(&body)?;

        // Write body to temp file since az devops invoke needs --in-file
        let temp_path =
            std::env::temp_dir().join(format!("env_redeploy_{release_id}_{environment_id}.json"));
        tokio::fs::write(&temp_path, &body_str).await?;

        let output = Command::new("az")
            .args(["devops", "invoke"])
            .args(["--area", "release"])
            .args(["--resource", "releases/environments"])
            .args([
                "--route-parameters",
                &format!("project={}", self.project),
                &format!("releaseId={release_id}"),
                &format!("environmentId={environment_id}"),
            ])
            .args(["--http-method", "PATCH"])
            .args(["--in-file", temp_path.to_str().unwrap()])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output()
            .await
            .context("Failed to redeploy release environment")?;

        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_path).await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to redeploy release environment: {stderr}");
        }

        Ok(())
    }

    /// List all repositories
    #[allow(dead_code)]
    pub async fn list_repositories(&self) -> Result<Vec<Repository>> {
        self.exec(&["repos", "list"]).await
    }

    /// List pull requests with optional filters
    #[allow(dead_code)]
    pub async fn list_pull_requests(
        &self,
        repository: Option<&str>,
        status: &str,
        creator: Option<&str>,
        top: Option<i32>,
    ) -> Result<Vec<PullRequest>> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let mut cmd = Command::new("az");
        cmd.args(["repos", "pr", "list"])
            .args(["--status", status])
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--output", "json"]);

        if let Some(repo) = repository {
            cmd.args(["--repository", repo]);
        }

        if let Some(user) = creator {
            cmd.args(["--creator", user]);
        }

        if let Some(limit) = top {
            cmd.args(["--top", &limit.to_string()]);
        }

        let future = cmd.output();
        let output = tokio::time::timeout(timeout, future)
            .await
            .context("Azure CLI request timed out")?
            .context("Failed to list pull requests")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to list pull requests: {stderr}");
        }

        let prs: Vec<PullRequest> = serde_json::from_slice(&output.stdout)?;
        Ok(prs)
    }

    /// Get a single pull request by ID
    #[allow(dead_code)]
    pub async fn get_pull_request(&self, id: i32) -> Result<PullRequest> {
        self.exec_no_project(&["repos", "pr", "show", "--id", &id.to_string()])
            .await
    }

    /// List threads (comments) on a pull request
    #[allow(dead_code)]
    pub async fn list_pr_threads(&self, repository_id: &str, pr_id: i32) -> Result<Vec<PRThread>> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let future = Command::new("az")
            .args(["devops", "invoke"])
            .args(["--area", "git"])
            .args(["--resource", "pullRequestThreads"])
            .args([
                "--route-parameters",
                &format!("project={}", self.project),
                &format!("repositoryId={repository_id}"),
                &format!("pullRequestId={pr_id}"),
            ])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output();

        let output = tokio::time::timeout(timeout, future)
            .await
            .context("Azure CLI request timed out")?
            .context("Failed to list PR threads")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to list PR threads: {stderr}");
        }

        let response: PRThreadsResponse = serde_json::from_slice(&output.stdout)?;
        Ok(response.value)
    }

    /// List policies evaluated on a pull request
    #[allow(dead_code)]
    pub async fn list_pr_policies(&self, pr_id: i32) -> Result<Vec<PRPolicy>> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let future = Command::new("az")
            .args(["repos", "pr", "policy", "list"])
            .args(["--id", &pr_id.to_string()])
            .args(["--org", &self.organization])
            .args(["--detect", "false"])
            .args(["--output", "json"])
            .output();

        let output = tokio::time::timeout(timeout, future)
            .await
            .context("Azure CLI request timed out")?
            .context("Failed to list PR policies")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to list PR policies: {stderr}");
        }

        let policies: Vec<PRPolicy> = serde_json::from_slice(&output.stdout)?;
        Ok(policies)
    }

    /// Set vote on a pull request
    /// Votes: "approve", "approve-with-suggestions", "reject", "reset", "wait-for-author"
    #[allow(dead_code)]
    pub async fn set_pr_vote(&self, pr_id: i32, vote: &str) -> Result<()> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let future = Command::new("az")
            .args(["repos", "pr", "set-vote"])
            .args(["--id", &pr_id.to_string()])
            .args(["--vote", vote])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output();

        let output = tokio::time::timeout(timeout, future)
            .await
            .context("Azure CLI request timed out")?
            .context("Failed to set PR vote")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to set PR vote: {stderr}");
        }

        Ok(())
    }

    /// Update a pull request (status, title, description, draft)
    #[allow(dead_code)]
    pub async fn update_pr(
        &self,
        pr_id: i32,
        status: Option<&str>,
        title: Option<&str>,
        description: Option<&str>,
        draft: Option<bool>,
    ) -> Result<PullRequest> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let mut cmd = Command::new("az");
        cmd.args(["repos", "pr", "update"])
            .args(["--id", &pr_id.to_string()])
            .args(["--org", &self.organization])
            .args(["--output", "json"]);

        if let Some(s) = status {
            cmd.args(["--status", s]);
        }

        if let Some(t) = title {
            cmd.args(["--title", t]);
        }

        if let Some(d) = description {
            cmd.args(["--description", d]);
        }

        if let Some(is_draft) = draft {
            cmd.args(["--draft", if is_draft { "true" } else { "false" }]);
        }

        let future = cmd.output();
        let output = tokio::time::timeout(timeout, future)
            .await
            .context("Azure CLI request timed out")?
            .context("Failed to update PR")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to update PR: {stderr}");
        }

        let pr: PullRequest = serde_json::from_slice(&output.stdout)?;
        Ok(pr)
    }

    /// Create a new pull request
    #[allow(dead_code)]
    pub async fn create_pr(
        &self,
        repository: &str,
        source_branch: &str,
        target_branch: &str,
        title: &str,
        description: Option<&str>,
        draft: bool,
    ) -> Result<PullRequest> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let mut cmd = Command::new("az");
        cmd.args(["repos", "pr", "create"])
            .args(["--repository", repository])
            .args(["--source-branch", source_branch])
            .args(["--target-branch", target_branch])
            .args(["--title", title])
            .args(["--draft", if draft { "true" } else { "false" }])
            .args(["--org", &self.organization])
            .args(["--project", &self.project])
            .args(["--output", "json"]);

        if let Some(desc) = description {
            cmd.args(["--description", desc]);
        }

        let future = cmd.output();
        let output = tokio::time::timeout(timeout, future)
            .await
            .context("Azure CLI request timed out")?
            .context("Failed to create PR")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to create PR: {stderr}");
        }

        let pr: PullRequest = serde_json::from_slice(&output.stdout)?;
        Ok(pr)
    }

    /// Add a comment to a pull request (creates a new thread)
    #[allow(dead_code)]
    pub async fn add_pr_comment(
        &self,
        repository_id: &str,
        pr_id: i32,
        content: &str,
    ) -> Result<()> {
        let body = serde_json::json!({
            "comments": [{
                "content": content,
                "parentCommentId": 0,
                "commentType": 1
            }],
            "status": 1
        });
        let body_str = serde_json::to_string(&body)?;

        // Write body to temp file since az devops invoke needs --in-file
        let temp_path =
            std::env::temp_dir().join(format!("pr_comment_{}_{}.json", pr_id, std::process::id()));
        tokio::fs::write(&temp_path, &body_str).await?;

        let output = Command::new("az")
            .args(["devops", "invoke"])
            .args(["--area", "git"])
            .args(["--resource", "pullRequestThreads"])
            .args([
                "--route-parameters",
                &format!("project={}", self.project),
                &format!("repositoryId={repository_id}"),
                &format!("pullRequestId={pr_id}"),
            ])
            .args(["--http-method", "POST"])
            .args(["--in-file", temp_path.to_str().unwrap()])
            .args(["--api-version", "7.1"])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output()
            .await
            .context("Failed to add PR comment")?;

        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_path).await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to add PR comment: {stderr}");
        }

        Ok(())
    }

    /// List work items linked to a pull request
    #[allow(dead_code)]
    pub async fn list_pr_work_items(&self, pr_id: i32) -> Result<serde_json::Value> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let future = Command::new("az")
            .args(["repos", "pr", "work-item", "list"])
            .args(["--id", &pr_id.to_string()])
            .args(["--org", &self.organization])
            .args(["--output", "json"])
            .output();

        let output = tokio::time::timeout(timeout, future)
            .await
            .context("Azure CLI request timed out")?
            .context("Failed to list PR work items")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to list PR work items: {stderr}");
        }

        let work_items: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        Ok(work_items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_work_item(id: i32, parent_id: Option<i32>) -> WorkItem {
        WorkItem {
            id,
            rev: 1,
            fields: WorkItemFields {
                title: format!("Item {id}"),
                state: "New".to_string(),
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

    #[test]
    fn test_build_hierarchy_empty() {
        let result = AzureCli::build_hierarchy(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_build_hierarchy_single_root_item() {
        let items = vec![make_work_item(1, None)];
        let result = AzureCli::build_hierarchy(items);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
        assert_eq!(result[0].depth, 0);
        assert!(result[0].children.is_empty());
    }

    #[test]
    fn test_build_hierarchy_multiple_root_items() {
        let items = vec![
            make_work_item(1, None),
            make_work_item(2, None),
            make_work_item(3, None),
        ];
        let result = AzureCli::build_hierarchy(items);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id, 1);
        assert_eq!(result[0].depth, 0);
        assert_eq!(result[1].id, 2);
        assert_eq!(result[1].depth, 0);
        assert_eq!(result[2].id, 3);
        assert_eq!(result[2].depth, 0);

        // All should have no children
        for item in &result {
            assert!(item.children.is_empty());
        }
    }

    #[test]
    fn test_build_hierarchy_parent_child_relationship() {
        let items = vec![
            make_work_item(1, None),    // Parent
            make_work_item(2, Some(1)), // Child
        ];
        let result = AzureCli::build_hierarchy(items);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
        assert_eq!(result[0].depth, 0);
        assert_eq!(result[0].children.len(), 1);

        let child = &result[0].children[0];
        assert_eq!(child.id, 2);
        assert_eq!(child.depth, 1);
        assert!(child.children.is_empty());
    }

    #[test]
    fn test_build_hierarchy_multi_level() {
        let items = vec![
            make_work_item(1, None),    // Grandparent
            make_work_item(2, Some(1)), // Parent
            make_work_item(3, Some(2)), // Child
        ];
        let result = AzureCli::build_hierarchy(items);

        assert_eq!(result.len(), 1);

        // Grandparent level
        let grandparent = &result[0];
        assert_eq!(grandparent.id, 1);
        assert_eq!(grandparent.depth, 0);
        assert_eq!(grandparent.children.len(), 1);

        // Parent level
        let parent = &grandparent.children[0];
        assert_eq!(parent.id, 2);
        assert_eq!(parent.depth, 1);
        assert_eq!(parent.children.len(), 1);

        // Child level
        let child = &parent.children[0];
        assert_eq!(child.id, 3);
        assert_eq!(child.depth, 2);
        assert!(child.children.is_empty());
    }

    #[test]
    fn test_build_hierarchy_orphan_children() {
        // Child whose parent_id points to non-existent item should be root
        let items = vec![
            make_work_item(1, None),
            make_work_item(2, Some(999)), // Parent 999 doesn't exist
        ];
        let result = AzureCli::build_hierarchy(items);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, 1);
        assert_eq!(result[0].depth, 0);
        assert_eq!(result[1].id, 2);
        assert_eq!(result[1].depth, 0); // Orphan treated as root

        // Both should have no children
        assert!(result[0].children.is_empty());
        assert!(result[1].children.is_empty());
    }

    #[test]
    fn test_build_hierarchy_order_preservation() {
        // Items should maintain WIQL order (important for StackRank)
        let items = vec![
            make_work_item(3, None),
            make_work_item(1, None),
            make_work_item(2, None),
        ];
        let result = AzureCli::build_hierarchy(items);

        assert_eq!(result.len(), 3);
        // Order should be preserved: 3, 1, 2
        assert_eq!(result[0].id, 3);
        assert_eq!(result[1].id, 1);
        assert_eq!(result[2].id, 2);
    }

    #[test]
    fn test_build_hierarchy_children_order_preservation() {
        // Children should also maintain original order
        let items = vec![
            make_work_item(1, None),
            make_work_item(5, Some(1)), // Child added in order 5, 3, 4
            make_work_item(3, Some(1)),
            make_work_item(4, Some(1)),
        ];
        let result = AzureCli::build_hierarchy(items);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].children.len(), 3);

        // Children should appear in original order: 5, 3, 4
        assert_eq!(result[0].children[0].id, 5);
        assert_eq!(result[0].children[1].id, 3);
        assert_eq!(result[0].children[2].id, 4);
    }

    #[test]
    fn test_build_hierarchy_complex_tree() {
        // Multiple parents with multiple children
        let items = vec![
            make_work_item(1, None),    // Root 1
            make_work_item(2, Some(1)), // Child of 1
            make_work_item(3, Some(1)), // Child of 1
            make_work_item(4, None),    // Root 2
            make_work_item(5, Some(4)), // Child of 4
            make_work_item(6, Some(2)), // Grandchild of 1
        ];
        let result = AzureCli::build_hierarchy(items);

        assert_eq!(result.len(), 2);

        // First root
        let root1 = &result[0];
        assert_eq!(root1.id, 1);
        assert_eq!(root1.depth, 0);
        assert_eq!(root1.children.len(), 2);

        // Children of root1
        assert_eq!(root1.children[0].id, 2);
        assert_eq!(root1.children[0].depth, 1);
        assert_eq!(root1.children[1].id, 3);
        assert_eq!(root1.children[1].depth, 1);

        // Grandchild of root1
        assert_eq!(root1.children[0].children.len(), 1);
        assert_eq!(root1.children[0].children[0].id, 6);
        assert_eq!(root1.children[0].children[0].depth, 2);

        // Second root
        let root2 = &result[1];
        assert_eq!(root2.id, 4);
        assert_eq!(root2.depth, 0);
        assert_eq!(root2.children.len(), 1);
        assert_eq!(root2.children[0].id, 5);
        assert_eq!(root2.children[0].depth, 1);
    }

    #[test]
    fn test_build_hierarchy_mixed_orphans_and_valid() {
        // Mix of valid parent-child relationships and orphans
        let items = vec![
            make_work_item(1, None),
            make_work_item(2, Some(1)),   // Valid child
            make_work_item(3, Some(999)), // Orphan
            make_work_item(4, Some(1)),   // Valid child
        ];
        let result = AzureCli::build_hierarchy(items);

        assert_eq!(result.len(), 2);

        // First root with valid children
        assert_eq!(result[0].id, 1);
        assert_eq!(result[0].children.len(), 2);
        assert_eq!(result[0].children[0].id, 2);
        assert_eq!(result[0].children[1].id, 4);

        // Orphan as separate root
        assert_eq!(result[1].id, 3);
        assert_eq!(result[1].depth, 0);
        assert!(result[1].children.is_empty());
    }
}

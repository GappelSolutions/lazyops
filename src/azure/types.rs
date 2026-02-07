use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: i32,
    pub rev: i32,
    pub fields: WorkItemFields,
    #[serde(default)]
    pub relations: Option<Vec<WorkItemRelation>>,
    #[serde(skip)]
    pub children: Vec<WorkItem>,
    #[serde(skip)]
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemFields {
    #[serde(rename = "System.Title")]
    pub title: String,
    #[serde(rename = "System.State")]
    pub state: String,
    #[serde(rename = "System.WorkItemType")]
    pub work_item_type: String,
    #[serde(rename = "System.AssignedTo")]
    pub assigned_to: Option<AssignedTo>,
    #[serde(rename = "System.IterationPath")]
    pub iteration_path: Option<String>,
    #[serde(rename = "System.Description")]
    pub description: Option<String>,
    #[serde(rename = "System.Parent")]
    pub parent_id: Option<i32>,
    #[serde(rename = "System.CreatedDate")]
    pub created_date: Option<DateTime<Utc>>,
    #[serde(rename = "System.ChangedDate")]
    pub changed_date: Option<DateTime<Utc>>,
    #[serde(rename = "System.Tags")]
    pub tags: Option<String>,
    #[serde(rename = "Microsoft.VSTS.Scheduling.RemainingWork")]
    pub remaining_work: Option<f64>,
    #[serde(rename = "Microsoft.VSTS.Scheduling.OriginalEstimate")]
    pub original_estimate: Option<f64>,
    #[serde(rename = "Microsoft.VSTS.Scheduling.CompletedWork")]
    pub completed_work: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignedTo {
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "uniqueName")]
    pub unique_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemRelation {
    pub rel: String,
    pub url: String,
    #[serde(default)]
    pub attributes: WorkItemRelationAttributes,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkItemRelationAttributes {
    #[serde(rename = "name")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sprint {
    pub id: String,
    pub name: String,
    pub path: String,
    pub attributes: SprintAttributes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprintAttributes {
    #[serde(rename = "startDate")]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(rename = "finishDate")]
    pub finish_date: Option<DateTime<Utc>>,
    #[serde(rename = "timeFrame")]
    pub time_frame: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "uniqueName")]
    pub unique_name: String,
}

// Pipeline types
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pipeline {
    pub id: i32,
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub queue_status: Option<String>,
    #[serde(default)]
    pub revision: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineRun {
    pub id: i32,
    #[serde(default)]
    pub build_number: Option<String>,
    #[serde(default)]
    pub status: Option<String>, // completed, inProgress, notStarted
    #[serde(default)]
    pub result: Option<String>, // succeeded, failed, canceled
    #[serde(default)]
    pub source_branch: Option<String>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub finish_time: Option<String>,
    #[serde(default)]
    pub queue_time: Option<String>,
    #[serde(default)]
    pub requested_for: Option<PipelineUser>,
    #[serde(default)]
    pub definition: Option<PipelineDefinitionRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineUser {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub unique_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineDefinitionRef {
    pub id: i32,
    #[serde(default)]
    pub name: Option<String>,
}

// Release types
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDefinition {
    pub id: i32,
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub is_disabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: Option<String>, // active, abandoned
    #[serde(default)]
    pub created_on: Option<String>,
    #[serde(default)]
    pub release_definition: Option<ReleaseDefinitionRef>,
    #[serde(default)]
    pub created_by: Option<PipelineUser>,
    #[serde(default)]
    pub environments: Option<Vec<ReleaseEnvironment>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDefinitionRef {
    pub id: i32,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseEnvironment {
    pub id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: Option<String>, // notStarted, inProgress, succeeded, rejected, canceled
    #[serde(default)]
    pub deploy_steps: Vec<ReleaseDeployStep>,
    #[serde(default)]
    pub pre_deploy_approvals: Vec<ReleaseApproval>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseApproval {
    pub id: i32,
    #[serde(default)]
    pub status: Option<String>, // pending, approved, rejected
    #[serde(default)]
    pub approval_type: Option<String>, // preDeploy, postDeploy
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDeployStep {
    pub id: i32,
    #[serde(default)]
    pub attempt: i32,
    #[serde(default)]
    pub deployment_id: i32,
    #[serde(default)]
    pub release_deploy_phases: Vec<ReleaseDeployPhase>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDeployPhase {
    pub id: i32,
    #[serde(default)]
    pub phase_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub phase_type: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub deployment_jobs: Vec<ReleaseDeploymentJob>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDeploymentJob {
    #[serde(default)]
    pub job: Option<ReleaseJob>,
    #[serde(default)]
    pub tasks: Vec<ReleaseTask>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseJob {
    pub id: i32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub log_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseTask {
    pub id: i32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub status: Option<String>, // succeeded, failed, inProgress, skipped
    #[serde(default)]
    pub log_url: Option<String>,
    #[serde(default)]
    pub rank: Option<i32>,
}

/// Build timeline record (job, task, stage)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineRecord {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "type")]
    #[serde(default)]
    pub record_type: Option<String>, // Stage, Job, Task, Checkpoint
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>, // pending, inProgress, completed
    #[serde(default)]
    pub result: Option<String>, // succeeded, failed, canceled
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default)]
    pub log: Option<TimelineLog>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub finish_time: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineLog {
    pub id: i32,
    #[serde(default)]
    pub url: Option<String>,
}

/// Timeline response
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineResponse {
    #[serde(default)]
    pub records: Vec<TimelineRecord>,
    #[serde(default)]
    pub change_id: Option<i32>,
    #[serde(default)]
    pub last_changed_on: Option<String>,
}

/// Build log response
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildLogResponse {
    #[serde(default)]
    pub value: Vec<String>,
}

// Approval types for release management
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: i32,
    #[serde(default)]
    pub approval_type: Option<String>, // preDeploy, postDeploy
    #[serde(default)]
    pub status: Option<String>, // pending, approved, rejected
    #[serde(default)]
    pub created_on: Option<String>,
    #[serde(default)]
    pub release: Option<ApprovalRelease>,
    #[serde(default)]
    pub release_environment: Option<ApprovalEnvironment>,
    #[serde(default)]
    pub approver: Option<IdentityRef>,
    #[serde(default)]
    pub comments: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRelease {
    pub id: i32,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalEnvironment {
    pub id: i32,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityRef {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub unique_name: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalsResponse {
    #[serde(default)]
    pub value: Vec<Approval>,
    #[serde(default)]
    pub count: i32,
}

// Release definition detail (for trigger dialog)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDefinitionDetail {
    pub id: i32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub environments: Vec<ReleaseDefinitionEnvironment>,
    #[serde(default)]
    pub artifacts: Vec<ReleaseArtifact>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDefinitionEnvironment {
    pub id: i32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub rank: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseArtifact {
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(rename = "type", default)]
    pub artifact_type: Option<String>,
    #[serde(default)]
    pub definition_reference: Option<serde_json::Value>,
}

// Pull Request types
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    #[serde(rename = "pullRequestId")]
    pub pull_request_id: i32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, rename = "sourceRefName")]
    pub source_branch: Option<String>,
    #[serde(default, rename = "targetRefName")]
    pub target_branch: Option<String>,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub merge_status: Option<String>,
    #[serde(default)]
    pub code_review_id: Option<i32>,
    #[serde(default)]
    pub creation_date: Option<String>,
    #[serde(default)]
    pub created_by: Option<PRIdentityRef>,
    #[serde(default)]
    pub auto_complete_set_by: Option<PRIdentityRef>,
    #[serde(default)]
    pub closed_by: Option<PRIdentityRef>,
    #[serde(default)]
    pub closed_date: Option<String>,
    #[serde(default)]
    pub completion_options: Option<PRCompletionOptions>,
    #[serde(default)]
    pub repository: Option<PRRepository>,
    #[serde(default)]
    pub reviewers: Vec<PRReviewer>,
    #[serde(default)]
    pub labels: Option<Vec<PRLabel>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRIdentityRef {
    pub display_name: String,
    #[serde(default)]
    pub unique_name: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRReviewer {
    pub display_name: String,
    #[serde(default)]
    pub unique_name: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub vote: i32,
    #[serde(default)]
    pub has_declined: bool,
    #[serde(default)]
    pub is_flagged: bool,
    #[serde(default)]
    pub is_required: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRCompletionOptions {
    #[serde(default)]
    pub merge_strategy: Option<String>,
    #[serde(default)]
    pub delete_source_branch: Option<bool>,
    #[serde(default)]
    pub squash_merge: Option<bool>,
    #[serde(default)]
    pub merge_commit_message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRRepository {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub project: Option<PRProject>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRProject {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRThread {
    pub id: i32,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub published_date: Option<String>,
    #[serde(default)]
    pub last_updated_date: Option<String>,
    #[serde(default)]
    pub comments: Vec<PRComment>,
    #[serde(default)]
    pub thread_context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRComment {
    pub id: i32,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub comment_type: Option<String>,
    #[serde(default)]
    pub published_date: Option<String>,
    #[serde(default)]
    pub author: Option<PRIdentityRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRPolicy {
    #[serde(default)]
    pub evaluation_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub configuration: Option<PRPolicyConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRPolicyConfig {
    #[serde(default)]
    pub is_blocking: bool,
    #[serde(default)]
    pub is_enabled: bool,
    #[serde(rename = "type", default)]
    pub policy_type: Option<PRPolicyType>,
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRPolicyType {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRThreadsResponse {
    #[serde(default)]
    pub value: Vec<PRThread>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PRLabel {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Result of a PR action (vote, comment, etc.)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum PRActionResult {
    Voted { pr_id: i32, vote: String },
    Updated { pr: PullRequest },
    Created { pr: PullRequest },
    Commented { pr_id: i32 },
    ActionError { message: String },
}

impl PullRequest {
    /// Vote icons for PR reviewers
    pub fn vote_icon(vote: i32) -> &'static str {
        match vote {
            10 => "✓",  // Approved
            5 => "✓~",  // Approved with suggestions
            0 => "○",   // No vote
            -5 => "⏳", // Waiting for author
            -10 => "✗", // Rejected
            _ => "?",
        }
    }

    /// Status icons for PR status
    pub fn status_icon(&self) -> &'static str {
        if self.is_draft {
            return "◑"; // Draft
        }
        match self.status.as_deref() {
            Some("active") => "●",    // Active
            Some("completed") => "✓", // Completed
            Some("abandoned") => "✗", // Abandoned
            _ => "?",
        }
    }

    /// Strip "refs/heads/" prefix from branch names
    pub fn short_branch(branch: &str) -> &str {
        branch.strip_prefix("refs/heads/").unwrap_or(branch)
    }
}

impl WorkItem {
    /// State icons - consistent progression from empty to filled to complete
    pub fn state_icon(&self) -> &'static str {
        match self.fields.state.as_str() {
            "New" => "○",               // Empty circle - not started
            "In Progress" => "◐",       // Half circle - working on it
            "Done In Stage" => "●",     // Filled - staged
            "Done Not Released" => "●", // Filled - not released
            "Done" => "●",              // Filled circle - complete
            "Tested w/Bugs" => "●",     // Red dot - has bugs
            "Removed" => "○",           // Empty (removed/cancelled)
            _ => "○",
        }
    }

    pub fn type_icon(&self) -> &'static str {
        match self.fields.work_item_type.as_str() {
            "Bug" => "⊗",
            "User Story" => "◈",
            "Task" => "☑",
            "Feature" => "★",
            "Epic" => "⚡",
            "Issue" => "⚠",
            "Test Case" => "◇",
            "Product Backlog Item" => "▣",
            _ => "•",
        }
    }

    pub fn available_states(&self) -> Vec<&'static str> {
        match self.fields.work_item_type.as_str() {
            "Task" => vec!["To Do", "In Progress", "Done", "Removed"],
            "Bug" | "Product Backlog Item" | "User Story" => vec![
                "New",
                "In Progress",
                "Done In Stage",
                "Done Not Released",
                "Done",
                "Tested w/Bugs",
            ],
            _ => vec![
                "New",
                "In Progress",
                "Done In Stage",
                "Done Not Released",
                "Done",
            ],
        }
    }
}

/// Result of a CI/CD action (cancel, retrigger, etc.)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CICDActionResult {
    PipelineRunCanceled {
        run_id: i32,
    },
    PipelineRunRetriggered {
        run: Box<PipelineRun>,
    },
    ReleaseAbandoned {
        release_id: i32,
    },
    ReleaseEnvironmentCanceled {
        release_id: i32,
        environment_name: String,
    },
    ReleaseEnvironmentRedeployed {
        release_id: i32,
        environment_name: String,
    },
    ActionError {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_work_item(state: &str, work_item_type: &str) -> WorkItem {
        WorkItem {
            id: 1,
            rev: 1,
            fields: WorkItemFields {
                title: "Test".to_string(),
                state: state.to_string(),
                work_item_type: work_item_type.to_string(),
                assigned_to: None,
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

    // state_icon tests
    #[test]
    fn test_state_icon_new() {
        let item = make_work_item("New", "Task");
        assert_eq!(item.state_icon(), "○");
    }

    #[test]
    fn test_state_icon_in_progress() {
        let item = make_work_item("In Progress", "Task");
        assert_eq!(item.state_icon(), "◐");
    }

    #[test]
    fn test_state_icon_done_in_stage() {
        let item = make_work_item("Done In Stage", "Task");
        assert_eq!(item.state_icon(), "●");
    }

    #[test]
    fn test_state_icon_done_not_released() {
        let item = make_work_item("Done Not Released", "Task");
        assert_eq!(item.state_icon(), "●");
    }

    #[test]
    fn test_state_icon_done() {
        let item = make_work_item("Done", "Task");
        assert_eq!(item.state_icon(), "●");
    }

    #[test]
    fn test_state_icon_tested_with_bugs() {
        let item = make_work_item("Tested w/Bugs", "Task");
        assert_eq!(item.state_icon(), "●");
    }

    #[test]
    fn test_state_icon_removed() {
        let item = make_work_item("Removed", "Task");
        assert_eq!(item.state_icon(), "○");
    }

    #[test]
    fn test_state_icon_unknown() {
        let item = make_work_item("Unknown State", "Task");
        assert_eq!(item.state_icon(), "○");
    }

    // type_icon tests
    #[test]
    fn test_type_icon_bug() {
        let item = make_work_item("New", "Bug");
        assert_eq!(item.type_icon(), "⊗");
    }

    #[test]
    fn test_type_icon_user_story() {
        let item = make_work_item("New", "User Story");
        assert_eq!(item.type_icon(), "◈");
    }

    #[test]
    fn test_type_icon_task() {
        let item = make_work_item("New", "Task");
        assert_eq!(item.type_icon(), "☑");
    }

    #[test]
    fn test_type_icon_feature() {
        let item = make_work_item("New", "Feature");
        assert_eq!(item.type_icon(), "★");
    }

    #[test]
    fn test_type_icon_epic() {
        let item = make_work_item("New", "Epic");
        assert_eq!(item.type_icon(), "⚡");
    }

    #[test]
    fn test_type_icon_issue() {
        let item = make_work_item("New", "Issue");
        assert_eq!(item.type_icon(), "⚠");
    }

    #[test]
    fn test_type_icon_test_case() {
        let item = make_work_item("New", "Test Case");
        assert_eq!(item.type_icon(), "◇");
    }

    #[test]
    fn test_type_icon_product_backlog_item() {
        let item = make_work_item("New", "Product Backlog Item");
        assert_eq!(item.type_icon(), "▣");
    }

    #[test]
    fn test_type_icon_unknown() {
        let item = make_work_item("New", "Unknown Type");
        assert_eq!(item.type_icon(), "•");
    }

    // available_states tests
    #[test]
    fn test_available_states_task() {
        let item = make_work_item("New", "Task");
        assert_eq!(
            item.available_states(),
            vec!["To Do", "In Progress", "Done", "Removed"]
        );
    }

    #[test]
    fn test_available_states_bug() {
        let item = make_work_item("New", "Bug");
        assert_eq!(
            item.available_states(),
            vec![
                "New",
                "In Progress",
                "Done In Stage",
                "Done Not Released",
                "Done",
                "Tested w/Bugs"
            ]
        );
    }

    #[test]
    fn test_available_states_user_story() {
        let item = make_work_item("New", "User Story");
        assert_eq!(
            item.available_states(),
            vec![
                "New",
                "In Progress",
                "Done In Stage",
                "Done Not Released",
                "Done",
                "Tested w/Bugs"
            ]
        );
    }

    #[test]
    fn test_available_states_product_backlog_item() {
        let item = make_work_item("New", "Product Backlog Item");
        assert_eq!(
            item.available_states(),
            vec![
                "New",
                "In Progress",
                "Done In Stage",
                "Done Not Released",
                "Done",
                "Tested w/Bugs"
            ]
        );
    }

    #[test]
    fn test_available_states_other_type() {
        let item = make_work_item("New", "Feature");
        assert_eq!(
            item.available_states(),
            vec![
                "New",
                "In Progress",
                "Done In Stage",
                "Done Not Released",
                "Done"
            ]
        );
    }
}

use crate::azure::{
    Pipeline, PipelineRun, PullRequest, Release, ReleaseDefinition, Repository, Sprint,
    TimelineRecord, User, WorkItem,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default CI/CD cache TTL: 10 minutes (600 seconds)
pub const CICD_CACHE_TTL_SECS: u64 = 600;

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub timestamp: u64,
    pub sprints: Vec<Sprint>,
    pub work_items: Vec<WorkItem>,
    pub users: Vec<User>,
    pub sprint_path: String,
    #[serde(default)]
    pub filter_state: Option<String>,
    #[serde(default)]
    pub filter_assignee: Option<String>,
    #[serde(default)]
    pub pinned_items: HashSet<i32>,
}

impl CacheEntry {
    pub fn new(
        sprints: Vec<Sprint>,
        work_items: Vec<WorkItem>,
        users: Vec<User>,
        sprint_path: &str,
        filter_state: Option<String>,
        filter_assignee: Option<String>,
        pinned_items: HashSet<i32>,
    ) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            sprints,
            work_items,
            users,
            sprint_path: sprint_path.to_string(),
            filter_state,
            filter_assignee,
            pinned_items,
        }
    }

    pub fn age_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(self.timestamp))
            .unwrap_or(0)
    }
}

fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("lazyops"))
}

pub fn save_last_project(name: &str) -> Result<()> {
    let dir = cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("last_project.txt");
    std::fs::write(path, name)?;
    Ok(())
}

pub fn load_last_project() -> Option<String> {
    let path = cache_dir()?.join("last_project.txt");
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn save_last_repo(project: &str, repo_name: &str) -> Result<()> {
    let dir = cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    std::fs::create_dir_all(&dir)?;
    let sanitized = sanitize_filename(project);
    let path = dir.join(format!("{sanitized}_last_repo.txt"));
    std::fs::write(path, repo_name)?;
    Ok(())
}

pub fn load_last_repo(project: &str) -> Option<String> {
    let sanitized = sanitize_filename(project);
    let path = cache_dir()?.join(format!("{sanitized}_last_repo.txt"));
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

fn cache_path(project: &str) -> Option<PathBuf> {
    cache_dir().map(|d| d.join(format!("{}.json", sanitize_filename(project))))
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn load(project: &str) -> Option<CacheEntry> {
    let path = cache_path(project)?;
    let contents = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn save(project: &str, entry: &CacheEntry) -> Result<()> {
    let dir = cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    std::fs::create_dir_all(&dir)?;
    let path = cache_path(project).ok_or_else(|| anyhow::anyhow!("No cache path"))?;
    let contents = serde_json::to_string_pretty(entry)?;
    std::fs::write(path, contents)?;
    Ok(())
}

// ============================================
// CI/CD Cache (Pipelines & Releases)
// ============================================

#[derive(Debug, Serialize, Deserialize)]
pub struct CICDCacheEntry {
    pub timestamp: u64,
    pub pipelines: Vec<Pipeline>,
    pub release_definitions: Vec<ReleaseDefinition>,
    #[serde(default)]
    pub pinned_pipelines: HashSet<i32>,
    #[serde(default)]
    pub pinned_releases: HashSet<i32>,
}

impl CICDCacheEntry {
    pub fn new(
        pipelines: Vec<Pipeline>,
        release_definitions: Vec<ReleaseDefinition>,
        pinned_pipelines: HashSet<i32>,
        pinned_releases: HashSet<i32>,
    ) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            pipelines,
            release_definitions,
            pinned_pipelines,
            pinned_releases,
        }
    }

    pub fn age_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(self.timestamp))
            .unwrap_or(0)
    }

    /// Pipeline and release definition lists rarely change (once a year).
    /// Users can manually refresh with 'r' when needed.
    pub fn is_valid(&self) -> bool {
        true
    }
}

fn cicd_cache_path(project: &str) -> Option<PathBuf> {
    let sanitized = sanitize_filename(project);
    cache_dir().map(|d| d.join(format!("{sanitized}_cicd.json")))
}

pub fn load_cicd(project: &str) -> Option<CICDCacheEntry> {
    let path = cicd_cache_path(project)?;
    let contents = std::fs::read_to_string(&path).ok()?;
    let entry: CICDCacheEntry = serde_json::from_str(&contents).ok()?;
    // Only return if cache is still valid
    if entry.is_valid() {
        Some(entry)
    } else {
        None
    }
}

pub fn save_cicd(project: &str, entry: &CICDCacheEntry) -> Result<()> {
    let dir = cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    std::fs::create_dir_all(&dir)?;
    let path = cicd_cache_path(project).ok_or_else(|| anyhow::anyhow!("No cache path"))?;
    let contents = serde_json::to_string_pretty(entry)?;
    std::fs::write(path, contents)?;
    Ok(())
}

// ============================================
// Pipeline Runs Cache (per pipeline)
// ============================================

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineRunsCacheEntry {
    pub timestamp: u64,
    pub pipeline_id: i32,
    pub runs: Vec<PipelineRun>,
}

impl PipelineRunsCacheEntry {
    pub fn new(pipeline_id: i32, runs: Vec<PipelineRun>) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            pipeline_id,
            runs,
        }
    }

    pub fn age_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(self.timestamp))
            .unwrap_or(0)
    }

    /// Cache is always valid (we use stale-while-revalidate)
    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        true
    }

    /// Check if cache should be refreshed in background
    pub fn needs_refresh(&self) -> bool {
        self.age_seconds() >= CICD_CACHE_TTL_SECS
    }
}

fn pipeline_runs_cache_path(project: &str, pipeline_id: i32) -> Option<PathBuf> {
    let sanitized = sanitize_filename(project);
    cache_dir().map(|d| d.join(format!("{sanitized}_pipeline_{pipeline_id}_runs.json")))
}

/// Load pipeline runs cache. Returns (entry, needs_refresh) if cache exists.
pub fn load_pipeline_runs(
    project: &str,
    pipeline_id: i32,
) -> Option<(PipelineRunsCacheEntry, bool)> {
    let path = pipeline_runs_cache_path(project, pipeline_id)?;
    let contents = std::fs::read_to_string(&path).ok()?;
    let entry: PipelineRunsCacheEntry = serde_json::from_str(&contents).ok()?;
    let needs_refresh = entry.needs_refresh();
    Some((entry, needs_refresh))
}

pub fn save_pipeline_runs(project: &str, entry: &PipelineRunsCacheEntry) -> Result<()> {
    let dir = cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    std::fs::create_dir_all(&dir)?;
    let path = pipeline_runs_cache_path(project, entry.pipeline_id)
        .ok_or_else(|| anyhow::anyhow!("No cache path"))?;
    let contents = serde_json::to_string_pretty(entry)?;
    std::fs::write(path, contents)?;
    Ok(())
}

// ============================================
// Release Items Cache (per release definition)
// ============================================

#[derive(Debug, Serialize, Deserialize)]
pub struct ReleasesCacheEntry {
    pub timestamp: u64,
    pub definition_id: i32,
    pub releases: Vec<Release>,
}

impl ReleasesCacheEntry {
    pub fn new(definition_id: i32, releases: Vec<Release>) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            definition_id,
            releases,
        }
    }

    pub fn age_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(self.timestamp))
            .unwrap_or(0)
    }

    /// Cache is always valid (we use stale-while-revalidate)
    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        true
    }

    /// Check if cache should be refreshed in background
    pub fn needs_refresh(&self) -> bool {
        self.age_seconds() >= CICD_CACHE_TTL_SECS
    }
}

fn releases_cache_path(project: &str, definition_id: i32) -> Option<PathBuf> {
    let sanitized = sanitize_filename(project);
    cache_dir().map(|d| d.join(format!("{sanitized}_release_def_{definition_id}.json")))
}

/// Load releases cache. Returns (entry, needs_refresh) if cache exists.
pub fn load_releases(project: &str, definition_id: i32) -> Option<(ReleasesCacheEntry, bool)> {
    let path = releases_cache_path(project, definition_id)?;
    let contents = std::fs::read_to_string(&path).ok()?;
    let entry: ReleasesCacheEntry = serde_json::from_str(&contents).ok()?;
    let needs_refresh = entry.needs_refresh();
    Some((entry, needs_refresh))
}

pub fn save_releases(project: &str, entry: &ReleasesCacheEntry) -> Result<()> {
    let dir = cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    std::fs::create_dir_all(&dir)?;
    let path = releases_cache_path(project, entry.definition_id)
        .ok_or_else(|| anyhow::anyhow!("No cache path"))?;
    let contents = serde_json::to_string_pretty(entry)?;
    std::fs::write(path, contents)?;
    Ok(())
}

// ============================================
// Timeline Cache (per build)
// ============================================

#[derive(Debug, Serialize, Deserialize)]
pub struct TimelineCacheEntry {
    pub timestamp: u64,
    pub build_id: i32,
    pub records: Vec<TimelineRecord>,
}

impl TimelineCacheEntry {
    pub fn new(build_id: i32, records: Vec<TimelineRecord>) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            build_id,
            records,
        }
    }

    pub fn age_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(self.timestamp))
            .unwrap_or(0)
    }

    /// Cache is always valid (we use stale-while-revalidate)
    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        true
    }

    /// Check if cache should be refreshed in background
    pub fn needs_refresh(&self) -> bool {
        self.age_seconds() >= CICD_CACHE_TTL_SECS
    }
}

fn timeline_cache_path(project: &str, build_id: i32) -> Option<PathBuf> {
    let sanitized = sanitize_filename(project);
    cache_dir().map(|d| d.join(format!("{sanitized}_build_{build_id}_timeline.json")))
}

/// Load timeline cache. Returns (entry, needs_refresh) if cache exists.
pub fn load_timeline(project: &str, build_id: i32) -> Option<(TimelineCacheEntry, bool)> {
    let path = timeline_cache_path(project, build_id)?;
    let contents = std::fs::read_to_string(&path).ok()?;
    let entry: TimelineCacheEntry = serde_json::from_str(&contents).ok()?;
    let needs_refresh = entry.needs_refresh();
    Some((entry, needs_refresh))
}

pub fn save_timeline(project: &str, entry: &TimelineCacheEntry) -> Result<()> {
    let dir = cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    std::fs::create_dir_all(&dir)?;
    let path = timeline_cache_path(project, entry.build_id)
        .ok_or_else(|| anyhow::anyhow!("No cache path"))?;
    let contents = serde_json::to_string_pretty(entry)?;
    std::fs::write(path, contents)?;
    Ok(())
}

// ============================================
// Build Log Cache (per build + log)
// ============================================

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildLogCacheEntry {
    pub timestamp: u64,
    pub build_id: i32,
    pub log_id: i32,
    pub lines: Vec<String>,
}

impl BuildLogCacheEntry {
    pub fn new(build_id: i32, log_id: i32, lines: Vec<String>) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            build_id,
            log_id,
            lines,
        }
    }

    pub fn age_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(self.timestamp))
            .unwrap_or(0)
    }

    /// Cache is always valid (we use stale-while-revalidate)
    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        true
    }

    /// Check if cache should be refreshed in background
    pub fn needs_refresh(&self) -> bool {
        self.age_seconds() >= CICD_CACHE_TTL_SECS
    }
}

fn build_log_cache_path(project: &str, build_id: i32, log_id: i32) -> Option<PathBuf> {
    let sanitized = sanitize_filename(project);
    cache_dir().map(|d| d.join(format!("{sanitized}_build_{build_id}_log_{log_id}.json")))
}

/// Load build log cache. Returns (entry, needs_refresh) if cache exists.
pub fn load_build_log(
    project: &str,
    build_id: i32,
    log_id: i32,
) -> Option<(BuildLogCacheEntry, bool)> {
    let path = build_log_cache_path(project, build_id, log_id)?;
    let contents = std::fs::read_to_string(&path).ok()?;
    let entry: BuildLogCacheEntry = serde_json::from_str(&contents).ok()?;
    let needs_refresh = entry.needs_refresh();
    Some((entry, needs_refresh))
}

pub fn save_build_log(project: &str, entry: &BuildLogCacheEntry) -> Result<()> {
    let dir = cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    std::fs::create_dir_all(&dir)?;
    let path = build_log_cache_path(project, entry.build_id, entry.log_id)
        .ok_or_else(|| anyhow::anyhow!("No cache path"))?;
    let contents = serde_json::to_string_pretty(entry)?;
    std::fs::write(path, contents)?;
    Ok(())
}

// ============================================
// PR Cache (repositories list)
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PRCacheEntry {
    pub timestamp: u64,
    pub repos: Vec<Repository>,
}

impl PRCacheEntry {
    pub fn new(repos: Vec<Repository>) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            repos,
        }
    }

    pub fn age_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(self.timestamp))
            .unwrap_or(0)
    }

    /// Pipeline and release definition lists rarely change (once a year).
    /// Users can manually refresh with 'r' when needed.
    pub fn is_valid(&self) -> bool {
        true
    }
}

fn pr_cache_path(project: &str) -> Option<PathBuf> {
    let sanitized = sanitize_filename(project);
    cache_dir().map(|d| d.join(format!("{sanitized}_pr.json")))
}

pub fn load_pr(project: &str) -> Option<PRCacheEntry> {
    let path = pr_cache_path(project)?;
    let contents = std::fs::read_to_string(&path).ok()?;
    let entry: PRCacheEntry = serde_json::from_str(&contents).ok()?;
    // Only return if cache is still valid
    if entry.is_valid() {
        Some(entry)
    } else {
        None
    }
}

pub fn save_pr(project: &str, entry: &PRCacheEntry) -> Result<()> {
    let dir = cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    std::fs::create_dir_all(&dir)?;
    let path = pr_cache_path(project).ok_or_else(|| anyhow::anyhow!("No cache path"))?;
    let contents = serde_json::to_string_pretty(entry)?;
    std::fs::write(path, contents)?;
    Ok(())
}

// ============================================
// PR List Cache (per repo, all panes)
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PRListCacheEntry {
    pub timestamp: u64,
    pub repo_name: String,
    pub active: Vec<PullRequest>,
    pub mine: Vec<PullRequest>,
    pub completed: Vec<PullRequest>,
    pub abandoned: Vec<PullRequest>,
}

impl PRListCacheEntry {
    pub fn new(
        repo_name: &str,
        active: Vec<PullRequest>,
        mine: Vec<PullRequest>,
        completed: Vec<PullRequest>,
        abandoned: Vec<PullRequest>,
    ) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            repo_name: repo_name.to_string(),
            active,
            mine,
            completed,
            abandoned,
        }
    }

    pub fn age_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(self.timestamp))
            .unwrap_or(0)
    }

    /// Cache is always valid (stale-while-revalidate)
    pub fn is_valid(&self) -> bool {
        true
    }

    /// Check if cache should be refreshed in background
    pub fn needs_refresh(&self) -> bool {
        self.age_seconds() >= CICD_CACHE_TTL_SECS
    }
}

fn pr_list_cache_path(project: &str, repo_name: &str) -> Option<PathBuf> {
    let sanitized_project = sanitize_filename(project);
    let sanitized_repo = sanitize_filename(repo_name);
    cache_dir().map(|d| d.join(format!("{sanitized_project}_pr_list_{sanitized_repo}.json")))
}

pub fn load_pr_list(project: &str, repo_name: &str) -> Option<PRListCacheEntry> {
    let path = pr_list_cache_path(project, repo_name)?;
    let contents = std::fs::read_to_string(&path).ok()?;
    let entry: PRListCacheEntry = serde_json::from_str(&contents).ok()?;
    if entry.is_valid() {
        Some(entry)
    } else {
        None
    }
}

pub fn save_pr_list(project: &str, entry: &PRListCacheEntry) -> Result<()> {
    let dir = cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    std::fs::create_dir_all(&dir)?;
    let path = pr_list_cache_path(project, &entry.repo_name)
        .ok_or_else(|| anyhow::anyhow!("No cache path"))?;
    let contents = serde_json::to_string_pretty(entry)?;
    std::fs::write(path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::TempDir;

    #[test]
    fn test_sanitize_filename_alphanumeric() {
        assert_eq!(sanitize_filename("myproject123"), "myproject123");
        assert_eq!(sanitize_filename("test-project"), "test-project");
        assert_eq!(sanitize_filename("test_project"), "test_project");
    }

    #[test]
    fn test_sanitize_filename_special_chars() {
        assert_eq!(sanitize_filename("my project"), "my_project");
        assert_eq!(sanitize_filename("path/to/file"), "path_to_file");
        assert_eq!(sanitize_filename("user@domain.com"), "user_domain_com");
        assert_eq!(sanitize_filename("name with spaces"), "name_with_spaces");
    }

    #[test]
    fn test_sanitize_filename_path_traversal() {
        // "../../../etc/passwd" -> 9 dots/slashes = 9 underscores, then "etc_passwd"
        assert_eq!(
            sanitize_filename("../../../etc/passwd"),
            "_________etc_passwd"
        );
        // "..\\..\\windows" -> 6 dots/backslashes = 6 underscores, then "windows"
        assert_eq!(sanitize_filename("..\\..\\windows"), "______windows");
    }

    #[test]
    fn test_sanitize_filename_empty() {
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn test_cache_entry_age_seconds_new() {
        let entry = CacheEntry::new(vec![], vec![], vec![], "test", None, None, HashSet::new());

        let age = entry.age_seconds();
        assert!(
            age <= 1,
            "Newly created entry should have age close to 0, got {age}"
        );
    }

    #[test]
    fn test_cache_entry_age_seconds_old() {
        let mut entry = CacheEntry::new(vec![], vec![], vec![], "test", None, None, HashSet::new());

        // Set timestamp to 100 seconds ago
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        entry.timestamp = now - 100;

        let age = entry.age_seconds();
        assert!(
            (99..=101).contains(&age),
            "Entry should be ~100 seconds old, got {age}"
        );
    }

    #[test]
    fn test_pipeline_runs_needs_refresh_fresh() {
        let entry = PipelineRunsCacheEntry::new(123, vec![]);

        assert!(
            !entry.needs_refresh(),
            "Fresh cache should not need refresh"
        );
    }

    #[test]
    fn test_pipeline_runs_needs_refresh_stale() {
        let mut entry = PipelineRunsCacheEntry::new(123, vec![]);

        // Set timestamp to just past TTL
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        entry.timestamp = now - CICD_CACHE_TTL_SECS - 1;

        assert!(entry.needs_refresh(), "Stale cache should need refresh");
    }

    #[test]
    fn test_pipeline_runs_needs_refresh_at_boundary() {
        let mut entry = PipelineRunsCacheEntry::new(123, vec![]);

        // Set timestamp exactly at TTL boundary
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        entry.timestamp = now - CICD_CACHE_TTL_SECS;

        assert!(
            entry.needs_refresh(),
            "Cache at TTL boundary should need refresh"
        );
    }

    #[test]
    fn test_cache_save_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("test.json");

        // Create a cache entry
        let original = CacheEntry::new(
            vec![],
            vec![],
            vec![],
            "test-sprint",
            Some("Active".to_string()),
            Some("user@example.com".to_string()),
            HashSet::from([1, 2, 3]),
        );

        // Save to temp file
        let contents = serde_json::to_string_pretty(&original).unwrap();
        std::fs::write(&cache_path, contents).unwrap();

        // Load back
        let loaded_contents = std::fs::read_to_string(&cache_path).unwrap();
        let loaded: CacheEntry = serde_json::from_str(&loaded_contents).unwrap();

        // Verify fields match
        assert_eq!(loaded.timestamp, original.timestamp);
        assert_eq!(loaded.sprint_path, original.sprint_path);
        assert_eq!(loaded.filter_state, original.filter_state);
        assert_eq!(loaded.filter_assignee, original.filter_assignee);
        assert_eq!(loaded.pinned_items, original.pinned_items);
    }

    #[test]
    fn test_load_nonexistent_cache() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent.json");

        let result = std::fs::read_to_string(&nonexistent);
        assert!(result.is_err(), "Reading nonexistent file should fail");
    }

    #[test]
    fn test_load_corrupted_cache() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("corrupted.json");

        // Write invalid JSON
        std::fs::write(&cache_path, "{ this is not valid json }").unwrap();

        // Try to load
        let contents = std::fs::read_to_string(&cache_path).unwrap();
        let result: Result<CacheEntry, _> = serde_json::from_str(&contents);

        assert!(result.is_err(), "Parsing corrupted JSON should fail");
    }

    #[test]
    fn test_pipeline_runs_cache_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("pipeline_runs.json");

        let original = PipelineRunsCacheEntry::new(456, vec![]);

        let contents = serde_json::to_string_pretty(&original).unwrap();
        std::fs::write(&cache_path, contents).unwrap();

        let loaded_contents = std::fs::read_to_string(&cache_path).unwrap();
        let loaded: PipelineRunsCacheEntry = serde_json::from_str(&loaded_contents).unwrap();

        assert_eq!(loaded.pipeline_id, original.pipeline_id);
        assert_eq!(loaded.timestamp, original.timestamp);
    }

    #[test]
    fn test_releases_cache_needs_refresh() {
        let entry = ReleasesCacheEntry::new(789, vec![]);
        assert!(
            !entry.needs_refresh(),
            "Fresh releases cache should not need refresh"
        );

        let mut old_entry = ReleasesCacheEntry::new(789, vec![]);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        old_entry.timestamp = now - CICD_CACHE_TTL_SECS - 1;
        assert!(
            old_entry.needs_refresh(),
            "Stale releases cache should need refresh"
        );
    }

    #[test]
    fn test_timeline_cache_needs_refresh() {
        let entry = TimelineCacheEntry::new(999, vec![]);
        assert!(
            !entry.needs_refresh(),
            "Fresh timeline cache should not need refresh"
        );

        let mut old_entry = TimelineCacheEntry::new(999, vec![]);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        old_entry.timestamp = now - CICD_CACHE_TTL_SECS - 1;
        assert!(
            old_entry.needs_refresh(),
            "Stale timeline cache should need refresh"
        );
    }

    #[test]
    fn test_build_log_cache_needs_refresh() {
        let entry = BuildLogCacheEntry::new(111, 222, vec![]);
        assert!(
            !entry.needs_refresh(),
            "Fresh build log cache should not need refresh"
        );

        let mut old_entry = BuildLogCacheEntry::new(111, 222, vec![]);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        old_entry.timestamp = now - CICD_CACHE_TTL_SECS - 1;
        assert!(
            old_entry.needs_refresh(),
            "Stale build log cache should need refresh"
        );
    }
}

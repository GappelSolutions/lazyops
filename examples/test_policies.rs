// Run from project dir: cargo run --example test_policies
use lazyops::azure::AzureCli;
use lazyops::config::Config;

#[tokio::main]
async fn main() {
    let config = Config::load();
    let project = &config.projects[0];
    let client = AzureCli::new(project);

    eprintln!("Testing list_pr_policies for PR #7917...");
    match client.list_pr_policies(7917).await {
        Ok(policies) => {
            eprintln!("SUCCESS: Got {} policies", policies.len());
            for p in &policies {
                let name = p
                    .configuration
                    .as_ref()
                    .and_then(|c| c.policy_type.as_ref())
                    .and_then(|t| t.display_name.as_deref())
                    .unwrap_or("?");
                let status = p.status.as_deref().unwrap_or("?");
                eprintln!("  {status} - {name}");
            }
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            eprintln!("Full chain: {e:?}");
        }
    }
}

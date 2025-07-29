use crate::github::{Client, GitHubClient, GitHubError};
use crate::Console;

pub struct ListAllCommand;

impl ListAllCommand {
    pub async fn execute(client: &GitHubClient) -> Result<(), GitHubError> {
        let console = Console::new(false);
        
        console.header(&format!("Listing all repositories in organization: {}", client.organization));
        
        // Fetch repository list
        let spinner = console.create_spinner("Fetching repository list...");
        let repositories = client.repositories().await?;
        let total_repos = repositories.len();
        
        console.finish_progress_success(&spinner, &format!("Found {} repositories", total_repos));
        
        if total_repos == 0 {
            console.warning("No repositories found in the organization");
            return Ok(());
        }
        
        // Check each repository for configuration
        let progress = console.create_scan_progress(total_repos as u64, "Checking for documents.toml files");
        
        let mut with_config = Vec::new();
        let mut without_config = Vec::new();
        let mut errors = Vec::new();
        
        for repository in repositories {
            progress.inc(1);
            
            match client.get_project_config(repository.as_str()).await {
                Ok(_) => {
                    with_config.push(repository);
                }
                Err(GitHubError::ConfigFileNotFound(_)) => {
                    without_config.push(repository);
                }
                Err(e) => {
                    errors.push((repository, e.to_string()));
                }
            }
        }
        
        console.finish_progress_success(&progress, "Repository scan completed");
        
        // Display results
        if !with_config.is_empty() {
            console.success(&format!("Repositories with documents.toml ({}):", with_config.len()));
            for repo in &with_config {
                println!("  ✓ {}", repo);
            }
        }
        
        if !without_config.is_empty() {
            console.info(&format!("Repositories without documents.toml ({}):", without_config.len()));
            for repo in &without_config {
                println!("  • {}", repo);
            }
        }
        
        if !errors.is_empty() {
            console.error(&format!("Repositories with errors ({}):", errors.len()));
            for (repo, error) in &errors {
                println!("  ✗ {}: {}", repo, error);
            }
        }
        
        // Summary
        console.summary("Repository Summary", &[
            ("Total repositories", total_repos.to_string()),
            ("With documents.toml", with_config.len().to_string()),
            ("Without documents.toml", without_config.len().to_string()),
            ("Errors", errors.len().to_string()),
        ]);
        
        Ok(())
    }
}

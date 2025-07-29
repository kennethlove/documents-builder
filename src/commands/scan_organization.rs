use crate::github::{Client, GitHubClient, GitHubError};
use crate::Console;
use clap::Args;

/// Arguments for the scan-org command
/// 
/// This command scans all repositories in the configured organization and identifies
/// those that have a documents.toml configuration file in their root directory.
#[derive(Args, Debug)]
pub struct ScanOrgArgs {
    /// Whether to show verbose output
    #[arg(short, long, help = "Show verbose output including repositories without the config file")]
    verbose: bool,
}

/// Command to scan all repositories in an organization for documents.toml configuration files
///
/// This command efficiently searches through all repositories in the configured GitHub organization
/// and identifies those that have a documents.toml configuration file in their root directory.
/// It provides a summary of the results and can optionally show verbose output.
///
/// The implementation uses GitHub's GraphQL API to check multiple repositories at once,
/// which significantly reduces the number of API calls and improves performance compared
/// to checking each repository individually.
pub struct ScanOrgCommand {
    /// Whether to show verbose output
    verbose: bool,
}

impl ScanOrgCommand {
    /// Creates a new instance of the ScanOrgCommand
    ///
    /// # Arguments
    ///
    /// * `args` - The command line arguments for the scan-org command
    pub fn new(args: ScanOrgArgs) -> Self {
        Self {
            verbose: args.verbose,
        }
    }

    /// Executes the scan-org command
    ///
    /// This method:
    /// 1. Uses GitHub's GraphQL API to efficiently check all repositories at once for documents.toml existence
    /// 2. Processes the results to identify repositories with the config file
    /// 3. Prints the repositories that have the config file
    /// 4. Provides a summary of how many repositories were found with the config file
    ///
    /// The implementation uses a single GraphQL query to check multiple repositories at once,
    /// which significantly reduces the number of API calls compared to checking each repository individually.
    ///
    /// # Arguments
    ///
    /// * `client` - The GitHub client to use for API requests
    ///
    /// # Returns
    ///
    /// * `Result<(), GitHubError>` - Ok if the command executed successfully, Err otherwise
    pub async fn execute(&self, client: &GitHubClient) -> Result<(), GitHubError> {
        let console = Console::new(self.verbose);
        
        // Header message
        console.header(&format!("Scanning organization {} for documents.toml files", client.organization));
        
        // Create spinner for initial API call
        let spinner = console.create_spinner("Fetching repository list from GitHub...");
        
        // Use GraphQL to efficiently check all repositories at once for documents.toml configuration file existence
        let repo_results = client.batch_check_config_file_exists().await?;
        
        let total_repos = repo_results.len();
        
        console.finish_progress_success(&spinner, &format!("Found {} repositories in organization", total_repos));
        
        // Create progress bar for scanning
        let progress = console.create_scan_progress(total_repos as u64, "Checking for documents.toml files");
        
        let mut found_count = 0;
        let mut found_repos = Vec::new();
        
        // Process the results
        for (repo_name, file_exists) in repo_results {
            progress.inc(1);
            
            if file_exists {
                found_repos.push(repo_name.clone());
                found_count += 1;
                console.verbose(&format!("✓ Found documents.toml in: {}", repo_name));
            } else {
                console.verbose(&format!("✗ No documents.toml in: {}", repo_name));
            }
        }
        
        console.finish_progress_success(&progress, "Scan completed");
        
        // Display found repositories
        if found_count > 0 {
            console.info("Repositories with documents.toml configuration:");
            for repo in &found_repos {
                println!("  • {}", repo);
            }
        } else {
            console.warning("No repositories found with documents.toml configuration");
        }
        
        // Summary
        console.summary("Scan Results", &[
            ("Total repositories", total_repos.to_string()),
            ("With documents.toml", found_count.to_string()),
            ("Without documents.toml", (total_repos - found_count).to_string()),
        ]);
        
        Ok(())
    }
}

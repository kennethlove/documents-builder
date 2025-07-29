use crate::github::{Client, GitHubClient, GitHubError};
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
        let _ = tracing_subscriber::fmt::try_init();
        tracing::info!("Scanning organization {} for repositories with documents.toml", client.organization);
        
        println!("Scanning organization {} for repositories with documents.toml", client.organization);
        
        // Use GraphQL to efficiently check all repositories at once for documents.toml configuration file existence
        tracing::info!("Using GraphQL to check for documents.toml existence in all repositories");
        let repo_results = client.batch_check_config_file_exists().await?;
        
        let total_repos = repo_results.len();
        tracing::info!("Found {} repositories in the organization", total_repos);
        
        if self.verbose {
            println!("Found {} repositories in the organization", total_repos);
        }
        
        let mut found_count = 0;
        
        // Process the results
        for (repo_name, file_exists) in repo_results {
            if file_exists {
                println!("Found documents.toml in repository: {}", repo_name);
                found_count += 1;
            } else if self.verbose {
                println!("No documents.toml found in repository: {}", repo_name);
            }
        }
        
        println!("\nSummary: Found {} repositories with documents.toml configuration", found_count);
        
        Ok(())
    }
}

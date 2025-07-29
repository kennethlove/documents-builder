use crate::github::{Client, GitHubClient, GitHubError};
use clap::Args;

/// Arguments for the process-org command
/// 
/// This command processes all repositories in the configured organization and identifies
/// those that have a documents.toml configuration file in their root directory,
/// optionally showing the file content.
#[derive(Args, Debug)]
pub struct ProcessOrgArgs {
    /// Whether to show verbose output
    #[arg(short, long, help = "Show verbose output including repositories without the config file")]
    verbose: bool,
    
    /// Whether to show file content
    #[arg(short, long, help = "Show the content of the documents.toml file when found")]
    show_content: bool,
}

/// Command to process all repositories in an organization for documents.toml configuration files
///
/// This command efficiently searches through all repositories in the configured GitHub organization
/// and identifies those that have a documents.toml configuration file in their root directory.
/// It provides a summary of the results and can optionally show verbose output and file content.
///
/// The implementation uses GitHub's GraphQL API to check multiple repositories at once and fetch content,
/// which significantly reduces the number of API calls and improves performance compared
/// to checking each repository individually.
pub struct ProcessOrgCommand {
    /// Whether to show verbose output
    verbose: bool,
    
    /// Whether to show file content
    show_content: bool,
}

impl ProcessOrgCommand {
    /// Creates a new instance of the ProcessOrgCommand
    ///
    /// # Arguments
    ///
    /// * `args` - The command line arguments for the process-org command
    pub fn new(args: ProcessOrgArgs) -> Self {
        Self {
            verbose: args.verbose,
            show_content: args.show_content,
        }
    }

    /// Executes the process-org command
    ///
    /// This method:
    /// 1. Uses GitHub's GraphQL API to efficiently check all repositories at once for documents.toml and fetch content
    /// 2. Processes the results to identify repositories with the config file
    /// 3. Prints the repositories that have the config file
    /// 4. Optionally displays the file content if requested
    /// 5. Provides a summary of how many repositories were found with the config file
    ///
    /// The implementation uses a single GraphQL query to check multiple repositories at once and fetch file content,
    /// which significantly reduces the number of API calls compared to checking and fetching separately.
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
        tracing::info!("Processing organization {} for repositories with documents.toml", client.organization);
        
        println!("Processing organization {} for repositories with documents.toml", client.organization);
        
        // Use GraphQL to efficiently check all repositories at once and fetch content
        tracing::info!("Using GraphQL to check for documents.toml and fetch content in all repositories");
        let repo_results = client.batch_check_file_content("documents.toml").await?;
        
        let total_repos = repo_results.len();
        tracing::info!("Found {} repositories in the organization", total_repos);
        
        if self.verbose {
            println!("Found {} repositories in the organization", total_repos);
        }
        
        let mut found_count = 0;
        
        // Process the results
        for repo_file in repo_results {
            if repo_file.exists {
                println!("Found documents.toml in repository: {}", repo_file.repo_name);
                found_count += 1;
                
                // Display file content if requested
                if self.show_content {
                    if let Some(content) = &repo_file.content {
                        println!("  Content:");
                        for line in content.lines() {
                            println!("    {}", line);
                        }
                        println!();
                    } else {
                        println!("  (Content not available)");
                        println!();
                    }
                }
            } else if self.verbose {
                println!("No documents.toml found in repository: {}", repo_file.repo_name);
            }
        }
        
        println!("\nSummary: Found {} repositories with documents.toml configuration", found_count);
        
        Ok(())
    }
}

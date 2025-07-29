use crate::github::{Client, GitHubClient};
use crate::processing::{RepositoryProcessor, OutputHandler};
use crate::web::AppError;
use crate::{OutputFormat, ProjectConfig};
use clap::Args;
use std::path::PathBuf;

/// Arguments for the process-organization command
/// 
/// This command processes all repositories in the configured organization that have
/// a documents.toml configuration file, generating document fragments for each.
#[derive(Args, Debug)]
pub struct ProcessOrganizationArgs {
    /// Output directory for generated fragments
    #[arg(long, short, help = "Output directory for generated fragments")]
    pub output: Option<PathBuf>,

    /// Output format (files, json, html)
    #[arg(long, value_enum, default_value = "files", help = "Output format")]
    pub format: OutputFormat,

    /// Force reprocessing even if output exists
    #[arg(long, help = "Force reprocessing even if output exists")]
    pub force: bool,

    /// Verbose progress reporting
    #[arg(long, help = "Verbose progress reporting")]
    pub verbose: bool,
}

/// Command to process all repositories in an organization that have documents.toml configuration files
///
/// This command efficiently searches through all repositories in the configured GitHub organization,
/// identifies those that have a documents.toml configuration file, and processes each repository
/// to generate document fragments using the shared processing infrastructure.
pub struct ProcessOrganizationCommand {
    output: Option<PathBuf>,
    format: OutputFormat,
    force: bool,
    verbose: bool,
}

impl ProcessOrganizationCommand {
    /// Creates a new instance of the ProcessOrganizationCommand
    ///
    /// # Arguments
    ///
    /// * `args` - The command line arguments for the process-organization command
    pub fn new(args: ProcessOrganizationArgs) -> Self {
        Self {
            output: args.output,
            format: args.format,
            force: args.force,
            verbose: args.verbose,
        }
    }

    /// Executes the process-org command
    ///
    /// This method:
    /// 1. Uses GitHub's GraphQL API to find repositories with documents.toml configuration files
    /// 2. For each repository with a config, processes it using the shared RepositoryProcessor
    /// 3. Saves results using the shared OutputHandler for consistent output
    /// 4. Provides progress reporting and error handling for batch processing
    ///
    /// # Arguments
    ///
    /// * `client` - The GitHub client to use for API requests
    ///
    /// # Returns
    ///
    /// * `Result<(), AppError>` - Ok if the command executed successfully, Err otherwise
    pub async fn execute(&self, client: &GitHubClient) -> Result<(), AppError> {
        let _ = tracing_subscriber::fmt::try_init();
        tracing::info!("Processing organization {} for repositories with documents.toml", client.organization);
        
        println!("Processing organization {} for repositories with documents.toml", client.organization);
        
        // Use GraphQL to efficiently check all repositories at once for documents.toml
        tracing::info!("Using GraphQL to check for documents.toml in all repositories");
        let repo_results = client.batch_fetch_config_file_content().await
            .map_err(|e| AppError::InternalServerError(format!("GitHub API error: {}", e)))?;
        
        let total_repos = repo_results.len();
        tracing::info!("Found {} repositories in the organization", total_repos);
        
        if self.verbose {
            println!("Found {} repositories in the organization", total_repos);
        }
        
        let mut processed_count = 0;
        let mut error_count = 0;
        
        // Process each repository that has a documents.toml config
        for repo_file in repo_results {
            if repo_file.exists {
                let repo_name = format!("{}/{}", client.organization, repo_file.repo_name);
                
                if self.verbose {
                    println!("Processing repository: {}", repo_name);
                }

                // Parse the already-fetched configuration content
                let config_content = repo_file.content.as_ref().unwrap();
                if config_content.is_empty() {
                    tracing::error!("Configuration file is empty for repository: {}", repo_name);
                    eprintln!("Configuration file is empty for repository: {}", repo_name);
                    error_count += 1;
                    continue;
                }
                
                match toml::from_str::<ProjectConfig>(config_content) {
                    Ok(config) => {
                        tracing::info!("Successfully parsed configuration for repository: {}", repo_name);
                        
                        // Create processor and run processing using shared infrastructure
                        let processor = RepositoryProcessor::new(
                            client.clone(), 
                            config, 
                            repo_file.repo_name.clone()
                        );
                        
                        match processor.process(self.verbose).await {
                            Ok(result) => {
                                // Determine output directory for this repository
                                let output_dir = self.output
                                    .clone()
                                    .unwrap_or_else(|| PathBuf::from("output"))
                                    .join(&repo_file.repo_name);
                                
                                // Use shared OutputHandler for consistent output handling
                                let output_handler = OutputHandler::new(
                                    output_dir,
                                    self.format.clone(),
                                    self.verbose,
                                );
                                
                                if let Err(e) = output_handler.save_results(&result) {
                                    tracing::error!("Error saving results for repository {}: {}", repo_name, e);
                                    eprintln!("Error saving results for repository {}: {}", repo_name, e);
                                    error_count += 1;
                                } else {
                                    processed_count += 1;
                                    if self.verbose {
                                        println!("✓ Successfully processed repository: {}", repo_name);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Error processing repository {}: {}", repo_name, e);
                                eprintln!("Error processing repository {}: {}", repo_name, e);
                                error_count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error parsing configuration for repository {}: {}", repo_name, e);
                        eprintln!("Error parsing configuration for repository {}: {}", repo_name, e);
                        error_count += 1;
                    }
                }
            } else if self.verbose {
                println!("No documents.toml found in repository: {}", repo_file.repo_name);
            }
        }
        
        // Print summary
        println!("\nProcessing Summary:");
        println!("  Total repositories in organization: {}", total_repos);
        println!("  Repositories with documents.toml: {}", processed_count + error_count);
        println!("  Successfully processed: {}", processed_count);
        if error_count > 0 {
            println!("  Failed to process: {}", error_count);
        }
        
        if error_count > 0 {
            tracing::warn!("Some repositories failed to process. Check logs for details.");
        }
        
        Ok(())
    }
}

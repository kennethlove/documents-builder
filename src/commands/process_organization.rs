use crate::github::{Client, GitHubClient};
use crate::processing::{RepositoryProcessor, OutputHandler};
use crate::web::AppError;
use crate::{Console, OutputFormat, ProjectConfig, RepoStatus};
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
        let console = Console::new(self.verbose);
        
        // Header message
        console.header(&format!("Processing organization {} for documents.toml repositories", client.organization));
        
        // Step 1: Fetch repository list and configurations
        let spinner = console.create_spinner("Fetching repository configurations from GitHub...");
        
        let repo_results = client.batch_fetch_config_file_content().await
            .map_err(|e| AppError::InternalServerError(format!("GitHub API error: {}", e)))?;
        
        let total_repos = repo_results.len();
        let repos_with_config: Vec<_> = repo_results.iter().filter(|r| r.exists).collect();
        let config_count = repos_with_config.len();
        
        console.finish_progress_success(&spinner, &format!("Found {} repositories ({} with documents.toml)", total_repos, config_count));
        
        if config_count == 0 {
            console.warning("No repositories found with documents.toml configuration");
            console.info("Make sure repositories have a documents.toml file in their root directory");
            return Ok(());
        }
        
        // Step 2: Process repositories with configurations
        let progress = console.create_process_progress(config_count as u64, "Processing repositories");
        
        let mut processed_count = 0;
        let mut error_count = 0;
        let mut skipped_count = 0;
        
        // Process each repository that has a documents.toml config
        for repo_file in repo_results {
            if repo_file.exists {
                console.repo_status(&repo_file.repo_name, RepoStatus::Processing);
                progress.inc(1);

                // Parse the already-fetched configuration content
                let config_content = repo_file.content.as_ref().unwrap();
                if config_content.is_empty() {
                    console.repo_status(&repo_file.repo_name, RepoStatus::Error("Configuration file is empty".to_string()));
                    error_count += 1;
                    continue;
                }
                
                match toml::from_str::<ProjectConfig>(config_content) {
                    Ok(config) => {
                        // Create processor and run processing using shared infrastructure
                        let processor = RepositoryProcessor::new(
                            client.clone(), 
                            config, 
                            repo_file.repo_name.clone()
                        );
                        
                        // Temporarily suspend progress bar to prevent log interference during processing
                        let processing_result = progress.suspend(|| {
                            // Use tokio::task::block_in_place to handle async code in suspend closure
                            tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(async {
                                    processor.process(false).await
                                })
                            })
                        });
                        
                        match processing_result {
                            Ok(result) => {
                                // Determine output directory for this repository
                                let output_dir = self.output
                                    .clone()
                                    .unwrap_or_else(|| PathBuf::from("output"))
                                    .join(&repo_file.repo_name);
                                
                                // Use shared OutputHandler for consistent output handling
                                let output_handler = OutputHandler::new(
                                    output_dir,
                                    self.format.clone()
                                );
                                
                                match output_handler.save_results(&result) {
                                    Ok(()) => {
                                        console.repo_status(&repo_file.repo_name, RepoStatus::Success);
                                        processed_count += 1;
                                    }
                                    Err(e) => {
                                        console.repo_status(&repo_file.repo_name, RepoStatus::Error(format!("Failed to save results: {}", e)));
                                        error_count += 1;
                                    }
                                }
                            }
                            Err(e) => {
                                console.repo_status(&repo_file.repo_name, RepoStatus::Error(format!("Processing failed: {}", e)));
                                error_count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        console.repo_status(&repo_file.repo_name, RepoStatus::Error(format!("Invalid configuration: {}", e)));
                        error_count += 1;
                    }
                }
            } else {
                console.repo_status(&repo_file.repo_name, RepoStatus::Skipped("No documents.toml found".to_string()));
                skipped_count += 1;
            }
        }
        
        console.finish_progress_success(&progress, "Organization processing completed");
        
        // Summary
        let summary_items = vec![
            ("Total repositories", total_repos.to_string()),
            ("With documents.toml", config_count.to_string()),
            ("Successfully processed", processed_count.to_string()),
            ("Failed to process", error_count.to_string()),
            ("Skipped (no config)", skipped_count.to_string()),
        ];
        
        console.summary("Processing Results", &summary_items);
        
        if processed_count > 0 {
            let output_base = self.output.clone().unwrap_or_else(|| PathBuf::from("output"));
            console.info(&format!("Results saved to: {}", output_base.display()));
        }
        
        if error_count > 0 {
            console.warning("Some repositories failed to process. Check logs for details.");
        } else if processed_count > 0 {
            console.success(&format!("Successfully processed {} repositories", processed_count));
        }
        
        Ok(())
    }
}

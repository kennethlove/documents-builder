use crate::OutputFormat;
use crate::github::{Client, GitHubClient, GitHubError};
use crate::processing::{RepositoryProcessor, OutputHandler};
use crate::web::AppError;
use crate::Console;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ProcessRepositoryArgs {
    /// GitHub repository to process
    pub repository: String,

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

pub struct ProcessRepositoryCommand {
    repository: String,
    output: Option<PathBuf>,
    format: OutputFormat,
    force: bool,
    verbose: bool,
}

impl ProcessRepositoryCommand {
    pub fn new(args: ProcessRepositoryArgs) -> Self {
        Self {
            repository: args.repository,
            output: args.output,
            format: args.format,
            force: args.force,
            verbose: args.verbose,
        }
    }

    pub async fn execute(&self, client: &GitHubClient) -> Result<(), AppError> {
        client.handle_rate_limits().await?;
        
        let console = Console::new(self.verbose);
        let output_dir = self
            .output
            .clone()
            .unwrap_or_else(|| PathBuf::from("output").join(&self.repository));

        // Header message
        console.header(&format!("Processing repository: {}", self.repository));

        console.verbose(&format!("Output directory: {}", output_dir.display()));
        console.verbose(&format!("Output format: {:?}", self.format));
        console.verbose(&format!("Force reprocessing: {}", self.force));

        // Step 1: Fetch configuration
        let spinner = console.create_spinner("Fetching repository configuration...");
        
        match client.get_project_config(self.repository.as_str()).await {
            Ok(config) => {
                console.finish_progress_success(&spinner, "Configuration found");
                tracing::info!("Found configuration for repository: {}", self.repository);
                
                if self.verbose {
                    console.verbose(&format!("Repository configuration: {:#?}", config));
                }

                // Step 2: Process repository
                let process_spinner = console.create_spinner("Processing documents...");
                let processor =
                    RepositoryProcessor::new(client.clone(), config, self.repository.clone());

                match processor.process(self.verbose).await {
                    Ok(result) => {
                        console.finish_progress_success(&process_spinner, "Documents processed");
                        
                        // Step 3: Save results
                        let save_spinner = console.create_spinner("Saving results...");
                        let output_handler = OutputHandler::new(
                            output_dir.clone(),
                            self.format.clone(),
                        );
                        
                        match output_handler.save_results(&result) {
                            Ok(()) => {
                                console.finish_progress_success(&save_spinner, "Results saved");
                                console.success(&format!("Successfully processed repository: {}", self.repository));
                                console.info(&format!("Output saved to: {}", output_dir.display()));
                                Ok(())
                            }
                            Err(e) => {
                                console.finish_progress_error(&save_spinner, "Failed to save results");
                                console.error(&format!("Error saving results: {}", e));
                                Err(e)
                            }
                        }
                    }
                    Err(e) => {
                        console.finish_progress_error(&process_spinner, "Processing failed");
                        console.error(&format!("Error processing repository {}: {}", self.repository, e));
                        tracing::error!("Error processing repository {}: {}", self.repository, e);
                        std::process::exit(1);
                    }
                }
            }
            Err(GitHubError::ConfigFileNotFound(_)) => {
                console.finish_progress_error(&spinner, "Configuration not found");
                console.error(&format!("No documents.toml configuration file found in repository: {}", self.repository));
                console.info("Make sure the repository has a documents.toml file in its root directory");
                tracing::error!("No configuration file found in repository: {}", self.repository);
                std::process::exit(1);
            }
            Err(e) => {
                console.finish_progress_error(&spinner, "Failed to fetch configuration");
                console.error(&format!("Error retrieving configuration for repository {}: {}", self.repository, e));
                tracing::error!("Error retrieving configuration for repository: {}: {}", self.repository, e);
                std::process::exit(1);
            }
        }
    }
}

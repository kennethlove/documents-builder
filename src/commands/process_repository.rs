use crate::OutputFormat;
use crate::github::{Client, GitHubClient, GitHubError};
use crate::processing::{RepositoryProcessor, OutputHandler};
use crate::web::AppError;
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
        let output_dir = self
            .output
            .clone()
            .unwrap_or_else(|| PathBuf::from("output").join(&self.repository));

        if self.verbose {
            tracing::debug!("Processing repository: {}", self.repository);
            tracing::debug!("Output directory: {}", output_dir.display());
            tracing::debug!("Output format: {:?}", self.format);
            tracing::debug!("Force reprocessing: {}", self.force);
        }

        match client.get_project_config(self.repository.as_str()).await {
            Ok(config) => {
                tracing::info!("Found configuration for repository: {}", self.repository);
                if self.verbose {
                    tracing::debug!("Repository configuration: {:#?}", config);
                }

                // Create processor and run processing
                let processor =
                    RepositoryProcessor::new(client.clone(), config, self.repository.clone());

                match processor.process(self.verbose).await {
                    Ok(result) => {
                        // Use shared OutputHandler for consistent output handling
                        let output_handler = OutputHandler::new(
                            output_dir,
                            self.format.clone(),
                            self.verbose,
                        );
                        
                        output_handler.save_results(&result)?;
                        Ok(())
                    }
                    Err(e) => {
                        tracing::error!("Error processing repository {}: {}", self.repository, e);
                        eprintln!("Error processing repository {}: {}", self.repository, e);
                        std::process::exit(1);
                    }
                }
            }
            Err(GitHubError::ConfigFileNotFound(_)) => {
                tracing::error!(
                    "No configuration file found in repository: {}",
                    self.repository
                );
                eprintln!(
                    "No configuration file found in repository: {}",
                    self.repository
                );
                std::process::exit(1);
            }
            Err(e) => {
                tracing::error!(
                    "Error retrieving configuration for repository: {}: {}",
                    self.repository,
                    e
                );
                eprintln!(
                    "Error retrieving configuration for repository {}: {}",
                    self.repository, e
                );
                std::process::exit(1);
            }
        }
    }
}

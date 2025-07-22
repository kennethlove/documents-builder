use crate::OutputFormat;
use crate::github::{Client, GitHubClient, GitHubError};
use crate::processing::RepositoryProcessor;
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
                        // Create output directory
                        match std::fs::create_dir_all(&output_dir) {
                            Ok(_) => {}
                            Err(e) => {
                                tracing::error!(
                                    "Failed to create output directory {}: {}",
                                    output_dir.display(),
                                    e
                                );
                                eprintln!(
                                    "Failed to create output directory {}: {}",
                                    output_dir.display(),
                                    e
                                );
                                std::process::exit(1);
                            }
                        }

                        match self.format {
                            OutputFormat::Files => {
                                // Save each fragment to a file
                                for fragment in &result.fragments {
                                    let filename = format!(
                                        "{}-{:?}.md",
                                        fragment.file_path.replace('/', "_"),
                                        fragment.fragment_type
                                    );
                                    let fragment_file = output_dir.join(filename);
                                    std::fs::write(&fragment_file, &fragment.content)?;
                                }
                                tracing::info!(
                                    "Processing completed. Files saved in: {}",
                                    output_dir.display()
                                );
                            }
                            OutputFormat::Json => {
                                let output_file = output_dir.join("fragments.json");
                                let json_content = serde_json::to_string_pretty(&result)?;
                                std::fs::write(&output_file, json_content)?;
                                tracing::info!(
                                    "Processing completed. JSON output saved in: {}",
                                    output_file.display()
                                );
                            }
                            OutputFormat::Html => {
                                tracing::info!("HTML output format is not yet implemented.");
                            }
                        };

                        // Save processing summary
                        let summary_file = output_dir.join("processing-summary.json");
                        let summary = serde_json::json!({
                            "repository": result.repository,
                            "processed_at": result.processed_at,
                            "file_processed": result.file_processed,
                            "fragments_generated": result.fragments_generated,
                            "processing_time_ms": result.processing_time_ms,
                        });
                        std::fs::write(&summary_file, serde_json::to_string_pretty(&summary)?)?;

                        tracing::info!("Repository {} processed successfully.", self.repository);
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

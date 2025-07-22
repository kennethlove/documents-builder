use crate::count_document_paths;
use crate::github::{Client, GitHubClient, GitHubError};
use crate::processing::ConfigValidator;
use crate::web::AppError;
use clap::Args;

#[derive(Args, Debug)]
pub struct ValidateConfigArgs {
    /// GitHub repository to validate configuration for
    repository: String,
    #[arg(
        short,
        long,
        help = "Check if referenced files actually exist in the repository"
    )]
    check_files: bool,
    #[arg(
        short,
        long,
        help = "Base directory for resolving relative paths in the config file (defaults to repository root)"
    )]
    base_dir: Option<String>,
}

pub struct ValidateConfigCommand {
    repository: String,
    check_files: bool,
    base_dir: Option<String>,
}

impl ValidateConfigCommand {
    pub fn new(args: ValidateConfigArgs) -> Self {
        Self {
            repository: args.repository,
            check_files: args.check_files,
            base_dir: args.base_dir,
        }
    }

    pub async fn execute(&self, client: &GitHubClient) -> Result<(), AppError> {
        tracing::info!(
            "Validating configuration for repository: {}",
            self.repository
        );

        // Fetch the configuration file from GitHub
        let config = match client.get_project_config(self.repository.as_str()).await {
            Ok(config) => config,
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
                    "Error retrieving configuration for repository {}: {}",
                    self.repository,
                    e
                );
                eprintln!(
                    "Error retrieving configuration for repository {}: {}",
                    self.repository, e
                );
                std::process::exit(1);
            }
        };

        let mut validator = ConfigValidator::new();

        if self.check_files {
            let base_path = self.base_dir.as_deref().unwrap_or(".");
            tracing::info!(
                "Checking file existence in repository relative to: {}",
                base_path
            );
            validator = validator.with_github_file_check(&client, &self.repository, &base_path);
        }

        let result = validator.validate(&config).await;

        if result.is_valid {
            tracing::info!("Configuration for {} is valid.", self.repository);

            if !result.warnings.is_empty() {
                tracing::warn!("Configuration file has warnings:");
                println!("Configuration file has warnings:");
                for warning in &result.warnings {
                    tracing::warn!(" - {}", warning);
                    println!(" - {}", warning);
                }
            }

            println!("Configuration for {} is valid.", self.repository);
            println!("Summary:");
            println!(" - Project: {}", config.project.name);
            println!(" - Description: {}", config.project.description);
            println!(" - Documents: {}", config.documents.len());

            let total_paths: usize = config
                .documents
                .values()
                .map(|doc| count_document_paths(doc))
                .sum();

            if total_paths > 0 {
                println!(" - Total document paths: {}", total_paths);
            } else {
                println!(" - No document paths found.");
            }

            std::process::exit(0);
        } else {
            tracing::info!("Configuration for {} is invalid.", self.repository);

            println!("Configuration for {} is invalid.", self.repository);
            if !result.errors.is_empty() {
                tracing::error!("Configuration file has errors:");
                for error in result.errors {
                    tracing::error!(" - {}", error);
                }
            }

            if !result.warnings.is_empty() {
                tracing::warn!("Configuration file has warnings:");
                println!("Configuration file has warnings:");
                for warning in &result.warnings {
                    tracing::warn!(" - {}", warning);
                    println!(" - {}", warning);
                }
            }

            std::process::exit(1);
        }
    }
}

use crate::count_document_paths;
use crate::github::{Client, GitHubClient, GitHubError};
use crate::processing::ConfigValidator;
use crate::web::AppError;
use crate::Console;
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
        let console = Console::new(false);
        
        console.header(&format!("Validating configuration for repository: {}", self.repository));
        
        tracing::info!("Validating configuration for repository: {}", self.repository);

        // Step 1: Fetch the configuration file from GitHub
        let spinner = console.create_spinner("Fetching configuration file...");
        let config = match client.get_project_config(self.repository.as_str()).await {
            Ok(config) => {
                console.finish_progress_success(&spinner, "Configuration file found");
                config
            }
            Err(GitHubError::ConfigFileNotFound(_)) => {
                console.finish_progress_error(&spinner, "Configuration file not found");
                console.error(&format!("No documents.toml configuration file found in repository: {}", self.repository));
                console.info("Make sure the repository has a documents.toml file in its root directory");
                tracing::error!("No configuration file found in repository: {}", self.repository);
                return Err(AppError::InternalServerError("Configuration file not found".to_string()));
            }
            Err(e) => {
                console.finish_progress_error(&spinner, "Failed to fetch configuration");
                console.error(&format!("Error retrieving configuration for repository {}: {}", self.repository, e));
                tracing::error!("Error retrieving configuration for repository {}: {}", self.repository, e);
                return Err(AppError::InternalServerError(format!("Failed to fetch configuration: {}", e)));
            }
        };

        // Step 2: Set up validator
        let mut validator = ConfigValidator::new();

        if self.check_files {
            let base_path = self.base_dir.as_deref().unwrap_or(".");
            console.verbose(&format!("Will check file existence relative to: {}", base_path));
            tracing::info!("Checking file existence in repository relative to: {}", base_path);
            validator = validator.with_github_file_check(&client, &self.repository, &base_path);
        }

        // Step 3: Validate configuration
        let validation_spinner = console.create_spinner("Validating configuration...");
        let result = validator.validate(&config).await;
        console.finish_progress_success(&validation_spinner, "Validation completed");
        
        // Display warnings if any
        if !result.warnings.is_empty() {
            console.warning(&format!("Configuration has {} warning(s):", result.warnings.len()));
            for warning in &result.warnings {
                println!("  ⚠️  {}", warning);
                tracing::warn!(" - {}", warning);
            }
        }

        // Display validation result
        if result.is_valid {
            console.config_status(&self.repository, true, None);
            tracing::info!("Configuration for {} is valid.", self.repository);
            
            // Display configuration summary
            let total_paths: usize = config
                .documents
                .values()
                .map(|doc| count_document_paths(doc))
                .sum();

            let summary_items = vec![
                ("Project", config.project.name.clone()),
                ("Description", config.project.description.clone()),
                ("Documents", config.documents.len().to_string()),
                ("Total document paths", if total_paths > 0 { total_paths.to_string() } else { "None".to_string() }),
            ];

            console.summary("Configuration Summary", &summary_items);
            
            if total_paths == 0 {
                console.warning("No document paths found in configuration");
            }

            console.success("Configuration validation passed");
            Ok(())
        } else {
            console.config_status(&self.repository, false, Some("Configuration contains errors"));
            tracing::info!("Configuration for {} is invalid.", self.repository);

            if !result.errors.is_empty() {
                console.error(&format!("Configuration has {} error(s):", result.errors.len()));
                for error in &result.errors {
                    println!("  ❌ {}", error);
                    tracing::error!(" - {}", error);
                }
            }

            Err(AppError::InternalServerError("Configuration validation failed".to_string()))
        }
    }
}

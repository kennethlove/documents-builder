use std::path::PathBuf;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use documents::commands::list_all::ListAllCommand;
use documents::commands::process_repository::ProcessRepositoryCommand;
use documents::commands::serve_webhook::ServeWebhookCommand;
use documents::commands::validate_config::ValidateConfigCommand;
use documents::github::{load_config, Client, GitHubClient, GitHubError};
use documents::OutputFormat;
use documents::processing::RepositoryProcessor;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    ExportFragments {
        /// GitHub repository to export fragments from
        repository: String,
        #[arg(long, short, help = "Output directory for generated fragments")]
        output: Option<PathBuf>,
        #[arg(long, help = "Output format", value_enum, default_value = "files")]
        format: OutputFormat,
        #[arg(long, help = "Include fragment metadata in export")]
        include_metadata: bool,
        #[arg(long, help = "Compress exported fragments into archive")]
        compress: bool,
        #[arg(long, help = "Filter fragments by type (e.g., 'text', 'code')", value_delimiter = ',')]
        fragment_type: Option<String>,
    },
    ListAll,
    ProcessRepo {
        repository: String,
        #[arg(long, short, help = "Output directory for generated fragments")]
        output: Option<PathBuf>,
        #[arg(long, help = "Output format", value_enum, default_value = "files")]
        format: OutputFormat,
        #[arg(long, help = "Force reprocessing even if output exists")]
        force: bool,
        #[arg(long, help = "Verbose progress reporting")]
        verbose: bool,
    },
    Serve {
        #[arg(long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "info")]
        log_level: String,
    },
    ValidateConfig {
        /// GitHub repository to validate configuration for
        repository: String,
        #[arg(short, long, help = "Check if referenced files actually exist in the repository")]
        check_files: bool,
        #[arg(short, long, help = "Base directory for resolving relative paths in the config file (defaults to repository root)")]
        base_dir: Option<String>,
    }
}

// Load configuration from the environment or file
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize logging
    let log_level = match &cli.command {
        Some(Commands::Serve { log_level, .. }) => log_level.clone(),
        Some(Commands::ProcessRepo { verbose: true, .. }) => "debug".to_string(),
        _ => "info".to_string(),
    };

    let _ = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| {
                format!("{}={},tower_http=debug,axum=debug", env!("CARGO_PKG_NAME"), log_level).into()
            })
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .try_init();


    // Load configuration
    let config = load_config()?;

    // Initialize GitHub client
    let github = GitHubClient::new(&config).await?;

    // Test authentication
    println!("Authenticated as {}", github.current_user().await?);
    println!("Using organization: {}", github.organization);

    match cli.command {
        Some(Commands::ExportFragments { .. }) => {
            tracing::warn!("The 'export-fragments' command is not implemented yet.");
            eprintln!("The 'export-fragments' command is not implemented yet.");
            std::process::exit(1);
        }
        Some(Commands::ListAll) => {
            ListAllCommand::execute(&github).await?;
        }
        Some(Commands::ProcessRepo { repository, output, format, force, verbose }) => {
            let command = ProcessRepositoryCommand::new(repository, output, format, force, verbose);
            command.execute(&github).await?;
        }
        Some(Commands::Serve { port , log_level }) => {
            ServeWebhookCommand::execute(port, log_level).await?;
        }
        Some(Commands::ValidateConfig { repository, check_files, base_dir }) => {
            let command = ValidateConfigCommand::new(repository, check_files, base_dir);
            command.execute(github).await?
        }
        None => {
            let _ = tracing_subscriber::fmt::try_init();
            tracing::info!("No command provided. Use --help to see available commands.");
        }
    }

    Ok(())
}


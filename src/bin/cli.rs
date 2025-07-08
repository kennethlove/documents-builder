use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use documents::commands::export_fragments::{ExportFragmentsArgs, ExportFragmentsCommand};
use documents::commands::list_all::ListAllCommand;
use documents::commands::process_repository::{ProcessRepositoryArgs, ProcessRepositoryCommand};
use documents::commands::serve_webhook::{ServeWebhookArgs, ServeWebhookCommand};
use documents::commands::validate_config::{ValidateConfigArgs, ValidateConfigCommand};
use documents::github::{load_config, Client, GitHubClient};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    ExportFragments(ExportFragmentsArgs),
    ListAll,
    ProcessRepo(ProcessRepositoryArgs),
    Serve(ServeWebhookArgs),
    ValidateConfig(ValidateConfigArgs),
}

// Load configuration from the environment or file
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize logging
    let log_level = match &cli.command {
        Some(Commands::Serve(ServeWebhookArgs { log_level, ..})) => log_level.clone(),
        Some(Commands::ProcessRepo(ProcessRepositoryArgs { verbose: true, .. })) => "debug".to_string(),
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
        Some(Commands::ExportFragments(args)) => {
            let command = ExportFragmentsCommand::new(args);
            command.execute(&github).await?;
        }
        Some(Commands::ListAll) => {
            ListAllCommand::execute(&github).await?;
        }
        Some(Commands::ProcessRepo(args)) => {
            let command = ProcessRepositoryCommand::new(args);
            command.execute(&github).await?;
        }
        Some(Commands::Serve(args)) => {
            ServeWebhookCommand::execute(args).await?;
        }
        Some(Commands::ValidateConfig(args)) => {
            let command = ValidateConfigCommand::new(args);
            command.execute(&github).await?
        }
        None => {
            let _ = tracing_subscriber::fmt::try_init();
            tracing::info!("No command provided. Use --help to see available commands.");
        }
    }

    Ok(())
}


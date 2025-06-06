use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use documents::github::{load_config, GitHubClient, GitHubError};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Scan {
        repository: String,
    },
    ListAll,
    Serve {
        #[arg(long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "info")]
        log_level: String,
    },
}

// Load configuration from environment or file
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize logging
    let log_level = match &cli.command {
        Some(Commands::Serve { log_level, .. }) => log_level.clone(),
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
        Some(Commands::Scan { repository }) => {
            let _ = tracing_subscriber::fmt::try_init();

            let config = github.get_project_config(repository.as_str()).await?;
            dbg!(&config);
        }
        Some(Commands::ListAll) => {
            let _ = tracing_subscriber::fmt::try_init();

            // Check for repositories with the config file.
            for repository in github.repositories().await? {
                match github.get_project_config(repository.as_str()).await {
                    Ok(_) => println!("Found config in repository: {}", repository),
                    Err(GitHubError::ConfigFileNotFound(_)) => println!("No config found in repository: {}", repository),
                    Err(e) => eprintln!("Error checking repository {}: {}", repository, e),
                }
            }
        }
        Some(Commands::Serve { port , ..}) => {
            tracing::info!("Starting documentation webhook server");
            tracing::info!("Server will listen on http://0.0.0.0:{}", port);
            tracing::info!("Health check available at http://0.0.0.0:{}/health", port);
            tracing::info!("GitHub webhook endpoint at http://0.0.0.0:{}/webhooks/github", port);
            documents::web::start_server(port).await?;
        }
        None => {
            let _ = tracing_subscriber::fmt::try_init();
            tracing::info!("No command provided. Use --help to see available commands.");
        }
    }

    Ok(())
}

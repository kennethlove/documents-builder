use std::path::PathBuf;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use documents::github::{load_config, GitHubClient, GitHubError};
use documents::processing::RepositoryProcessor;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum OutputFormat {
    Files,
    Html,
    Json,
}

#[derive(Subcommand, Debug)]
enum Commands {
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
    Scan {
        repository: String,
    },
    Serve {
        #[arg(long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "info")]
        log_level: String,
    },
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
        Some(Commands::ProcessRepo { repository, output, format, force, verbose }) => {
            let output_dir = output.unwrap_or_else(|| {
                PathBuf::from("output").join(&repository)
            });

            if verbose {
                tracing::debug!("Processing repository: {}", repository);
                tracing::debug!("Output directory: {}", output_dir.display());
                tracing::debug!("Output format: {:?}", format);
                tracing::debug!("Force reprocessing: {}", force);
            }

            match github.get_project_config(repository.as_str()).await {
                Ok(config) => {
                    tracing::info!("Found configuration for repository: {}", repository);
                    if verbose {
                        tracing::debug!("Repository configuration: {:#?}", config);
                    }

                    // Create processor and run processing
                    let processor = RepositoryProcessor::new(github, config, repository.clone());

                    match processor.process(verbose).await {
                        Ok(result) => {
                            // Create output directory
                            std::fs::create_dir_all(&output_dir)?;

                            match format {
                                OutputFormat::Files => {
                                    // Save each fragment to a file
                                    for fragment in &result.fragments {
                                        let filename = format!("{}-{:?}.md",
                                            fragment.file_path.replace('/', "_"),
                                            fragment.fragment_type
                                        );
                                        let fragment_file = output_dir.join(filename);
                                        std::fs::write(&fragment_file, &fragment.content)?;
                                    }
                                    tracing::info!("Processing completed. Files saved in: {}", output_dir.display());
                                },
                                OutputFormat::Json => {
                                    let output_file = output_dir.join("fragments.json");
                                    let json_content = serde_json::to_string_pretty(&result)?;
                                    std::fs::write(&output_file, json_content)?;
                                    tracing::info!("Processing completed. JSON output saved in: {}", output_file.display());
                                },
                                OutputFormat::Html => {
                                    tracing::info!("HTML output format is not yet implemented.");
                                },
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

                            tracing::info!("Repository {} processed successfully.", repository);
                        }
                        Err(e) => {
                            tracing::error!("Error processing repository {}: {}", repository, e);
                            eprintln!("Error processing repository {}: {}", repository, e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(GitHubError::ConfigFileNotFound(_)) => {
                    tracing::error!("No configuration file found in repository: {}", repository);
                    eprintln!("No configuration file found in repository: {}", repository);
                    std::process::exit(1);
                }
                Err(e) => {
                    tracing::error!("Error retrieving configuration for repository: {}: {}", repository, e);
                    eprintln!("Error retrieving configuration for repository {}: {}", repository, e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Scan { repository }) => {
            let _ = tracing_subscriber::fmt::try_init();

            let config = github.get_project_config(repository.as_str()).await?;
            dbg!(&config);
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

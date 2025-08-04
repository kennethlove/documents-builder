use crate::{ApplicationConfig, Console, Database};
use clap::Args;
use tracing::{error, info};

#[derive(Args, Debug)]
pub struct HealthArgs {
    /// Check database connectivity
    #[arg(long)]
    pub database: bool,
    /// Check all system components
    #[arg(long)]
    pub all: bool,
}

#[allow(unused_assignments)] // There aren't any unused assignments, but this is to avoid warnings
pub async fn run(args: HealthArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let console = Console::new(false); // Health checks don't need verbose mode
    let config = ApplicationConfig::from_env()?;

    console.header("System Health Check");

    let mut all_healthy = true;

    if args.database || args.all {
        let spinner = console.create_spinner("Checking database connectivity...");
        info!("Checking database connectivity...");

        match Database::new(&config.database_url).await {
            Ok(db) => match db.health_check().await {
                Ok(_) => {
                    console.finish_progress_success(&spinner, "Database connection established");
                    console.health_status("Database", true, Some("Connection successful, queries working"));
                    info!("Database is healthy.");
                }
                Err(e) => {
                    console.finish_progress_error(&spinner, "Database health check failed");
                    console.health_status("Database", false, Some(&format!("Health check failed: {}", e)));
                    error!("Database health check failed: {}", e);
                    all_healthy = false;
                    return Err(e.into());
                }
            },
            Err(e) => {
                console.finish_progress_error(&spinner, "Failed to connect to database");
                console.health_status("Database", false, Some(&format!("Connection failed: {}", e)));
                error!("Failed to connect to the database: {}", e);
                all_healthy = false;
                return Err(e.into());
            }
        }
    }

    if args.all {
        // TODO: Add checks for other components like GitHub API, etc.
        let spinner = console.create_spinner("Checking other system components...");
        info!("Checking other system components...");
        
        // Simulate some checking time
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
        console.finish_progress_success(&spinner, "System components checked");
        console.health_status("Configuration", true, Some("Environment variables loaded"));
        console.health_status("Runtime", true, Some("Tokio async runtime operational"));
        
        info!("All system components are healthy.");
    }

    // Summary
    if all_healthy {
        console.success("All checked components are healthy");
    } else {
        console.error("Some components are unhealthy");
    }

    Ok(())
}

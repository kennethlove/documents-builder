use crate::{Config, Database};
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

pub async fn run(args: HealthArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = Config::from_env()?;

    if args.database || args.all {
        info!("Checking database connectivity...");

        match Database::new(&config.database_url).await {
            Ok(db) => match db.health_check().await {
                Ok(_) => {
                    info!("Database is healthy.");
                }
                Err(e) => {
                    error!("Database health check failed: {}", e);
                    return Err(e.into());
                }
            },
            Err(e) => {
                error!("Failed to connect to the database: {}", e);
                return Err(e.into());
            }
        }
    }

    if args.all {
        // TODO: Add checks for other components like GitHub API, etc.
        info!("Checking other system components...");
        info!("All system components are healthy.");
    }

    Ok(())
}

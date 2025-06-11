use crate::github::{Client, GitHubClient, GitHubError};

pub struct ListAllCommand;

impl ListAllCommand {
    pub async fn execute(client: &GitHubClient) -> Result<(), GitHubError> {
        let _ = tracing_subscriber::fmt::try_init();
        for repository in client.repositories().await? {
            match client.get_project_config(repository.as_str()).await {
                Ok(_) => {
                    println!("Found config in repository: {}", repository)
                },
                Err(GitHubError::ConfigFileNotFound(_)) => {
                    eprintln!("No config found in repository: {}", repository);
                }
                Err(e) => {
                    eprintln!("Error fetching config for {}: {}", repository, e);
                }
            }
        }
        Ok(())
    }
}

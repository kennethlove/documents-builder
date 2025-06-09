use octocrab::{Octocrab, OctocrabBuilder};
use serde::{Deserialize};
use crate::ProjectConfig;

#[derive(thiserror::Error, Debug)]
pub enum GitHubError {
    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    #[error("API rate limit exceeded")]
    RateLimitExceeded,

    #[error("Organization not found: {0}")]
    OrganizationNotFound(String),

    #[error("API error: {0}")]
    ApiError(#[from] octocrab::Error),
    
    #[error("Repository not found: {0}")]
    RepositoryNotFound(String),
    
    #[error("Configuration file not found in repository: {0}")]
    ConfigFileNotFound(String),
    
    #[error("Failed to read configuration file: {0}")]
    ConfigFileReadError(String),
    
    #[error("Config file is empty in repository: {0}")]
    ConfigFileEmpty(String),
    
    #[error("Environment variable not set: {0}")]
    EnvVarNotSet(#[from] std::env::VarError),
    #[error("Failed to request file: {0}")]
    RequestFailed(String),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Failed to decode content for file: {0}")]
    InvalidFormat(String),
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub github_token: String,
    pub organization: String,
}

pub fn load_config() -> Result<Config, GitHubError> {
    // Using dotenvy for .env file loading
    dotenvy::dotenv().ok();

    let config = Config {
        github_token: std::env::var("GITHUB_API_TOKEN")?,
        organization: std::env::var("GITHUB_ORGANIZATION")?,
    };

    Ok(config)
}

pub struct GitHubClient {
    pub client: Octocrab,
    pub organization: String,
}

impl GitHubClient {
    pub async fn new(config: &Config) -> Result<Self, GitHubError> {
        let client = OctocrabBuilder::new()
            .personal_token(config.github_token.clone())
            .build()?;

        Ok(Self {
            client,
            organization: config.organization.clone(),
        })
    }

    pub async fn current_user(&self) -> Result<String, GitHubError> {
        // Test authentication by making a simple API call
        let current_user = self.client.current().user().await?;
        Ok(current_user.login)
    }

    pub async fn handle_rate_limits(&self) -> Result<(), GitHubError> {
        let rate_limit = self.client
            .ratelimit()
            .get()
            .await
            .map_err(GitHubError::ApiError)?;

        if rate_limit.rate.remaining == 0 {
            return Err(GitHubError::RateLimitExceeded);
        }

        Ok(())
    }
    
    pub async fn repositories(&self) -> Result<Vec<String>, GitHubError> {
        let repos = self.client
            .orgs(&self.organization)
            .list_repos()
            .send()
            .await
            .map_err(GitHubError::ApiError)?;

        Ok(repos.items.into_iter().map(|repo| repo.name).collect())
    }
    
    pub async fn scan_for_config_file(&self, repo_name: &str) -> Result<Option<String>, GitHubError> {
        let repo_name = repo_name.trim();
        let contents = self.client
            .repos(&self.organization, repo_name)
            .get_content()
            .path("documents.toml")
            .send()
            .await
            .map_err(|_| { GitHubError::ConfigFileNotFound(repo_name.to_string())})?;
        
        if contents.items.is_empty() {
            return Ok(None);
        }
        
        // Check if the file exists in the repository
        let mut url = String::new();
        for item in contents.items {
            if item.name == "documents.toml" {
                url = item.download_url.unwrap_or_default();
                break;
            }
        }
        Ok(Some(url))
    }
    
    pub async fn read_config_file(&self, repo_name: &str) -> Result<String, GitHubError> {
        let contents = self.client
            .repos(&self.organization, repo_name)
            .get_content()
            .path("documents.toml")
            .send()
            .await
            .map_err(GitHubError::ApiError)?;

        if contents.items.is_empty() {
            return Err(GitHubError::ConfigFileEmpty(repo_name.to_string()));
        }

        let item = contents.items.first().ok_or(GitHubError::ConfigFileNotFound(repo_name.into()))?;
        let content = item.decoded_content().ok_or(GitHubError::ConfigFileReadError(repo_name.into()))?;
        
        Ok(content.clone())
    }
    
    pub async fn get_project_config(&self, repo_name: &str) -> Result<ProjectConfig, GitHubError> {
        if let Err(e) = self.scan_for_config_file(repo_name).await {
            return Err(e);
        }
        let config = self.read_config_file(repo_name).await?;
        if config.is_empty() {
            Err(GitHubError::ConfigFileEmpty(repo_name.to_string()))
        } else  {
            toml::from_str(&config).map_err(|e| GitHubError::ConfigFileReadError(format!("Failed to parse config: {}", e)))
        }
    }

    pub async fn get_file_content(&self, repo_name: &str, file_path: &str) -> Result<String, GitHubError> {
        let content = self.client
            .repos(&self.organization, repo_name)
            .get_content()
            .path(file_path)
            .send()
            .await
            .map_err(|e| GitHubError::RequestFailed(format!("Failed to get file content: {}", e)))?;

        let file_content = content.items.first()
            .ok_or_else(|| GitHubError::FileNotFound(format!("File not found: {}", file_path)))?;

        let decoded_content = file_content.decoded_content()
            .ok_or_else(|| GitHubError::InvalidFormat(format!("Failed to decode content for file: {}", file_path)))?;

        Ok(decoded_content)
    }
    
    pub async fn file_exists(&self, repo_name: &str, file_path: &str) -> Result<bool, GitHubError> {
        let content = self.client
            .repos(&self.organization, repo_name)
            .get_content()
            .path(file_path)
            .send()
            .await
            .map_err(GitHubError::ApiError)?;

        Ok(!content.items.is_empty())
    }
}

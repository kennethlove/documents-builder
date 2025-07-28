use crate::Config;
use crate::ProjectConfig;
use async_trait::async_trait;
use octocrab::{Octocrab, OctocrabBuilder};

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

#[derive(Debug, Clone)]
pub struct RepositoryFile {
    pub path: String,
    pub name: String,
    pub size: Option<u64>,
    pub file_type: String,
}

#[async_trait]
pub trait Client {
    async fn current_user(&self) -> Result<String, GitHubError>;

    async fn handle_rate_limits(&self) -> Result<(), GitHubError>;

    async fn repositories(&self) -> Result<Vec<String>, GitHubError>;

    async fn scan_for_config_file(&self, repo_name: &str) -> Result<Option<String>, GitHubError>;

    async fn read_config_file(&self, repo_name: &str) -> Result<String, GitHubError>;

    async fn get_project_config(&self, repo_name: &str) -> Result<ProjectConfig, GitHubError>;

    async fn get_file_content(
        &self,
        repo_name: &str,
        file_path: &str,
    ) -> Result<String, GitHubError>;

    async fn file_exists(&self, repo_name: &str, file_path: &str) -> Result<bool, GitHubError>;

    async fn list_repository_files(
        &self,
        repo_name: &str,
        path: Option<&str>,
    ) -> Result<Vec<RepositoryFile>, GitHubError>;
}

#[derive(Clone, Debug)]
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
            organization: config.github_organization.clone(),
        })
    }
}

#[async_trait]
impl Client for GitHubClient {
    async fn current_user(&self) -> Result<String, GitHubError> {
        // Test authentication by making a simple API call
        let current_user = self.client.current().user().await?;
        Ok(current_user.login)
    }

    async fn handle_rate_limits(&self) -> Result<(), GitHubError> {
        let rate_limit = self
            .client
            .ratelimit()
            .get()
            .await
            .map_err(GitHubError::ApiError)?;

        if rate_limit.rate.remaining == 0 {
            return Err(GitHubError::RateLimitExceeded);
        }

        Ok(())
    }

    async fn repositories(&self) -> Result<Vec<String>, GitHubError> {
        let repos = self
            .client
            .orgs(&self.organization)
            .list_repos()
            .send()
            .await
            .map_err(GitHubError::ApiError)?;

        Ok(repos.items.into_iter().map(|repo| repo.name).collect())
    }

    async fn scan_for_config_file(&self, repo_name: &str) -> Result<Option<String>, GitHubError> {
        let repo_name = repo_name.trim();
        let contents = self
            .client
            .repos(&self.organization, repo_name)
            .get_content()
            .path("documents.toml")
            .send()
            .await
            .map_err(|_| GitHubError::ConfigFileNotFound(repo_name.to_string()))?;

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

    async fn read_config_file(&self, repo_name: &str) -> Result<String, GitHubError> {
        let contents = self
            .client
            .repos(&self.organization, repo_name)
            .get_content()
            .path("documents.toml")
            .send()
            .await
            .map_err(GitHubError::ApiError)?;

        if contents.items.is_empty() {
            return Err(GitHubError::ConfigFileEmpty(repo_name.to_string()));
        }

        let item = contents
            .items
            .first()
            .ok_or(GitHubError::ConfigFileNotFound(repo_name.into()))?;
        let content = item
            .decoded_content()
            .ok_or(GitHubError::ConfigFileReadError(repo_name.into()))?;

        Ok(content.clone())
    }

    async fn get_project_config(&self, repo_name: &str) -> Result<ProjectConfig, GitHubError> {
        if let Err(e) = self.scan_for_config_file(repo_name).await {
            return Err(e);
        }
        let config = self.read_config_file(repo_name).await?;
        if config.is_empty() {
            Err(GitHubError::ConfigFileEmpty(repo_name.to_string()))
        } else {
            toml::from_str(&config).map_err(|e| {
                GitHubError::ConfigFileReadError(format!("Failed to parse config: {}", e))
            })
        }
    }

    async fn get_file_content(
        &self,
        repo_name: &str,
        file_path: &str,
    ) -> Result<String, GitHubError> {
        let content = self
            .client
            .repos(&self.organization, repo_name)
            .get_content()
            .path(file_path)
            .send()
            .await
            .map_err(|e| {
                GitHubError::RequestFailed(format!("Failed to get file content: {}", e))
            })?;

        let file_content = content
            .items
            .first()
            .ok_or_else(|| GitHubError::FileNotFound(format!("File not found: {}", file_path)))?;

        let decoded_content = file_content.decoded_content().ok_or_else(|| {
            GitHubError::InvalidFormat(format!("Failed to decode content for file: {}", file_path))
        })?;

        Ok(decoded_content)
    }

    async fn file_exists(&self, repo_name: &str, file_path: &str) -> Result<bool, GitHubError> {
        let content = self
            .client
            .repos(&self.organization, repo_name)
            .get_content()
            .path(file_path)
            .send()
            .await
            .map_err(GitHubError::ApiError)?;

        Ok(!content.items.is_empty())
    }

    async fn list_repository_files(
        &self,
        repo_name: &str,
        path: Option<&str>,
    ) -> Result<Vec<RepositoryFile>, GitHubError> {
        let path = path.unwrap_or("");
        let contents = self
            .client
            .repos(&self.organization, repo_name)
            .get_content()
            .path(path)
            .send()
            .await
            .map_err(GitHubError::ApiError)?;

        let mut files = Vec::new();
        for item in contents.items {
            files.push(RepositoryFile {
                path: item.path,
                name: item.name,
                size: Some(item.size as u64),
                file_type: item.r#type,
            });
        }

        Ok(files)
    }
}

#[cfg(test)]
pub mod tests {
    use crate::github::{Client, GitHubError, RepositoryFile};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::path::PathBuf;

    // Mock implementation of the Client trait for testing
    pub struct MockGitHubClient {
        file_contents: HashMap<String, String>,
        files: Vec<RepositoryFile>,
    }

    impl MockGitHubClient {
        pub fn new() -> Self {
            Self {
                file_contents: HashMap::new(),
                files: Vec::new(),
            }
        }

        pub fn add_file(&mut self, path: &str, content: &str) {
            self.file_contents
                .insert(path.to_string(), content.to_string());
            self.files.push(RepositoryFile {
                path: path.to_string(),
                name: path.split('/').last().unwrap_or(path).to_string(),
                size: Some(content.len() as u64),
                file_type: "file".to_string(),
            });
        }

        pub fn add_directory(&mut self, path: &str) {
            self.files.push(RepositoryFile {
                path: path.to_string(),
                name: path.split('/').last().unwrap_or(path).to_string(),
                size: None,
                file_type: "dir".to_string(),
            });
        }
    }

    #[async_trait]
    impl Client for MockGitHubClient {
        async fn current_user(&self) -> Result<String, GitHubError> {
            Ok("test-user".to_string())
        }

        async fn handle_rate_limits(&self) -> Result<(), GitHubError> {
            Ok(())
        }

        async fn repositories(&self) -> Result<Vec<String>, GitHubError> {
            Ok(vec!["test-repo".to_string()])
        }

        async fn scan_for_config_file(
            &self,
            _repo_name: &str,
        ) -> Result<Option<String>, GitHubError> {
            Ok(Some("documents.toml".to_string()))
        }

        async fn read_config_file(&self, _repo_name: &str) -> Result<String, GitHubError> {
            Ok("[project]\nname = \"Test Project\"\ndescription = \"A test project\"".to_string())
        }

        async fn get_project_config(
            &self,
            _repo_name: &str,
        ) -> Result<crate::ProjectConfig, GitHubError> {
            let mut documents = HashMap::new();
            documents.insert(
                "doc1".to_string(),
                crate::DocumentConfig {
                    title: "Document 1".to_string(),
                    path: Some(PathBuf::from("docs/file1.md")),
                    sub_documents: None,
                },
            );

            Ok(crate::ProjectConfig {
                project: crate::ProjectDetails {
                    name: "Test Project".to_string(),
                    description: "A test project".to_string(),
                },
                documents,
            })
        }

        async fn get_file_content(
            &self,
            _repo_name: &str,
            file_path: &str,
        ) -> Result<String, GitHubError> {
            match self.file_contents.get(file_path) {
                Some(content) => Ok(content.clone()),
                None => Err(GitHubError::FileNotFound(format!(
                    "File not found: {}",
                    file_path
                ))),
            }
        }

        async fn file_exists(
            &self,
            _repo_name: &str,
            file_path: &str,
        ) -> Result<bool, GitHubError> {
            Ok(self.file_contents.contains_key(file_path))
        }

        async fn list_repository_files(
            &self,
            _: &str,
            path: Option<&str>,
        ) -> Result<Vec<RepositoryFile>, GitHubError> {
            let search_path = path.unwrap_or("");
            let mut result = Vec::new();

            for file in &self.files {
                let file_dir = if file.path.contains('/') {
                    file.path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
                } else {
                    ""
                };

                if file_dir == search_path {
                    result.push(file.clone());
                }
            }

            Ok(result)
        }
    }
}

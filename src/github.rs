use crate::Config;
use crate::ProjectConfig;
use async_trait::async_trait;
use octocrab::{Octocrab, OctocrabBuilder};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

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
    #[error("GraphQL error: {0}")]
    GraphQLError(String),
    #[error("Batch operation failed: {0}")]
    BatchOperationFailed(String),
    #[error("Query complexity exceeded")]
    QueryComplexityExceeded,
}

#[derive(Debug, Clone)]
pub struct RepositoryFile {
    pub path: String,
    pub name: String,
    pub size: Option<u64>,
    pub file_type: String,
}

/// Represents a file in a repository with its content
#[derive(Debug, Clone)]
pub struct RepositoryFileContent {
    /// Name of the repository
    pub repo_name: String,
    /// Whether the file exists in the repository
    pub exists: bool,
    /// Content of the file, if it exists
    pub content: Option<String>,
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

    /// Batch fetch multiple files from a single repository using GraphQL
    ///
    /// This method uses GitHub's GraphQL API to efficiently fetch the content of multiple files
    /// from a single repository in one request, reducing the number of API calls.
    ///
    /// # Arguments
    ///
    /// * `repo_name` - The name of the repository
    /// * `file_paths` - A vector of file paths to fetch
    ///
    /// # Returns
    ///
    /// * `Result<HashMap<String, Option<String>>, GitHubError>` - A map of file paths to their content.
    ///   The value is `Some(content)` if the file exists, or `None` if it doesn't exist.
    async fn batch_fetch_files(
        &self,
        repo_name: &str,
        file_paths: &[String],
    ) -> Result<HashMap<String, Option<String>>, GitHubError>;
    
    /// Batch check multiple repositories for the existence of the documents.toml configuration file using GraphQL
    ///
    /// This method uses GitHub's GraphQL API to efficiently check multiple repositories
    /// at once for the existence of the documents.toml configuration file, reducing the number of API calls.
    ///
    /// # Returns
    ///
    /// * `Result<HashMap<String, bool>, GitHubError>` - A map of repository names to a boolean
    ///   indicating whether the documents.toml configuration file exists in that repository
    async fn batch_check_config_file_exists(&self) -> Result<HashMap<String, bool>, GitHubError>;
    
    /// Batch fetch documents.toml configuration file content from multiple repositories using GraphQL
    ///
    /// This method uses GitHub's GraphQL API to efficiently check multiple repositories
    /// at once for the existence of the documents.toml configuration file and fetch its content if it exists,
    /// reducing the number of API calls compared to checking and fetching separately.
    ///
    /// # Returns
    ///
    /// * `Result<Vec<RepositoryFileContent>, GitHubError>` - A vector of repository file content information
    ///   including repository name, whether the documents.toml file exists, and the file content if it exists
    async fn batch_fetch_config_file_content(&self) -> Result<Vec<RepositoryFileContent>, GitHubError>;

    /// Enhanced batch operation to fetch files from multiple repositories at once
    async fn batch_fetch_files_multi_repo(
        &self,
        repo_file_map: &HashMap<String, Vec<String>>,
    ) -> Result<HashMap<String, HashMap<String, Option<String>>>, GitHubError>;

    /// Validate that all files referenced in configurations exist and are accessible
    async fn batch_validate_referenced_files(
        &self,
        file_references: &HashMap<String, Vec<String>>,
    ) -> Result<HashMap<String, HashMap<String, bool>>, GitHubError>;
}

#[derive(Clone, Debug)]
pub struct GitHubClient {
    pub client: Octocrab,
    pub organization: String,
    rate_limit_buffer: u32, // Number of requests to keep as a buffer
}

impl GitHubClient {
    pub async fn new(config: &Config) -> Result<Self, GitHubError> {
        let client = OctocrabBuilder::new()
            .personal_token(config.github_token.clone())
            .build()?;

        Ok(Self {
            client,
            organization: config.github_organization.clone(),
            rate_limit_buffer: 100, // Keep 100 requests as a buffer
        })
    }

    /// Enhanced rate limit handling with exponential backoff and proactive throttling
    async fn check_and_handle_rate_limits(&self) -> Result<(), GitHubError> {
        const MAX_RETRIES: u32 = 3;
        let mut retry_count = 0;

        loop {
            let rate_limit = self.client.ratelimit().get().await.map_err(GitHubError::ApiError)?;

            // If we're getting close to the limit, wait
            if rate_limit.rate.remaining <= self.rate_limit_buffer as usize {
                let reset_time = chrono::DateTime::from_timestamp(rate_limit.rate.reset as i64, 0)
                    .unwrap_or_else(|| chrono::Utc::now());
                let reset_duration = reset_time.signed_duration_since(chrono::Utc::now());

                if reset_duration.num_seconds() > 0 && reset_duration.num_seconds() < 3600 {
                    tracing::info!(
                        "Rate limit approaching (remaining: {}), waiting {} seconds until reset",
                        rate_limit.rate.remaining,
                        reset_duration.num_seconds()
                    );

                    sleep(Duration::from_secs(reset_duration.num_seconds() as u64 + 1)).await;
                    continue;
                }
            }

            // If we have sufficient requests remaining, proceed
            if rate_limit.rate.remaining > 0 {
                return Ok(());
            }

            // If we're completely rate-limited, implement retry with backoff
            retry_count += 1;
            if retry_count > MAX_RETRIES {
                return Err(GitHubError::RateLimitExceeded);
            }

            let backoff_duration = Duration::from_secs(2_u64.pow(retry_count));
            tracing::warn!(
                "Rate limit exceeded, retrying in {} seconds (attempt {}/{})",
                backoff_duration.as_secs(),
                retry_count,
                MAX_RETRIES
            );

            sleep(backoff_duration).await;
        }
    }

    /// Execute GraphQL query with automatic rate limiting and retry logic
    async fn execute_graphql_with_retries(
        &self,
        query: &serde_json::Value,
    ) -> Result<serde_json::Value, GitHubError> {
        const MAX_RETRIES: u32 = 3;
        let mut retry_count = 0;

        loop {
            // Check rate limits before making the request
            self.check_and_handle_rate_limits().await?;

            let query_response: Result<serde_json::Value, _> = self.client.graphql(query).await;

            match query_response {
                Ok(response) => {
                    // Check for GraphQL errors in the response
                    if let Some(errors) = response.get("errors") {
                        let error_msg = errors.to_string();

                        // Handle specific GraphQL errors
                        if error_msg.contains("rate limit") || error_msg.contains("RATE_LIMITED") {
                            retry_count += 1;
                            if retry_count > MAX_RETRIES {
                                return Err(GitHubError::RateLimitExceeded);
                            }

                            let backoff_duration = Duration::from_secs(2_u64.pow(retry_count));
                            tracing::warn!("GraphQL rate limited, retrying in {} seconds (attempt {}/{})",
                                backoff_duration.as_secs(),
                                retry_count,
                                MAX_RETRIES
                            );
                            sleep(backoff_duration).await;
                            continue;
                        }

                        if error_msg.contains("complexity") {
                            return Err(GitHubError::QueryComplexityExceeded);
                        }

                        return Err(GitHubError::GraphQLError(error_msg));
                    }

                    return Ok(response);
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count > MAX_RETRIES {
                        return Err(GitHubError::ApiError(e));
                    }

                    let backoff_duration = Duration::from_secs(2_u64.pow(retry_count));
                    tracing::warn!("GraphQL request failed, retrying in {} seconds (attempt {}/{})",
                        backoff_duration.as_secs(),
                        retry_count,
                        MAX_RETRIES
                    );
                    sleep(backoff_duration).await;
                }
            }
        }
    }

    /// Split large file lists into smaller batches to avoid GraphQL complexity limits
    fn create_file_batches(&self, file_paths: &[String], batch_size: usize) -> Vec<Vec<String>> {
        file_paths
            .chunks(batch_size)
            .map(|chunk| chunk.to_vec())
            .collect()
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
        self.check_and_handle_rate_limits().await
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

    async fn batch_fetch_files(
        &self,
        repo_name: &str,
        file_paths: &[String],
    ) -> Result<HashMap<String, Option<String>>, GitHubError> {
        if file_paths.is_empty() {
            return Ok(HashMap::new());
        }

        let mut final_result = HashMap::new();

        // Split into batches to avoid complexity limits
        let batches = self.create_file_batches(file_paths, 50); // 50 files per batch

        for batch in batches {
            // Build GraphQL query to fetch multiple files from a single repository
            let mut file_queries = Vec::new();
            for (index, file_path) in batch.iter().enumerate() {
                file_queries.push(format!(
                    r#"file{}: object(expression: "HEAD:{}") {{
                      ... on Blob {{
                        id
                        text
                      }}
                    }}"#,
                    index,
                    file_path
                ));
            }

            let query = format!(
                r#"
                query {{
                  repository(owner: "{org}", name: "{repo}") {{
                    {files}
                  }}
                }}
                "#,
                org = self.organization,
                repo = repo_name,
                files = file_queries.join("\n                      ")
            );

            let query = serde_json::json!({"query": &query});

            // Execute the GraphQL query with retries
            let response = self.execute_graphql_with_retries(&query).await?;

            // Extract file data from response
            let repository = response["data"]["repository"]
                .as_object()
                .ok_or_else(|| GitHubError::RequestFailed("Invalid GraphQL response format".to_string()))?;

            // Process each file in this batch
            for (index, file_path) in batch.iter().enumerate() {
                let file_key = format!("file{}", index);
                let file_object = &repository[&file_key];

                // Check if the file exists (object will be null if file doesn't exist)
                let content = if !file_object.is_null() {
                    file_object["text"].as_str().map(|s| s.to_string())
                } else {
                    None
                };

                final_result.insert(file_path.clone(), content);
            }
        }

        Ok(final_result)
    }

    async fn batch_check_config_file_exists(&self) -> Result<HashMap<String, bool>, GitHubError> {
        let mut result = HashMap::new();
        let mut cursor: Option<String> = None;

        // Handle pagination to get all repositories
        loop {
            // Create the GraphQL query to check for documents.toml configuration file
            let query = format!(
                r#"
                query {{
                  organization(login: "{org}") {{
                    repositories(first: 100, after: {cursor}) {{
                      pageInfo {{
                        hasNextPage
                        endCursor
                      }}
                      nodes {{
                        name
                        object(expression: "HEAD:documents.toml") {{
                          ... on Blob {{
                            id
                          }}
                        }}
                      }}
                    }}
                  }}
                }}
                "#,
                org = self.organization,
                cursor = match &cursor {
                    Some(c) => format!("\"{}\"", c),
                    None => "null".to_string()
                }
            );

            let query = serde_json::json!({"query": &query});

            // Execute the GraphQL query
            let response = self.execute_graphql_with_retries(&query).await?;

            // Extract repository data from response
            let repos = response["data"]["organization"]["repositories"]["nodes"]
                .as_array()
                .ok_or_else(|| GitHubError::RequestFailed("Invalid GraphQL response format".to_string()))?;

            // Process each repository
            for repo in repos {
                let repo_name = repo["name"]
                    .as_str()
                    .ok_or_else(|| GitHubError::RequestFailed("Invalid repository name in response".to_string()))?;

                // Check if the file exists (object will be null if file doesn't exist)
                let file_exists = !repo["object"].is_null();
                result.insert(repo_name.to_string(), file_exists);
            }

            // Check if there are more pages
            let has_next_page = response["data"]["organization"]["repositories"]["pageInfo"]["hasNextPage"]
                .as_bool()
                .unwrap_or(false);

            if !has_next_page {
                break;
            }

            // Update cursor for next page
            cursor = response["data"]["organization"]["repositories"]["pageInfo"]["endCursor"]
                .as_str()
                .map(|s| s.to_string());
        }

        Ok(result)
    }

    async fn batch_fetch_config_file_content(&self) -> Result<Vec<RepositoryFileContent>, GitHubError> {
        let mut result = Vec::new();
        let mut cursor: Option<String> = None;

        // Handle pagination to get all repositories
        loop {
            // Create the GraphQL query for documents.toml file, including text content
            let query = format!(
                r#"
                query {{
                  organization(login: "{org}") {{
                    repositories(first: 100, after: {cursor}) {{
                      pageInfo {{
                        hasNextPage
                        endCursor
                      }}
                      nodes {{
                        name
                        object(expression: "HEAD:documents.toml") {{
                          ... on Blob {{
                            id
                            text
                          }}
                        }}
                      }}
                    }}
                  }}
                }}
                "#,
                org = self.organization,
                cursor = match &cursor {
                    Some(c) => format!("\"{}\"", c),
                    None => "null".to_string()
                }
            );

            let query = serde_json::json!({"query": &query});

            // Execute the GraphQL query
            let response = self.execute_graphql_with_retries(&query).await?;

            // Extract repository data from response
            let repos = response["data"]["organization"]["repositories"]["nodes"]
                .as_array()
                .ok_or_else(|| GitHubError::RequestFailed("Invalid GraphQL response format".to_string()))?;

            // Process each repository
            for repo in repos {
                let repo_name = repo["name"]
                    .as_str()
                    .ok_or_else(|| GitHubError::RequestFailed("Invalid repository name in response".to_string()))?
                    .to_string();

                // Check if the file exists (object will be null if file doesn't exist)
                let file_exists = !repo["object"].is_null();

                // Get the file content if it exists
                let content = if file_exists {
                    repo["object"]["text"].as_str().map(|s| s.to_string())
                } else {
                    None
                };

                result.push(RepositoryFileContent {
                    repo_name,
                    exists: file_exists,
                    content,
                });
            }

            // Check if there are more pages
            let has_next_page = response["data"]["organization"]["repositories"]["pageInfo"]["hasNextPage"]
                .as_bool()
                .unwrap_or(false);

            if !has_next_page {
                break;
            }

            // Update cursor for next page
            cursor = response["data"]["organization"]["repositories"]["pageInfo"]["endCursor"]
                .as_str()
                .map(|s| s.to_string());
        }

        Ok(result)
    }

    async fn batch_fetch_files_multi_repo(
        &self,
        repo_file_map: &HashMap<String, Vec<String>>,
    ) -> Result<HashMap<String, HashMap<String, Option<String>>>, GitHubError> {
        let mut result = HashMap::new();

        for (repo_name, file_paths) in repo_file_map {
            let files = self.batch_fetch_files(repo_name, file_paths).await?;
            result.insert(repo_name.to_string(), files);
        }

        Ok(result)
    }

    async fn batch_validate_referenced_files(
        &self,
        file_references: &HashMap<String, Vec<String>>,
    ) -> Result<HashMap<String, HashMap<String, bool>>, GitHubError> {
        let mut result = HashMap::new();

        for (repo_name, files) in file_references {
            let files = self.batch_fetch_files(repo_name, files).await?;

            // Convert content results to existence checks
            let existence_map: HashMap<String, bool> = files
                .into_iter()
                .map(|(path, content)| (path, content.is_some()))
                .collect();

            result.insert(repo_name.to_string(), existence_map);
        }

        Ok(result)
    }
}

#[cfg(test)]
pub mod tests {
    use crate::github::{Client, GitHubError, RepositoryFile, RepositoryFileContent};
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

        async fn batch_fetch_files(
            &self,
            _repo_name: &str,
            file_paths: &[String],
        ) -> Result<HashMap<String, Option<String>>, GitHubError> {
            let mut result = HashMap::new();
            
            for file_path in file_paths {
                let content = self.file_contents.get(file_path).cloned();
                result.insert(file_path.clone(), content);
            }
            
            Ok(result)
        }

        async fn batch_check_config_file_exists(&self) -> Result<HashMap<String, bool>, GitHubError> {
            let mut result = HashMap::new();

            // For testing, we'll return that test-repo has the documents.toml configuration file
            result.insert("test-repo".to_string(), true);

            Ok(result)
        }

        async fn batch_fetch_config_file_content(&self) -> Result<Vec<RepositoryFileContent>, GitHubError> {
            let mut result = Vec::new();

            // For testing, we'll return that test-repo has the documents.toml configuration file
            result.push(RepositoryFileContent {
                repo_name: "test-repo".to_string(),
                exists: true,
                content: Some("[project]\nname = \"Test Project\"\ndescription = \"A test project\"".to_string()),
            });

            Ok(result)
        }

        async fn batch_fetch_files_multi_repo(&self, repo_file_map: &HashMap<String, Vec<String>>) -> Result<HashMap<String, HashMap<String, Option<String>>>, GitHubError> {
            let mut result = HashMap::new();

            for (repo_name, file_paths) in repo_file_map {
                let files = self.batch_fetch_files(repo_name, file_paths).await?;
                result.insert(repo_name.clone(), files);
            }

            Ok(result)
        }

        async fn batch_validate_referenced_files(
            &self,
            file_references: &HashMap<String, Vec<String>>,
        ) -> Result<HashMap<String, HashMap<String, bool>>, GitHubError> {
            let mut result = HashMap::new();

            for (repo_name, files) in file_references {
                let mut existence_map = HashMap::new();
                for file in files {
                    existence_map.insert(file.clone(), self.file_contents.contains_key(file));
                }
                result.insert(repo_name.clone(), existence_map);
            }

            Ok(result)
        }
    }

    #[cfg(test)]
    mod batch_content_tests {
        use super::*;

        #[tokio::test]
        async fn test_batch_fetch_config_file_content() {
            let client = MockGitHubClient::new();
            
            let results = client.batch_fetch_config_file_content().await.unwrap();
            
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].repo_name, "test-repo");
            assert_eq!(results[0].exists, true);
            assert!(results[0].content.is_some());
            
            let content = results[0].content.as_ref().unwrap();
            assert!(content.contains("Test Project"));
        }

        #[tokio::test]
        async fn test_batch_fetch_files() {
            let mut client = MockGitHubClient::new();
            client.add_file("docs/file1.md", "Content of file 1");
            client.add_file("docs/file2.md", "Content of file 2");

            let file_paths = vec!["docs/file1.md".to_string(), "docs/file2.md".to_string(), "docs/missing.md".to_string()];
            let results = client.batch_fetch_files("test-repo", &file_paths).await.unwrap();

            assert_eq!(results.len(), 3);
            assert_eq!(results.get("docs/file1.md").unwrap().as_deref(), Some("Content of file 1"));
            assert_eq!(results.get("docs/file2.md").unwrap().as_deref(), Some("Content of file 2"));
            assert!(results.get("docs/file3.md").is_none());
        }

        #[tokio::test]
        async fn test_batch_validate_referenced_files() {
            let mut client = MockGitHubClient::new();
            client.add_file("docs/file1.md", "Content of file 1");
            client.add_file("docs/file2.md", "Content of file 2");

            let mut file_references = HashMap::new();
            file_references.insert("test-repo".to_string(), vec!["docs/file1.md".to_string(), "docs/file2.md".to_string(), "docs/missing.md".to_string()]);

            let results = client.batch_validate_referenced_files(&file_references).await.unwrap();

            assert_eq!(results.len(), 1);
            let existence_map = results.get("test-repo").unwrap();
            assert_eq!(existence_map.get("docs/file1.md").unwrap(), &true);
            assert_eq!(existence_map.get("docs/file2.md").unwrap(), &true);
            assert_eq!(existence_map.get("docs/missing.md").unwrap(), &false);
        }
    }
}

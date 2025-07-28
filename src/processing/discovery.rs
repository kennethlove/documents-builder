use crate::processing::pipeline::{DiscoveredFile, PipelineError, ProcessingContext};
use regex::Regex;
use std::collections::HashSet;
use tracing::debug;

pub struct FileDiscoverer<'a> {
    context: &'a ProcessingContext,
}

#[derive(Debug, Clone)]
pub enum PatternType {
    Glob(String),  // e.g. "docs/**/*.md"
    Regex(String), // e.g. "^docs/.*\.md$"
    Exact(String), // e.g. "README.md"
}

impl PatternType {
    pub fn from_string(pattern: &str) -> Self {
        if pattern.starts_with("regex:") {
            PatternType::Regex(pattern.strip_prefix("regex:").unwrap().to_string())
        } else if pattern.contains("*") || pattern.contains("?") || pattern.contains("[") {
            PatternType::Glob(pattern.to_string())
        } else {
            PatternType::Exact(pattern.to_string())
        }
    }
}

impl<'a> FileDiscoverer<'a> {
    pub fn new(context: &'a ProcessingContext) -> Self {
        Self { context }
    }

    pub async fn discover(&self) -> Result<Vec<DiscoveredFile>, PipelineError> {
        let mut discovered_files = Vec::new();

        // Process each document configuration
        for (key, document) in &self.context.config.documents {
            if let Some(path) = &document.path {
                // Single file
                discovered_files.push(DiscoveredFile {
                    path: path.display().to_string(),
                    pattern_source: key.clone(),
                    estimated_size: None,
                });
            } else if let Some(sub_documents) = &document.sub_documents {
                // Multiple files from sub_documents
                for sub_document in sub_documents {
                    if let Some(sub_path) = &sub_document.path {
                        discovered_files.push(DiscoveredFile {
                            path: sub_path.display().to_string(),
                            pattern_source: format!("{}:{}", key, sub_document.title),
                            estimated_size: None,
                        });
                    }
                }
            }
        }

        // Discover markdown files using common patterns
        let additional_files = self.discover_with_patterns().await?;
        discovered_files.extend(additional_files);

        // Remove duplicates
        discovered_files.sort_by(|a, b| a.path.cmp(&b.path));
        discovered_files.dedup_by(|a, b| a.path == b.path);

        debug!(
            "Discovered {} files in repository {}",
            discovered_files.len(),
            self.context.repository
        );
        Ok(discovered_files)
    }

    async fn discover_with_patterns(&self) -> Result<Vec<DiscoveredFile>, PipelineError> {
        // This would use GitHub API to list repository contents
        // and match against common document patterns.
        let mut pattern_files = Vec::new();

        // Common patterns to look for
        let patterns = vec![
            PatternType::Exact("README.md".to_string()),
            PatternType::Exact("CONTRIBUTING.md".to_string()),
            PatternType::Exact("CHANGELOG.md".to_string()),
            PatternType::Glob("docs/**/*.md".to_string()),
            PatternType::Glob("*.md".to_string()),
            PatternType::Regex("regex:^[A-Z]+\\.md$".to_string()), // Files like README.md, CONTRIBUTING.md, etc.
        ];

        for pattern in patterns {
            if let Ok(files) = self.find_files_by_pattern(&pattern).await {
                pattern_files.extend(files.into_iter().map(|path| DiscoveredFile {
                    path,
                    pattern_source: format!("pattern:{}", self.pattern_to_string(&pattern)),
                    estimated_size: None,
                }));
            }
        }

        Ok(pattern_files)
    }

    async fn find_files_by_pattern(
        &self,
        pattern: &PatternType,
    ) -> Result<Vec<String>, PipelineError> {
        match pattern {
            PatternType::Exact(path) => {
                match self
                    .context
                    .github_client
                    .file_exists(&self.context.repository, path)
                    .await
                {
                    Ok(true) => Ok(vec![path.clone()]),
                    Ok(false) => Ok(vec![]),
                    Err(e) => {
                        debug!(
                            "Error checking file existence for exact path {}: {}",
                            path, e
                        );
                        Ok(vec![])
                    }
                }
            }
            PatternType::Glob(glob_pattern) => self.find_files_by_glob(glob_pattern).await,
            PatternType::Regex(regex_pattern) => self.find_files_by_regex(regex_pattern).await,
        }
    }

    async fn find_files_by_glob(&self, glob_pattern: &str) -> Result<Vec<String>, PipelineError> {
        let mut matching_files = Vec::new();
        let mut visited_paths = HashSet::new();

        // Parse glob pattern to understand the directory structure
        let pattern = glob::Pattern::new(glob_pattern).map_err(|e| {
            PipelineError::InvalidPattern(format!("Invalid glob pattern '{}': {}", glob_pattern, e))
        })?;

        // Start recursive search from root
        self.search_directory_recursive("", &pattern, &mut matching_files, &mut visited_paths)
            .await?;

        debug!(
            "Found {} files matching glob pattern '{}'",
            matching_files.len(),
            glob_pattern
        );
        Ok(matching_files)
    }

    async fn find_files_by_regex(&self, regex_pattern: &str) -> Result<Vec<String>, PipelineError> {
        let regex = Regex::new(regex_pattern).map_err(|e| {
            PipelineError::InvalidPattern(format!(
                "Invalid regular expression '{}': {}",
                regex_pattern, e
            ))
        })?;

        let mut matching_files = Vec::new();
        let mut visited_paths = HashSet::new();

        // Start recursive search from root
        let pin = self
            .search_directory_recursive_regex("", &regex, &mut matching_files, &mut visited_paths)
            .await;
        pin.await?;

        debug!(
            "Found {} files matching regex pattern '{}'",
            matching_files.len(),
            regex_pattern
        );
        Ok(matching_files)
    }

    fn search_directory_recursive<'b>(
        &'b self,
        current_path: &'b str,
        pattern: &'b glob::Pattern,
        matching_files: &'b mut Vec<String>,
        visited_paths: &'b mut HashSet<String>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), PipelineError>> + 'b>> {
        Box::pin(async move {
            if visited_paths.contains(current_path) {
                return Ok(());
            }
            visited_paths.insert(current_path.to_string());

            let files = match self
                .context
                .github_client
                .list_repository_files(&self.context.repository, Some(current_path))
                .await
            {
                Ok(files) => files,
                Err(e) => {
                    debug!("Error listing files in directory '{}': {}", current_path, e);
                    return Ok(());
                }
            };

            for file in files {
                let file_path = if current_path.is_empty() {
                    file.path.clone()
                } else {
                    format!("{}/{}", current_path, file.name)
                };

                if file.file_type == "file" {
                    if pattern.matches(&file_path) {
                        matching_files.push(file_path);
                    }
                } else if file.file_type == "dir" {
                    // Recursively search in subdirectories
                    self.search_directory_recursive(
                        &file_path,
                        pattern,
                        matching_files,
                        visited_paths,
                    )
                    .await?;
                }
            }

            Ok(())
        })
    }

    async fn search_directory_recursive_regex<'b>(
        &'b self,
        current_path: &'b str,
        regex: &'b Regex,
        matching_files: &'b mut Vec<String>,
        visited_paths: &'b mut HashSet<String>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), PipelineError>> + 'b>> {
        Box::pin(async move {
            if visited_paths.contains(current_path) {
                return Ok(());
            }
            visited_paths.insert(current_path.to_string());

            let files = match self
                .context
                .github_client
                .list_repository_files(&self.context.repository, Some(current_path))
                .await
            {
                Ok(files) => files,
                Err(e) => {
                    debug!("Error listing files in directory '{}': {}", current_path, e);
                    return Ok(());
                }
            };

            for file in files {
                let file_path = if current_path.is_empty() {
                    file.path.clone()
                } else {
                    format!("{}/{}", current_path, file.name)
                };

                if file.file_type == "file" {
                    if regex.is_match(&file_path) {
                        matching_files.push(file_path);
                    }
                } else if file.file_type == "dir" {
                    // Recursively search in subdirectories
                    self.search_directory_recursive_regex(
                        &file_path,
                        regex,
                        matching_files,
                        visited_paths,
                    )
                    .await
                    .await?;
                }
            }

            Ok(())
        })
    }

    fn pattern_to_string(&self, pattern: &PatternType) -> String {
        match pattern {
            PatternType::Glob(p) => p.clone(),
            PatternType::Regex(p) => format!("regex:{}", p),
            PatternType::Exact(p) => p.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectDetails;
    use crate::github::GitHubClient;
    use crate::github::tests::MockGitHubClient;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    // Helper function to create a dummy GitHubClient for tests
    fn create_dummy_github_client() -> GitHubClient {
        let client = octocrab::Octocrab::builder().build().unwrap();
        GitHubClient {
            client,
            organization: "test-org".to_string(),
        }
    }

    // Helper function to create a test ProcessingContext
    fn create_test_context() -> ProcessingContext {
        let config = crate::ProjectConfig {
            project: ProjectDetails {
                name: "Test Project".to_string(),
                description: "A test project".to_string(),
            },
            documents: HashMap::new(),
        };

        // Create a mock GitHub client
        let mock_client = MockGitHubClient::new();

        // Wrap the mock client in an Arc
        let github_client = Arc::new(mock_client);

        // Create a repository processor with a dummy GitHub client
        let processor = crate::processing::RepositoryProcessor::new(
            create_dummy_github_client(),
            config.clone(),
            "test-repo".to_string(),
        );

        ProcessingContext {
            repository: "test-repo".to_string(),
            github_client,
            config,
            processor,
        }
    }

    fn create_test_context_with_files() -> ProcessingContext {
        let config = crate::ProjectConfig {
            project: ProjectDetails {
                name: "Test Project".to_string(),
                description: "A test project".to_string(),
            },
            documents: HashMap::new(),
        };

        // Create a mock GitHub client with test files
        let mut mock_client = MockGitHubClient::new();
        mock_client.add_file("README.md", "# Test Project");
        mock_client.add_file("CHANGELOG.md", "# Changelog");
        mock_client.add_directory("docs");
        mock_client.add_file("docs/guide.md", "# Guide");
        mock_client.add_file("docs/api.md", "# API");
        mock_client.add_directory("docs/tutorials");
        mock_client.add_file("docs/tutorials/getting-started.md", "# Getting Started");

        let github_client = Arc::new(mock_client);

        let processor = crate::processing::RepositoryProcessor::new(
            create_dummy_github_client(),
            config.clone(),
            "test-repo".to_string(),
        );

        ProcessingContext {
            repository: "test-repo".to_string(),
            github_client,
            config,
            processor,
        }
    }

    #[test]
    fn test_pattern_type_from_string() {
        assert!(matches!(
            PatternType::from_string("README.md"),
            PatternType::Exact(_)
        ));

        assert!(matches!(
            PatternType::from_string("*.md"),
            PatternType::Glob(_)
        ));
        assert!(matches!(
            PatternType::from_string("docs/**/*.md"),
            PatternType::Glob(_)
        ));

        assert!(matches!(
            PatternType::from_string("regex:^[A-Z]+\\.md$"),
            PatternType::Regex(_)
        ));
    }

    #[tokio::test]
    async fn test_new() {
        let context = create_test_context();
        let discoverer = FileDiscoverer::new(&context);

        assert_eq!(discoverer.context.repository, "test-repo");
    }

    #[tokio::test]
    async fn test_discover_empty_config() {
        let context = create_test_context();
        let discoverer = FileDiscoverer::new(&context);

        let result = discoverer.discover().await.unwrap();
        assert!(result.is_empty(), "Expected empty result for empty config");
    }

    #[tokio::test]
    async fn test_discover_with_single_file() {
        let mut context = create_test_context();

        // Add a document with a path
        context.config.documents.insert(
            "doc1".to_string(),
            crate::DocumentConfig {
                title: "Document 1".to_string(),
                path: Some(PathBuf::from("docs/doc1.md")),
                sub_documents: None,
            },
        );

        let discoverer = FileDiscoverer::new(&context);
        let result = discoverer.discover().await.unwrap();

        assert_eq!(result.len(), 1, "Expected one discovered file");
        assert_eq!(result[0].path, "docs/doc1.md");
        assert_eq!(result[0].pattern_source, "doc1");
    }

    #[tokio::test]
    async fn test_discover_with_sub_documents() {
        let mut context = create_test_context();

        // Add a document with sub-documents
        let sub_docs = vec![
            crate::DocumentConfig {
                title: "Sub Doc 1".to_string(),
                path: Some(PathBuf::from("docs/sub1.md")),
                sub_documents: None,
            },
            crate::DocumentConfig {
                title: "Sub Doc 2".to_string(),
                path: Some(PathBuf::from("docs/sub2.md")),
                sub_documents: None,
            },
        ];

        context.config.documents.insert(
            "parent".to_string(),
            crate::DocumentConfig {
                title: "Parent Document".to_string(),
                path: None,
                sub_documents: Some(sub_docs),
            },
        );

        let discoverer = FileDiscoverer::new(&context);
        let result = discoverer.discover().await.unwrap();

        assert_eq!(result.len(), 2, "Expected two discovered files");
        assert!(
            result
                .iter()
                .any(|f| f.path == "docs/sub1.md" && f.pattern_source == "parent:Sub Doc 1")
        );
        assert!(
            result
                .iter()
                .any(|f| f.path == "docs/sub2.md" && f.pattern_source == "parent:Sub Doc 2")
        );
    }

    #[tokio::test]
    async fn test_find_files_by_exact_pattern() {
        let context = create_test_context_with_files();
        let discoverer = FileDiscoverer::new(&context);

        let pattern = PatternType::Exact("README.md".to_string());
        let result = discoverer.find_files_by_pattern(&pattern).await.unwrap();

        assert_eq!(result.len(), 1, "Expected one discovered file");
        assert_eq!(
            result[0], "README.md",
            "Expected file path to match README.md"
        );
    }

    #[tokio::test]
    async fn test_find_files_by_glob_pattern() {
        let context = create_test_context_with_files();
        let discoverer = FileDiscoverer::new(&context);

        let pattern = PatternType::Glob("*.md".to_string());
        let result = discoverer.find_files_by_pattern(&pattern).await.unwrap();

        assert!(
            result.contains(&"README.md".to_string()),
            "Expected to find README.md"
        );
        assert!(
            result.contains(&"CHANGELOG.md".to_string()),
            "Expected to find CHANGELOG.md"
        );
    }

    #[tokio::test]
    async fn test_find_files_by_regex_pattern() {
        let context = create_test_context_with_files();
        let discoverer = FileDiscoverer::new(&context);

        let pattern = PatternType::Regex("^[A-Z]+\\.md$".to_string());
        let result = discoverer.find_files_by_pattern(&pattern).await.unwrap();

        assert!(
            result.contains(&"README.md".to_string()),
            "Expected to find README.md"
        );
        assert!(
            result.contains(&"CHANGELOG.md".to_string()),
            "Expected to find CHANGELOG.md"
        );
    }
}

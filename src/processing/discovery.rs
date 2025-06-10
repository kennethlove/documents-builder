use tracing::debug;
use crate::processing::pipeline::{DiscoveredFile, PipelineError, ProcessingContext};

pub struct FileDiscoverer<'a> {
    context: &'a ProcessingContext,
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
            // TODO: Add support for glob and regex patterns in the future
            // e.g. "docs/**/*.md" or "^docs/.*\.md$"
        }

        // Discover markdown files using common patterns
        let additional_files = self.discover_with_patterns().await?;
        discovered_files.extend(additional_files);

        // Remove duplicates
        discovered_files.sort_by(|a, b| a.path.cmp(&b.path));
        discovered_files.dedup_by(|a, b| a.path == b.path);

        debug!("Discovered {} files in repository {}", discovered_files.len(), self.context.repository);
        Ok(discovered_files)
    }

    async fn discover_with_patterns(&self) -> Result<Vec<DiscoveredFile>, PipelineError> {
        // This would use GitHub API to list repository contents
        // and match against common document patterns.
        let mut pattern_files = Vec::new();

        // Common patterns to look for
        let patterns = vec![
            "README.md",
            "CONTRIBUTING.md",
            "CHANGELOG.md",
            "docs/**/*.md",
        ];

        for pattern in patterns {
            if let Ok(files) = self.find_files_by_pattern(pattern).await {
                pattern_files.extend(files.into_iter().map(|path| DiscoveredFile {
                    path,
                    pattern_source: pattern.to_string(),
                    estimated_size: None,
                }));
            }
        }

        Ok(pattern_files)
    }

    async fn find_files_by_pattern(&self, pattern: &str) -> Result<Vec<String>, PipelineError> {
        // For now, return empty - this would integrate with GitHub API
        // to list repository contents and filter by the pattern.
        debug!("Pattern discovery not implemented: {}", pattern);
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectDetails;
    use crate::github::GitHubClient;
    use std::collections::HashMap;
    use std::path::PathBuf;

    // Helper function to create a test ProcessingContext
    fn create_test_context() -> ProcessingContext {
        let config = crate::ProjectConfig {
            project: ProjectDetails {
                name: "Test Project".to_string(),
                description: "A test project".to_string(),
            },
            documents: HashMap::new(),
        };

        let github_client = GitHubClient {
            client: octocrab::Octocrab::default(),
            organization: "test-org".to_string(),
        };

        let processor = crate::processing::RepositoryProcessor::new(
            github_client.clone(),
            config.clone(),
            "test-repo".to_string()
        );

        ProcessingContext {
            repository: "test-repo".to_string(),
            github_client,
            config,
            processor,
        }
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
        assert!(result.iter().any(|f| f.path == "docs/sub1.md" && f.pattern_source == "parent:Sub Doc 1"));
        assert!(result.iter().any(|f| f.path == "docs/sub2.md" && f.pattern_source == "parent:Sub Doc 2"));
    }

    #[tokio::test]
    async fn test_discover_with_patterns() {
        let context = create_test_context();
        let discoverer = FileDiscoverer::new(&context);

        // Since find_files_by_pattern returns empty vec, this should also return empty
        let result = discoverer.discover_with_patterns().await.unwrap();
        assert!(result.is_empty(), "Expected empty result for pattern discovery");
    }

    #[tokio::test]
    async fn test_find_files_by_pattern() {
        let context = create_test_context();
        let discoverer = FileDiscoverer::new(&context);

        let result = discoverer.find_files_by_pattern("README.md").await.unwrap();
        assert!(result.is_empty(), "Expected empty result for pattern search");
    }
}

pub mod discovery;
pub mod output_handler;
pub mod pipeline;
pub mod processor;
pub mod validate_config;
pub mod validation;

pub use output_handler::OutputHandler;
pub use pipeline::{
    CodeBlock, DocumentProcessingPipeline, Heading, Image, Link, PipelineError, ProcessedDocument,
    ProcessingMetadata,
};
pub use validate_config::ConfigValidator;

use crate::ProjectConfig;
use crate::github::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("GitHub API error: {0}")]
    GitHubApiError(#[from] crate::github::GitHubError),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("File processing error: {0}")]
    FileProcessingError(String),
    #[error("Batch operation failed: {0}")]
    BatchOperationError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

#[derive(Clone)]
pub struct ProcessingContext {
    pub repository: String,
    pub github_client: Arc<dyn Client + Send + Sync>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub path: String,
    pub name: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ValidatedFile {
    pub path: String,
    pub name: String,
    pub size: Option<u64>,
    pub content: String,
    pub frontmatter: HashMap<String, String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProcessedFile {
    pub path: String,
    pub name: String,
    pub size: Option<u64>,
    pub content: String,
    pub html_content: String,
    pub frontmatter: HashMap<String, String>,
    pub metadata: FileMetadata,
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub title: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub tags: Vec<String>,
    pub word_count: usize,
    pub reading_time_minutes: u32,
}

pub struct FileProcessor {
    pub repository: String,
    pub github_client: Arc<dyn Client + Send + Sync>,
}

impl FileProcessor {
    pub fn new(repository: String, github_client: Arc<dyn Client + Send + Sync>) -> Self {
        Self {
            repository,
            github_client,
        }
    }

    pub async fn process_files_batch(
        &self,
        file_paths: &[String],
    ) -> Result<Vec<ProcessedFile>, ProcessingError> {
        tracing::info!("Starting batch file processing for {} files", file_paths.len());

        if file_paths.is_empty() {
            return Ok(Vec::new());
        }

        // Batch fetch all file contents
        let file_contents = self
            .github_client
            .batch_fetch_files(&self.repository, file_paths)
            .await?;

        let mut processed_files = Vec::new();

        // Process each file with its pre-fetched content
        for file_path in file_paths {
            match file_contents.get(file_path) {
                Some(Some(content)) => {
                    let processed_file = self.process_file_content(file_path, content).await?;
                    processed_files.push(processed_file);
                }
                Some(None) => {
                    tracing::warn!("File not found during batch processing: {}", file_path);
                    return Err(ProcessingError::FileProcessingError(format!("File not found: {}", file_path)));
                }
                None => {
                    tracing::error!("File not included in batch results: {}", file_path);
                    return Err(ProcessingError::BatchOperationError(format!("File not included in batch results: {}", file_path)));
                }
            }
        }

        tracing::info!("Batch file processing completed for {} files", processed_files.len());
        Ok(processed_files)
    }

    /// Legacy method for backward compatibility
    pub async fn process_file(&self, file_path: &str) -> Result<ProcessedFile, ProcessingError> {
        let content = self
            .github_client
            .get_file_content(&self.repository, file_path)
            .await?;

        self.process_file_content(file_path, &content).await
    }

    /// Process file content
    async fn process_file_content(
        &self,
        file_path: &str,
        content: &str,
    ) -> Result<ProcessedFile, ProcessingError> {
        tracing::debug!("Processing file: {}", file_path);

        // Parse frontmatter
        let (frontmatter, markdown_content) = self.parse_frontmatter(content);

        // Convert Markdown to HTML
        let html_content = self.markdown_to_html(&markdown_content);

        // Extract metadata
        let metadata = self.extract_metadata(&frontmatter, &markdown_content);

        let processed_file = ProcessedFile {
            path: file_path.to_string(),
            name: file_path.split('/').last().unwrap_or(file_path).to_string(),
            size: Some(content.len() as u64),
            content: markdown_content,
            html_content,
            frontmatter,
            metadata,
        };

        tracing::debug!("File processing completed: {}", file_path);
        Ok(processed_file)
    }

    fn parse_frontmatter(&self, content: &str) -> (HashMap<String, String>, String) {
        let lines: Vec<&str> = content.lines().collect();
        let mut frontmatter = HashMap::new();
        let mut markdown_start = 0;

        if lines.first() == Some(&"---") {
            for (i, line) in lines.iter().enumerate().skip(1) {
                if line == &"---" {
                    markdown_start = i + 1;
                    break;
                }

                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim().to_string();
                    let value = value.trim().trim_matches('"').to_string();
                    frontmatter.insert(key, value);
                }
            }
        }

        let markdown_content = lines.get(markdown_start..)
            .map(|lines| lines.join("\n"))
            .unwrap_or_else(|| content.to_string());

        (frontmatter, markdown_content)
    }

    fn markdown_to_html(&self, markdown: &str) -> String {
        // TODO: Replace with actual Markdown to HTML conversion logic
        let mut html = markdown.to_string();

        // Convert headers
        html = html.replace("# ", "<h1>").replace("\n", "</h1>\n");
        html = html.replace("## ", "<h2>").replace("\n", "</h2>\n");
        html = html.replace("### ", "<h3>").replace("\n", "</h3>\n");

        // Convert paragraphs
        let lines: Vec<&str> = html.lines().collect();
        let mut result = Vec::new();
        let mut in_paragraph = false;

        for line in lines {
            if line.trim().is_empty() {
                if in_paragraph {
                    result.push("</p>");
                    in_paragraph = false;
                }
                result.push("");
            } else if line.starts_with("<h") {
                if in_paragraph {
                    result.push("</p>");
                    in_paragraph = false;
                }
            } else {
                if !in_paragraph {
                    result.push("<p>");
                    in_paragraph = true;
                }
                result.push(line);
            }
        }

        if in_paragraph {
            result.push("</p>");
        }

        result.join("\n")
    }

    fn extract_metadata(&self, frontmatter: &HashMap<String, String>, markdown_content: &str) -> FileMetadata {
        let title = frontmatter.get("title").cloned()
            .or_else(|| {
                markdown_content.lines()
                    .find(|line| line.starts_with("# "))
                    .map(|line| line.trim_start_matches("# ").to_string())
            });

        let created_at = frontmatter.get("created_at")
            .and_then(|date_str| chrono::DateTime::parse_from_rfc3339(date_str).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let updated_at = frontmatter.get("updated_at")
            .and_then(|date_str| chrono::DateTime::parse_from_rfc3339(date_str).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let tags = frontmatter.get("tags")
            .map(|tags_str| {
                tags_str.split(',')
                    .map(|tag| tag.trim().to_string())
                    .collect()
                }).unwrap_or_default();

        let word_count = markdown_content.split_whitespace().count();
        let reading_time_minutes = ((word_count as f64 / 200.0).ceil() as u32).max(1);

        FileMetadata {
            title,
            created_at,
            updated_at,
            tags,
            word_count,
            reading_time_minutes,
        }
    }
}

/// Batch validation helper for multiple repositories
pub async fn validate_files_across_repos(
    file_references: &HashMap<String, Vec<String>>,
    github_client: Arc<dyn Client + Send + Sync>,
) -> Result<HashMap<String, HashMap<String, bool>>, ProcessingError> {
    tracing::info!("Starting batch validation for {} repositories", file_references.len());

    let validation_results = github_client
        .batch_validate_referenced_files(file_references)
        .await?;

    tracing::info!("Batch validation completed across repositories");
    Ok(validation_results)
}

/// File collection helper
pub async fn collect_all_referenced_files(
    repo_file_map: &HashMap<String, Vec<String>>,
    github_client: Arc<dyn Client + Send + Sync>,
) -> Result<HashMap<String, HashMap<String, String>>, ProcessingError> {
    tracing::info!("Collecting referenced files from {} repositories", repo_file_map.len());

    let batch_results = github_client
        .batch_fetch_files_multi_repo(repo_file_map)
        .await?;

    // Convert <Option<String>> to String, filtering out missing files
    let mut final_results = HashMap::new();

    for (repo_name, files) in batch_results {
        let mut repo_files = HashMap::new();

        for (file_path, content) in files {
            if let Some(content) = content {
                repo_files.insert(file_path, content);
            } else {
                tracing::warn!("File not found: {} in repository {}", file_path, repo_name);
            }
        }

        final_results.insert(repo_name, repo_files);
    }

    tracing::info!("File collection completed");
    Ok(final_results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::tests::MockGitHubClient;

    fn create_test_processor() -> FileProcessor {
        let mut github_client = MockGitHubClient::new();
        github_client.add_file("docs/test.md", "---\ntitle: Test Document\n---\n# Test Document\nThis is a test document.");

        FileProcessor::new(
            "test-repo".to_string(),
            Arc::new(github_client),
        )
    }

    #[tokio::test]
    async fn test_process_files_batch() {
        let mut github_client = MockGitHubClient::new();
        github_client.add_file("docs/file1.md", "---\ntitle: File 1\n---\n# Content 1");
        github_client.add_file("docs/file2.md", "---\ntitle: File 2\n---\n# Content 2");

        let processor = FileProcessor::new(
            "test-repo".to_string(),
            Arc::new(github_client),
        );

        let file_paths = vec!["docs/file1.md".to_string(), "docs/file2.md".to_string()];
        let processed_files = processor.process_files_batch(&file_paths).await.unwrap();

        assert_eq!(processed_files.len(), 2);
        assert_eq!(processed_files[0].metadata.title, Some("File 1".to_string()));
        assert_eq!(processed_files[1].metadata.title, Some("File 2".to_string()));
    }

    #[tokio::test]
    async fn test_process_file_content() {
        let processor = create_test_processor();

        let content = "---\ntitle: Test Document\n---\n# Test Document\nThis is a test document.";
        let processed = processor.process_file_content("test.md", content).await.unwrap();

        assert_eq!(processed.metadata.title, Some("Test Document".to_string()));
        assert!(processed.html_content.contains("<h1>"));
        assert!(processed.html_content.contains("<p>"));
    }

    #[tokio::test]
    async fn test_extract_metadata() {
        let processor = create_test_processor();

        let mut frontmatter = HashMap::new();
        frontmatter.insert("title".to_string(), "Test Document".to_string());
        frontmatter.insert("tags".to_string(), "rust, testing, markdown".to_string());

        let content = "# Heading\n\nThis is test content with multiple words for counting.";
        let metadata = processor.extract_metadata(&frontmatter, content);

        assert_eq!(metadata.title, Some("Test Document".to_string()));
        assert_eq!(metadata.tags, vec!["rust", "testing", "markdown"]);
        assert!(metadata.word_count > 10);
        assert!(metadata.reading_time_minutes > 0);
    }
}

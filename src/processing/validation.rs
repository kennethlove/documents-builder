use crate::processing::pipeline::{
    DiscoveredFile, PipelineError, ProcessingContext, ValidatedFile,
};
use std::collections::HashMap;

pub struct ContentValidator<'a> {
    context: &'a ProcessingContext,
}

impl<'a> ContentValidator<'a> {
    pub fn new(context: &'a ProcessingContext) -> Self {
        Self { context }
    }

    pub async fn validate_batch(
        &self,
        files: Vec<DiscoveredFile>,
    ) -> Result<Vec<ValidatedFile>, PipelineError> {
        tracing::info!("Starting batch validation of {} files", files.len());

        if files.is_empty() {
            tracing::warn!("No files to validate");
            return Ok(Vec::new());
        }

        // Group files by repository for bulk fetching
        let mut repo_file_map: HashMap<String, Vec<String>> = HashMap::new();
        for file in &files {
            let repo_files = repo_file_map.entry(self.context.repository.clone()).or_insert_with(Vec::new);
            repo_files.push(file.path.clone());
        }

        // Batch fetch all file contents
        tracing::info!("Batch fetching content for {} files from {} repositories", files.len(), repo_file_map.len());

        let batch_results = self
            .context
            .github_client
            .batch_fetch_files_multi_repo(&repo_file_map)
            .await
            .map_err(|e| PipelineError::GitHub(e))?;

        // Get the file contents for our repository
        let file_contents = batch_results
            .get(&self.context.repository)
            .ok_or_else(|| PipelineError::Validation("Repository not found in batch results".to_string()))?;

        let mut validated_files = Vec::new();
        let mut validation_errors = Vec::new();

        // Process each file with its pre-fetched content
        for file in files {
            match self.validate_file_with_content(file.clone(), file_contents).await {
                Ok(validated_file) => validated_files.push(validated_file),
                Err(e) => {
                    tracing::warn!("Validation error for file {}: {}", file.path, e);
                    validation_errors.push(format!("file {}: {}", file.path, e));
                }
            }
        }

        if !validation_errors.is_empty() {
            tracing::warn!("Validation completed with {} errors", validation_errors.len());
            return Err(PipelineError::Validation(
                format!("Multiple validation errors occurred: {}", validation_errors.join(", "))
            ));
        }

        tracing::info!("Batch validation completed successfully for {} files", validated_files.len());
        Ok(validated_files)
    }

    fn parse_frontmatter(&self, content: &str) -> (HashMap<String, String>, String) {
        let lines: Vec<&str> = content.lines().collect();
        let mut frontmatter = HashMap::new();
        let mut markdown_start = 0;

        if lines.first() == Some(&"---") {
            for (i, line) in lines.iter().enumerate().skip(1) {
                if *line == "---" {
                    markdown_start = i + 1; // Start after the closing delimiter
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

    #[allow(dead_code)]
    fn parse_yaml_frontmatter(&self, yaml_text: &str) -> HashMap<String, String> {
        let mut frontmatter = HashMap::new();

        for line in yaml_text.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_string();
                let value = value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                frontmatter.insert(key, value);
            }
        }
        frontmatter
    }

    fn validate_content(
        &self,
        markdown_content: &str,
        frontmatter: &HashMap<String, String>,
    ) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check for title
        if !frontmatter.contains_key("title") && !markdown_content.starts_with('#') {
            warnings.push("Missing title in frontmatter or as first heading".to_string());
        }

        // Check for very short content
        if markdown_content.trim().len() < 50 {
            warnings.push("Content is too short, consider adding more information".to_string());
        }

        // Basic check for broken links
        if markdown_content.contains("](") {
            let broken_links = self.find_potentially_broken_links(markdown_content);
            if !broken_links.is_empty() {
                warnings.push(format!(
                    "Found {} potentially broken links",
                    broken_links.len()
                ));
            }
        }

        warnings
    }

    fn find_potentially_broken_links(&self, markdown_content: &str) -> Vec<String> {
        // Simple regex to find markdown links that might be broken
        // This is a basic implementation and can be improved
        let mut broken_links = Vec::new();

        // Look for links with empty URLs or suspicious patterns
        for line in markdown_content.lines() {
            if line.contains("]()") || line.contains("](../") {
                broken_links.push(line.trim().to_string());
            }
        }

        broken_links
    }

    async fn validate_file_with_content(
        &self,
        file: DiscoveredFile,
        file_contents: &HashMap<String, Option<String>>,
    ) -> Result<ValidatedFile, PipelineError> {
        tracing::debug!("Validating file: {}", file.path);

        // Get content from batch results
        let content = match file_contents.get(&file.path) {
            Some(Some(content)) => content.clone(),
            Some(None) => {
                return Err(PipelineError::Validation(format!("File {} not found", file.path)));
            }
            None => {
                return Err(PipelineError::Validation(
                    format!("File not included in batch results: {}", file.path)
                ));
            }
        };

        // Parse frontmatter and content
        let (frontmatter, markdown_content) = self.parse_frontmatter(&content);

        // Validate content and collect warnings
        let warnings = self.validate_content(&markdown_content, &frontmatter);

        let validated_file = ValidatedFile {
            discovered: file,
            content,
            frontmatter,
            markdown_content,
            validation_warnings: warnings,
        };

        tracing::debug!(
            "File validation completed: {} (warnings: {})",
            validated_file.discovered.path,
            validated_file.validation_warnings.len()
        );

        Ok(validated_file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectDetails;
    use crate::github::tests::MockGitHubClient;
    use std::collections::HashMap;
    use std::sync::Arc;

    // Helper function to create a dummy GitHubClient for tests
    fn create_dummy_github_client() -> MockGitHubClient {
        MockGitHubClient::new()
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

    #[tokio::test]
    async fn test_parse_frontmatter_no_frontmatter() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let content = "# This is a document\nWith no frontmatter";
        let (frontmatter, markdown) = validator.parse_frontmatter(content);

        assert!(frontmatter.is_empty(), "Expected empty frontmatter");
        assert_eq!(
            markdown, content,
            "Expected original content to be returned"
        );
    }

    #[tokio::test]
    async fn test_parse_frontmatter_with_valid_frontmatter() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let content = "---\ntitle: Test Document\nauthor: Test Author\n---\n# Document content";
        let (frontmatter, markdown) = validator.parse_frontmatter(content);

        assert_eq!(frontmatter.len(), 2, "Expected 2 frontmatter items");
        assert_eq!(frontmatter.get("title"), Some(&"Test Document".to_string()));
        assert_eq!(frontmatter.get("author"), Some(&"Test Author".to_string()));
        assert_eq!(
            markdown, "# Document content",
            "Expected markdown content without frontmatter"
        );
    }

    #[tokio::test]
    async fn test_parse_frontmatter_with_incomplete_frontmatter() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        // Missing closing delimiter
        let content = "---\ntitle: Test Document\nauthor: Test Author\n# Document content";
        let (frontmatter, markdown) = validator.parse_frontmatter(content);

        assert!(
            frontmatter.is_empty(),
            "Expected empty frontmatter for incomplete frontmatter"
        );
        assert_eq!(
            markdown, content,
            "Expected original content to be returned"
        );
    }

    #[tokio::test]
    async fn test_parse_yaml_frontmatter() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let yaml_text = "title: Test Document\nauthor: Test Author\ntags: rust, testing";
        let frontmatter = validator.parse_yaml_frontmatter(yaml_text);

        assert_eq!(frontmatter.len(), 3, "Expected 3 frontmatter items");
        assert_eq!(frontmatter.get("title"), Some(&"Test Document".to_string()));
        assert_eq!(frontmatter.get("author"), Some(&"Test Author".to_string()));
        assert_eq!(frontmatter.get("tags"), Some(&"rust, testing".to_string()));
    }

    #[tokio::test]
    async fn test_parse_yaml_frontmatter_with_quotes() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let yaml_text = "title: \"Test Document\"\nauthor: 'Test Author'\ntags: 'rust, testing'";
        let frontmatter = validator.parse_yaml_frontmatter(yaml_text);

        assert_eq!(frontmatter.len(), 3, "Expected 3 frontmatter items");
        assert_eq!(frontmatter.get("title"), Some(&"Test Document".to_string()));
        assert_eq!(frontmatter.get("author"), Some(&"Test Author".to_string()));
        assert_eq!(frontmatter.get("tags"), Some(&"rust, testing".to_string()));
    }

    #[tokio::test]
    async fn test_parse_yaml_frontmatter_empty() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let yaml_text = "";
        let frontmatter = validator.parse_yaml_frontmatter(yaml_text);

        assert!(
            frontmatter.is_empty(),
            "Expected empty frontmatter for empty yaml"
        );
    }

    #[tokio::test]
    async fn test_validate_content_missing_title() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let markdown = "This is content w/o heading or frontmatter title";
        let frontmatter = HashMap::new();

        let warnings = validator.validate_content(markdown, &frontmatter);

        assert_eq!(warnings.len(), 2, "Expected 2 warnings");
        assert!(warnings.contains(&"Missing title in frontmatter or as first heading".to_string()));
        assert!(
            warnings
                .contains(&"Content is too short, consider adding more information".to_string())
        );
    }

    #[tokio::test]
    async fn test_validate_content_with_title_in_frontmatter() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let markdown = "This content is too short.";
        let mut frontmatter = HashMap::new();
        frontmatter.insert("title".to_string(), "Document Title".to_string());

        let warnings = validator.validate_content(markdown, &frontmatter);

        assert_eq!(warnings.len(), 1, "Expected 1 warning");
        assert!(
            warnings
                .contains(&"Content is too short, consider adding more information".to_string())
        );
    }

    #[tokio::test]
    async fn test_validate_content_with_heading_title() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let markdown = "# Document Title\nThis isn't enough content.";
        let frontmatter = HashMap::new();

        let warnings = validator.validate_content(markdown, &frontmatter);

        assert_eq!(warnings.len(), 1, "Expected 1 warning");
        assert!(
            warnings
                .contains(&"Content is too short, consider adding more information".to_string())
        );
    }

    #[tokio::test]
    async fn test_validate_content_with_broken_links() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let markdown = "# Document Title\nThis is a document with [broken link]() and [another broken link](../path).\nThe content is long enough to avoid the short content warning. The content is long enough to avoid the short content warning.";
        let frontmatter = HashMap::new();

        let warnings = validator.validate_content(markdown, &frontmatter);

        assert_eq!(warnings.len(), 1, "Expected 1 warning");
        assert!(
            warnings[0].contains("potentially broken links"),
            "Expected warning about broken links"
        );
    }

    #[tokio::test]
    async fn test_validate_content_no_warnings() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let markdown = "# Document Title\nThis is a document with proper content and no issues. The content is long enough to avoid the short content warning. The content is long enough to avoid the short content warning.";
        let frontmatter = HashMap::new();

        let warnings = validator.validate_content(markdown, &frontmatter);

        assert!(
            warnings.is_empty(),
            "Expected no warnings for valid content"
        );
    }

    #[tokio::test]
    async fn test_find_potentially_broken_links() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let markdown = "# Document\n[Broken link]()\n[Another broken link](../path)\n[Valid link](https://example.com)";
        let broken_links = validator.find_potentially_broken_links(markdown);

        assert_eq!(broken_links.len(), 2, "Expected 2 broken links");
        assert!(broken_links.contains(&"[Broken link]()".to_string()));
        assert!(broken_links.contains(&"[Another broken link](../path)".to_string()));
    }

    #[tokio::test]
    async fn test_find_potentially_broken_links_no_links() {
        let context = create_test_context();
        let validator = ContentValidator::new(&context);

        let markdown = "# Document\nThis document has no links at all.";
        let broken_links = validator.find_potentially_broken_links(markdown);

        assert!(broken_links.is_empty(), "Expected no broken links");
    }

    #[tokio::test]
    async fn test_batch_verification_with_multiple_files() {
        let mut github_client = create_dummy_github_client();
        github_client.add_file("docs/file1.md", "---\ntitle: File 1\n---\n# File 1 Content");
        github_client.add_file("docs/file2.md", "---\ntitle: File 2\n---\n# File 2 Content");

        let context = create_test_context();

        let validator = ContentValidator::new(&context);

        let files = vec![
            DiscoveredFile {
                path: "docs/file1.md".to_string(),
                pattern_source: "*.md".to_string(),
                estimated_size: Some(100),
            },
            DiscoveredFile {
                path: "docs/file2.md".to_string(),
                pattern_source: "*.md".to_string(),
                estimated_size: Some(100),
            },
        ];

        let validated_files = validator.validate_batch(files).await.unwrap();

        assert_eq!(validated_files.len(), 2);
        assert_eq!(validated_files[0].discovered.path, "docs/file1.md");
        assert_eq!(validated_files[1].discovered.path, "docs/file2.md");
        assert!(validated_files[0].frontmatter.contains_key("title"));
        assert!(validated_files[1].frontmatter.contains_key("title"));
    }
}

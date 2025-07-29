// Integration tests for the processing pipeline
//
// These tests use mockito to mock the GitHub client's behavior.

use async_trait::async_trait;
use documents::github::{Client, GitHubClient, GitHubError, RepositoryFile, RepositoryFileContent};
use documents::processing::RepositoryProcessor;
use documents::processing::{DocumentProcessingPipeline, ProcessingContext};
use documents::{DocumentConfig, ProjectConfig, ProjectDetails};
use octocrab::Octocrab;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// Mock implementation of the Client trait for testing
struct MockGitHubClient {
    file_contents: HashMap<String, String>,
    files: Vec<RepositoryFile>,
}

impl MockGitHubClient {
    fn new() -> Self {
        Self {
            file_contents: HashMap::new(),
            files: Vec::new(),
        }
    }

    fn add_file(&mut self, path: &str, content: &str) {
        self.file_contents
            .insert(path.to_string(), content.to_string());
        self.files.push(RepositoryFile {
            path: path.to_string(),
            name: path.split('/').last().unwrap_or(path).to_string(),
            size: Some(content.len() as u64),
            file_type: "file".to_string(),
        });
    }

    fn add_directory(&mut self, path: &str) {
        self.files.push(RepositoryFile {
            path: path.to_string(),
            name: path.split('/').last().unwrap_or(path).to_string(),
            size: None,
            file_type: "dir".to_string(),
        });
    }
}

// Helper function to create a dummy GitHubClient for tests
fn create_dummy_github_client() -> GitHubClient {
    let client = Octocrab::builder().build().unwrap();
    GitHubClient {
        client,
        organization: "test-org".to_string(),
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

    async fn scan_for_config_file(&self, _repo_name: &str) -> Result<Option<String>, GitHubError> {
        Ok(Some("documents.toml".to_string()))
    }

    async fn read_config_file(&self, _repo_name: &str) -> Result<String, GitHubError> {
        Ok("[project]\nname = \"Test Project\"\ndescription = \"A test project\"".to_string())
    }

    async fn get_project_config(&self, _repo_name: &str) -> Result<ProjectConfig, GitHubError> {
        let mut documents = HashMap::new();
        documents.insert(
            "doc1".to_string(),
            DocumentConfig {
                title: "Document 1".to_string(),
                path: Some(PathBuf::from("docs/file1.md")),
                sub_documents: None,
            },
        );

        Ok(ProjectConfig {
            project: ProjectDetails {
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

    async fn file_exists(&self, _repo_name: &str, file_path: &str) -> Result<bool, GitHubError> {
        Ok(self.file_contents.contains_key(file_path))
    }

    async fn list_repository_files(
        &self,
        repo_name: &str,
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

    async fn batch_check_file_exists(&self, file_path: &str) -> Result<HashMap<String, bool>, GitHubError> {
        let mut result = HashMap::new();
        
        // For testing, we'll return that test-repo has the file if it exists in file_contents
        let file_exists = self.file_contents.contains_key(file_path);
        result.insert("test-repo".to_string(), file_exists);
        
        Ok(result)
    }
    
    async fn batch_check_file_content(&self, file_path: &str) -> Result<Vec<RepositoryFileContent>, GitHubError> {
        let mut result = Vec::new();
        
        // For testing, check if the file exists in file_contents and return appropriate result
        let file_exists = self.file_contents.contains_key(file_path);
        let content = if file_exists {
            self.file_contents.get(file_path).cloned()
        } else {
            None
        };
        
        result.push(RepositoryFileContent {
            repo_name: "test-repo".to_string(),
            exists: file_exists,
            content,
        });
        
        Ok(result)
    }
}

#[tokio::test]
async fn test_pipeline_execute() {
    // Create a test context
    let context = create_test_context();

    // Create the pipeline
    let pipeline = DocumentProcessingPipeline::new(context);

    // Execute the pipeline
    let result = pipeline.execute().await;

    // Check that the pipeline executed successfully
    assert!(
        result.is_ok(),
        "Pipeline execution failed: {:?}",
        result.err()
    );

    // Get the processed documents
    let processed_documents = result.unwrap();

    // Check that we have the expected number of documents
    assert_eq!(
        processed_documents.len(),
        2,
        "Expected 2 processed documents, got {}",
        processed_documents.len()
    );

    // Check the first document
    let doc1 = processed_documents
        .iter()
        .find(|d| d.file_path == "docs/file1.md")
        .expect("Document 1 not found in processed documents");

    assert_eq!(doc1.title, "Test Document");
    assert!(!doc1.headings.is_empty(), "Document 1 should have headings");
    assert_eq!(doc1.headings[0].text, "Heading 1");
    assert!(!doc1.links.is_empty(), "Document 1 should have links");
    assert_eq!(doc1.links[0].text, "a link");
    assert_eq!(doc1.links[0].url, "https://example.com");

    // Check the second document
    let doc2 = processed_documents
        .iter()
        .find(|d| d.file_path == "docs/file2.md")
        .expect("Document 2 not found in processed documents");

    assert_eq!(doc2.title, "Another Document");
    assert!(!doc2.headings.is_empty(), "Document 2 should have headings");
    assert_eq!(doc2.headings[0].text, "Another Document");
    assert!(doc2.links.is_empty(), "Document 2 should not have links");
}

#[tokio::test]
async fn test_pipeline_with_missing_file() {
    // Create a test context
    let mut context = create_test_context();

    // Modify the config to include a non-existent file
    let doc3 = DocumentConfig {
        title: "Document 3".to_string(),
        path: Some(PathBuf::from("docs/non_existent.md")),
        sub_documents: None,
    };
    context.config.documents.insert("doc3".to_string(), doc3);

    // Create the pipeline
    let pipeline = DocumentProcessingPipeline::new(context);

    // Execute the pipeline
    let result = pipeline.execute().await;

    // The pipeline should still succeed, but with fewer documents
    assert!(
        result.is_ok(),
        "Pipeline execution failed: {:?}",
        result.err()
    );

    // Get the processed documents
    let processed_documents = result.unwrap();

    // Check that we have the expected number of documents (only the existing ones)
    assert_eq!(
        processed_documents.len(),
        2,
        "Expected 2 processed documents, got {}",
        processed_documents.len()
    );
}

#[tokio::test]
async fn test_pipeline_with_invalid_content() {
    // Create a test context with an invalid file
    let context = create_test_context_with_invalid_file();

    // Create the pipeline
    let pipeline = DocumentProcessingPipeline::new(context);

    // Execute the pipeline
    let result = pipeline.execute().await;

    // The pipeline should still succeed, but the document might have default values
    assert!(
        result.is_ok(),
        "Pipeline execution failed: {:?}",
        result.err()
    );

    // Get the processed documents
    let processed_documents = result.unwrap();

    // Check that we have one document
    assert_eq!(
        processed_documents.len(),
        1,
        "Expected 1 processed document, got {}",
        processed_documents.len()
    );

    // Check the document
    let doc = &processed_documents[0];
    assert_eq!(doc.file_path, "docs/invalid.md");
    assert!(doc.headings.is_empty(), "Document should not have headings");
    assert!(doc.links.is_empty(), "Document should not have links");
}

// Helper function to create a test context
fn create_test_context() -> ProcessingContext {
    // Create a test config
    let mut documents = HashMap::new();
    let doc1 = DocumentConfig {
        title: "Document 1".to_string(),
        path: Some(PathBuf::from("docs/file1.md")),
        sub_documents: None,
    };
    let doc2 = DocumentConfig {
        title: "Document 2".to_string(),
        path: Some(PathBuf::from("docs/file2.md")),
        sub_documents: None,
    };
    documents.insert("doc1".to_string(), doc1);
    documents.insert("doc2".to_string(), doc2);

    let config = ProjectConfig {
        project: ProjectDetails {
            name: "Test Project".to_string(),
            description: "A test project".to_string(),
        },
        documents,
    };

    // Create a mock GitHub client with test files
    let mut mock_client = MockGitHubClient::new();

    mock_client.add_directory("docs");

    // Add test files
    mock_client.add_file(
        "docs/file1.md",
        r#"---
title: Test Document
---
# Heading 1

This is a test document with [a link](https://example.com).
"#,
    );

    mock_client.add_file(
        "docs/file2.md",
        r#"# Another Document

This is another test document without any links.
"#,
    );

    // Wrap the mock client in an Arc
    let github_client = Arc::new(mock_client);

    // Create a repository processor with a dummy GitHub client
    let processor = RepositoryProcessor::new(
        create_dummy_github_client(),
        config.clone(),
        "test-org/test-repo".to_string(),
    );

    // Create the context
    ProcessingContext {
        repository: "test-org/test-repo".to_string(),
        github_client,
        config,
        processor,
    }
}

// Helper function to create a test context with an invalid file
fn create_test_context_with_invalid_file() -> ProcessingContext {
    // Create a test config with only the invalid file
    let mut documents = HashMap::new();
    let doc = DocumentConfig {
        title: "Invalid Document".to_string(),
        path: Some(PathBuf::from("docs/invalid.md")),
        sub_documents: None,
    };
    documents.insert("invalid".to_string(), doc);

    let config = ProjectConfig {
        project: ProjectDetails {
            name: "Test Project".to_string(),
            description: "A test project".to_string(),
        },
        documents,
    };

    // Create a mock GitHub client with an invalid file
    let mut mock_client = MockGitHubClient::new();

    mock_client.add_directory("docs");

    // Add an invalid file
    mock_client.add_file(
        "docs/invalid.md",
        r#"This is not a valid markdown file
It has no proper structure or headings
And it's missing frontmatter"#,
    );

    // Wrap the mock client in an Arc
    let github_client = Arc::new(mock_client);

    // Create a repository processor with a dummy GitHub client
    let processor = RepositoryProcessor::new(
        create_dummy_github_client(),
        config.clone(),
        "test-org/test-repo".to_string(),
    );

    // Create the context
    ProcessingContext {
        repository: "test-org/test-repo".to_string(),
        github_client,
        config,
        processor,
    }
}

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use tracing::info;
use crate::github::GitHubClient;
use crate::processing::discovery::FileDiscoverer;
use crate::processing::processor::ContentProcessor;
use crate::processing::RepositoryProcessor;
use crate::processing::validation::ContentValidator;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("GitHub API error: {0}")]
    GitHub(#[from] crate::github::GitHubError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Processing error: {0}")]
    Processing(String),
}

#[derive(Clone, Debug)]
pub struct ProcessingContext {
    pub repository: String,
    pub github_client: GitHubClient,
    pub config: crate::ProjectConfig,
    pub processor: RepositoryProcessor,
}

pub struct DocumentProcessingPipeline {
    pub context: ProcessingContext,
}

impl DocumentProcessingPipeline {
    pub fn new(context: ProcessingContext) -> Self {
        Self { context }
    }
    
    pub async fn execute(&self) -> Result<Vec<ProcessedDocument>, PipelineError> {
        info!("Starting document processing pipeline for repository: {}", self.context.repository);
        
        // Step 1, discover files
        let discovered_files = self.discover_files().await?;
        
        // Step 2, validate files
        let validated_files = self.validate_files(discovered_files).await?;
        
        // Step 3, process files
        let processed_documents = self.process_files(validated_files).await?;
        
        info!("Pipeline completed: {} documents processed", processed_documents.len());
        Ok(processed_documents)
    }
    
    async fn discover_files(&self) -> Result<Vec<DiscoveredFile>, PipelineError> {
        let discoverer = FileDiscoverer::new(&self.context);
        discoverer.discover().await
    }
    
    async fn validate_files(&self, files: Vec<DiscoveredFile>) -> Result<Vec<ValidatedFile>, PipelineError> {
        let validator = ContentValidator::new(&self.context);
        validator.validate_batch(files).await
    }
    
    async fn process_files(&self, files: Vec<ValidatedFile>) -> Result<Vec<ProcessedDocument>, PipelineError> {
        let processor = ContentProcessor::new();
        processor.process_batch(files).await
    }
}

#[derive(Clone, Debug)]
pub struct DiscoveredFile {
    pub path: String,
    pub pattern_source: String, // Which pattern was used to discover this file
    pub estimated_size: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct ValidatedFile {
    pub discovered: DiscoveredFile,
    pub content: String,
    pub frontmatter: HashMap<String, String>,
    pub markdown_content: String,
    pub validation_warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProcessedDocument {
    pub file_path: String,
    pub title: String,
    pub content: String,
    pub frontmatter: HashMap<String, String>,
    pub word_count: usize,
    pub headings: Vec<Heading>,
    pub links: Vec<Link>,
    pub images: Vec<Image>,
    pub code_blocks: Vec<CodeBlock>,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    pub processing_metadata: ProcessingMetadata,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub anchor: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Link {
    pub text: String,
    pub url: String,
    pub is_internal: bool,
    pub is_valid: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Image {
    pub alt_text: String,
    pub url: String,
    pub is_internal: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub content: String,
    pub line_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProcessingMetadata {
    pub processed_at: chrono::DateTime<chrono::Utc>,
    pub processing_time_ms: u64,
    pub warnings: Vec<String>,
    pub quality_score: f32, // 0.0 to 1.0 based on various quality metrics
}
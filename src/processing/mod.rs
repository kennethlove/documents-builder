pub mod validate_config;
pub mod pipeline;
pub mod discovery;
pub mod validation;
pub mod processor;

pub use pipeline::{
    DocumentProcessingPipeline, ProcessingContext, ProcessedDocument,
    Heading, Link, Image, CodeBlock, ProcessingMetadata, PipelineError
};
pub use validate_config::ConfigValidator;

use crate::github::Client;
use crate::ProjectConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};

#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("GitHub API error: {0}")]
    GitHub(#[from] crate::github::GitHubError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Processing error: {0}")]
    Processing(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
    pub repository: String,
    pub processed_at: chrono::DateTime<chrono::Utc>,
    pub file_processed: usize,
    pub fragments_generated: usize,
    pub processing_time_ms: u64,
    pub fragments: Vec<DocumentFragment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentFragment {
    pub id: String,
    pub file_path: String,
    pub fragment_type: FragmentType,
    pub title: String,
    pub content: String,
    pub metadata: HashMap<String, String>,
    pub word_count: usize,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FragmentType {
    Content,
    Navigation,
}

#[derive(Clone)]
pub struct RepositoryProcessor {
    github: Arc<dyn Client + Send + Sync>,
    config: ProjectConfig,
    repository: String,
}

impl std::fmt::Debug for RepositoryProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepositoryProcessor")
            .field("config", &self.config)
            .field("repository", &self.repository)
            .finish()
    }
}

impl RepositoryProcessor {
    pub fn new(github: impl Client + Send + Sync + 'static, config: ProjectConfig, repository: String) -> Self {
        RepositoryProcessor {
            github: Arc::new(github),
            config,
            repository,
        }
    }

    pub async fn process(&self, verbose: bool) -> Result<ProcessingResult, ProcessingError> {
        let start_time = std::time::Instant::now();

        info!("Starting processing of repository {}", self.repository);

        // Step 1: Discover markdown files
        let markdown_files = self.discover_markdown_files().await?;

        if verbose {
            debug!("Discovered {} markdown files", markdown_files.len());
            for file in &markdown_files {
                debug!("  - {}", file);
            }
        }

        // Step 2: Process each markdown file
        let mut fragments = Vec::new();
        let mut files_processed = 0;

        for file_path in markdown_files {
            if verbose {
                debug!("Processing file: {}", file_path);
            }

            match self.process_markdown_file(&file_path).await {
                Ok(mut file_fragments) => {
                    files_processed += 1;
                    fragments.append(&mut file_fragments);

                    if verbose {
                        debug!("  Generated {} fragments", file_fragments.len());
                    }
                }
                Err(e) => {
                    warn!("Failed to process file {}: {}", file_path, e);
                }
            }
        }

        let processing_time = start_time.elapsed();

        let result = ProcessingResult {
            repository: self.repository.clone(),
            processed_at: chrono::Utc::now(),
            file_processed: files_processed,
            fragments_generated: fragments.len(),
            processing_time_ms: processing_time.as_millis() as u64,
            fragments,
        };

        info!(
            "Completed processing for {}: {} files, {} fragments generated in {}ms",
            self.repository,
            result.file_processed,
            result.fragments_generated,
            result.processing_time_ms
        );

        Ok(result)
    }

    async fn discover_markdown_files(&self) -> Result<Vec<String>, ProcessingError> {
        debug!("Discovering markdown files for repository {}", self.repository);

        let patterns = self.config.documents.clone();
        let mut discovered_files = Vec::new();

        for document in patterns.values() {
            if let Some(path) = &document.path {
                discovered_files.push(path.display().to_string());
            } else if let Some(sub_documents) = &document.sub_documents {
                for sub_doc in sub_documents {
                    if let Some(sub_path) = &sub_doc.path {
                        discovered_files.push(sub_path.display().to_string());
                    }
                }
            } else {
                warn!("Document configuration for {} does not specify a path or sub-documents", document.title);
            }
        }

        discovered_files.sort();
        discovered_files.dedup();

        debug!("Discovered {} markdown files", discovered_files.len());

        Ok(discovered_files)
    }

    async fn process_markdown_file(&self, file_path: &str) -> Result<Vec<DocumentFragment>, ProcessingError> {
        debug!("Processing markdown file: {}", file_path);

        // Fetch the file's content
        let content = self.github.get_file_content(&self.repository, file_path).await
            .map_err(ProcessingError::GitHub)?;

        let (frontmatter, markdown_content) = self.extract_frontmatter(&content);

        // Generate fragments
        let mut fragments = Vec::new();

        let content_fragment = DocumentFragment {
            id: format!("{}#{}", self.repository, file_path),
            file_path: file_path.to_string(),
            fragment_type: FragmentType::Content,
            title: frontmatter.get("title").cloned().unwrap_or_else(|| "Untitled".to_string()),
            content: markdown_content.clone(),
            metadata: frontmatter.clone(),
            word_count: self.count_words(&markdown_content),
            last_modified: None,
        };
        fragments.push(content_fragment);

        Ok(fragments)
    }

    fn extract_frontmatter(&self, content: &str) -> (HashMap<String, String>, String) {
        if content.starts_with("---\n") {
            if let Some(end_pos) = content[4..].find("\n---\n") {
                let frontmatter = &content[4..end_pos + 4];
                let markdown_content = &content[end_pos + 8..];

                let mut metadata = HashMap::new();
                for line in frontmatter.lines() {
                    if let Some((key, value)) = line.split_once(':') {
                        metadata.insert(key.trim().to_string(), value.trim_matches('"').to_string());
                    }
                }

                return (metadata, markdown_content.to_string());
            }
        }

        (HashMap::new(), content.to_string())
    }

    fn count_words(&self, content: &str) -> usize {
        content.split_whitespace().count()
    }
}

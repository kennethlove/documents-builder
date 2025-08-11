pub mod discovery;
pub mod output_handler;
pub mod path_normalization;
pub mod pipeline;
pub mod processor;
pub mod validate_config;
pub mod validation;
mod navigation;

pub use path_normalization::{PathNormalizer, PathNormalizationError};
pub use output_handler::OutputHandler;
pub use pipeline::{
    CodeBlock, DocumentProcessingPipeline, Heading, Image, Link, PipelineError, ProcessedDocument,
    ProcessingContext, ProcessingMetadata,
};
pub use validate_config::ConfigValidator;

use crate::{DocumentConfig, ProjectConfig};
use crate::github::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

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
    ordered_root_documents: Vec<DocumentConfig>,
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
    pub fn new(
        github: impl Client + Send + Sync + 'static,
        config: ProjectConfig,
        repository: String,
    ) -> Self {
        RepositoryProcessor {
            github: Arc::new(github),
            config,
            repository,
            ordered_root_documents: Vec::new(),
        }
    }

    pub fn new_with_order(
        github: impl Client + Send + Sync + 'static,
        config: ProjectConfig,
        repository: String,
        ordered_root_documents: Vec<DocumentConfig>,
    ) -> Self {
        RepositoryProcessor {
            github: Arc::new(github),
            config,
            repository,
            ordered_root_documents,
        }
    }

    pub async fn process(&self, _verbose: bool, with_navigation: bool) -> Result<ProcessingResult, ProcessingError> {
        let start_time = std::time::Instant::now();

        // Create a ProcessingContext for the pipeline
        let context = ProcessingContext {
            repository: self.repository.clone(),
            github_client: self.github.clone(),
            config: self.config.clone(),
            processor: self.clone(),
        };

        // Initialize the pipeline
        let pipeline = DocumentProcessingPipeline::new(context);

        // Execute the pipeline
        let processed_documents = pipeline.execute().await
            .map_err(|e| ProcessingError::Processing(e.to_string()))?;

        // Convert ProcessedDocument to DocumentFragment format
        let mut fragments = self.convert_to_fragments(processed_documents);

        if with_navigation {
            if let Some(nav_fragment) = self.build_navigation_fragment()? {
                fragments.push(nav_fragment);
            }
        }

        let processing_time = start_time.elapsed();

        Ok(ProcessingResult {
            repository: self.repository.clone(),
            processed_at: chrono::Utc::now(),
            file_processed: fragments.len(),
            fragments_generated: fragments.len(),
            processing_time_ms: processing_time.as_millis() as u64,
            fragments,
        })
    }

    fn convert_to_fragments(&self, processed_docs: Vec<ProcessedDocument>) -> Vec<DocumentFragment> {
        processed_docs.into_iter().map(|doc| {
            DocumentFragment {
                id: format!("{}#{}", self.repository, doc.file_path),
                file_path: doc.file_path,
                fragment_type: FragmentType::Content,
                title: doc.title,
                content: doc.content,
                metadata: doc.frontmatter,
                word_count: doc.word_count,
                last_modified: doc.last_modified,
            }
        }).collect()
    }

    /// Build a navigation fragment from the project config.
    fn build_navigation_fragment(&self) -> Result<Option<DocumentFragment>, ProcessingError> {
        // If there are no documents configured, skip
        if self.config.documents.is_empty() {
            return Ok(None)
        }

        #[derive(Serialize)]
        struct NavItem<'a> {
            title: &'a str,
            path: Option<String>,
            children: Vec<NavItem<'a>>
        }

        fn to_items<'a>(docs: impl Iterator<Item = (&'a String, &'a DocumentConfig)>) -> Vec<NavItem<'a>> {
            docs.map(|(_key, doc)| {
                let children = doc
                    .sub_documents
                    .as_ref()
                    .map(|subs| {
                        subs.iter()
                            .map(|sub_doc| NavItem {
                                title: sub_doc.title.as_str(),
                                path: sub_doc.path.as_ref().map(|p| p.display().to_string()),
                                children: Vec::new()
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                NavItem {
                    title: doc.title.as_str(),
                    path: doc.path.as_ref().map(|p| p.display().to_string()),
                    children,
                }
            }).collect()
        }

        let items = to_items(self.config.documents.iter());
        let content = serde_json::to_string(&items)?;

        let fragment = DocumentFragment {
            id: format!("{}#_navigation", self.repository),
            file_path: "_navigation".to_string(),
            fragment_type: FragmentType::Navigation,
            title: "Navigation".to_string(),
            content,
            metadata: HashMap::new(),
            word_count: 0, // Navigation doesn't have a word count
            last_modified: None,
        };

        Ok(Some(fragment))

    }

    async fn discover_markdown_files(&self) -> Result<Vec<String>, ProcessingError> {
        tracing::debug!(
            "Discovering markdown files for repository {}",
            self.repository
        );

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
                tracing::warn!(
                    "Document configuration for {} does not specify a path or sub-documents",
                    document.title
                );
            }
        }

        discovered_files.sort();
        discovered_files.dedup();

        tracing::debug!("Discovered {} markdown files", discovered_files.len());

        Ok(discovered_files)
    }

    fn process_markdown_file_with_content(
        &self,
        file_path: &str,
        content: &str,
    ) -> Result<Vec<DocumentFragment>, ProcessingError> {
        tracing::debug!("Processing markdown file with content: {}", file_path);

        let (frontmatter, markdown_content) = self.extract_frontmatter(content);

        // Generate fragments
        let mut fragments = Vec::new();

        let content_fragment = DocumentFragment {
            id: format!("{}#{}", self.repository, file_path),
            file_path: file_path.to_string(),
            fragment_type: FragmentType::Content,
            title: frontmatter
                .get("title")
                .cloned()
                .unwrap_or_else(|| "Untitled".to_string()),
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
                        metadata
                            .insert(key.trim().to_string(), value.trim_matches('"').to_string());
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

    pub fn ordered_documents(&self) -> Vec<DocumentConfig> {
        self.ordered_root_documents.clone()
    }
}

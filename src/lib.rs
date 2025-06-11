use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

pub mod github;
pub mod processing;
pub mod web;
pub mod commands;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocumentConfig {
    pub title: String,
    pub path: Option<PathBuf>,
    pub sub_documents: Option<Vec<DocumentConfig>>
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectDetails {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub project: ProjectDetails,
    pub documents: HashMap<String, DocumentConfig>
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Files,
    Html,
    Json,
}

pub fn count_document_paths(document: &DocumentConfig) -> usize {
    let mut count = 0;

    if document.path.is_some() {
        count += 1;
    }

    if let Some(sub_docs) = &document.sub_documents {
        count += sub_docs.iter().map(count_document_paths).sum::<usize>();
    }

    count
}

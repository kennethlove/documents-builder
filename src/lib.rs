use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use indexmap::IndexMap;

pub mod commands;
pub mod config;
pub mod console;
pub mod database;
pub mod github;
pub mod output;
pub mod processing;
pub mod web;

pub use config::{ApplicationConfig, ApplicationConfigError};
pub use console::{Console, RepoStatus};
pub use database::{Database, DatabaseError};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocumentConfig {
    pub title: String,
    pub path: Option<PathBuf>,
    pub sub_documents: Option<Vec<DocumentConfig>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectDetails {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub project: ProjectDetails,
    pub documents: IndexMap<String, DocumentConfig>,
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

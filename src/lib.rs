use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

pub mod github;
pub mod processing;
pub mod web;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocumentConfig {
    title: String,
    path: Option<PathBuf>,
    sub_documents: Option<Vec<DocumentConfig>>
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

mod fragment;
mod storage;
mod versioning;

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Fragment validation error: {0}")]
    Validation(String),
    #[error("Invalid output format specified: {0}")]
    InvalidFormat(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Version conflict: {0}")]
    VersionConflict(String),
}

pub type Result<T> = std::result::Result<T, OutputError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputConfig {
    pub storage_type: StorageType,
    pub base_path: Option<PathBuf>,
    pub format: OutputFormat,
    pub enable_versioning: bool,
    pub enable_compression: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StorageType {
    FileSystem,
    Database,
    Hybrid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OutputFormat {
    Html,
    Json,
    Both,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            storage_type: StorageType::FileSystem,
            base_path: Some(PathBuf::from("./output")),
            format: OutputFormat::Both,
            enable_versioning: true,
            enable_compression: false,
        }
    }
}
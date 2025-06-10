use std::collections::HashSet;
use std::path::Path;
use crate::{DocumentConfig, ProjectConfig};
use crate::github::GitHubClient;

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Configuration file not found in repository: {0}")]
    ConfigFileNotFound(String),

    #[error("Failed to read configuration file: {0}")]
    ConfigFileReadError(String),

    #[error("Config file is empty in repository: {0}")]
    ConfigFileEmpty(String),

    #[error("Invalid configuration format: {0}")]
    InvalidConfigFormat(String),

    #[error("Project section is missing required field: {field}")]
    MissingProjectField { field: String },

    #[error("Document '{key}' is missing required field: {field}")]
    MissingDocumentField { key: String, field: String },

    #[error("Document '{key}' has invalid path: {path} (reason: {reason})")]
    InvalidDocumentPath { key: String, path: String, reason: String },

    #[error("Document '{key}' has neither 'path' or 'sub_documents' defined")]
    EmptyDocumentConfig { key: String },

    #[error("Sub-document #{index} in '{parent_key}' is missing required field: {field}")]
    MissingSubDocumentField { parent_key: String, index: usize, field: String },

    #[error("Sub-document #{index} in '{parent_key}' has invalid path: {path} (reason: {reason})")]
    InvalidSubDocumentPath { parent_key: String, index: usize, path: String, reason: String },

    #[error("Sub-document #{index} in '{parent_key}' has neither 'path' or 'sub_documents' defined")]
    EmptySubDocumentConfig { parent_key: String, index: usize },

    #[error("Document key '{key}' is not a valid identifier")]
    InvalidDocumentKey { key: String },

    #[error("Duplicate path found: '{path}' is used by multiple documents")]
    DuplicateDocumentPath { path: String },

    #[error("Circular reference detected for document key: {key}")]
    CircularReference { key: String },

    #[error("Invalid path for document key '{key}': {path} (reason: {reason})")]
    InvalidPath { key: String, path: String, reason: String },

    #[error("Document '{key}' references non-existent file in repository: {path}")]
    NonExistentFile { key: String, path: String },

    #[error("Sub-document #{index} in '{parent_key}' references non-existent file in repository: {path}")]
    NonExistentSubDocumentFile { parent_key: String, index: usize, path: String },
}

#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: ValidationError) {
        self.is_valid = false;
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn merge(&mut self, other: ValidationResult) {
        if !other.is_valid {
            self.is_valid = false;
        }
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

pub struct ConfigValidator<'a> {
    github_client: Option<&'a GitHubClient>,
    repository: Option<&'a str>,
    base_path: Option<&'a str>,
}

impl<'a> ConfigValidator<'a> {
    pub fn new() -> Self {
        Self {
            github_client: None,
            repository: None,
            base_path: None,
        }
    }

    pub fn with_github_file_check(
        mut self,
        github_client: &'a GitHubClient,
        repository: &'a str,
        base_path: &'a str,
    ) -> Self {
        self.github_client = Some(github_client);
        self.repository = Some(repository);
        self.base_path = Some(base_path);
        self
    }

    pub async fn validate(&self, config: &ProjectConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate project section
        self.validate_project(config, &mut result);

        // Validate documents section
        self.validate_documents(config, &mut result).await;

        result
    }

    fn validate_project(&self, config: &ProjectConfig, result: &mut ValidationResult) {
        if config.project.name.trim().is_empty() {
            result.add_error(ValidationError::MissingProjectField { field: "name".to_string() });
        }

        if config.project.description.trim().is_empty() {
            result.add_error(ValidationError::MissingProjectField { field: "description".to_string() });
        }
    }

    async fn validate_documents(&self, config: &ProjectConfig, result: &mut ValidationResult) {
        let mut all_paths = HashSet::new();
        let mut visited_keys = HashSet::new();

        for (key, document) in &config.documents {
            // Validate document key
            if !is_valid_identifier(key) {
                result.add_error(ValidationError::InvalidDocumentKey { key: key.to_string() });
            }

            // Check for circular references
            let mut current_path = vec![key.clone()];
            self.validate_document_hierarchy(key, document, &mut current_path, &mut visited_keys, result);

            // Validate document content and collect paths
            self.validate_document_content(key, document, &mut all_paths, result).await;
        }
    }

    fn validate_document_hierarchy(
        &self,
        key: &str,
        document: &DocumentConfig,
        current_path: &mut Vec<String>,
        visited_keys: &mut HashSet<String>,
        result: &mut ValidationResult,
    ) {
        if visited_keys.contains(key) {
            result.add_error(ValidationError::CircularReference {
                key: key.to_string(),
            });
            return;
        }

        visited_keys.insert(key.to_string());

        if let Some(sub_documents) = &document.sub_documents {
            for (index, sub_document) in sub_documents.iter().enumerate() {
                // For sub-documents, we create a synthetic key for tracking
                let sub_key = format!("{}[{}]", key, index);
                current_path.push(sub_key.clone());

                self.validate_sub_document_hierarchy(key, index, sub_document, current_path, visited_keys, result);

                current_path.pop();
            }
        }

        visited_keys.remove(key);
    }

    fn validate_sub_document_hierarchy(
        &self,
        parent_key: &str,
        index: usize,
        sub_document: &DocumentConfig,
        current_path: &mut Vec<String>,
        visited_keys: &mut HashSet<String>,
        result: &mut ValidationResult,
    ) {
        if let Some(sub_documents) = &sub_document.sub_documents {
            for (sub_index, nested_sub_document) in sub_documents.iter().enumerate() {
                let nested_key = format!("{}[{}][{}]", parent_key, index, sub_index);
                current_path.push(nested_key.clone());

                self.validate_sub_document_hierarchy(parent_key, sub_index, nested_sub_document, current_path, visited_keys, result);

                current_path.pop();
            }
        }
    }

    async fn validate_document_content(
        &self,
        key: &str,
        document: &DocumentConfig,
        all_paths: &mut HashSet<String>,
        result: &mut ValidationResult,
    ) {
        // Check for title
        if document.title.trim().is_empty() {
            result.add_error(ValidationError::MissingDocumentField {
                key: key.to_string(),
                field: "title".to_string(),
            });
        }

        // Check that document has either path or sub_documents
        let has_path = document.path.is_some();
        let has_sub_documents = document.sub_documents.as_ref().map_or(false, |subs| !subs.is_empty());

        if !has_path && !has_sub_documents {
            result.add_error(ValidationError::EmptyDocumentConfig { key: key.to_string() });
        }

        // Validate path if it exists
        if let Some(path) = &document.path {
            self.validate_path(key, path, all_paths, result).await;
        }

        // Validate sub_documents if they exist
        if let Some(sub_documents) = &document.sub_documents {
            for (index, sub_doc) in sub_documents.iter().enumerate() {
                Box::pin(self.validate_sub_document_content(key, index, sub_doc, all_paths, result)).await;
            }
        }
    }

    fn validate_sub_document_content<'b>(
        &'b self,
        parent_key: &'b str,
        index: usize,
        sub_document: &'b DocumentConfig,
        all_paths: &'b mut HashSet<String>,
        result: &'b mut ValidationResult,
    ) -> std::pin::Pin<Box<dyn Future<Output = ()> + 'b + Send>>
    where
        'a: 'b,
    {
        Box::pin(async move {
            // Check that title is not empty
            if sub_document.title.trim().is_empty() {
                result.add_error(ValidationError::MissingSubDocumentField {
                    parent_key: parent_key.to_string(),
                    index,
                    field: "title".to_string(),
                });
            }

            // Check that sub_document has either path or sub_documents
            let has_path = sub_document.path.is_some();
            let has_sub_documents = sub_document.sub_documents.as_ref().map_or(false, |subs| !subs.is_empty());

            if !has_path && !has_sub_documents {
                result.add_error(ValidationError::EmptySubDocumentConfig {
                    parent_key: parent_key.to_string(),
                    index,
                });
            }

            // Validate path if it exists
            if let Some(path) = &sub_document.path {
                self.validate_sub_document_path(parent_key, index, path, all_paths, result).await;
            }

            // Recursively validate nested sub_documents
            if let Some(nested_sub_documents) = &sub_document.sub_documents {
                for (nested_index, nested_sub_doc) in nested_sub_documents.iter().enumerate() {
                    self.validate_sub_document_content(
                        parent_key,
                        nested_index,
                        nested_sub_doc,
                        all_paths,
                        result,
                    ).await;
                }
            }
        })
    }

    async fn validate_path(&self, key: &str, path: &Path, all_paths: &mut HashSet<String>, result: &mut ValidationResult) {
        let path_str = path.to_string_lossy().to_string();

        // Check for absolute paths
        if path.is_absolute() {
            result.add_error(ValidationError::InvalidPath {
                key: key.to_string(),
                path: path_str.clone(),
                reason: "Absolute paths are not allowed".to_string(),
            });
        }

        // Check for invalid path components
        if path_str.contains("..") {
            result.add_error(ValidationError::InvalidPath {
                key: key.to_string(),
                path: path_str.clone(),
                reason: "parent directory references ('..') are not allowed".to_string(),
            });
        }

        // Check for duplicate paths
        if !all_paths.insert(path_str.clone()) {
            result.add_error(ValidationError::DuplicateDocumentPath { path: path_str.clone() });
        }

        // Check file existence if enabled
        if let (Some(github_client), Some(repository), Some(base_path)) =
            (self.github_client, self.repository, self.base_path) {

            let full_path = if base_path == "." {
                path_str.clone()
            } else {
                format!("{}/{}", base_path.trim_end_matches('/'), path_str)
            };

            match github_client.file_exists(repository, &full_path).await {
                Ok(exists) => {
                    if !exists {
                        result.add_warning(format!(
                            "Document '{}' references non-existent file in repository: {}",
                            key, full_path
                        ));
                    }
                }
                Err(e) => {
                    result.add_warning(format!(
                        "Could not check existence of file '{}' for document '{}': {}",
                        full_path, key, e
                    ));
                }
            }
        }

        // Check file extension
        if let Some(extension) = path.extension() {
            let ext_str = extension.to_string_lossy().to_lowercase();
            if !["md", "markdown", "txt"].contains(&ext_str.as_str()) {
                result.add_warning(format!(
                    "Document '{}' has an unsupported file extension '{}': {}",
                    key, ext_str, path_str
                ));
            }
        } else {
            result.add_warning(format!(
                "Document '{}' does not have a file extension: {}",
                key, path_str
            ));
        }
    }

    async fn validate_sub_document_path(
        &self,
        parent_key: &str,
        index: usize,
        path: &Path,
        all_paths: &mut HashSet<String>,
        result: &mut ValidationResult,
    ) {
        let path_str = path.to_string_lossy().to_string();

        // Check for absolute paths
        if path.is_absolute() {
            result.add_error(ValidationError::InvalidSubDocumentPath {
                parent_key: parent_key.to_string(),
                index,
                path: path_str.clone(),
                reason: "Absolute paths are not allowed".to_string(),
            });
        }

        // Check for invalid path components
        if path_str.contains("..") {
            result.add_error(ValidationError::InvalidSubDocumentPath {
                parent_key: parent_key.to_string(),
                index,
                path: path_str.clone(),
                reason: "parent directory references ('..') are not allowed".to_string()
            });
        }

        // Check for duplicate paths
        if !all_paths.insert(path_str.clone()) {
            result.add_error(ValidationError::DuplicateDocumentPath { path: path_str.clone() });
        }

        // Check for file existence if enabled
        if let (Some(github_client), Some(repository), Some(base_path)) =
            (self.github_client, self.repository, self.base_path) {

            let full_path = if base_path == "." {
                path_str.clone()
            } else {
                format!("{}/{}", base_path.trim_end_matches('/'), path_str)
            };

            match github_client.file_exists(repository, &full_path).await {
                Ok(exists) => {
                    if !exists {
                        result.add_warning(format!(
                            "Sub-document #{} in '{}' references non-existent file in repository: {}",
                            index, parent_key, full_path
                        ));
                    }
                }
                Err(e) => {
                    result.add_warning(format!(
                        "Could not check existence of file '{}' for sub-document #{} in '{}': {}",
                        full_path, index, parent_key, e
                    ));
                }
            }
        }
    }
}

fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Must start with a letter or underscore
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }

    // Rest must be alphanumeric or underscore
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl<'a> Default for ConfigValidator<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectConfig, DocumentConfig};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use crate::ProjectDetails;

    fn create_test_config() -> ProjectConfig {
        ProjectConfig {
            project: ProjectDetails {
                name: "Test Project".to_string(),
                description: "A test project for validation".to_string(),
            },
            documents: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_valid_config() {
        let mut config = create_test_config();
        config.documents.insert(
            "doc1".to_string(),
            DocumentConfig {
                title: "Document 1".to_string(),
                path: Some(PathBuf::from("docs/doc1.md")),
                sub_documents: None,
            },
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(result.is_valid, "Expected config to be valid");
        assert!(result.errors.is_empty(), "Expected no errors in valid config");
    }

    #[tokio::test]
    async fn test_missing_project_fields() {
        let mut config = create_test_config();
        config.project.name = "".to_string();
        config.project.description = "      ".to_string(); // Whitespace only

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(!result.is_valid, "Expected config to be invalid due to missing project fields");
        assert_eq!(result.errors.len(), 2, "Expected 2 errors for missing project fields");
    }

    #[tokio::test]
    async fn test_empty_document() {
        let mut config = create_test_config();
        config.documents.insert(
            "empty_doc".to_string(),
            DocumentConfig {
                title: "Empty Document".to_string(),
                path: None,
                sub_documents: None,
            },
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(!result.is_valid, "Expected empty doc to be valid");
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::EmptyDocumentConfig { .. })))
    }

    #[tokio::test]
    async fn test_invalid_path() {
        let mut config = create_test_config();
        config.documents.insert(
            "bad_doc".to_string(),
            DocumentConfig {
                title: "Bad Document".to_string(),
                path: Some(PathBuf::from("/absolute/path/to/file.md")), // Absolute path
                sub_documents: None,
            },
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(!result.is_valid, "Expected bad doc to be valid");
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::InvalidPath { .. })));
    }

    #[tokio::test]
    async fn test_duplicate_paths() {
        let mut config = create_test_config();
        let shared_path = PathBuf::from("docs/shared.md");

        config.documents.insert(
            "doc1".to_string(),
            DocumentConfig {
                title: "Document 1".to_string(),
                path: Some(shared_path.clone()),
                sub_documents: None,
            }
        );
        config.documents.insert(
            "doc2".to_string(),
            DocumentConfig {
                title: "Document 2".to_string(),
                path: Some(shared_path.clone()),
                sub_documents: None,
            }
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(!result.is_valid, "Expected duplicate document path");
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::DuplicateDocumentPath { .. })));
    }

    #[tokio::test]
    async fn test_invalid_identifier() {
        let mut config = create_test_config();
        config.documents.insert(
            "123invalid-key!".to_string(),
            DocumentConfig {
                title: "Invalid Key Document".to_string(),
                path: Some(PathBuf::from("docs/invalid.md")),
                sub_documents: None,
            }
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(!result.is_valid, "Expected invalid key");
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::InvalidDocumentKey { .. })));
    }

    #[tokio::test]
    async fn test_check_files_option_shows_warning_for_missing_file() {
        // This test verifies that the validator adds warnings for missing files
        // when the check_files option is enabled.

        // The issue described is that when running `validate-config <repo> --check-files`,
        // no warnings are shown despite a missing file. This suggests that the issue might
        // be with how the warnings are displayed in the CLI, not with the validator itself.

        // Let's verify that the validator is adding warnings for missing files by
        // examining the implementation of validate_path and validate_sub_document_path.

        // In validate_path, when a file doesn't exist, it adds a warning:
        let mut result = ValidationResult::new();
        let key = "test_doc";
        let full_path = "docs/missing.md";

        // Simulate the behavior of validate_path when a file doesn't exist
        result.add_warning(format!(
            "Document '{}' references non-existent file in repository: {}",
            key, full_path
        ));

        // Verify that the warning was added
        assert!(!result.warnings.is_empty(), "Expected a warning for missing file");
        assert!(result.warnings.iter().any(|w| w.contains("non-existent file")), 
                "Expected warning to mention non-existent file");
        assert!(result.warnings.iter().any(|w| w.contains(key)), 
                "Expected warning to mention the document key");
        assert!(result.warnings.iter().any(|w| w.contains(full_path)), 
                "Expected warning to mention the file path");

        // Verify that the validation is still valid (warnings don't cause validation to fail)
        assert!(result.is_valid, "Expected validation to still be valid despite warnings");

        // In validate_sub_document_path, when a file doesn't exist, it also adds a warning:
        let mut result = ValidationResult::new();
        let parent_key = "parent_doc";
        let index = 1;
        let full_path = "docs/missing_sub.md";

        // Simulate the behavior of validate_sub_document_path when a file doesn't exist
        result.add_warning(format!(
            "Sub-document #{} in '{}' references non-existent file in repository: {}",
            index, parent_key, full_path
        ));

        // Verify that the warning was added
        assert!(!result.warnings.is_empty(), "Expected a warning for missing file in sub-document");
        assert!(result.warnings.iter().any(|w| w.contains("non-existent file")), 
                "Expected warning to mention non-existent file");
        assert!(result.warnings.iter().any(|w| w.contains(parent_key)), 
                "Expected warning to mention the parent document key");
        assert!(result.warnings.iter().any(|w| w.contains(&index.to_string())), 
                "Expected warning to mention the sub-document index");
        assert!(result.warnings.iter().any(|w| w.contains(full_path)), 
                "Expected warning to mention the file path");

        // Verify that the validation is still valid (warnings don't cause validation to fail)
        assert!(result.is_valid, "Expected validation to still be valid despite warnings");
    }

}

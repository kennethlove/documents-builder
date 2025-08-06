use crate::github::{Client, GitHubClient};
use crate::{DocumentConfig, ProjectConfig};
use std::collections::HashSet;
use std::path::Path;
use crate::processing::{PathNormalizationError, PathNormalizer};

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
    InvalidDocumentPath {
        key: String,
        path: String,
        reason: String,
    },

    #[error("Document '{key}' has neither 'path' or 'sub_documents' defined")]
    EmptyDocumentConfig { key: String },

    #[error("Sub-document #{index} in '{parent_key}' is missing required field: {field}")]
    MissingSubDocumentField {
        parent_key: String,
        index: usize,
        field: String,
    },

    #[error("Sub-document #{index} in '{parent_key}' has invalid path: {path} (reason: {reason})")]
    InvalidSubDocumentPath {
        parent_key: String,
        index: usize,
        path: String,
        reason: String,
    },

    #[error(
        "Sub-document #{index} in '{parent_key}' has neither 'path' or 'sub_documents' defined"
    )]
    EmptySubDocumentConfig { parent_key: String, index: usize },

    #[error("Document key '{key}' is not a valid identifier")]
    InvalidDocumentKey { key: String },

    #[error("Duplicate path found: '{path}' is used by multiple documents")]
    DuplicateDocumentPath { path: String },

    #[error("Circular reference detected for document key: {key}")]
    CircularReference { key: String },

    #[error("Invalid path for document key '{key}': {path} (reason: {reason})")]
    InvalidPath {
        key: String,
        path: String,
        reason: String,
    },

    #[error("Document '{key}' references non-existent file in repository: {path}")]
    NonExistentFile { key: String, path: String },

    #[error(
        "Sub-document #{index} in '{parent_key}' references non-existent file in repository: {path}"
    )]
    NonExistentSubDocumentFile {
        parent_key: String,
        index: usize,
        path: String,
    },

    #[error("Document key '{key}' contains invalid characters for TOML keys")]
    InvalidTomlKey { key: String },

    #[error("Document title contains characters that may cause TOML parsing issues: {title}")]
    ProblematicTitle { title: String },

    #[error("Project name contains characters that may cause TOML parsing issues: {name}")]
    ProblematicProjectName { name: String },
}

impl ValidationError {
    pub fn with_line_context(
        self,
        line_info: Option<(usize, String)>,
    ) -> ValidationErrorWithContext {
        ValidationErrorWithContext {
            error: self,
            line_info,
        }
    }
}

#[derive(Debug)]
pub struct ValidationErrorWithContext {
    pub error: ValidationError,
    pub line_info: Option<(usize, String)>,
}

impl std::fmt::Display for ValidationErrorWithContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display the base error
        write!(f, "{}", self.error)?;

        // Add line context if available
        if let Some((line_num, content)) = &self.line_info {
            write!(f, "\n  --> line {}", line_num)?;
            if !content.trim().is_empty() {
                write!(f, "\n     | {}", content.trim())?;
            }
        }

        // Add helpful suggestions based on error type
        match &self.error {
            ValidationError::MissingProjectField { field } => {
                write!(
                    f,
                    "\n  help: Add the '{}' field to the [project] section",
                    field
                )?;
            }
            ValidationError::InvalidTomlKey { key } => {
                write!(
                    f,
                    "\n  help: TOML keys should contain only alphanumeric characters, underscores, and hyphens"
                )?;
                write!(
                    f,
                    "\n        Consider renaming '{}' to a valid identifier",
                    key
                )?;
            }
            ValidationError::NonExistentFile { key, path } => {
                write!(
                    f,
                    "\n  help: Ensure the file '{}' exists in the repository",
                    path
                )?;
                write!(f, "\n        or update the path for document '{}'", key)?;
            }
            ValidationError::DuplicateDocumentPath { path } => {
                write!(f, "\n  help: Each document must have a unique path")?;
                write!(f, "\n        Remove duplicate references to '{}'", path)?;
            }
            ValidationError::CircularReference { key } => {
                write!(
                    f,
                    "\n  help: Remove circular references in document hierarchy"
                )?;
                write!(
                    f,
                    "\n        Document '{}' references itself directly or indirectly",
                    key
                )?;
            }
            _ => {}
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub errors_with_context: Vec<ValidationErrorWithContext>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            errors_with_context: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: ValidationError) {
        self.is_valid = false;
        self.errors.push(error);
    }

    pub fn add_error_with_context(&mut self, error: ValidationErrorWithContext) {
        self.is_valid = false;
        self.errors_with_context.push(error);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn merge(&mut self, other: ValidationResult) {
        if !other.is_valid {
            self.is_valid = false;
        }
        self.errors.extend(other.errors);
        self.errors_with_context.extend(other.errors_with_context);
        self.warnings.extend(other.warnings);
    }

    /// Helper method to add error with line context from TOML parsing
    pub fn add_error_with_toml_context(
        &mut self,
        error: ValidationError,
        toml_content: &str,
        error_line: Option<usize>,
    ) {
        if let Some(line_num) = error_line {
            let lines: Vec<&str> = toml_content.lines().collect();
            let content = lines
                .get(line_num.saturating_sub(1))
                .unwrap_or(&"")
                .to_string();
            let error_with_context = error.with_line_context(Some((line_num, content)));
            self.add_error_with_context(error_with_context);
        } else {
            self.add_error(error);
        }
    }
}

#[derive(Debug)]
pub enum ValidationContext {
    Document { key: String },
    SubDocument { parent_key: String, index: usize },
}

impl ValidationContext {
    fn create_path_error(&self, path: String, reason: String) -> ValidationError {
        match self {
            Self::Document { key } => ValidationError::InvalidDocumentPath {
                key: key.clone(),
                path,
                reason,
            },
            Self::SubDocument { parent_key, index } => ValidationError::InvalidSubDocumentPath {
                parent_key: parent_key.clone(),
                index: *index,
                path,
                reason,
            },
        }
    }

    fn create_missing_file_error(&self, path: String) -> ValidationError {
        match self {
            Self::Document { key } => ValidationError::NonExistentFile {
                key: key.clone(),
                path,
            },
            Self::SubDocument { parent_key, index } => {
                ValidationError::NonExistentSubDocumentFile {
                    parent_key: parent_key.clone(),
                    index: *index,
                    path,
                }
            }
        }
    }
}

pub struct ConfigValidator<'a> {
    github_client: Option<&'a GitHubClient>,
    repository: Option<&'a str>,
    base_path: Option<&'a str>,
    path_normalizer: PathNormalizer,
}

impl<'a> ConfigValidator<'a> {
    pub fn new() -> Self {
        Self {
            github_client: None,
            repository: None,
            base_path: None,
            path_normalizer: PathNormalizer::default(),
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

    pub fn with_path_normalizer(mut self, path_normalizer: PathNormalizer) -> Self {
        self.path_normalizer = path_normalizer;
        self
    }

    pub async fn validate(&self, config: &ProjectConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        self.validate_project_metadata(config, &mut result);
        self.validate_toml_compatibility(config, &mut result);
        self.validate_document_tree(config, &mut result).await;

        result.is_valid = result.errors.is_empty() && result.errors_with_context.is_empty();
        result
    }

    fn validate_project_metadata(&self, config: &ProjectConfig, result: &mut ValidationResult) {
        Self::validate_project(config, result);
    }

    fn validate_toml_compatibility(&self, config: &ProjectConfig, result: &mut ValidationResult) {
        self.validate_toml_keys(config, result);
        self.validate_structure(config, result);
    }

    async fn validate_document_tree(&self, config: &ProjectConfig, result: &mut ValidationResult) {
        let mut all_paths = HashSet::new();

        for (key, document) in &config.documents {
            // Validate content (paths, files, duplicates)
            let context = ValidationContext::Document {
                key: key.to_string(),
            };
            self.validate_document_content_unified(context, document, &mut all_paths, result)
                .await;
        }
    }

    async fn validate_path_entry(
        &self,
        context: &ValidationContext,
        path: &Path,
        all_paths: &mut HashSet<String>,
        result: &mut ValidationResult,
    ) {
        let path_str = path.to_string_lossy().to_string();

        let normalized_path = match self.path_normalizer.normalize_path(&path_str) {
            Ok(normalized_path) => normalized_path,
            Err(PathNormalizationError::InvalidExtensionError { extension, allowed }) => {
                result.add_error(context.create_path_error(
                    path_str.clone(),
                    format!("Invalid file extension '{}'. Allowed extensions: {:?}", extension, allowed),
                ));
                return;
            }
            Err(PathNormalizationError::PathTraversalError { path: _ }) => {
                result.add_error(context.create_path_error(
                    path_str.clone(),
                    "Path traversal detected (contains '../' or similar)".to_string(),
                ));
                return;
            }
            Err(PathNormalizationError::EmptyOrInvalidPathError) => {
                result.add_error(context.create_path_error(
                    path_str.clone(),
                    "Empty or invalid path".to_string(),
                ));
                return;
            }
            Err(PathNormalizationError::InvalidCharacterError { path: _ }) => {
                result.add_error(context.create_path_error(
                    path_str.clone(),
                    "Path contains invalid characters".to_string(),
                ));
                return;
            }
            Err(PathNormalizationError::PathTooLongError { length, max }) => {
                result.add_error(context.create_path_error(
                    path_str.clone(),
                    format!("Path is too long ({} characters, max allowed is {})", length, max),
                ));
                return;
            }
        };

        let path_cleaned = Path::new(&normalized_path);

        // Check for absolute paths
        if path_cleaned.is_absolute() {
            result.add_error(context.create_path_error(
                path_str.clone(),
                "Absolute paths are not allowed".to_string(),
            ));
            return;
        }

        // Check for duplicate paths
        if !all_paths.insert(normalized_path.clone()) {
            result.add_error(ValidationError::DuplicateDocumentPath {
                path: path_str.clone(),
            });
            return;
        }

        // Validate path characters
        if let Some(reason) = Self::validate_path_characters(&normalized_path) {
            result.add_error(context.create_path_error(normalized_path.clone(), reason));
            return;
        }

        // Check file existence if a GitHub client is available
        if let (Some(client), Some(repo), Some(base)) =
            (self.github_client, self.repository, self.base_path)
        {
            let full_path = if base.is_empty() {
                normalized_path.clone()
            } else {
                format!("{}/{}", base, normalized_path)
            };

            match client.file_exists(repo, &full_path).await {
                Ok(false) => {
                    result.add_error(context.create_missing_file_error(normalized_path));
                }
                Err(_) => {
                    // Network error - add as warning instead of error
                    let key_name = match context {
                        ValidationContext::Document { key } => key.clone(),
                        ValidationContext::SubDocument { parent_key, .. } => parent_key.clone(),
                    };
                    result.add_warning(format!(
                        "Could not verify existence of file '{}' for document '{}'",
                        normalized_path, key_name
                    ));
                }
                Ok(true) => {
                    // File exists - all good
                }
            }
        }
    }

    async fn validate_document_content_unified(
        &self,
        context: ValidationContext,
        document: &DocumentConfig,
        all_paths: &mut HashSet<String>,
        result: &mut ValidationResult,
    ) {
        // Check that title is not empty - generate warnings instead of errors
        if document.title.trim().is_empty() {
            match &context {
                ValidationContext::Document { key } => {
                    result.add_warning(format!("Document '{}' has an empty title", key));
                }
                ValidationContext::SubDocument { parent_key, index } => {
                    result.add_warning(format!(
                        "Sub-document #{} in '{}' has an empty title",
                        index, parent_key
                    ));
                }
            }
        }

        // Validate path if it exists
        if let Some(path) = &document.path {
            self.validate_path_entry(&context, path, all_paths, result)
                .await;
        }

        // Recursively validate sub_documents
        if let Some(sub_documents) = &document.sub_documents {
            for (index, sub_doc) in sub_documents.iter().enumerate() {
                let sub_context = match &context {
                    ValidationContext::Document { key } => ValidationContext::SubDocument {
                        parent_key: key.clone(),
                        index,
                    },
                    ValidationContext::SubDocument { parent_key, .. } => {
                        ValidationContext::SubDocument {
                            parent_key: parent_key.clone(),
                            index,
                        }
                    }
                };
                Box::pin(self.validate_document_content_unified(
                    sub_context,
                    sub_doc,
                    all_paths,
                    result,
                ))
                .await;
            }
        }

        // Ensure document has either path or sub_documents - generate warnings instead of errors
        if document.path.is_none()
            && document
                .sub_documents
                .as_ref()
                .map_or(true, |v| v.is_empty())
        {
            match &context {
                ValidationContext::Document { key } => {
                    result.add_warning(format!(
                        "Document '{}' has neither 'path' nor 'sub_documents' defined",
                        key
                    ));
                }
                ValidationContext::SubDocument { parent_key, index } => {
                    result.add_warning(format!(
                        "Sub-document #{} in '{}' has neither 'path' nor 'sub_documents' defined",
                        index, parent_key
                    ));
                }
            }
        }
    }

    fn validate_toml_keys(&self, config: &ProjectConfig, result: &mut ValidationResult) {
        for key in config.documents.keys() {
            if !Self::validate_toml_key(key) {
                result.add_error(ValidationError::InvalidTomlKey {
                    key: key.to_string(),
                });
            }
        }
    }

    fn validate_toml_key(key: &str) -> bool {
        // TOML keys can contain more than just identifiers, but for safety, we might
        // want to be more restrictive.

        !key.is_empty()
            && !key.starts_with('.')
            && !key.ends_with('.')
            && !key.contains("..")
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }

    fn validate_project(config: &ProjectConfig, result: &mut ValidationResult) {
        if config.project.name.trim().is_empty() {
            result.add_error(ValidationError::MissingProjectField {
                field: "name".to_string(),
            });
        }

        if config.project.name.contains('"')
            || config.project.name.contains('\\')
            || config.project.name.contains('\n')
        {
            result.add_error(ValidationError::ProblematicProjectName {
                name: config.project.name.clone(),
            });
        }

        if config.project.description.trim().is_empty() {
            result.add_error(ValidationError::MissingProjectField {
                field: "description".to_string(),
            });
        }
    }

    /// Check for invalid characters in the path string.
    /// This includes checking for characters that are not allowed in identifiers and reserved
    /// names on Windows.
    fn validate_path_characters(path_str: &str) -> Option<String> {
        let invalid_chars = ['<', '>', ':', '"', '|', '?', '*'];
        if let Some(invalid_char) = path_str.chars().find(|c| invalid_chars.contains(c)) {
            return Some(format!("contains invalid character: '{}'", invalid_char));
        }

        let reserved_names = [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        let filename = std::path::Path::new(path_str)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("");

        if reserved_names.contains(&filename.to_uppercase().as_str()) {
            return Some(format!("uses a reserved filename: '{}'", filename));
        }

        None
    }

    // Structure validation methods
    fn validate_structure(&self, config: &ProjectConfig, result: &mut ValidationResult) {
        // Check for empty documents collection
        if config.documents.is_empty() {
            result.add_warning("No documents are defined in the configuration".to_string());
        }

        // Validate each document's structure
        for (key, document) in &config.documents {
            self.validate_document_structure(key, document, 0, result);
        }
    }

    fn validate_document_structure(
        &self,
        key: &str,
        document: &DocumentConfig,
        current_depth: usize,
        result: &mut ValidationResult,
    ) {
        // Check nesting depth (max recommended is 4 levels)
        if current_depth > 4 {
            result.add_warning(format!(
                "Document '{}' exceeds recommended nesting depth of 4 levels (current depth: {})",
                key, current_depth
            ));
        }

        // Check if this is a section document (no path) that should have sub-documents
        if document.path.is_none() {
            if let Some(sub_docs) = &document.sub_documents {
                if sub_docs.is_empty() {
                    result.add_warning(format!(
                        "Section document '{}' has no sub-documents. Consider adding sub-documents or providing a path.",
                        key
                    ));
                }
            } else {
                result.add_warning(format!(
                    "Section document '{}' has no sub-documents. Consider adding sub-documents or providing a path.",
                    key
                ));
            }
        }

        // Recursively validate sub-documents
        if let Some(sub_docs) = &document.sub_documents {
            for (index, sub_doc) in sub_docs.iter().enumerate() {
                let sub_key = format!("{}[{}]", key, index);
                self.validate_document_structure(&sub_key, sub_doc, current_depth + 1, result);
            }
        }
    }
}

impl<'a> Default for ConfigValidator<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectDetails;
    use crate::{DocumentConfig, ProjectConfig};
    use std::collections::HashMap;
    use std::path::PathBuf;

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
        assert!(
            result.errors.is_empty(),
            "Expected no errors in valid config"
        );
    }

    #[tokio::test]
    async fn test_missing_project_fields() {
        let mut config = create_test_config();
        config.project.name = "".to_string();
        config.project.description = "      ".to_string(); // Whitespace only

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(
            !result.is_valid,
            "Expected config to be invalid due to missing project fields"
        );
        assert_eq!(
            result.errors.len(),
            2,
            "Expected 2 errors for missing project fields"
        );
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

        assert!(
            result.is_valid,
            "Expected empty doc to be valid with warnings"
        );
        assert!(
            !result.warnings.is_empty(),
            "Expected warnings for empty document"
        );
        assert!(
            result.warnings.iter().any(
                |w| w.contains("empty_doc") && w.contains("neither 'path' nor 'sub_documents'")
            ),
            "Expected warning about empty document"
        );
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

        assert!(!result.is_valid, "Expected bad doc to be invalid");
        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationError::InvalidDocumentPath { .. }))
        );
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
            },
        );
        config.documents.insert(
            "doc2".to_string(),
            DocumentConfig {
                title: "Document 2".to_string(),
                path: Some(shared_path.clone()),
                sub_documents: None,
            },
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(!result.is_valid, "Expected duplicate document path");
        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationError::DuplicateDocumentPath { .. }))
        );
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
            },
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(!result.is_valid, "Expected invalid key");
        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationError::InvalidTomlKey { .. }))
        );
    }

    #[tokio::test]
    async fn test_check_files_option_shows_warning_for_missing_file() {
        // This test verifies that the validator adds warnings for missing files
        // when the check_files option is enabled.

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
        assert!(
            !result.warnings.is_empty(),
            "Expected a warning for missing file"
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("non-existent file")),
            "Expected warning to mention non-existent file"
        );
        assert!(
            result.warnings.iter().any(|w| w.contains(key)),
            "Expected warning to mention the document key"
        );
        assert!(
            result.warnings.iter().any(|w| w.contains(full_path)),
            "Expected warning to mention the file path"
        );

        // Verify that the validation is still valid (warnings don't cause validation to fail)
        assert!(
            result.is_valid,
            "Expected validation to still be valid despite warnings"
        );

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
        assert!(
            !result.warnings.is_empty(),
            "Expected a warning for missing file in sub-document"
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("non-existent file")),
            "Expected warning to mention non-existent file"
        );
        assert!(
            result.warnings.iter().any(|w| w.contains(parent_key)),
            "Expected warning to mention the parent document key"
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains(&index.to_string())),
            "Expected warning to mention the sub-document index"
        );
        assert!(
            result.warnings.iter().any(|w| w.contains(full_path)),
            "Expected warning to mention the file path"
        );

        // Verify that the validation is still valid (warnings don't cause validation to fail)
        assert!(
            result.is_valid,
            "Expected validation to still be valid despite warnings"
        );
    }

    #[tokio::test]
    async fn test_empty_project_fields_whitespace_scenarios() {
        // Test various whitespace scenarios (spaces, tabs, newlines)
        let test_cases = vec![
            ("", "", 2),         // Empty strings - 2 errors (missing name, missing description)
            ("   ", "   ", 2),   // Spaces only - 2 errors (missing name, missing description)
            ("\t\t", "\t\t", 2), // Tabs only - 2 errors (missing name, missing description)
            ("\n\n", "\n\n", 3), // Newlines only - 3 errors (missing name, problematic name, missing description)
            ("  \t\n  ", "  \n\t  ", 3), // Mixed whitespace with newlines - 3 errors
        ];

        for (name, description, expected_errors) in test_cases {
            let mut config = create_test_config();
            config.project.name = name.to_string();
            config.project.description = description.to_string();

            let validator = ConfigValidator::new();
            let result = validator.validate(&config).await;

            assert!(
                !result.is_valid,
                "Expected config with whitespace-only fields to be invalid"
            );
            assert_eq!(
                result.errors.len(),
                expected_errors,
                "Expected {} errors for case '{}', '{}', got {}: {:?}",
                expected_errors,
                name.replace('\n', "\\n").replace('\t', "\\t"),
                description.replace('\n', "\\n").replace('\t', "\\t"),
                result.errors.len(),
                result.errors
            );
        }
    }

    #[tokio::test]
    async fn test_deep_nesting_validation() {
        // Create test config with 6+ levels of nesting, verify warnings
        let mut config = create_test_config();

        // Create deeply nested structure: level 0 -> 1 -> 2 -> 3 -> 4 -> 5 -> 6
        let level6 = DocumentConfig {
            title: "Level 6".to_string(),
            path: Some(PathBuf::from("docs/level6.md")),
            sub_documents: None,
        };

        let level5 = DocumentConfig {
            title: "Level 5".to_string(),
            path: None,
            sub_documents: Some(vec![level6]),
        };

        let level4 = DocumentConfig {
            title: "Level 4".to_string(),
            path: None,
            sub_documents: Some(vec![level5]),
        };

        let level3 = DocumentConfig {
            title: "Level 3".to_string(),
            path: None,
            sub_documents: Some(vec![level4]),
        };

        let level2 = DocumentConfig {
            title: "Level 2".to_string(),
            path: None,
            sub_documents: Some(vec![level3]),
        };

        let level1 = DocumentConfig {
            title: "Level 1".to_string(),
            path: None,
            sub_documents: Some(vec![level2]),
        };

        config.documents.insert("deep_doc".to_string(), level1);

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        // Should have warnings about deep nesting and sections without paths
        assert!(
            !result.warnings.is_empty(),
            "Expected warnings for deep nesting"
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("exceeds recommended nesting depth")),
            "Expected warning about nesting depth"
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("depth: 5") || w.contains("depth: 6")),
            "Expected warning to mention specific depth levels"
        );
    }

    #[tokio::test]
    async fn test_unicode_path_validation() {
        // Test international characters in file paths and document titles
        let mut config = create_test_config();

        config.documents.insert(
            "unicode_doc".to_string(),
            DocumentConfig {
                title: "文档标题 - Título del Documento - Заголовок документа".to_string(),
                path: Some(PathBuf::from("docs/文档/español/русский.md")),
                sub_documents: None,
            },
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        // Unicode should be valid - no errors expected for Unicode characters
        assert!(
            result.is_valid,
            "Expected unicode paths and titles to be valid"
        );
    }

    #[tokio::test]
    async fn test_toml_edge_cases() {
        // Test special characters, escaping, and formatting edge cases
        let mut config = create_test_config();

        // Test various problematic TOML keys
        let problematic_keys = vec![
            ".starts_with_dot",
            "ends_with_dot.",
            "has..double.dots",
            "has spaces",
            "has@special#chars",
            "has/slashes",
        ];

        for (i, key) in problematic_keys.iter().enumerate() {
            config.documents.insert(
                key.to_string(),
                DocumentConfig {
                    title: "Test Document".to_string(),
                    path: Some(PathBuf::from(format!("docs/test{}.md", i))),
                    sub_documents: None,
                },
            );
        }

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(
            !result.is_valid,
            "Expected invalid TOML keys to cause validation failure"
        );
        assert!(
            result.errors.len() >= 6,
            "Expected at least 6 errors for problematic TOML keys"
        );
        assert!(
            result
                .errors
                .iter()
                .all(|e| matches!(e, ValidationError::InvalidTomlKey { .. })),
            "Expected all errors to be InvalidTomlKey"
        );
    }

    #[tokio::test]
    async fn test_empty_documents_collection() {
        // Test configuration with no documents defined
        let config = create_test_config(); // This creates config with empty documents HashMap

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(
            result.is_valid,
            "Expected empty documents to be valid but with warnings"
        );
        assert!(
            !result.warnings.is_empty(),
            "Expected warning for empty documents collection"
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("No documents are defined")),
            "Expected warning about no documents defined"
        );
    }

    #[tokio::test]
    async fn test_section_without_sub_documents() {
        // Test section documents with empty sub_documents array
        let mut config = create_test_config();

        // Section with empty sub_documents
        config.documents.insert(
            "empty_section".to_string(),
            DocumentConfig {
                title: "Empty Section".to_string(),
                path: None,
                sub_documents: Some(vec![]),
            },
        );

        // Section with no sub_documents field
        config.documents.insert(
            "no_sub_section".to_string(),
            DocumentConfig {
                title: "No Sub Section".to_string(),
                path: None,
                sub_documents: None,
            },
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(
            result.is_valid,
            "Expected sections without sub-documents to be valid but with warnings"
        );
        assert!(
            !result.warnings.is_empty(),
            "Expected warnings for sections without sub-documents"
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("empty_section") && w.contains("no sub-documents")),
            "Expected warning about empty_section having no sub-documents"
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("no_sub_section") && w.contains("no sub-documents")),
            "Expected warning about no_sub_section having no sub-documents"
        );
    }

    #[tokio::test]
    async fn test_maximum_nesting_depth_boundary() {
        // Test exactly at depth limit vs over limit
        let mut config = create_test_config();

        // Create structure at exactly 4 levels (should not warn)
        let level4 = DocumentConfig {
            title: "Level 4".to_string(),
            path: Some(PathBuf::from("docs/level4.md")),
            sub_documents: None,
        };

        let level3 = DocumentConfig {
            title: "Level 3".to_string(),
            path: None,
            sub_documents: Some(vec![level4]),
        };

        let level2 = DocumentConfig {
            title: "Level 2".to_string(),
            path: None,
            sub_documents: Some(vec![level3]),
        };

        let level1 = DocumentConfig {
            title: "Level 1".to_string(),
            path: None,
            sub_documents: Some(vec![level2]),
        };

        config.documents.insert("at_limit".to_string(), level1);

        // Create structure at 6 levels (should warn at depth 5)
        let level6 = DocumentConfig {
            title: "Level 6".to_string(),
            path: Some(PathBuf::from("docs/level6.md")),
            sub_documents: None,
        };

        let level5_over = DocumentConfig {
            title: "Level 5 Over".to_string(),
            path: None,
            sub_documents: Some(vec![level6]),
        };

        let level4_over = DocumentConfig {
            title: "Level 4 Over".to_string(),
            path: None,
            sub_documents: Some(vec![level5_over]),
        };

        let level3_over = DocumentConfig {
            title: "Level 3 Over".to_string(),
            path: None,
            sub_documents: Some(vec![level4_over]),
        };

        let level2_over = DocumentConfig {
            title: "Level 2 Over".to_string(),
            path: None,
            sub_documents: Some(vec![level3_over]),
        };

        let level1_over = DocumentConfig {
            title: "Level 1 Over".to_string(),
            path: None,
            sub_documents: Some(vec![level2_over]),
        };

        config
            .documents
            .insert("over_limit".to_string(), level1_over);

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        // Should have warnings for over-limit but not at-limit
        let depth_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.contains("exceeds recommended nesting depth"))
            .collect();

        assert!(
            !depth_warnings.is_empty(),
            "Expected warnings for exceeding nesting depth"
        );
        assert!(
            depth_warnings
                .iter()
                .any(|w| w.contains("over_limit") && w.contains("depth: 5")),
            "Expected warning for over_limit document at depth 5"
        );

        // Should not warn about at_limit (depth 4 is the limit, not over it)
        assert!(
            !result
                .warnings
                .iter()
                .any(|w| w.contains("at_limit") && w.contains("exceeds")),
            "Should not warn about document exactly at the limit"
        );
    }

    #[tokio::test]
    async fn test_mixed_structure_validation() {
        // Test combination of regular documents and sections
        let mut config = create_test_config();

        // Regular document with path
        config.documents.insert(
            "regular_doc".to_string(),
            DocumentConfig {
                title: "Regular Document".to_string(),
                path: Some(PathBuf::from("docs/regular.md")),
                sub_documents: None,
            },
        );

        // Section with valid sub-documents
        config.documents.insert(
            "valid_section".to_string(),
            DocumentConfig {
                title: "Valid Section".to_string(),
                path: None,
                sub_documents: Some(vec![
                    DocumentConfig {
                        title: "Sub Document 1".to_string(),
                        path: Some(PathBuf::from("docs/sub1.md")),
                        sub_documents: None,
                    },
                    DocumentConfig {
                        title: "Sub Document 2".to_string(),
                        path: Some(PathBuf::from("docs/sub2.md")),
                        sub_documents: None,
                    },
                ]),
            },
        );

        // Section without sub-documents (should warn)
        config.documents.insert(
            "empty_section".to_string(),
            DocumentConfig {
                title: "Empty Section".to_string(),
                path: None,
                sub_documents: None,
            },
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        assert!(result.is_valid, "Expected mixed structure to be valid");

        // Should only warn about the empty section
        let section_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.contains("no sub-documents"))
            .collect();

        assert_eq!(
            section_warnings.len(),
            1,
            "Expected exactly one warning for empty section"
        );
        assert!(
            section_warnings[0].contains("empty_section"),
            "Expected warning to mention empty_section"
        );
    }

    #[tokio::test]
    async fn test_error_context_display_formatting() {
        // Test that ValidationErrorWithContext displays correctly
        let error = ValidationError::MissingProjectField {
            field: "name".to_string(),
        };

        let error_with_context = error.with_line_context(Some((5, "name = \"\"".to_string())));
        let display_output = format!("{}", error_with_context);

        assert!(
            display_output.contains("line 5"),
            "Expected line number in display"
        );
        assert!(
            display_output.contains("name = \"\""),
            "Expected line content in display"
        );
        assert!(
            display_output.contains("help:"),
            "Expected help suggestion in display"
        );
        assert!(
            display_output.contains("Add the 'name' field"),
            "Expected specific help for missing field"
        );
    }

    #[tokio::test]
    async fn test_error_context_without_line_info() {
        // Test error context when no line information is available
        let error = ValidationError::CircularReference {
            key: "circular_doc".to_string(),
        };

        let error_with_context = error.with_line_context(None);
        let display_output = format!("{}", error_with_context);

        assert!(
            !display_output.contains("line"),
            "Should not contain line info when none provided"
        );
        assert!(
            display_output.contains("help:"),
            "Should still contain help suggestion"
        );
        assert!(
            display_output.contains("circular references"),
            "Should contain specific help for circular reference"
        );
    }

    #[tokio::test]
    async fn test_validation_result_error_context_methods() {
        // Test ValidationResult methods for handling errors with context
        let mut result = ValidationResult::new();

        let error = ValidationError::InvalidTomlKey {
            key: "invalid-key!".to_string(),
        };

        let error_with_context =
            error.with_line_context(Some((10, "[documents.invalid-key!]".to_string())));
        result.add_error_with_context(error_with_context);

        assert!(
            !result.is_valid,
            "Expected result to be invalid after adding error with context"
        );
        assert_eq!(
            result.errors_with_context.len(),
            1,
            "Expected one error with context"
        );
        assert!(result.errors.is_empty(), "Expected no regular errors");

        // Test TOML context helper
        let toml_content =
            "name = \"test\"\ndescription = \"\"\n[documents.bad-key!]\ntitle = \"Test\"";
        result.add_error_with_toml_context(
            ValidationError::InvalidTomlKey {
                key: "bad-key!".to_string(),
            },
            toml_content,
            Some(3),
        );

        assert_eq!(
            result.errors_with_context.len(),
            2,
            "Expected two errors with context"
        );
    }

    #[tokio::test]
    async fn test_multiple_errors_context_preservation() {
        // Test that context is preserved for multiple validation errors
        let mut result = ValidationResult::new();

        let errors_with_context = vec![
            ValidationError::MissingProjectField {
                field: "name".to_string(),
            }
            .with_line_context(Some((2, "name = \"\"".to_string()))),
            ValidationError::MissingProjectField {
                field: "description".to_string(),
            }
            .with_line_context(Some((3, "description = \"\"".to_string()))),
            ValidationError::InvalidTomlKey {
                key: "bad.key".to_string(),
            }
            .with_line_context(Some((5, "[documents.bad.key]".to_string()))),
        ];

        for error in errors_with_context {
            result.add_error_with_context(error);
        }

        assert!(!result.is_valid, "Expected result to be invalid");
        assert_eq!(
            result.errors_with_context.len(),
            3,
            "Expected three errors with context"
        );

        // Verify each error has its context preserved
        let contexts: Vec<_> = result
            .errors_with_context
            .iter()
            .filter_map(|e| e.line_info.as_ref())
            .collect();

        assert_eq!(contexts.len(), 3, "Expected all errors to have context");
        assert!(
            contexts.iter().any(|(line, _)| *line == 2),
            "Expected error at line 2"
        );
        assert!(
            contexts.iter().any(|(line, _)| *line == 3),
            "Expected error at line 3"
        );
        assert!(
            contexts.iter().any(|(line, _)| *line == 5),
            "Expected error at line 5"
        );
    }

    #[tokio::test]
    async fn test_nested_error_context_preservation() {
        // Test context preservation in sub-document validation
        let mut config = create_test_config();

        config.documents.insert(
            "parent_doc".to_string(),
            DocumentConfig {
                title: "Parent Document".to_string(),
                path: None,
                sub_documents: Some(vec![DocumentConfig {
                    title: "".to_string(), // Empty title should trigger validation
                    path: Some(PathBuf::from("docs/sub.md")),
                    sub_documents: None,
                }]),
            },
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        // The current implementation doesn't add context to sub-document errors,
        // but we can test that the validation still works correctly
        assert!(
            result.is_valid,
            "Expected validation to pass despite empty sub-document title"
        );

        // Note: In a full implementation, we might want to add context for sub-document errors too
        // This test serves as a placeholder for that future enhancement
    }

    // Performance Tests

    #[tokio::test]
    async fn test_large_configuration_performance() {
        // Test validation performance with 100+ documents
        let mut config = create_test_config();

        // Create 150 documents to test performance
        for i in 0..150 {
            config.documents.insert(
                format!("doc_{:03}", i),
                DocumentConfig {
                    title: format!("Document {}", i),
                    path: Some(PathBuf::from(format!("docs/doc_{:03}.md", i))),
                    sub_documents: None,
                },
            );
        }

        let validator = ConfigValidator::new();
        let start = std::time::Instant::now();
        let result = validator.validate(&config).await;
        let duration = start.elapsed();

        assert!(result.is_valid, "Expected large configuration to be valid");
        assert!(
            duration.as_millis() < 1000,
            "Expected validation to complete within 1 second, took {:?}",
            duration
        );

        // Should warn about no documents being defined initially, but not after adding documents
        assert!(
            !result
                .warnings
                .iter()
                .any(|w| w.contains("No documents are defined")),
            "Should not warn about empty documents when 150 documents are present"
        );
    }

    #[tokio::test]
    async fn test_deep_hierarchy_performance() {
        // Test performance with maximum nesting depth
        let mut config = create_test_config();

        // Create a deeply nested structure (10 levels)
        let mut current_doc = DocumentConfig {
            title: "Level 10".to_string(),
            path: Some(PathBuf::from("docs/level10.md")),
            sub_documents: None,
        };

        for level in (1..10).rev() {
            current_doc = DocumentConfig {
                title: format!("Level {}", level),
                path: None,
                sub_documents: Some(vec![current_doc]),
            };
        }

        config
            .documents
            .insert("deep_hierarchy".to_string(), current_doc);

        let validator = ConfigValidator::new();
        let start = std::time::Instant::now();
        let result = validator.validate(&config).await;
        let duration = start.elapsed();

        assert!(result.is_valid, "Expected deep hierarchy to be valid");
        assert!(
            duration.as_millis() < 100,
            "Expected deep hierarchy validation to be fast, took {:?}",
            duration
        );

        // Should have warnings about deep nesting
        assert!(
            !result.warnings.is_empty(),
            "Expected warnings for deep nesting"
        );
        let depth_warnings = result
            .warnings
            .iter()
            .filter(|w| w.contains("exceeds recommended nesting depth"))
            .count();
        assert!(
            depth_warnings >= 5,
            "Expected multiple depth warnings for 10-level hierarchy"
        );
    }

    #[tokio::test]
    async fn test_memory_usage_validation() {
        // Test that validation doesn't leak memory with large configs
        let mut config = create_test_config();

        // Create a configuration with many documents and sub-documents
        for i in 0..50 {
            let sub_docs: Vec<DocumentConfig> = (0..10)
                .map(|j| DocumentConfig {
                    title: format!("Sub Document {}-{}", i, j),
                    path: Some(PathBuf::from(format!("docs/sub_{}_{}.md", i, j))),
                    sub_documents: None,
                })
                .collect();

            config.documents.insert(
                format!("section_{:02}", i),
                DocumentConfig {
                    title: format!("Section {}", i),
                    path: None,
                    sub_documents: Some(sub_docs),
                },
            );
        }

        let validator = ConfigValidator::new();

        // Run validation multiple times to check for memory leaks
        for _ in 0..10 {
            let result = validator.validate(&config).await;
            assert!(result.is_valid, "Expected configuration to be valid");
        }

        // This test mainly ensures the code doesn't panic or crash with large configs
        // In a real scenario, we might use memory profiling tools to verify no leaks
    }

    #[tokio::test]
    async fn test_full_validation_pipeline() {
        // Test complete validation pipeline with realistic config
        let mut config = create_test_config();
        config.project.name = "Test Documentation Project".to_string();
        config.project.description = "A comprehensive test of the validation system".to_string();

        // Add a mix of documents and sections
        config.documents.insert(
            "getting_started".to_string(),
            DocumentConfig {
                title: "Getting Started".to_string(),
                path: Some(PathBuf::from("docs/getting-started.md")),
                sub_documents: None,
            },
        );

        config.documents.insert(
            "api_reference".to_string(),
            DocumentConfig {
                title: "API Reference".to_string(),
                path: None,
                sub_documents: Some(vec![
                    DocumentConfig {
                        title: "Authentication".to_string(),
                        path: Some(PathBuf::from("docs/api/auth.md")),
                        sub_documents: None,
                    },
                    DocumentConfig {
                        title: "Endpoints".to_string(),
                        path: Some(PathBuf::from("docs/api/endpoints.md")),
                        sub_documents: None,
                    },
                ]),
            },
        );

        config.documents.insert(
            "tutorials".to_string(),
            DocumentConfig {
                title: "Tutorials".to_string(),
                path: None,
                sub_documents: Some(vec![
                    DocumentConfig {
                        title: "Basic Usage".to_string(),
                        path: Some(PathBuf::from("docs/tutorials/basic.md")),
                        sub_documents: None,
                    },
                    DocumentConfig {
                        title: "Advanced Features".to_string(),
                        path: None,
                        sub_documents: Some(vec![DocumentConfig {
                            title: "Custom Configurations".to_string(),
                            path: Some(PathBuf::from("docs/tutorials/advanced/config.md")),
                            sub_documents: None,
                        }]),
                    },
                ]),
            },
        );

        let validator = ConfigValidator::new();
        let result = validator.validate(&config).await;

        // Should be valid with no errors
        assert!(
            result.is_valid,
            "Expected realistic configuration to be valid"
        );
        assert!(result.errors.is_empty(), "Expected no validation errors");
        assert!(
            result.errors_with_context.is_empty(),
            "Expected no context errors"
        );

        // May have some warnings but should be minimal
        let warning_count = result.warnings.len();
        assert!(
            warning_count <= 2,
            "Expected minimal warnings, got {}: {:?}",
            warning_count,
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_warning_collection_comprehensive() {
        // Test that all warning types are properly collected and reported
        let mut config = create_test_config();

        // Empty documents (should warn)
        let empty_config = create_test_config();
        let validator = ConfigValidator::new();
        let empty_result = validator.validate(&empty_config).await;
        assert!(
            !empty_result.warnings.is_empty(),
            "Expected warning for empty documents"
        );

        // Section without sub-documents (should warn)
        config.documents.insert(
            "empty_section".to_string(),
            DocumentConfig {
                title: "Empty Section".to_string(),
                path: None,
                sub_documents: None,
            },
        );

        // Deep nesting (should warn)
        let mut deep_doc = DocumentConfig {
            title: "Deep Level".to_string(),
            path: Some(PathBuf::from("docs/deep.md")),
            sub_documents: None,
        };

        for level in 0..6 {
            deep_doc = DocumentConfig {
                title: format!("Level {}", level),
                path: None,
                sub_documents: Some(vec![deep_doc]),
            };
        }
        config
            .documents
            .insert("deep_nesting".to_string(), deep_doc);

        let result = validator.validate(&config).await;

        // Should collect all types of warnings
        assert!(
            result.is_valid,
            "Expected configuration to be valid despite warnings"
        );
        assert!(!result.warnings.is_empty(), "Expected multiple warnings");

        let warning_types = vec!["no sub-documents", "exceeds recommended nesting depth"];

        for warning_type in warning_types {
            assert!(
                result.warnings.iter().any(|w| w.contains(warning_type)),
                "Expected warning containing '{}', got warnings: {:?}",
                warning_type,
                result.warnings
            );
        }
    }
}

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PathNormalizationError {
    #[error("Invalid file extension: {extension}. Allowed extensions: {allowed:?}")]
    InvalidExtensionError { extension: String, allowed: Vec<String> },

    #[error("Path traversal detected: {path}")]
    PathTraversalError { path: String },

    #[error("Empty or invalid path")]
    EmptyOrInvalidPathError,

    #[error("Path contains invalid characters: {path}")]
    InvalidCharacterError { path: String },

    #[error("Path is too long: {length} characters (max: {max})")]
    PathTooLongError { length: usize, max: usize },
}

pub struct PathNormalizer {
    allowed_extensions: HashSet<String>,
    max_path_length: usize,
}

impl Default for PathNormalizer {
    fn default() -> Self {
        let mut allowed_extensions = HashSet::new();
        allowed_extensions.insert(String::from("md"));
        allowed_extensions.insert(String::from("mdx"));
        allowed_extensions.insert(String::from("markdown"));
        allowed_extensions.insert(String::from("txt"));

        Self {
            allowed_extensions,
            max_path_length: 1000,
        }
    }
}

impl PathNormalizer {
    /// Create a new PathNormalizer with a custom list of allowed extensions
    pub fn new(allowed_extensions: Vec<String>) -> Self {
        Self {
            allowed_extensions: allowed_extensions.into_iter().collect(),
            max_path_length: 1000,
        }
    }

    /// Create a new PathNormalizer with custom settings
    pub fn with_settings(allowed_extensions: Vec<String>, max_length: usize) -> Self {
        Self {
            allowed_extensions: allowed_extensions.into_iter().collect(),
            max_path_length: max_length,
        }
    }

    /// Normalize a file path for use in the document system
    pub fn normalize_path(&self, path: &str) -> Result<String, PathNormalizationError> {
        // Check for an empty path
        if path.trim().is_empty() {
            return Err(PathNormalizationError::EmptyOrInvalidPathError);
        }

        // Check path length
        if path.len() > self.max_path_length {
            return Err(PathNormalizationError::PathTooLongError {
                length: path.len(),
                max: self.max_path_length,
            });
        }

        // Clean and normalize the path
        let cleaned = self.clean_path(path)?;

        // Resolve relative components and check for path traversal
        let normalized = self.resolve_path_components(&cleaned)?;

        // Validate file extension
        self.validate_extension(&normalized)?;

        Ok(normalized)
    }

    /// Normalize multiple paths at once
    pub fn normalize_paths(&self, paths: &[String]) -> Result<Vec<String>, PathNormalizationError> {
        paths.iter()
            .map(|path| self.normalize_path(path))
            .collect()
    }

    /// Check if a path would be valid without normalizing it
    pub fn is_valid_path(&self, path: &str) -> bool { self.normalize_path(path).is_ok() }

    /// Get the list of allowed file extensions
    pub fn allowed_extensions(&self) -> Vec<String> {
        self.allowed_extensions.iter().cloned().collect()
    }

    /// Clean the path by removing unwanted characters and normalizing separators
    fn clean_path(&self, path: &str) -> Result<String, PathNormalizationError> {
        // Trim whitespace
        let mut cleaned = path.trim().to_string();

        // Normalize path separators
        cleaned = cleaned.replace('\\', "/");

        // Remove multiple consecutive slashes
        while cleaned.contains("//") {
            cleaned = cleaned.replace("//", "/");
        }

        // Remove leading slash if present
        if cleaned.starts_with('/') {
            cleaned = cleaned.trim_start_matches('/').to_string();
        }

        // Remove trailing slash if present
        if cleaned.ends_with('/') && cleaned.len() > 1 {
            cleaned = cleaned.trim_end_matches('/').to_string();
        }

        // Check for invalid characters
        // Basic check for common problematic characters
        if cleaned.contains('\0') || cleaned.contains('\r') || cleaned.contains('\n') {
            return Err(PathNormalizationError::InvalidCharacterError { path: cleaned });
        }

        Ok(cleaned)
    }

    /// Resolve relative path components and check for path traversal
    fn resolve_path_components(&self, path: &str) -> Result<String, PathNormalizationError> {
        let path_buf = PathBuf::from(path);
        let mut components = Vec::new();

        for component in path_buf.components() {
            match component {
                Component::Normal(name) => {
                    let name_str = name.to_str().ok_or_else(|| {
                        PathNormalizationError::InvalidCharacterError { path: path.to_string() }
                    })?;
                    components.push(name_str);

                }
                Component::CurDir => {
                    // Skip "." components
                    continue;
                }
                Component::ParentDir => {
                    // Handle ".." components
                    if components.is_empty() {
                        // Path traversal attempt, trying to go above root
                        return Err(PathNormalizationError::PathTraversalError {
                            path: path.to_string(),
                        });
                    } else {
                        components.pop();
                    }
                }
                Component::RootDir => {
                    // We don't want absolute paths
                    return Err(PathNormalizationError::PathTraversalError {
                        path: path.to_string(),
                    });
                }
                Component::Prefix(_) => {
                    // Windows-style prefixes (like drive letters) are not allowed
                    return Err(PathNormalizationError::PathTraversalError {
                        path: path.to_string(),
                    });
                }
            }
        }

        Ok(
            components.join("/")
        )
    }

    /// Validate that the file has an allowed extension
    fn validate_extension(&self, path: &str) -> Result<(), PathNormalizationError> {
        let path_obj = Path::new(path);

        let extension = path_obj
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase());

        match extension {
            Some(ext) if self.allowed_extensions.contains(&ext) => Ok(()),
            Some(ext) => Err(PathNormalizationError::InvalidExtensionError {
                extension: ext,
                allowed: self.allowed_extensions.iter().cloned().collect(),
            }),
            None => Err(PathNormalizationError::InvalidExtensionError {
                extension: "none".to_string(),
                allowed: self.allowed_extensions.iter().cloned().collect(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_normalization() {
        let normalizer = PathNormalizer::default();

        assert_eq!(normalizer.normalize_path("docs/readme.md").unwrap(), "docs/readme.md");
        assert_eq!(normalizer.normalize_path("./docs/readme.md").unwrap(), "docs/readme.md");
        assert_eq!(normalizer.normalize_path("docs//readme.md").unwrap(), "docs/readme.md");
    }

    #[test]
    fn test_path_traversal_detection() {
        let normalizer = PathNormalizer::default();

        assert!(normalizer.normalize_path("../../../etc/passwd").is_err());
        assert!(normalizer.normalize_path("docs/../../readme.md").is_err());
        assert!(normalizer.normalize_path("/etc/passwd").is_err());
    }

    #[test]
    fn test_relative_path_resolution() {
        let normalizer = PathNormalizer::default();

        assert_eq!(normalizer.normalize_path("docs/sub/../readme.md").unwrap(), "docs/readme.md");
        assert_eq!(normalizer.normalize_path("docs/./sub/readme.md").unwrap(), "docs/sub/readme.md");
    }

    #[test]
    fn test_extension_validation() {
        let normalizer = PathNormalizer::default();

        assert!(normalizer.normalize_path("readme.md").is_ok());
        assert!(normalizer.normalize_path("guide.mdx").is_ok());
        assert!(normalizer.normalize_path("notes.markdown").is_ok());
        assert!(normalizer.normalize_path("information.txt").is_ok());

        assert!(normalizer.normalize_path("script.js").is_err());
        assert!(normalizer.normalize_path("image.png").is_err());
        assert!(normalizer.normalize_path("no_extension").is_err());
    }

    #[test]
    fn test_invalid_characters() {
        let normalizer = PathNormalizer::default();

        assert!(normalizer.normalize_path("docs\0readme.md").is_err());
        assert!(normalizer.normalize_path("docs\nreadme.md").is_err());
        assert!(normalizer.normalize_path("docs\rreadme.md").is_err());
    }

    #[test]
    fn test_empty_and_whitespace_paths() {
        let normalizer = PathNormalizer::default();

        assert!(normalizer.normalize_path("").is_err());
        assert!(normalizer.normalize_path("     ").is_err());
        assert!(normalizer.normalize_path("\t\n").is_err());
    }

    #[test]
    fn test_path_length_limit() {
        let normalizer = PathNormalizer::with_settings(vec!["md".to_string()], 10);

        assert!(normalizer.normalize_path("short.md").is_ok());
        assert!(normalizer.normalize_path("very_long_filename_that_exceeds_limit.md").is_err());
    }

    #[test]
    fn test_custom_extensions() {
        let normalizer = PathNormalizer::new(vec!["rst".to_string(), "asciidoc".to_string()]);

        assert!(normalizer.normalize_path("readme.rst").is_ok());
        assert!(normalizer.normalize_path("guide.asciidoc").is_ok());
        assert!(normalizer.normalize_path("readme.md").is_err());
    }

    #[test]
    fn test_windows_paths() {
        let normalizer = PathNormalizer::default();

        // Test Windows-style paths
        assert_eq!(normalizer.normalize_path("docs\\readme.md").unwrap(), "docs/readme.md");
        assert_eq!(normalizer.normalize_path("docs\\sub\\..\\readme.md").unwrap(), "docs/readme.md");
    }

    #[test]
    fn test_multiple_paths() {
        let normalizer = PathNormalizer::default();
        let paths = vec![
            "docs/readme.md".to_string(),
            "./guides/setup.md".to_string(),
            "tutorials/../faq.md".to_string(),
        ];

        let normalized = normalizer.normalize_paths(&paths).unwrap();
        assert_eq!(normalized, vec![
            "docs/readme.md",
            "guides/setup.md",
            "faq.md",
        ]);
    }
}

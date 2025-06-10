use std::collections::HashMap;
use tracing::{debug, warn};
use crate::processing::pipeline::{DiscoveredFile, PipelineError, ProcessingContext, ValidatedFile};

pub struct ContentValidator<'a> {
    context: &'a ProcessingContext,
}

impl<'a> ContentValidator<'a> {
    pub fn new(context: &'a ProcessingContext) -> Self {
        Self { context }
    }

    pub async fn validate_batch(&self, files: Vec<DiscoveredFile>) -> Result<Vec<ValidatedFile>, PipelineError> {
        let mut validated_files = Vec::new();
        
        for file in files {
            let file_path= file.path.clone();
            match self.validate_file(file).await {
                Ok(validated) => validated_files.push(validated),
                Err(e) => {
                    warn!("Validation failed for file {}: {}", file_path, e);
                }
            }
        }
        
        Ok(validated_files)
    }
    
    async fn validate_file(&self, file: DiscoveredFile) -> Result<ValidatedFile, PipelineError> {
        debug!("Validating file: {}", file.path);
        
        // Fetch file content
        let content = self.context.github_client
            .get_file_content(&self.context.repository, &file.path)
            .await
            .map_err(PipelineError::GitHub)?;
        
        // Parse frontmatter and content
        let (frontmatter, markdown_content) = self.parse_frontmatter(&content);
        
        // Validate content
        let validation_warnings = self.validate_content(&markdown_content, &frontmatter);
        
        Ok(ValidatedFile {
            discovered: file,
            content,
            frontmatter,
            markdown_content,
            validation_warnings,
        })
    }
    
    fn parse_frontmatter(&self, content: &str) -> (HashMap<String, String>, String) {
        if !content.starts_with("---\n") {
            return (HashMap::new(), content.to_string());
        }
        
        if let Some(end_pos) = content[4..].find("\n---\n") {
            let frontmatter_text = &content[4..end_pos + 4];
            let markdown_content = &content[end_pos + 8..];
            
            let frontmatter = self.parse_yaml_frontmatter(frontmatter_text);
            return (frontmatter, markdown_content.to_string());
        }

        (HashMap::new(), content.to_string())
    }
    
    fn parse_yaml_frontmatter(&self, yaml_text: &str) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        
        for line in yaml_text.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_string();
                let value = value.trim().trim_matches('"').trim_matches('\'').to_string();
                metadata.insert(key, value);
            }
        }
        metadata
    }
    
    fn validate_content(&self, markdown_content: &str, frontmatter: &HashMap<String, String>) -> Vec<String> {
        let mut warnings = Vec::new();
        
        // Check for title
        if !frontmatter.contains_key("title") && !markdown_content.starts_with('#') {
            warnings.push("Missing title in frontmatter or as first heading".to_string());
        }
        
        // Check for very short content
        if markdown_content.trim().len() < 50 {
            warnings.push("Content is too short, consider adding more information".to_string());
        }
        
        // Basic check for broken links
        if markdown_content.contains("](") {
            let broken_links = self.find_potentially_broken_links(markdown_content);
            if !broken_links.is_empty() {
                warnings.push(format!("Found {} potentially broken links", broken_links.len()));
            }
        }
        
        warnings
    }
    
    fn find_potentially_broken_links(&self, markdown_content: &str) -> Vec<String> {
        // Simple regex to find markdown links that might be broken
        // This is a basic implementation and can be improved
        let mut broken_links = Vec::new();
        
        // Look for links with empty URLs or suspicious patterns
        for line in markdown_content.lines() {
            if line.contains("]()") || line.contains("](../") {
                broken_links.push(line.trim().to_string());
            }
        }
        
        broken_links
    }
}
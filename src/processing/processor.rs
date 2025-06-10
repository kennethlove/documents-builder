use tracing::{debug, warn};
use crate::processing::pipeline::{CodeBlock, Heading, Image, Link, PipelineError, ProcessedDocument, ProcessingMetadata, ValidatedFile};

pub struct ContentProcessor {}

impl ContentProcessor {
    pub fn new() -> Self {
        Self {}
    }
    
    pub async fn process_batch(&self, files: Vec<ValidatedFile>) -> Result<Vec<ProcessedDocument>, PipelineError> {
        let mut processed_documents = Vec::new();
        
        for file in files {
            let start_time = std::time::Instant::now();
            
            match self.process_file(file, start_time).await {
                Ok(document) => processed_documents.push(document),
                Err(e) => warn!("Failed to process file: {}", e),
            }
        }
        
        Ok(processed_documents)
    }
    
    async fn process_file(&self, file: ValidatedFile, start_time: std::time::Instant) -> Result<ProcessedDocument, PipelineError> {
        debug!("Processing file: {}", file.discovered.path);
        
        // Extract document structure
        let headings = self.extract_headings(&file.markdown_content);
        let links = self.extract_links(&file.markdown_content);
        let images = self.extract_images(&file.markdown_content);
        let code_blocks = self.extract_code_blocks(&file.markdown_content);
        
        // Calculate metrics
        let word_count = self.count_words(&file.markdown_content);
        let quality_score = self.calculate_quality_score(&file, &headings, &links);
        
        let processing_time = start_time.elapsed();
        
        Ok(ProcessedDocument {
            file_path: file.discovered.path,
            title: self.extract_title(&file.frontmatter, &headings),
            content: file.markdown_content,
            frontmatter: file.frontmatter,
            word_count,
            headings,
            links,
            images,
            code_blocks,
            last_modified: None, // TODO: Get from GitHub API
            processing_metadata: ProcessingMetadata {
                processed_at: chrono::Utc::now(),
                processing_time_ms: processing_time.as_millis() as u64,
                warnings: file.validation_warnings,
                quality_score,
            },
        })
    }
    
    fn extract_title(&self, frontmatter: &std::collections::HashMap<String, String>, headings: &[Heading]) -> String {
        // Try frontmatter first
        if let Some(title) = frontmatter.get("title") {
            return title.clone();
        }
        
        // Try first heading
        if let Some(first_heading) = headings.first() {
            return first_heading.text.clone();
        }
        
        "Untitled Document".to_string()
    }
    
    fn extract_headings(&self, markdown_content: &str) -> Vec<Heading> {
        let mut headings = Vec::new();
        
        for line in markdown_content.lines() {
            if line.starts_with('#') {
                let level = line.chars().take_while(|&c| c == '#').count() as u8;
                if level <= 6 {
                    let text = line.trim_start_matches('#').trim().to_string();
                    let anchor = self.create_anchor(&text);
                    
                    headings.push(Heading {
                        level,
                        text,
                        anchor,
                    });
                }
            }
        }
        headings
    }
    
    fn create_anchor(&self, text: &str) -> String {
        text.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    }
    
    fn extract_links(&self, markdown_content: &str) -> Vec<Link> {
        let mut links = Vec::new();
        
        // Simple regex-like parsing for markdown links: [text](url)
        let mut chars = markdown_content.chars().peekable();
        let mut current_pos = 0;
        
        while let Some(ch) = chars.next() {
            if ch == '[' {
                if let Some(link) = self.parse_link(&markdown_content[current_pos..]) {
                    links.push(link);
                }
            }
            current_pos += ch.len_utf8();
        }
        
        links
    }
    
    fn parse_link(&self, text: &str) -> Option<Link> {
        // Simple implementation - could be enhanced with proper markdown parsing
        if let Some(end_bracket) = text.find("](") {
            if let Some(end_paren) = text[end_bracket + 2..].find(')') {
                let link_text = &text[1..end_bracket];
                let url = &text[end_bracket + 2..end_bracket + 2 + end_paren];
                
                return Some(Link {
                    text: link_text.to_string(),
                    url: url.to_string(),
                    is_internal: self.is_internal_link(url),
                    is_valid: None, // TODO: Add link validation
                });
            }
        }
        None
    }
    
    fn is_internal_link(&self, url: &str) -> bool {
        !url.starts_with("http://") && !url.starts_with("https://")
    }
    
    fn extract_images(&self, markdown_content: &str) -> Vec<Image> {
        let mut images = Vec::new();
        
        // Look for markdown images: ![alt text](url)
        for line in markdown_content.lines() {
            if line.contains("![") {
                if let Some(image) = self.parse_image(line) {
                    images.push(image);
                }
            }
        }
        
        images
    }
    
    fn parse_image(&self, text: &str) -> Option<Image> {
        if let Some(start) = text.find("![") {
            if let Some(end_bracket) = text[start..].find("](") {
                if let Some(end_paren) = text[start + end_bracket + 2..].find(')') {
                    let alt_text = &text[start + 2..start + end_bracket];
                    let url = &text[start + end_bracket + 2..start + end_bracket + 2 + end_paren];
                    
                    return Some(Image {
                        alt_text: alt_text.to_string(),
                        url: url.to_string(),
                        is_internal: self.is_internal_link(url),
                    });
                }
            }
        }
        None
    }
    
    fn extract_code_blocks(&self, markdown_content: &str) -> Vec<CodeBlock> {
        let mut code_blocks = Vec::new();
        let mut in_code_block = false;
        let mut current_block_lines = Vec::new();
        let mut current_language = None;
        
        for line in markdown_content.lines() {
            if line.starts_with("```") {
                if in_code_block {
                    // End of code block
                    let content = current_block_lines.join("\n");
                    code_blocks.push(CodeBlock {
                        language: current_language.clone(),
                        content,
                        line_count: current_block_lines.len(),
                    });
                    current_block_lines.clear();
                    current_language = None;
                    in_code_block = false;
                } else {
                    // Start of code block
                    current_language = if line.len() > 3 {
                        Some(line[3..].trim().to_string())
                    } else {
                        None
                    };
                    in_code_block = true;
                }
            } else if in_code_block {
                // Inside a code block
                current_block_lines.push(line.to_string());
            }
        }
        
        code_blocks
    }
    
    fn count_words(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
    
    fn calculate_quality_score(&self, file: &ValidatedFile, headings: &[Heading], links: &[Link]) -> f32 {
        let mut score = 1.0;
        
        // Penalize for warnings
        score -= file.validation_warnings.len() as f32 * 0.1;
        
        // Bonus for good structure
        if !headings.is_empty() {
            score += 0.1;
        }
        
        // Bonus for internal links
        let internal_links = links.iter().filter(|l| l.is_internal).count();
        if internal_links > 0 {
            score += (internal_links as f32 * 0.05).min(0.2);
        }
        
        // Ensure score is within bounds
        score.max(0.0).min(1.0)
    }
}
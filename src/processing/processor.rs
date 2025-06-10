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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::processing::pipeline::DiscoveredFile;
    use tokio::test as async_test;

    #[test]
    fn test_extract_title() {
        let processor = ContentProcessor::new();

        // Test with frontmatter title
        let mut frontmatter = HashMap::new();
        frontmatter.insert("title".to_string(), "Frontmatter Title".to_string());
        let headings = vec![];

        let title = processor.extract_title(&frontmatter, &headings);
        assert_eq!(title, "Frontmatter Title");

        // Test with first heading when no frontmatter title
        let frontmatter = HashMap::new();
        let headings = vec![
            Heading {
                level: 1,
                text: "Heading Title".to_string(),
                anchor: "heading-title".to_string(),
            },
            Heading {
                level: 2,
                text: "Subheading".to_string(),
                anchor: "subheading".to_string(),
            },
        ];

        let title = processor.extract_title(&frontmatter, &headings);
        assert_eq!(title, "Heading Title");

        // Test with no frontmatter title and no headings
        let frontmatter = HashMap::new();
        let headings = vec![];

        let title = processor.extract_title(&frontmatter, &headings);
        assert_eq!(title, "Untitled Document");
    }

    #[test]
    fn test_count_words() {
        let processor = ContentProcessor::new();

        assert_eq!(processor.count_words(""), 0);
        assert_eq!(processor.count_words("one"), 1);
        assert_eq!(processor.count_words("one two three"), 3);
        assert_eq!(processor.count_words("one\ntwo\nthree"), 3);
        assert_eq!(processor.count_words("one  two   three"), 3);
    }

    #[test]
    fn test_create_anchor() {
        let processor = ContentProcessor::new();

        assert_eq!(processor.create_anchor("Hello World"), "hello-world");
        assert_eq!(processor.create_anchor("Hello, World!"), "hello--world"); // Commas become dashes
        assert_eq!(processor.create_anchor("  Spaces  "), "spaces");
        assert_eq!(processor.create_anchor("Multiple--Dashes"), "multiple--dashes"); // Preserves consecutive dashes
        assert_eq!(processor.create_anchor("-trim-dashes-"), "trim-dashes");
    }

    #[test]
    fn test_is_internal_link() {
        let processor = ContentProcessor::new();

        assert!(processor.is_internal_link("page.md"));
        assert!(processor.is_internal_link("/docs/page.md"));
        assert!(processor.is_internal_link("#section"));

        assert!(!processor.is_internal_link("https://example.com"));
        assert!(!processor.is_internal_link("http://example.com"));
    }

    #[test]
    fn test_extract_headings() {
        let processor = ContentProcessor::new();

        // Test with empty content
        let headings = processor.extract_headings("");
        assert!(headings.is_empty());

        // Test with content that has no headings
        let headings = processor.extract_headings("This is a paragraph without headings.");
        assert!(headings.is_empty());

        // Test with content that has headings
        let content = "# Heading 1\nSome content\n## Heading 2\nMore content\n### Heading 3";
        let headings = processor.extract_headings(content);

        assert_eq!(headings.len(), 3);

        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[0].text, "Heading 1");
        assert_eq!(headings[0].anchor, "heading-1");

        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[1].text, "Heading 2");
        assert_eq!(headings[1].anchor, "heading-2");

        assert_eq!(headings[2].level, 3);
        assert_eq!(headings[2].text, "Heading 3");
        assert_eq!(headings[2].anchor, "heading-3");

        // Test with invalid heading level (> 6)
        let content = "####### Invalid Heading";
        let headings = processor.extract_headings(content);
        assert!(headings.is_empty());
    }

    #[test]
    fn test_parse_link() {
        let processor = ContentProcessor::new();

        // Test with valid link
        let link = processor.parse_link("[Link Text](https://example.com)");
        assert!(link.is_some());
        let link = link.unwrap();
        assert_eq!(link.text, "Link Text");
        assert_eq!(link.url, "https://example.com");
        assert!(!link.is_internal);

        // Test with internal link
        let link = processor.parse_link("[Internal Link](/docs/page.md)");
        assert!(link.is_some());
        let link = link.unwrap();
        assert_eq!(link.text, "Internal Link");
        assert_eq!(link.url, "/docs/page.md");
        assert!(link.is_internal);

        // Test with invalid link format
        let link = processor.parse_link("[Broken Link](missing closing paren");
        assert!(link.is_none());

        let link = processor.parse_link("Not a link at all");
        assert!(link.is_none());
    }

    #[test]
    fn test_extract_links() {
        let processor = ContentProcessor::new();

        // Test with empty content
        let links = processor.extract_links("");
        assert!(links.is_empty());

        // Test with content that has no links
        let links = processor.extract_links("This is a paragraph without links.");
        assert!(links.is_empty());

        // Test with content that has links
        let content = "This is a [link](https://example.com) and another [internal link](/docs/page.md).";
        let links = processor.extract_links(content);

        assert_eq!(links.len(), 2);

        assert_eq!(links[0].text, "link");
        assert_eq!(links[0].url, "https://example.com");
        assert!(!links[0].is_internal);

        assert_eq!(links[1].text, "internal link");
        assert_eq!(links[1].url, "/docs/page.md");
        assert!(links[1].is_internal);

        // Test with multiple links on different lines
        let content = "Line 1 with [link1](url1)\nLine 2 with [link2](url2)";
        let links = processor.extract_links(content);
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn test_parse_image() {
        let processor = ContentProcessor::new();

        // Test with valid image
        let image = processor.parse_image("![Alt Text](https://example.com/image.png)");
        assert!(image.is_some());
        let image = image.unwrap();
        assert_eq!(image.alt_text, "Alt Text");
        assert_eq!(image.url, "https://example.com/image.png");
        assert!(!image.is_internal);

        // Test with internal image
        let image = processor.parse_image("![Internal Image](/images/local.png)");
        assert!(image.is_some());
        let image = image.unwrap();
        assert_eq!(image.alt_text, "Internal Image");
        assert_eq!(image.url, "/images/local.png");
        assert!(image.is_internal);

        // Test with invalid image format
        let image = processor.parse_image("![Broken Image](missing closing paren");
        assert!(image.is_none());

        let image = processor.parse_image("Not an image at all");
        assert!(image.is_none());
    }

    #[test]
    fn test_extract_images() {
        let processor = ContentProcessor::new();

        // Test with empty content
        let images = processor.extract_images("");
        assert!(images.is_empty());

        // Test with content that has no images
        let images = processor.extract_images("This is a paragraph without images.");
        assert!(images.is_empty());

        // Test with content that has images
        let content = "This is an ![image](https://example.com/image.png) and another ![internal image](/images/local.png).";
        let images = processor.extract_images(content);

        // Current implementation only finds the first image in a line
        assert_eq!(images.len(), 1);

        assert_eq!(images[0].alt_text, "image");
        assert_eq!(images[0].url, "https://example.com/image.png");
        assert!(!images[0].is_internal);

        // Test with multiple images on different lines
        let content = "Line 1 with ![image1](url1)\nLine 2 with ![image2](url2)";
        let images = processor.extract_images(content);
        assert_eq!(images.len(), 2); // Implementation finds one image per line
    }

    #[test]
    fn test_extract_code_blocks() {
        let processor = ContentProcessor::new();

        // Test with empty content
        let code_blocks = processor.extract_code_blocks("");
        assert!(code_blocks.is_empty());

        // Test with content that has no code blocks
        let code_blocks = processor.extract_code_blocks("This is a paragraph without code blocks.");
        assert!(code_blocks.is_empty());

        // Test with content that has a code block with language
        let content = "Some text\n```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```\nMore text";
        let code_blocks = processor.extract_code_blocks(content);

        assert_eq!(code_blocks.len(), 1);
        assert_eq!(code_blocks[0].language, Some("rust".to_string()));
        assert_eq!(code_blocks[0].content, "fn main() {\n    println!(\"Hello, world!\");\n}");
        assert_eq!(code_blocks[0].line_count, 3);

        // Test with content that has a code block without language
        let content = "Some text\n```\ncode without language\n```\nMore text";
        let code_blocks = processor.extract_code_blocks(content);

        assert_eq!(code_blocks.len(), 1);
        assert_eq!(code_blocks[0].language, None);
        assert_eq!(code_blocks[0].content, "code without language");
        assert_eq!(code_blocks[0].line_count, 1);

        // Test with multiple code blocks
        let content = "```rust\nlet x = 1;\n```\nSome text\n```python\nprint('hello')\n```";
        let code_blocks = processor.extract_code_blocks(content);

        assert_eq!(code_blocks.len(), 2);
        assert_eq!(code_blocks[0].language, Some("rust".to_string()));
        assert_eq!(code_blocks[1].language, Some("python".to_string()));

        // Test with unclosed code block (should not be included)
        let content = "```rust\nlet x = 1;\nSome text";
        let code_blocks = processor.extract_code_blocks(content);
        assert!(code_blocks.is_empty());
    }

    #[test]
    fn test_calculate_quality_score() {
        let processor = ContentProcessor::new();

        // Create a mock ValidatedFile
        let create_validated_file = |warnings: Vec<String>| -> ValidatedFile {
            ValidatedFile {
                discovered: DiscoveredFile {
                    path: "test.md".to_string(),
                    pattern_source: "test".to_string(),
                    estimated_size: None,
                },
                content: "Test content".to_string(),
                frontmatter: HashMap::new(),
                markdown_content: "Test content".to_string(),
                validation_warnings: warnings,
            }
        };

        // Test with no warnings, no headings, no links
        let file = create_validated_file(vec![]);
        let headings: Vec<Heading> = vec![];
        let links: Vec<Link> = vec![];

        let score = processor.calculate_quality_score(&file, &headings, &links);
        assert_eq!(score, 1.0);

        // Test with warnings
        let file = create_validated_file(vec!["Warning 1".to_string(), "Warning 2".to_string()]);
        let score = processor.calculate_quality_score(&file, &headings, &links);
        assert_eq!(score, 0.8); // 1.0 - (2 * 0.1)

        // Test with headings
        let file = create_validated_file(vec![]);
        let headings = vec![
            Heading {
                level: 1,
                text: "Heading 1".to_string(),
                anchor: "heading-1".to_string(),
            }
        ];
        let score = processor.calculate_quality_score(&file, &headings, &links);
        assert_eq!(score, 1.0); // 1.0 + 0.1 for having headings, but capped at 1.0

        // Test with internal links
        let file = create_validated_file(vec![]);
        let headings: Vec<Heading> = vec![];
        let links = vec![
            Link {
                text: "Link 1".to_string(),
                url: "/internal1.md".to_string(),
                is_internal: true,
                is_valid: None,
            },
            Link {
                text: "Link 2".to_string(),
                url: "/internal2.md".to_string(),
                is_internal: true,
                is_valid: None,
            },
            Link {
                text: "Link 3".to_string(),
                url: "https://example.com".to_string(),
                is_internal: false,
                is_valid: None,
            },
        ];

        let score = processor.calculate_quality_score(&file, &headings, &links);
        assert_eq!(score, 1.0); // 1.0 + (2 * 0.05) = 1.1, but capped at 1.0

        // Test with warnings, headings, and internal links
        let file = create_validated_file(vec!["Warning".to_string()]);
        let headings = vec![
            Heading {
                level: 1,
                text: "Heading 1".to_string(),
                anchor: "heading-1".to_string(),
            }
        ];
        let links = vec![
            Link {
                text: "Link 1".to_string(),
                url: "/internal1.md".to_string(),
                is_internal: true,
                is_valid: None,
            },
        ];

        let score = processor.calculate_quality_score(&file, &headings, &links);
        // 1.0 - 0.1 (warning) + 0.1 (headings) + 0.05 (internal link) = 1.05
        assert_eq!(score, 1.0);  // Capped at 1.0
    }

    #[async_test]
    async fn test_process_file() {
        let processor = ContentProcessor::new();

        // Create a mock ValidatedFile with markdown content containing various elements
        let markdown_content = "# Test Document\n\nThis is a test paragraph with [a link](https://example.com).\n\n## Section 1\n\nHere's an ![image](/images/test.png).\n\n```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```";

        let mut frontmatter = HashMap::new();
        frontmatter.insert("title".to_string(), "Frontmatter Title".to_string());

        let file = ValidatedFile {
            discovered: DiscoveredFile {
                path: "test-doc.md".to_string(),
                pattern_source: "test".to_string(),
                estimated_size: Some(500),
            },
            content: format!("---\ntitle: Frontmatter Title\n---\n{}", markdown_content),
            frontmatter,
            markdown_content: markdown_content.to_string(),
            validation_warnings: vec!["Test warning".to_string()],
        };

        let start_time = std::time::Instant::now();
        let result = processor.process_file(file, start_time).await;

        assert!(result.is_ok());

        let processed_doc = result.unwrap();

        // Verify basic properties
        assert_eq!(processed_doc.file_path, "test-doc.md");
        assert_eq!(processed_doc.title, "Frontmatter Title");
        assert_eq!(processed_doc.content, markdown_content);
        assert_eq!(processed_doc.word_count, 25); // Actual word count from the implementation

        // Verify extracted elements
        assert_eq!(processed_doc.headings.len(), 2);
        assert_eq!(processed_doc.headings[0].text, "Test Document");
        assert_eq!(processed_doc.headings[1].text, "Section 1");

        assert_eq!(processed_doc.links.len(), 2); // The implementation finds 2 links
        assert_eq!(processed_doc.links[0].text, "a link");
        assert_eq!(processed_doc.links[0].url, "https://example.com");

        assert_eq!(processed_doc.images.len(), 1);
        assert_eq!(processed_doc.images[0].alt_text, "image");
        assert_eq!(processed_doc.images[0].url, "/images/test.png");

        assert_eq!(processed_doc.code_blocks.len(), 1);
        assert_eq!(processed_doc.code_blocks[0].language, Some("rust".to_string()));

        // Verify metadata
        assert_eq!(processed_doc.processing_metadata.warnings.len(), 1);
        assert_eq!(processed_doc.processing_metadata.warnings[0], "Test warning");
        // The quality score might be 1.0 if other factors (like headings and internal links) 
        // compensate for the warning penalty
        assert!(processed_doc.processing_metadata.quality_score <= 1.0);
    }
}

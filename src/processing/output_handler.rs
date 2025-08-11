use crate::processing::{ProcessingResult, DocumentFragment};
use crate::OutputFormat;
use crate::web::AppError;
use std::path::PathBuf;
use tracing;

/// Handles output generation for processed repositories
pub struct OutputHandler {
    output_dir: PathBuf,
    format: OutputFormat,
}

impl OutputHandler {
    /// Create a new OutputHandler
    pub fn new(output_dir: PathBuf, format: OutputFormat) -> Self {
        Self {
            output_dir,
            format,
        }
    }

    /// Save processing results to the specified output format
    pub fn save_results(&self, result: &ProcessingResult) -> Result<(), AppError> {
        // Create output directory
        if let Err(e) = std::fs::create_dir_all(&self.output_dir) {
            tracing::error!(
                "Failed to create output directory {}: {}",
                self.output_dir.display(),
                e
            );
            eprintln!(
                "Failed to create output directory {}: {}",
                self.output_dir.display(),
                e
            );
            return Err(AppError::IoError(e));
        }

        match self.format {
            OutputFormat::Files => self.save_as_files(&result.fragments)?,
            OutputFormat::Json => self.save_as_json(result)?,
            OutputFormat::Html => {
                tracing::info!("HTML output format is not yet implemented.");
            }
        }

        // Save processing summary
        self.save_summary(result)?;

        tracing::info!(
            "Repository {} processed successfully. Output saved in: {}",
            result.repository,
            self.output_dir.display()
        );

        Ok(())
    }

    /// Save fragments as individual files
    fn save_as_files(&self, fragments: &[DocumentFragment]) -> Result<(), AppError> {
        for fragment in fragments {
            let filename = match fragment.fragment_type {
                crate::processing::FragmentType::Content => {
                    format!(
                        "{}-{:?}.md",
                        fragment.file_path.replace('/', "_"),
                        fragment.fragment_type
                    )
                },
                crate::processing::FragmentType::Navigation => {
                    format!(
                        "{}-{:?}.json",
                        fragment.file_path.replace('/', "_"),
                        fragment.fragment_type
                    )
                }
            };
            let fragment_file = self.output_dir.join(filename);
            std::fs::write(&fragment_file, &fragment.content)?;
        }

        tracing::info!(
            "Processing completed. Files saved in: {}",
            self.output_dir.display()
        );

        Ok(())
    }

    /// Save all results as a single JSON file
    fn save_as_json(&self, result: &ProcessingResult) -> Result<(), AppError> {
        let output_file = self.output_dir.join("fragments.json");
        let json_content = serde_json::to_string_pretty(result)?;
        std::fs::write(&output_file, json_content)?;

        tracing::info!(
            "Processing completed. JSON output saved in: {}",
            output_file.display()
        );

        Ok(())
    }

    /// Save processing summary
    fn save_summary(&self, result: &ProcessingResult) -> Result<(), AppError> {
        let summary_file = self.output_dir.join("processing-summary.json");
        let summary = serde_json::json!({
            "repository": result.repository,
            "processed_at": result.processed_at,
            "file_processed": result.file_processed,
            "fragments_generated": result.fragments_generated,
            "processing_time_ms": result.processing_time_ms,
        });
        std::fs::write(&summary_file, serde_json::to_string_pretty(&summary)?)?;

        Ok(())
    }
}

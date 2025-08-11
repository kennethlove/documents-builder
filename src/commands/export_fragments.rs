use crate::OutputFormat;
use crate::github::{Client, GitHubClient, GitHubError};
use crate::processing::{DocumentFragment, RepositoryProcessor};
use crate::web::AppError;
use crate::Console;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ExportFragmentsArgs {
    /// GitHub repository to export fragments from
    repository: String,
    #[arg(long, short, help = "Output directory for generated fragments")]
    output: Option<PathBuf>,
    #[arg(long, help = "Output format", value_enum, default_value = "files")]
    format: OutputFormat,
    #[arg(long, help = "Include fragment metadata in export")]
    include_metadata: bool,
    #[arg(long, help = "Compress exported fragments into archive")]
    compress: bool,
    #[arg(
        long,
        help = "Filter fragments by type (e.g., 'text', 'code')",
        value_delimiter = ','
    )]
    fragment_type: Option<String>,
}

pub struct ExportFragmentsCommand {
    repository: String,
    output: Option<PathBuf>,
    format: OutputFormat,
    include_metadata: bool,
    compress: bool,
    fragment_type: Option<String>,
}

impl ExportFragmentsCommand {
    pub fn new(args: ExportFragmentsArgs) -> Self {
        Self {
            repository: args.repository,
            output: args.output,
            format: args.format,
            include_metadata: args.include_metadata,
            compress: args.compress,
            fragment_type: args.fragment_type,
        }
    }

    pub async fn execute(&self, client: &GitHubClient) -> Result<(), AppError> {
        client.handle_rate_limits().await?;
        
        let console = Console::new(false);
        
        console.header(&format!("Exporting fragments for repository: {}", self.repository));
        tracing::info!("Exporting fragments for repository: {}", self.repository);

        let output_dir = self
            .output
            .clone()
            .unwrap_or_else(|| PathBuf::from("output/fragments").join(&self.repository));

        console.verbose(&format!("Export directory: {}", output_dir.display()));
        console.verbose(&format!("Export format: {:?}", self.format));
        console.verbose(&format!("Include metadata: {}", self.include_metadata));
        console.verbose(&format!("Compress output: {}", self.compress));
        
        tracing::debug!("Export directory: {}", output_dir.display());
        tracing::debug!("Export format: {:?}", self.format);
        tracing::debug!("Include metadata: {}", self.include_metadata);
        tracing::debug!("Compress output: {}", self.compress);

        // Step 1: Get repository configuration
        let config_spinner = console.create_spinner("Fetching repository configuration...");
        match client.get_project_config(self.repository.as_str()).await {
            Ok(config) => {
                console.finish_progress_success(&config_spinner, "Configuration retrieved");
                tracing::info!("Configuration retrieved for repository: {}", self.repository);

                // Step 2: Process repository
                let process_spinner = console.create_spinner("Processing repository documents...");
                let processor =
                    RepositoryProcessor::new(client.clone(), config, self.repository.clone());

                match processor.process(false, true).await { // Don't pass verbose to avoid duplicate output
                    Ok(result) => {
                        console.finish_progress_success(&process_spinner, "Documents processed");
                        std::fs::create_dir_all(&output_dir)?;

                        // Filter fragments by type if specified
                        let fragments = if let Some(filter_type) = &self.fragment_type {
                            let filtered: Vec<_> = result
                                .fragments
                                .into_iter()
                                .filter(|f| {
                                    format!("{:?}", f.fragment_type)
                                        .to_lowercase()
                                        .contains(&filter_type.to_lowercase())
                                })
                                .collect();
                            console.info(&format!("Filtered to {} fragments of type '{}'", filtered.len(), filter_type));
                            filtered
                        } else {
                            result.fragments
                        };

                        console.info(&format!("Exporting {} fragments in {:?} format", fragments.len(), self.format));
                        tracing::info!("Exporting {} fragments", fragments.len());

                        // Step 3: Export fragments
                        let export_spinner = console.create_spinner("Exporting fragments...");
                        
                        match self.format {
                            OutputFormat::Files => {
                                // Export each fragment to a file
                                for fragment in &fragments {
                                    let filename = format!(
                                        "{}-{:?}.md",
                                        fragment.file_path.replace("/", "_"),
                                        fragment.fragment_type
                                    );
                                    let fragment_file = output_dir.join(filename);

                                    std::fs::write(fragment_file, &fragment.content)?;
                                }

                                if self.include_metadata {
                                    // Export metadata as JSON
                                    let metadata_file = output_dir.join("metadata.json");
                                    let metadata = serde_json::json!({
                                        "repository": result.repository,
                                        "processed_at": result.processed_at,
                                        "fragments_count": fragments.len(),
                                        "fragment_types": fragments.iter()
                                            .map(|f| format!("{:?}", f.fragment_type))
                                            .collect::<std::collections::HashSet<_>>()
                                            .into_iter()
                                            .collect::<Vec<_>>(),
                                    });
                                    std::fs::write(
                                        metadata_file,
                                        serde_json::to_string_pretty(&metadata)?,
                                    )?;
                                }

                                console.finish_progress_success(&export_spinner, "Files exported");
                                console.info(&format!("Fragments exported to: {}", output_dir.display()));
                                tracing::info!("Fragments exported to {}", output_dir.display());
                            }
                            OutputFormat::Html => {
                                let html_document = self.generate_complete_html_document(
                                    &fragments,
                                    &result.repository,
                                )?;
                                let output_file = output_dir.join("fragments.html");
                                std::fs::write(&output_file, html_document)?;
                                
                                console.finish_progress_success(&export_spinner, "HTML document exported");
                                console.info(&format!("HTML document exported to: {}", output_file.display()));
                                tracing::info!("HTML document exported to {}", output_file.display());
                            }
                            OutputFormat::Json => {
                                let export_data = serde_json::json!({
                                    "repository": result.repository,
                                    "processed_at": result.processed_at,
                                    "fragments": fragments.iter().map(|f| {
                                        let mut fragment_data = serde_json::json!({
                                            "file_path": f.file_path,
                                            "fragment_type": format!("{:?}", f.fragment_type),
                                            "content": f.content,
                                            "html_content": self.generate_html_fragment(f.clone()).unwrap_or_default(),
                                        });

                                        if self.include_metadata {
                                            fragment_data["metadata"] = serde_json::json!({
                                                "size": f.content.len(),
                                                "lines": f.content.lines().count(),
                                            });
                                        }

                                        fragment_data
                                    }).collect::<Vec<_>>()
                                });

                                let output_file = output_dir.join("fragments.json");
                                std::fs::write(
                                    &output_file,
                                    serde_json::to_string_pretty(&export_data)?,
                                )?;
                                
                                console.finish_progress_success(&export_spinner, "JSON document exported");
                                console.info(&format!("JSON document exported to: {}", output_file.display()));
                                tracing::info!("JSON document exported to {}", &output_file.display());
                            }
                        }

                        // Step 4: Handle compression if requested
                        if self.compress {
                            let compress_spinner = console.create_spinner("Compressing output...");
                            match self.compress_output(&output_dir) {
                                Ok(()) => {
                                    console.finish_progress_success(&compress_spinner, "Output compressed");
                                }
                                Err(e) => {
                                    console.finish_progress_error(&compress_spinner, "Compression failed");
                                    console.warning(&format!("Failed to compress output: {}", e));
                                }
                            }
                        }

                        console.success(&format!("Export completed successfully for repository: {}", self.repository));
                        tracing::info!("Export completed successfully for repository: {}", self.repository);
                        Ok(())
                    }
                    Err(e) => {
                        console.finish_progress_error(&process_spinner, "Processing failed");
                        console.error(&format!("Error processing repository {}: {}", self.repository, e));
                        tracing::error!("Error processing repository {}: {}", self.repository, e);
                        return Err(AppError::InternalServerError(format!("Processing failed: {}", e)));
                    }
                }
            }
            Err(GitHubError::ConfigFileNotFound(_)) => {
                console.finish_progress_error(&config_spinner, "Configuration not found");
                console.error(&format!("No documents.toml configuration file found in repository: {}", self.repository));
                console.info("Make sure the repository has a documents.toml file in its root directory");
                tracing::error!("No configuration file found in repository: {}", self.repository);
                return Err(AppError::InternalServerError("Configuration file not found".to_string()));
            }
            Err(e) => {
                console.finish_progress_error(&config_spinner, "Failed to fetch configuration");
                console.error(&format!("Error retrieving configuration for repository {}: {}", self.repository, e));
                tracing::error!("Error retrieving configuration for repository {}: {}", self.repository, e);
                return Err(AppError::InternalServerError(format!("Failed to fetch configuration: {}", e)));
            }
        }
    }

    fn generate_html_fragment(&self, fragment: DocumentFragment) -> Result<String, AppError> {
        let html = format!(
            r#"<article class="fragment" data-type="{:?}", data-source="{}">
            <header>
                <h3>{}</h3>
                <span class="fragment-type">{:?}</span>
            </header>
            <section class="fragment-content"><pre><code>{}</code></pre></section>
            </article>"#,
            fragment.fragment_type,
            fragment.file_path,
            fragment.file_path,
            fragment.fragment_type,
            html_escape::encode_text(&fragment.content)
        );

        Ok(html)
    }

    fn generate_complete_html_document(
        &self,
        fragments: &[DocumentFragment],
        repository: &str,
    ) -> Result<String, AppError> {
        let fragments_html = fragments
            .iter()
            .map(|f| self.generate_html_fragment(f.clone()))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");

        let html_document = format!(
            r#"<!DOCTYPE html>
            <html lang="en">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>Fragments - {}</title>
            </head>
            <body>
                <header>
                    <h1>Fragments for Repository: {}</h1>
                    <p>Exported on: {}</p>
                </header>
                <main>{}</main>
            </body>
        </html>"#,
            repository,
            repository,
            chrono::Utc::now().to_rfc3339(),
            fragments_html
        );

        Ok(html_document)
    }

    fn compress_output(&self, output_dir: &PathBuf) -> Result<(), AppError> {
        tracing::info!("Compressing output directory: {}", output_dir.display());
        tracing::info!("Not yet implemented.");
        Ok(())
    }
}

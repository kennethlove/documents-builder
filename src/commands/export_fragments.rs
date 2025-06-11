use std::path::PathBuf;
use clap::Args;
use tracing::{debug, error, info};
use crate::github::{Client, GitHubClient, GitHubError};
use crate::OutputFormat;
use crate::processing::{DocumentFragment, RepositoryProcessor};
use crate::web::AppError;

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
    #[arg(long, help = "Filter fragments by type (e.g., 'text', 'code')", value_delimiter = ',')]
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
        info!("Exporting fragments for repository: {}", self.repository);

        let output_dir = self.output.clone().unwrap_or_else(|| {
            PathBuf::from("output/fragments").join(&self.repository)
        });

        debug!("Export directory: {}", output_dir.display());
        debug!("Export format: {:?}", self.format);
        debug!("Include metadata: {}", self.include_metadata);
        debug!("Compress output: {}", self.compress);

        // Get repository configuration
        match client.get_project_config(self.repository.as_str()).await {
            Ok(config) => {
                info!("Configuration retrieved for repository: {}", self.repository);

                let processor = RepositoryProcessor::new(client.clone(), config, self.repository.clone());

                match processor.process(true).await {
                    Ok(result) => {
                        std::fs::create_dir_all(&output_dir)?;

                        // Filter fragments by type if specified
                        let fragments = if let Some(filter_type) = &self.fragment_type {
                            result.fragments.into_iter()
                                .filter(|f| format!("{:?}", f.fragment_type).to_lowercase().contains(&filter_type.to_lowercase()))
                                .collect()
                        } else {
                            result.fragments
                        };

                        info!("Exporting {} fragments", fragments.len());

                        match self.format {
                            OutputFormat::Files => {
                                // Export each fragment to a file
                                for fragment in &fragments {
                                    let filename = format!("{}-{:?}.md", fragment.file_path.replace("/", "_"), fragment.fragment_type);
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
                                    std::fs::write(metadata_file, serde_json::to_string_pretty(&metadata)?)?;
                                }

                                info!("Fragments exported to {}", output_dir.display());
                            }
                            OutputFormat::Html => {
                                let html_document = self.generate_complete_html_document(&fragments, &result.repository)?;
                                let output_file = output_dir.join("fragments.html");
                                std::fs::write(&output_file, html_document)?;
                                info!("HTML document exported to {}", output_file.display());
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
                                std::fs::write(&output_file, serde_json::to_string_pretty(&export_data)?)?;
                                info!("JSON document exported to {}", &output_file.display());
                            }
                        }

                        // Handle compression if requested
                        if self.compress {
                            self.compress_output(&output_dir)?;
                        }

                        info!("Export completed successfully for repository: {}", self.repository);
                        Ok(())
                    }
                    Err(e) => {
                        tracing::error!("Error processing repository {}: {}", self.repository, e);
                        eprintln!("Error processing repository {}: {}", self.repository, e);
                        std::process::exit(1);
                    }
                }
            }
            Err(GitHubError::ConfigFileNotFound(_)) => {
                error!("No configuration file found in repository: {}", self.repository);
                eprintln!("No configuration file found in repository: {}", self.repository);
                std::process::exit(1);
            }
            Err(e) => {
                error!("Error retrieving configuration for repository {}: {}", self.repository, e);
                eprintln!("Error retrieving configuration for repository {}: {}", self.repository, e);
                std::process::exit(1);
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

    fn generate_complete_html_document(&self, fragments: &[DocumentFragment], repository: &str) -> Result<String, AppError> {
        let fragments_html = fragments.iter()
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
        info!("Compressing output directory: {}", output_dir.display());
        info!("Not yet implemented.");
        Ok(())
    }
}

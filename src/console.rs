use console::{style, Emoji};
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::time::Duration;

/// Emojis for consistent visual feedback
pub static CHECKMARK: Emoji<'_, '_> = Emoji("✅ ", "✓ ");
pub static CROSS: Emoji<'_, '_> = Emoji("❌ ", "✗ ");
pub static ROCKET: Emoji<'_, '_> = Emoji("🚀 ", ">> ");
pub static MAGNIFYING_GLASS: Emoji<'_, '_> = Emoji("🔍 ", ">> ");
pub static GEAR: Emoji<'_, '_> = Emoji("⚙️ ", ">> ");
pub static PACKAGE: Emoji<'_, '_> = Emoji("📦 ", ">> ");

/// Console output utility for consistent formatting across all commands
pub struct Console {
    verbose: bool,
    multi_progress: MultiProgress,
}

impl Console {
    /// Create a new Console instance
    pub fn new(verbose: bool) -> Self {
        Self {
            verbose,
            multi_progress: MultiProgress::new(),
        }
    }

    /// Print a header message with consistent formatting
    pub fn header(&self, message: &str) {
        println!("{} {}", ROCKET, style(message).bold().cyan());
    }

    /// Print a success message
    pub fn success(&self, message: &str) {
        println!("{} {}", CHECKMARK, style(message).green());
    }

    /// Print an error message
    pub fn error(&self, message: &str) {
        eprintln!("{} {}", CROSS, style(message).red());
    }

    /// Print a warning message
    pub fn warning(&self, message: &str) {
        println!("⚠️  {}", style(message).yellow());
    }

    /// Print an info message
    pub fn info(&self, message: &str) {
        println!("{} {}", style("ℹ️").blue(), message);
    }

    /// Print a verbose message (only if verbose mode is enabled)
    pub fn verbose(&self, message: &str) {
        if self.verbose {
            println!("  {}", style(message).dim());
        }
    }

    /// Print a summary section with consistent formatting
    pub fn summary(&self, title: &str, items: &[(&str, String)]) {
        println!("\n{}", style(title).bold().underlined());
        for (label, value) in items {
            println!("  {}: {}", style(label).bold(), value);
        }
    }

    /// Create a progress bar for scanning operations
    pub fn create_scan_progress(&self, total: u64, message: &str) -> ProgressBar {
        let pb = self.multi_progress.add(ProgressBar::new(total));
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}"
            )
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  ")
        );
        pb.set_message(format!("{} {}", MAGNIFYING_GLASS, message));
        pb.enable_steady_tick(Duration::from_millis(100));
        pb
    }

    /// Create a progress bar for processing operations
    pub fn create_process_progress(&self, total: u64, message: &str) -> ProgressBar {
        let pb = self.multi_progress.add(ProgressBar::new(total));
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.green/blue}] {pos}/{len} {msg}"
            )
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  ")
        );
        pb.set_message(format!("{} {}", GEAR, message));
        pb.enable_steady_tick(Duration::from_millis(100));
        pb
    }

    /// Create a spinner for indeterminate operations
    pub fn create_spinner(&self, message: &str) -> ProgressBar {
        let pb = self.multi_progress.add(ProgressBar::new_spinner());
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        );
        pb.set_message(format!("{} {}", PACKAGE, message));
        pb.enable_steady_tick(Duration::from_millis(100));
        pb
    }

    /// Finish a progress bar with a success message
    pub fn finish_progress_success(&self, pb: &ProgressBar, message: &str) {
        // Print the success message directly
        println!("{} {}", CHECKMARK, style(message).green());
        // Clear the progress bar from display
        pb.finish_and_clear();
    }

    /// Finish a progress bar with an error message
    pub fn finish_progress_error(&self, pb: &ProgressBar, message: &str) {
        // Print the error message directly
        eprintln!("{} {}", CROSS, style(message).red());
        // Clear the progress bar from display
        pb.finish_and_clear();
    }

    /// Print repository processing status
    pub fn repo_status(&self, repo_name: &str, status: RepoStatus) {
        match status {
            RepoStatus::Processing => {
                if self.verbose {
                    println!("  {} Processing repository: {}", GEAR, style(repo_name).cyan());
                }
            }
            RepoStatus::Success => {
                if self.verbose {
                    println!("  {} Successfully processed: {}", CHECKMARK, style(repo_name).green());
                }
            }
            RepoStatus::Error(ref error) => {
                println!("  {} Failed to process {}: {}", CROSS, style(repo_name).red(), error);
            }
            RepoStatus::Skipped(ref reason) => {
                if self.verbose {
                    println!("  {} Skipped {}: {}", style("⏭️").dim(), style(repo_name).dim(), style(reason).dim());
                }
            }
        }
    }

    /// Print configuration validation status
    pub fn config_status(&self, repo_name: &str, is_valid: bool, details: Option<&str>) {
        if is_valid {
            println!("  {} Configuration valid for: {}", CHECKMARK, style(repo_name).green());
        } else {
            println!("  {} Configuration invalid for: {}", CROSS, style(repo_name).red());
            if let Some(details) = details {
                println!("    {}", style(details).dim());
            }
        }
    }

    /// Print health check status
    pub fn health_status(&self, component: &str, is_healthy: bool, details: Option<&str>) {
        if is_healthy {
            println!("  {} {}: {}", CHECKMARK, component, style("Healthy").green());
        } else {
            println!("  {} {}: {}", CROSS, component, style("Unhealthy").red());
        }
        
        if let Some(details) = details {
            println!("    {}", style(details).dim());
        }
    }
}

/// Repository processing status
#[derive(Debug, Clone)]
pub enum RepoStatus {
    Processing,
    Success,
    Error(String),
    Skipped(String),
}

impl Default for Console {
    fn default() -> Self {
        Self::new(false)
    }
}

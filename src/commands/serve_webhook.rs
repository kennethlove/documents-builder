pub struct ServeWebhookCommand;

impl ServeWebhookCommand {
    pub async fn execute(port: u16, _log_level: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Starting documentation webhook server");
        tracing::info!("Server will listen on http://0.0.0.0:{}", port);
        tracing::info!("Health check available at http://0.0.0.0:{}/health", port);
        tracing::info!("GitHub webhook endpoint at http://0.0.0.0:{}/webhooks/github", port);
        crate::web::start_server(port).await?;
        Ok(())
    }
}

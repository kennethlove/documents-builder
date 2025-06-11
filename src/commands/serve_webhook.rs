use clap::Args;

#[derive(Args, Debug)]
pub struct ServeWebhookArgs {
    #[arg(long, default_value = "3000")]
    pub port: u16,
    #[arg(long, default_value = "info")]
    pub log_level: String,
}
pub struct ServeWebhookCommand;

impl ServeWebhookCommand {
    pub async fn execute(args: ServeWebhookArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Starting documentation webhook server");
        tracing::info!("Server will listen on http://0.0.0.0:{}", args.port);
        tracing::info!("Health check available at http://0.0.0.0:{}/health", args.port);
        tracing::info!("GitHub webhook endpoint at http://0.0.0.0:{}/webhooks/github", args.port);
        crate::web::start_server(args.port).await?;
        Ok(())
    }
}

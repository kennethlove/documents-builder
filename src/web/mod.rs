use axum::body::Body;
use axum::http;
use axum::response::IntoResponse;
use axum::{
    Router,
    extract::Query,
    http::{HeaderMap, StatusCode},
    middleware,
    response::{Html, Json, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

#[derive(Serialize, Debug)]
struct HealthCheckResponse {
    status: String,
    timestamp: String,
    version: String,
}

#[derive(Serialize, Debug)]
struct ErrorResponse {
    error: String,
    message: String,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

#[derive(Deserialize, Debug)]
struct WebhookQuery {
    #[serde(default)]
    test: bool,
}

pub fn create_app() -> Router {
    Router::new()
        // Health check endpoint
        .route("/health", get(health_check))
        .route("/webhooks/github", post(github_webhook))
        .fallback(handler_404)
        .layer(
            ServiceBuilder::new()
                .layer(middleware::from_fn(error_handling_middleware))
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|request: &axum::http::Request<_>| {
                            tracing::info_span!(
                                "http_request",
                                method = %request.method(),
                                uri = %request.uri(),
                                version = ?request.version(),
                            )
                        })
                        .on_request(|_request: &axum::http::Request<_>, _span: &tracing::Span| {
                            info!("Started processing request");
                        })
                        .on_response(
                            |response: &axum::http::Response<_>,
                             latency: std::time::Duration,
                             _span: &tracing::Span| {
                                info!(
                                    status = %response.status(),
                                    latency = ?latency,
                                    "Finished processing request"
                                );
                            },
                        ),
                )
                .layer(
                    CorsLayer::new()
                        .allow_origin(tower_http::cors::Any) // In production, specify allowed origins
                        .allow_methods([
                            http::Method::GET,
                            http::Method::POST,
                            http::Method::OPTIONS,
                        ])
                        .allow_headers([
                            http::header::CONTENT_TYPE,
                            http::header::ACCEPT,
                            http::header::AUTHORIZATION,
                            // GitHub webhook headers
                            http::header::HeaderName::from_static("x-github-event"),
                            http::header::HeaderName::from_static("x-github-delivery"),
                            http::header::HeaderName::from_static("x-hub-signature-256"),
                        ])
                        .expose_headers([
                            http::header::CONTENT_TYPE,
                            http::header::CACHE_CONTROL,
                            http::header::ETAG,
                        ])
                        .max_age(Duration::from_secs(3600)), // 1 hour
                ),
        )
}

async fn error_handling_middleware(
    request: http::Request<Body>,
    next: middleware::Next,
) -> Response {
    let uri = request.uri().clone();
    let method = request.method().clone();

    // Add timeout to prevent hanging requests
    let response = match tokio::time::timeout(Duration::from_secs(30), next.run(request)).await {
        Ok(res) => res,
        Err(_) => {
            error!("Request to {} {} timed out", method, uri);
            return AppError::ServiceUnavailable("Request timed out".to_string()).into_response();
        }
    };

    // Add security headers to all requests
    let mut response = response;
    let headers = response.headers_mut();

    // Security headers
    headers.insert(
        "X-Content-Type-Options",
        http::header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "X-Frame-Options",
        http::header::HeaderValue::from_static("DENY"),
    );
    headers.insert(
        "X-XSS-Protection",
        http::header::HeaderValue::from_static("1; mode=block"),
    );
    headers.insert(
        "Referrer-Policy",
        http::header::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );

    // Cache control for API responses
    if uri.path().starts_with("/api/") || uri.path() == "/health" {
        headers.insert(
            "Cache-Control",
            http::header::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        );
    }

    // Log errors for monitoring
    match response.status() {
        status if status.is_server_error() => {
            error!("Server error for {} {}: {}", method, uri, response.status());
        }
        status if status.is_client_error() => {
            warn!("Client error for {} {}: {}", method, uri, response.status());
        }
        _ => {} // Success responses are already logged by the trace layer
    }

    response
}

async fn health_check() -> impl IntoResponse {
    debug!("Processing health check request");

    let response = HealthCheckResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    info!("Health check requested");
    axum::Json(response)
}

async fn github_webhook(
    Query(params): Query<WebhookQuery>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    debug!("Processing github webhook request");
    debug!("Headers: {:?}", headers);
    debug!("Query parameters: {:?}", params);
    debug!("Body length: {}", body.len());

    if params.test {
        info!("Received test webhook with body: {}", body);
        return Html("<h1>Test webhook received</h1>".to_string());
    }

    if let Some(event_type) = headers.get("X-GitHub-Event") {
        debug!("GitHub event type: {:?}", event_type);
    }
    if let Some(event_type) = headers.get("X-GitHub-Delivery") {
        debug!("GitHub delivery ID: {:?}", event_type);
    }

    // Here you would handle the actual GitHub webhook payload
    info!("Received GitHub webhook with body: {}", body);

    // For now, just return a success response
    Html("<h1>GitHub webhook received</h1><p>Processing not yet implemented</p>".to_string())
}

async fn handler_404() -> AppError {
    AppError::NotFound("The requested endpoint was not found".to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Bad Request: {0}")]
    BadRequest(String),
    #[error("Internal Server Error: {0}")]
    InternalServerError(String),
    #[error("Not Found: {0}")]
    NotFound(String),
    #[error("Service Unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON Serialization Error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
            AppError::InternalServerError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_server_error",
                msg,
            ),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg),
            AppError::ServiceUnavailable(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", msg)
            }
            AppError::IoError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "io_error",
                &msg.to_string(),
            ),
            AppError::SerializationError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "serialization_error",
                &msg.to_string(),
            ),
        };

        let error_response = ErrorResponse {
            error: error_type.to_string(),
            message: message.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            details: None, // Optionally include more details
        };

        error!("API ERROR: {} - {}", status, message);

        (status, Json(error_response)).into_response()
    }
}

pub async fn start_server(port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = create_app();

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        error!("Failed to bind to address {}: {}", addr, e);
        e
    })?;
    info!("Server starting on http://0.0.0.0:{}", port);
    info!("Press Ctrl+C to stop the server");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| {
            error!("Server error: {}", e);
            e
        })?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down gracefully");
        },
        _ = terminate => {
            info!("Received termination signal, shutting down gracefully");
        },
    }
}

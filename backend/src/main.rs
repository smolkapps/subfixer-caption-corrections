//! SubFixer server binary: serves the JSON API and (if present) the built
//! frontend from `./static`.

use std::sync::Arc;
use subfixer::api::{router, AppState};
use subfixer::{Store, SubFixerError};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db_path = std::env::var("SUBFIXER_DB").unwrap_or_else(|_| "subfixer.db".to_string());
    let store: AppState = Arc::new(Store::open(&db_path)?);
    tracing::info!("opened store at {db_path}");

    // Serve the built SPA from ./static if it exists; the API is always mounted.
    // Unknown non-API paths fall back to index.html so client-side routes
    // (e.g. /leaderboard) survive a refresh or deep link; unknown /api/* paths
    // stay JSON 404s instead of leaking HTML.
    let static_dir = std::env::var("SUBFIXER_STATIC").unwrap_or_else(|_| "static".to_string());
    let index_html = std::path::Path::new(&static_dir).join("index.html");
    let spa = ServeDir::new(&static_dir).fallback(ServeFile::new(index_html));
    let app = router(store)
        .route("/api/*rest", axum::routing::any(api_not_found))
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http());

    let addr = std::env::var("SUBFIXER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("subfixer listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Catch-all for unregistered `/api/*` paths: a JSON 404 rather than the SPA
/// fallback's index.html.
async fn api_not_found() -> SubFixerError {
    SubFixerError::NotFound("no such API endpoint".into())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}

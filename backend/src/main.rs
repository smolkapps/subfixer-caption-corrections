//! SubFixer server binary: serves the JSON API and (if present) the built
//! frontend from `./static`.

use std::sync::Arc;
use subfixer::api::{app, AppState};
use subfixer::Store;

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

    // Compose the API + SPA static serving + client-side-route fallback. The
    // routing rules (JSON 404 for unknown `/api/*`, trailing-slash
    // normalization, HTML fallback only for browser navigations) all live in
    // `subfixer::api::app` so they are exercised by the integration tests.
    let static_dir = std::env::var("SUBFIXER_STATIC").unwrap_or_else(|_| "static".to_string());
    let app = app(store, &static_dir);

    let addr = std::env::var("SUBFIXER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("subfixer listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}

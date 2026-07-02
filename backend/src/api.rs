//! HTTP API: Axum router and handlers wiring the store to JSON endpoints.

use crate::diff::{diff_captions, CaptionDiff};
use crate::error::SubFixerError;
use crate::store::{NewCorrection, Store};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, Method, Request, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{any, get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower::Layer as _;
use tower::ServiceExt as _; // for `oneshot`
use tower_http::cors::{Any, CorsLayer};
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

/// Shared application state.
pub type AppState = Arc<Store>;

/// Build the API router (no static file serving; mounted by [`app`]).
///
/// Every `/api/*` path is owned here: the real endpoints, plus catch-alls for
/// `/api`, `/api/`, and any other `/api/...` path that resolves to a JSON 404.
/// The catch-alls are registered *before* `.layer(cors)` so that even the 404
/// carries the CORS headers a cross-origin client needs to read it.
pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/health", get(health))
        .route("/api/corrections", post(submit))
        .route("/api/corrections", get(list_for_video))
        .route("/api/leaderboard", get(leaderboard))
        .route("/api/anonymity", post(set_anonymity))
        .route("/api/preview-diff", post(preview_diff))
        // Unknown API paths return a JSON 404 rather than falling through to the
        // SPA's index.html. `/api` and `/api/` are spelled out because the
        // wildcard below only matches a non-empty suffix.
        .route("/api", any(api_not_found))
        .route("/api/", any(api_not_found))
        .route("/api/*rest", any(api_not_found))
        .with_state(state)
        .layer(cors)
}

/// Build the full application the binary serves: the API router, the SPA static
/// files, and an `index.html` fallback for client-side routes.
///
/// Composition notes tied to specific failure modes:
/// - The SPA `index.html` fallback only fires for requests that `Accept:
///   text/html` (i.e. browser navigations). A missing hashed asset requested by
///   `<script type="module">` (which sends `Accept: */*`) therefore gets an
///   honest 404 instead of `index.html`, so deploy skew surfaces as a clear
///   404 rather than a confusing module-MIME refusal on a stale bundle.
/// - A [`NormalizePathLayer`] trims a trailing slash *before* routing, so
///   `/api/health/` resolves to the real endpoint (JSON) instead of the SPA
///   fallback (HTML). It must wrap the router from the outside to run ahead of
///   routing, so it is nested inside an outer [`Router`] via `fallback_service`.
pub fn app(state: AppState, static_dir: &str) -> Router {
    let index_html: PathBuf = Path::new(static_dir).join("index.html");

    // Fallback used by `ServeDir` when no static file matches: serve
    // `index.html` for browser navigations (`Accept: text/html`), otherwise a
    // JSON 404. This is what keeps a missing hashed asset (requested by
    // `<script type=module>` with `Accept: */*`) from being masked as a 200
    // `index.html` and surfacing later as a blank module-MIME page.
    let spa_index = tower::service_fn(move |req: Request<Body>| {
        let index_html = index_html.clone();
        async move {
            let wants_html = req.method() == Method::GET
                && req
                    .headers()
                    .get(header::ACCEPT)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|accept| accept.contains("text/html"));

            let resp: Response = if wants_html {
                ServeFile::new(index_html).oneshot(req).await.into_response()
            } else {
                (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response()
            };
            Ok::<_, std::convert::Infallible>(resp)
        }
    });
    let spa = ServeDir::new(static_dir).fallback(spa_index);

    let composed = router(state)
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http());

    let normalized = NormalizePathLayer::trim_trailing_slash().layer(composed);
    Router::new().fallback_service(normalized)
}

/// Catch-all for unregistered `/api/*` paths: a JSON 404 rather than the SPA
/// fallback's index.html.
async fn api_not_found() -> SubFixerError {
    SubFixerError::NotFound("no such API endpoint".into())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "subfixer" }))
}

async fn submit(
    State(store): State<AppState>,
    Json(input): Json<NewCorrection>,
) -> Result<Json<Value>, SubFixerError> {
    let c = store.submit_correction(input)?;
    Ok(Json(json!(c)))
}

#[derive(Deserialize)]
struct VideoQuery {
    url: String,
}

async fn list_for_video(
    State(store): State<AppState>,
    Query(q): Query<VideoQuery>,
) -> Result<Json<Value>, SubFixerError> {
    let list = store.corrections_for_video(&q.url)?;
    Ok(Json(json!({ "corrections": list })))
}

#[derive(Deserialize)]
struct LeaderboardQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    100
}

async fn leaderboard(
    State(store): State<AppState>,
    Query(q): Query<LeaderboardQuery>,
) -> Result<Json<Value>, SubFixerError> {
    let board = store.leaderboard(q.limit)?;
    Ok(Json(json!({ "leaderboard": board })))
}

#[derive(Deserialize)]
struct AnonymityInput {
    fixer_name: String,
    anonymous: bool,
}

async fn set_anonymity(
    State(store): State<AppState>,
    Json(input): Json<AnonymityInput>,
) -> Result<Json<Value>, SubFixerError> {
    let val = store.set_anonymous(&input.fixer_name, input.anonymous)?;
    Ok(Json(
        json!({ "fixer_name": input.fixer_name, "anonymous": val }),
    ))
}

#[derive(Deserialize)]
struct PreviewInput {
    original_text: String,
    corrected_text: String,
}

/// Stateless preview: compute the word-level diff without persisting. Lets the
/// frontend show "you changed N words" live as the user types.
async fn preview_diff(Json(input): Json<PreviewInput>) -> Json<Value> {
    let CaptionDiff {
        ops,
        words_inserted,
        words_deleted,
        words_changed,
    } = diff_captions(&input.original_text, &input.corrected_text);
    Json(json!({
        "ops": ops,
        "words_inserted": words_inserted,
        "words_deleted": words_deleted,
        "words_changed": words_changed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt; // for `oneshot`

    fn app() -> Router {
        let store = Arc::new(Store::open_in_memory().unwrap());
        router(store)
    }

    async fn body_json(resp: axum::response::Response) -> Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn health_ok() {
        let resp = app()
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["status"], "ok");
    }

    #[tokio::test]
    async fn submit_then_list_and_leaderboard() {
        let app = app();

        let payload = json!({
            "video_url": "https://youtu.be/dQw4w9WgXcQ",
            "start_sec": 30,
            "original_text": "teh quick brown fox",
            "corrected_text": "the quick brown fox",
            "fixer_name": "Alice"
        });
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/corrections")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["words_changed"], 1);
        assert_eq!(v["video_id"], "dQw4w9WgXcQ");

        // list via a different URL form
        let resp = app
            .clone()
            .oneshot(
                Request::get(
                    "/api/corrections?url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3DdQw4w9WgXcQ",
                )
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["corrections"].as_array().unwrap().len(), 1);

        // leaderboard
        let resp = app
            .oneshot(
                Request::get("/api/leaderboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let board = v["leaderboard"].as_array().unwrap();
        assert_eq!(board.len(), 1);
        assert_eq!(board[0]["display_name"], "Alice");
        assert_eq!(board[0]["words_changed"], 1);
    }

    #[tokio::test]
    async fn noop_correction_returns_400() {
        let resp = app()
            .oneshot(
                Request::post("/api/corrections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "video_url": "https://youtu.be/dQw4w9WgXcQ",
                            "start_sec": 1,
                            "original_text": "no change",
                            "corrected_text": "no change",
                            "fixer_name": "Alice"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = body_json(resp).await;
        assert!(v["error"].as_str().unwrap().contains("no-op"));
    }

    #[tokio::test]
    async fn bad_timestamp_returns_400() {
        let resp = app()
            .oneshot(
                Request::post("/api/corrections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "video_url": "https://youtu.be/dQw4w9WgXcQ",
                            "start_sec": -10,
                            "original_text": "a b",
                            "corrected_text": "a c",
                            "fixer_name": "Alice"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn anonymity_then_leaderboard_masks_name() {
        let app = app();
        // create the fixer via a submission
        app.clone()
            .oneshot(
                Request::post("/api/corrections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "video_url": "https://youtu.be/dQw4w9WgXcQ",
                            "start_sec": 1,
                            "original_text": "teh",
                            "corrected_text": "the",
                            "fixer_name": "Secret"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // opt out
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/anonymity")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({ "fixer_name": "Secret", "anonymous": true }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app
            .oneshot(
                Request::get("/api/leaderboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let board = v["leaderboard"].as_array().unwrap();
        assert_eq!(board[0]["display_name"], crate::leaderboard::ANON_LABEL);
        assert!(board[0].get("fixer_id").is_none());
    }

    #[tokio::test]
    async fn preview_diff_is_stateless() {
        let resp = app()
            .oneshot(
                Request::post("/api/preview-diff")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "original_text": "helo world",
                            "corrected_text": "hello world"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["words_changed"], 1);
    }

    #[tokio::test]
    async fn missing_url_query_returns_error_status() {
        // Query without required `url` -> axum rejects with 400.
        let resp = app()
            .oneshot(
                Request::get("/api/corrections")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}

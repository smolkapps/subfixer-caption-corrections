//! HTTP API: Axum router and handlers wiring the store to JSON endpoints.

use crate::diff::{diff_captions, CaptionDiff};
use crate::error::SubFixerError;
use crate::store::{NewCorrection, Store};
use axum::{
    extract::{Query, State},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Shared application state.
pub type AppState = Arc<Store>;

/// Build the API router (no static file serving; mounted by the binary).
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
        .with_state(state)
        .layer(cors)
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

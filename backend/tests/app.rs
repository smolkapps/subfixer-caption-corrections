//! Integration tests for the composed application (`subfixer::api::app`): the
//! API router, the SPA static serving, the client-side-route fallback, and the
//! `/api/*` 404 hygiene. These exercise behavior that lives only in the
//! composed app and is not reachable through the bare API `router`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use subfixer::api::{app, AppState};
use subfixer::Store;
use tower::ServiceExt; // for `oneshot`

/// A static-asset fixture on disk: `index.html` plus a hashed JS asset. Uses the
/// per-test-binary temp dir cargo hands to integration tests.
fn static_dir() -> String {
    // Unique dir per call: concurrent tests must not race on a shared
    // index.html (one test's truncate-then-write vs another's ServeFile read).
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = format!("{}/spa-{n}", env!("CARGO_TARGET_TMPDIR"));
    let assets = format!("{dir}/assets");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(
        format!("{dir}/index.html"),
        "<!doctype html><title>SubFixer</title><div id=app></div>",
    )
    .unwrap();
    std::fs::write(format!("{assets}/app-abc123.js"), "export const x = 1;\n").unwrap();
    dir
}

fn build_app() -> axum::Router {
    let store: AppState = Arc::new(Store::open_in_memory().unwrap());
    app(store, &static_dir())
}

fn header(resp: &axum::response::Response, name: &str) -> String {
    resp.headers()
        .get(name)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default()
}

async fn send(req: Request<Body>) -> axum::response::Response {
    build_app().oneshot(req).await.unwrap()
}

async fn body_string(resp: axum::response::Response) -> String {
    use http_body_util::BodyExt;
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Deep link / refresh of a client-side route returns the SPA shell as HTML.
#[tokio::test]
async fn deep_link_serves_index_html() {
    let resp = send(
        Request::get("/leaderboard")
            .header("accept", "text/html")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        header(&resp, "content-type").starts_with("text/html"),
        "expected HTML for a deep link"
    );
    assert!(body_string(resp).await.contains("id=app"));
}

/// The site root serves the SPA shell (guards against path normalization
/// mangling `/`).
#[tokio::test]
async fn root_serves_index_html() {
    let resp = send(
        Request::get("/")
            .header("accept", "text/html")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(header(&resp, "content-type").starts_with("text/html"));
}

/// A real hashed asset is served by `ServeDir` with the correct MIME type.
#[tokio::test]
async fn real_asset_served_with_js_mime() {
    let resp = send(
        Request::get("/assets/app-abc123.js")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = header(&resp, "content-type");
    assert!(
        ct.contains("javascript"),
        "expected a javascript MIME, got {ct:?}"
    );
}

/// An unknown `/api/*` path returns a JSON 404 rather than leaking the SPA HTML.
#[tokio::test]
async fn unknown_api_path_is_json_404() {
    let resp = send(Request::get("/api/bogus").body(Body::empty()).unwrap()).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert!(header(&resp, "content-type").starts_with("application/json"));
}

/// The unknown-`/api/*` JSON 404 must carry CORS headers, otherwise a
/// cross-origin client cannot read it. Regression test for the catch-all being
/// registered outside the `CorsLayer`.
#[tokio::test]
async fn unknown_api_404_has_cors_headers() {
    let resp = send(
        Request::get("/api/bogus")
            .header("origin", "https://example.com")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(header(&resp, "access-control-allow-origin"), "*");
}

/// A wrong method on a real endpoint stays a 405 (the catch-all does not
/// swallow it into a 404).
#[tokio::test]
async fn wrong_method_on_real_endpoint_is_405() {
    let resp = send(Request::delete("/api/health").body(Body::empty()).unwrap()).await;
    assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
}

/// Bare `/api` and `/api/` are unknown API paths, not SPA routes.
#[tokio::test]
async fn bare_api_root_is_json_404() {
    for path in ["/api", "/api/"] {
        let resp = send(Request::get(path).body(Body::empty()).unwrap()).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{path}");
        assert!(
            header(&resp, "content-type").starts_with("application/json"),
            "{path} should be JSON, not HTML"
        );
    }
}

/// A trailing slash on a *real* API endpoint must not degrade to the SPA HTML;
/// path normalization folds `/api/health/` onto `/api/health`.
#[tokio::test]
async fn trailing_slash_on_real_endpoint_stays_json() {
    let resp = send(Request::get("/api/health/").body(Body::empty()).unwrap()).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = header(&resp, "content-type");
    assert!(
        ct.starts_with("application/json"),
        "trailing-slash real endpoint should return JSON, got {ct:?}"
    );
    assert!(body_string(resp).await.contains("\"status\":\"ok\""));
}

/// A missing hashed asset requested by `<script type=module>` (which sends
/// `Accept: */*`, not `text/html`) must 404 rather than be masked as a 200
/// `index.html`, so deploy skew surfaces instead of a blank module-MIME page.
#[tokio::test]
async fn missing_asset_is_not_masked_as_index_html() {
    let resp = send(
        Request::get("/assets/app-stale999.js")
            .header("accept", "*/*")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let ct = header(&resp, "content-type");
    assert!(
        !ct.starts_with("text/html"),
        "a missing asset must not be served as HTML, got {ct:?}"
    );
}

//! End-to-end tests of the HTTP API, driven in-process through the router
//! (no sockets): upload, preview, export, save, browsing and the token.

use std::fs;
use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use glitchrx_core::{Recipe, Restart, Step};
use glitchrx_serve::{AppState, router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn app(root: Option<PathBuf>, token: Option<&str>) -> Router {
    router(AppState::new(root, token.map(str::to_string)))
}

/// Send a request; return status, headers and body bytes.
async fn send(app: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, body)
}

fn post_json(uri: &str, v: Value) -> Request<Body> {
    Request::post(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(v.to_string()))
        .unwrap()
}

fn json_of(body: &[u8]) -> Value {
    serde_json::from_slice(body).unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(body)))
}

async fn upload(app: &Router, name: &str, bytes: Vec<u8>) -> Value {
    let (s, _, b) = send(
        app,
        Request::post(format!("/api/docs?name={name}"))
            .body(Body::from(bytes))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    json_of(&b)
}

fn sample_recipe() -> Value {
    json!({
        "mode": "safe",
        "restart": "10%",
        "steps": [
            {"effect": "echo", "seed": "18446744073709551000", "params": {"gain": "0.7"}},
            {"effect": "shift", "seed": "42"}
        ]
    })
}

/// The same recipe built directly with the core, as the CLI would.
fn core_recipe() -> Recipe {
    Recipe {
        restart: Some(Restart::MilliPercent(10_000)),
        steps: vec![
            Step::new("echo", "gain=0.7", 18446744073709551000).unwrap(),
            Step::new("shift", "", 42).unwrap(),
        ],
        ..Recipe::default()
    }
}

#[tokio::test]
async fn effects_and_config() {
    let app = app(None, None);
    let (s, h, b) = send(
        &app,
        Request::get("/api/effects").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(h.contains_key(header::CONTENT_SECURITY_POLICY));
    let effects = json_of(&b);
    assert_eq!(effects.as_array().unwrap().len(), 8);
    assert_eq!(effects[0]["id"], "echo");

    let (_, _, b) = send(
        &app,
        Request::get("/api/config").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(json_of(&b)["browse"], false);
}

#[tokio::test]
async fn upload_preview_export() {
    let app = app(None, None);
    let bytes = fixture("baseline-420.jpg");
    let doc = upload(&app, "baseline-420.jpg", bytes.clone()).await;
    let id = doc["id"].as_str().unwrap().to_string();
    assert!(doc["width"].as_u64().unwrap() > 0);

    // Preview: a clean JPEG plus decode headers.
    let (s, h, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/preview"),
            json!({"recipe": sample_recipe(), "max_w": 200, "max_h": 200}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(h[header::CONTENT_TYPE], "image/jpeg");
    assert!(h.contains_key("x-decode"));
    assert!(h.contains_key("x-render-ms"));
    assert!(b.starts_with(&[0xFF, 0xD8]));

    // Original (before/after).
    let (s, _, _) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/preview"),
            json!({"original": true, "max_w": 200, "max_h": 200}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Export of the glitched file equals what the CLI's core call writes.
    let (s, h, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/export"),
            json!({"recipe": sample_recipe(), "format": "jpeg"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        h[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains("baseline-420-glitch.jpg")
    );
    assert_eq!(b, core_recipe().apply(bytes).unwrap());

    // Baked PNG.
    let (s, h, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/export"),
            json!({"recipe": sample_recipe(), "format": "png"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "image/png");
    assert!(b.starts_with(b"\x89PNG"));
}

#[tokio::test]
async fn errors_are_json_with_useful_codes() {
    let app = app(None, None);
    // Not a JPEG.
    let (s, _, b) = send(
        &app,
        Request::post("/api/docs?name=x.png")
            .body(Body::from(&b"\x89PNG...."[..]))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(json_of(&b)["error"].as_str().unwrap().contains("JPEG"));

    // Bad parameter names the effect.
    let doc = upload(&app, "g.jpg", fixture("grayscale.jpg")).await;
    let id = doc["id"].as_str().unwrap();
    let (s, _, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/preview"),
            json!({"recipe": {"steps": [{"effect": "echo", "seed": "1", "params": {"gain": "9"}}]},
                   "max_w": 100, "max_h": 100}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(json_of(&b)["error"].as_str().unwrap().starts_with("echo"));

    // Unknown document.
    let (s, _, _) = send(
        &app,
        Request::get("/api/docs/nope").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn recipe_toml_round_trip() {
    let app = app(None, None);
    let (s, _, toml) = send(&app, post_json("/api/recipe/to-toml", sample_recipe())).await;
    assert_eq!(s, StatusCode::OK);
    let text = String::from_utf8(toml).unwrap();
    assert!(text.contains("glitcharaxia-recipe"), "{text}");
    assert_eq!(Recipe::from_toml(&text).unwrap(), core_recipe());

    let (s, _, b) = send(
        &app,
        Request::post("/api/recipe/from-toml")
            .body(Body::from(text))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let back = json_of(&b);
    assert_eq!(back["steps"][0]["seed"], "18446744073709551000");
    assert_eq!(back["restart"], "10%");
}

#[tokio::test]
async fn browse_open_and_save_never_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("pics");
    fs::create_dir_all(root.join("trip")).unwrap();
    let original = fixture("baseline-444.jpg");
    fs::write(root.join("trip/photo.jpg"), &original).unwrap();
    fs::write(tmp.path().join("secret.jpg"), &original).unwrap();
    let app = app(Some(root.canonicalize().unwrap()), None);

    let (s, _, b) = send(
        &app,
        Request::get("/api/browse?path=trip")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json_of(&b)["files"][0]["path"], "trip/photo.jpg");

    // Outside the root: refused, and indistinguishable from missing.
    for p in ["../secret.jpg", "trip/../../secret.jpg"] {
        let (s, _, _) = send(&app, post_json("/api/docs/open", json!({"path": p}))).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{p}");
        let (s, _, _) = send(
            &app,
            Request::get(format!("/api/thumb?path={p}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{p}");
    }

    let (s, _, b) = send(
        &app,
        Request::get("/api/thumb?path=trip/photo.jpg")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(b.starts_with(&[0xFF, 0xD8]));

    let (s, _, b) = send(
        &app,
        post_json("/api/docs/open", json!({"path": "trip/photo.jpg"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let doc = json_of(&b);
    assert_eq!(doc["path"], "trip/photo.jpg");
    let id = doc["id"].as_str().unwrap();

    // Two saves: auto-numbered, the second does not overwrite the first.
    let save = json!({"recipe": sample_recipe(), "format": "jpeg", "with_recipe": true});
    let (s, _, b) = send(
        &app,
        post_json(&format!("/api/docs/{id}/save"), save.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(
        json_of(&b)["saved"],
        json!(["trip/photo-glitch.jpg", "trip/photo-glitch.recipe.toml"])
    );
    let (_, _, b) = send(&app, post_json(&format!("/api/docs/{id}/save"), save)).await;
    assert_eq!(json_of(&b)["saved"][0], "trip/photo-glitch-1.jpg");
    let (_, _, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/save"),
            json!({"recipe": sample_recipe(), "format": "png"}),
        ),
    )
    .await;
    assert_eq!(json_of(&b)["saved"][0], "trip/photo-glitch-2.png");

    // The original is untouched; the saved file is the CLI's output.
    assert_eq!(fs::read(root.join("trip/photo.jpg")).unwrap(), original);
    assert_eq!(
        fs::read(root.join("trip/photo-glitch.jpg")).unwrap(),
        core_recipe().apply(original).unwrap()
    );
}

#[tokio::test]
async fn save_needs_root_and_a_server_file() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(Some(tmp.path().canonicalize().unwrap()), None);
    let doc = upload(&app, "up.jpg", fixture("grayscale.jpg")).await;
    let id = doc["id"].as_str().unwrap();
    let (s, _, _) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/save"),
            json!({"recipe": sample_recipe(), "format": "jpeg"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let no_root = self::app(None, None);
    let (s, _, _) = send(
        &no_root,
        Request::get("/api/browse").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn token_gates_everything() {
    let app = app(None, Some("s3cret"));
    let (s, _, _) = send(&app, Request::get("/").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, _) = send(
        &app,
        Request::get("/api/effects").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, _) = send(
        &app,
        Request::get("/?token=wrong").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // The printed URL sets the cookie and redirects to the clean path.
    let (s, h, _) = send(
        &app,
        Request::get("/?token=s3cret").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    assert_eq!(h[header::LOCATION], "/");
    let cookie = h[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));

    let (s, _, _) = send(
        &app,
        Request::get("/api/effects")
            .header(header::COOKIE, "other=1; glitchrx_token=s3cret")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn index_is_served() {
    let app = app(None, None);
    let (s, h, b) = send(&app, Request::get("/").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        h[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert!(String::from_utf8_lossy(&b).contains("<html"));
    let (s, _, _) = send(&app, Request::get("/nope.js").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

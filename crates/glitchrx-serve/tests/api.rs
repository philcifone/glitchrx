//! End-to-end tests of the HTTP API, driven in-process through the router
//! (no sockets): upload, preview, export, save, browsing and the token.

use std::fs;
use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use glitchrx_core::codec::{self, Format};
use glitchrx_core::{Encoding, Recipe, Step};
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
        "encoding": "ulaw",
        "steps": [
            {"effect": "echo", "seed": "9223372036854775000", "params": {"decay": "0.7"}},
            {"effect": "shift", "seed": "42", "params": {"encoding": "s16le"}}
        ]
    })
}

/// The same recipe built directly with the core, as the CLI would.
fn core_recipe() -> Recipe {
    Recipe {
        encoding: Encoding::ULaw,
        steps: vec![
            Step::new("echo", "decay=0.7", 9223372036854775000).unwrap(),
            Step::new("shift", "encoding=s16le", 42).unwrap(),
        ],
        ..Recipe::default()
    }
}

/// What the CLI writes for `bytes` with [`core_recipe`] in `format`.
fn cli_output(bytes: &[u8], format: Format) -> Vec<u8> {
    let mut img = codec::decode(bytes).unwrap();
    core_recipe().apply(&mut img.raster).unwrap();
    codec::encode(&img.raster, format, img.icc.as_deref()).unwrap()
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
    assert_eq!(effects.as_array().unwrap().len(), 15);
    assert_eq!(effects[0]["id"], "echo");

    let (_, _, b) = send(
        &app,
        Request::get("/api/config").body(Body::empty()).unwrap(),
    )
    .await;
    let config = json_of(&b);
    assert_eq!(config["browse"], false);
    assert_eq!(config["encodings"].as_array().unwrap().len(), 8);
}

#[tokio::test]
async fn upload_preview_export() {
    let app = app(None, None);
    let bytes = fixture("baseline-420.jpg");
    let doc = upload(&app, "baseline-420.jpg", bytes.clone()).await;
    let id = doc["id"].as_str().unwrap().to_string();
    assert!(doc["width"].as_u64().unwrap() > 0);

    // Preview: a JPEG of the bent preview raster.
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
    assert!(h.contains_key("x-render-ms"));
    assert!(h["x-width"].to_str().unwrap().parse::<u32>().unwrap() <= 200);
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

    // Every export format equals what the CLI writes.
    for (format, f, name, mime) in [
        (
            "tiff",
            Format::Tiff,
            "baseline-420-glitch.tif",
            "image/tiff",
        ),
        ("png", Format::Png, "baseline-420-glitch.png", "image/png"),
        (
            "jpeg",
            Format::Jpeg,
            "baseline-420-glitch.jpg",
            "image/jpeg",
        ),
        // The 2a name, still accepted.
        (
            "jpeg-baked",
            Format::Jpeg,
            "baseline-420-glitch.jpg",
            "image/jpeg",
        ),
    ] {
        let (s, h, b) = send(
            &app,
            post_json(
                &format!("/api/docs/{id}/export"),
                json!({"recipe": sample_recipe(), "format": format}),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{format}");
        assert_eq!(h[header::CONTENT_TYPE], mime);
        assert!(
            h[header::CONTENT_DISPOSITION]
                .to_str()
                .unwrap()
                .contains(name)
        );
        assert_eq!(b, cli_output(&bytes, f), "{format}");
    }
}

#[tokio::test]
async fn exact_preview_and_budget() {
    let bytes = fixture("baseline-420.jpg");
    let req = |exact: bool, id: &str| {
        post_json(
            &format!("/api/docs/{id}/preview"),
            json!({"recipe": sample_recipe(), "max_w": 4096, "max_h": 4096, "exact": exact}),
        )
    };

    // An exact preview at (at least) full size is the export itself.
    let app = app(None, None);
    let id = upload(&app, "a.jpg", bytes.clone()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, h, b) = send(&app, req(true, &id)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["x-width"], "480");
    let mut want = codec::decode(&bytes).unwrap();
    core_recipe().apply(&mut want.raster).unwrap();
    let got = codec::decode(&b).unwrap().raster;
    // Compared after the preview's own JPEG encoding, so only roughly.
    let err: u64 = got
        .bytes
        .iter()
        .zip(&want.raster.bytes)
        .map(|(a, b)| a.abs_diff(*b) as u64)
        .sum::<u64>()
        / got.bytes.len() as u64;
    assert!(err < 4, "mean error {err}");

    // Over budget: 503 with X-Too-Heavy, a readable message, and the
    // photo still works afterwards.
    let mut state = AppState::new(None, None);
    state.preview_budget = std::time::Duration::ZERO;
    let app = router(state);
    let id = upload(&app, "a.jpg", bytes).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    for exact in [false, true] {
        let (s, h, b) = send(&app, req(exact, &id)).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(h["x-too-heavy"], "1");
        assert!(
            json_of(&b)["error"]
                .as_str()
                .unwrap()
                .contains("export still works")
        );
    }
    let (s, _, _) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/export"),
            json!({"recipe": sample_recipe(), "format": "png"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn errors_are_json_with_useful_codes() {
    let app = app(None, None);
    // Not an image we read.
    let (s, _, b) = send(
        &app,
        Request::post("/api/docs?name=x.gif")
            .body(Body::from(&b"GIF89a...."[..]))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(
        json_of(&b)["error"]
            .as_str()
            .unwrap()
            .contains("JPEG, PNG or TIFF")
    );

    // A broken PNG.
    let (s, _, _) = send(
        &app,
        Request::post("/api/docs?name=x.png")
            .body(Body::from(&b"\x89PNG\r\n\x1a\n...."[..]))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // Bad parameter names the effect.
    let doc = upload(&app, "g.jpg", fixture("grayscale.jpg")).await;
    let id = doc["id"].as_str().unwrap();
    let (s, _, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/preview"),
            json!({"recipe": {"steps": [{"effect": "echo", "seed": "1", "params": {"decay": "9"}}]},
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
    assert_eq!(back["steps"][0]["seed"], "9223372036854775000");
    assert_eq!(back["encoding"], "ulaw");
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
    let save = json!({"recipe": sample_recipe(), "format": "tiff", "with_recipe": true});
    let (s, _, b) = send(
        &app,
        post_json(&format!("/api/docs/{id}/save"), save.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(
        json_of(&b)["saved"],
        json!(["trip/photo-glitch.tif", "trip/photo-glitch.recipe.toml"])
    );
    let (_, _, b) = send(&app, post_json(&format!("/api/docs/{id}/save"), save)).await;
    assert_eq!(json_of(&b)["saved"][0], "trip/photo-glitch-1.tif");
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
        fs::read(root.join("trip/photo-glitch.tif")).unwrap(),
        cli_output(&original, Format::Tiff)
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

/// A browser without the token gets a sign-in page (an app on the iOS home
/// screen may start without the cookie); the manifest and icons are public,
/// since browsers fetch them without cookies.
#[tokio::test]
async fn sign_in_page_and_public_icons() {
    let app = app(None, Some("s3cret"));
    let page = Request::get("/")
        .header(header::ACCEPT, "text/html,application/xhtml+xml")
        .body(Body::empty())
        .unwrap();
    let (s, h, b) = send(&app, page).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(
        h[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert!(h.contains_key(header::CONTENT_SECURITY_POLICY));
    let html = String::from_utf8_lossy(&b);
    assert!(html.contains(r#"name="token""#) && !html.contains("s3cret"));

    for path in [
        "/manifest.webmanifest",
        "/apple-touch-icon.png",
        "/icon-512.png",
    ] {
        let (s, _, _) = send(&app, Request::get(path).body(Body::empty()).unwrap()).await;
        // 404 when the frontend is not built (placeholder only), never 401.
        assert!(
            s == StatusCode::OK || s == StatusCode::NOT_FOUND,
            "{path}: {s}"
        );
    }
    // Lookalike paths are still guarded.
    let (s, _, _) = send(
        &app,
        Request::get("/manifest.webmanifest/../api/effects")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
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

/// Stand-in for the desktop app's native dialogs: answers from a script
/// and records where each dialog was asked to start.
struct FakeDialogs {
    pick: std::sync::Mutex<Vec<Option<PathBuf>>>,
    save: std::sync::Mutex<Vec<Option<PathBuf>>>,
    asked: std::sync::Mutex<Vec<(Option<PathBuf>, String)>>,
}

impl glitchrx_serve::dialogs::Dialogs for FakeDialogs {
    fn pick_photo(&self, _dir: Option<&Path>) -> Option<PathBuf> {
        self.pick.lock().unwrap().remove(0)
    }
    fn save_as(&self, dir: Option<&Path>, name: &str) -> Option<PathBuf> {
        self.asked
            .lock()
            .unwrap()
            .push((dir.map(Path::to_path_buf), name.to_string()));
        self.save.lock().unwrap().remove(0)
    }
}

#[tokio::test]
async fn desktop_dialogs_open_and_save_as() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    let original = fixture("baseline-444.jpg");
    let photo = dir.join("photo.jpg");
    fs::write(&photo, &original).unwrap();
    fs::write(dir.join("photo-glitch.tif"), b"earlier").unwrap();
    let taken = dir.join("taken.tif");
    fs::write(&taken, b"replace me").unwrap();

    let fake = std::sync::Arc::new(FakeDialogs {
        pick: std::sync::Mutex::new(vec![None, Some(photo.clone())]),
        save: std::sync::Mutex::new(vec![None, Some(taken.clone()), Some(photo.clone())]),
        asked: std::sync::Mutex::new(Vec::new()),
    });
    let app = router(AppState::new(None, None).with_dialogs(fake.clone()));

    let (_, _, b) = send(
        &app,
        Request::get("/api/config").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(json_of(&b)["desktop"], true);

    // Cancelled, then chosen.
    let (s, _, _) = send(&app, post_json("/api/desktop/open", json!({}))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, b) = send(&app, post_json("/api/desktop/open", json!({}))).await;
    assert_eq!(s, StatusCode::OK);
    let id = json_of(&b)["id"].as_str().unwrap().to_string();

    let body = json!({"recipe": sample_recipe(), "format": "tiff", "with_recipe": true});
    let uri = format!("/api/docs/{id}/save-as");
    let (s, _, _) = send(&app, post_json(&uri, body.clone())).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    // The dialog starts next to the original with the next free name.
    assert_eq!(
        fake.asked.lock().unwrap()[0],
        (Some(dir.clone()), "photo-glitch-1.tif".to_string())
    );

    // A file the user confirmed replacing in the dialog is replaced, with
    // exactly the CLI's bytes, and the recipe lands beside it.
    let (s, _, b) = send(&app, post_json(&uri, body.clone())).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(
        fs::read(&taken).unwrap(),
        cli_output(&original, Format::Tiff)
    );
    assert!(dir.join("taken.recipe.toml").exists());
    assert_eq!(json_of(&b)["saved"].as_array().unwrap().len(), 2);

    // The original is never written, whatever the dialog says.
    let (s, _, _) = send(&app, post_json(&uri, body)).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(fs::read(&photo).unwrap(), original);
}

#[tokio::test]
async fn desktop_routes_are_absent_in_the_browser_server() {
    let app = app(None, None);
    let (_, _, b) = send(
        &app,
        Request::get("/api/config").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(json_of(&b)["desktop"], false);
    let (s, _, _) = send(&app, post_json("/api/desktop/open", json!({}))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

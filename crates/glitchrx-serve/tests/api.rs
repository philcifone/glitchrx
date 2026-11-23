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

/// A zoomed view asks for part of the picture: cut from the full-resolution
/// render (or the original), shrunk only if it is bigger than the box, with
/// its pixel rectangle in `X-Crop`.
#[tokio::test]
async fn zoomed_crops() {
    let bytes = fixture("baseline-420.jpg"); // 480 x 320
    let app = app(None, None);
    let id = upload(&app, "a.jpg", bytes.clone()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let crop = json!({"x": 0.25, "y": 0.5, "w": 0.5, "h": 0.25});
    let mut want = codec::decode(&bytes).unwrap().raster;
    let original = want.crop(120, 160, 240, 80);
    core_recipe().apply(&mut want).unwrap();
    let bent = want.crop(120, 160, 240, 80);
    let mean_error = |got: &[u8], want: &[u8]| {
        got.iter()
            .zip(want)
            .map(|(a, b)| a.abs_diff(*b) as u64)
            .sum::<u64>()
            / got.len() as u64
    };
    for (body, want) in [
        (
            json!({"recipe": sample_recipe(), "max_w": 4096, "max_h": 4096, "crop": crop}),
            &bent,
        ),
        (
            json!({"original": true, "max_w": 4096, "max_h": 4096, "crop": crop}),
            &original,
        ),
    ] {
        let (s, h, b) = send(&app, post_json(&format!("/api/docs/{id}/preview"), body)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h["x-crop"], "120,160,240,80");
        assert_eq!(
            (&h["x-width"], &h["x-height"]),
            (&"240".parse().unwrap(), &"80".parse().unwrap())
        );
        let got = codec::decode(&b).unwrap().raster;
        // After the preview's own JPEG encoding, so only roughly. A busy patch suffers more from JPEG than a whole picture: the
        // right crop measures about 5, one 2 px off about 31.
        let err = mean_error(&got.bytes, &want.bytes);
        assert!(err < 8, "mean error {err}");
    }

    // Bigger than the box: shrunk to fit, same rectangle.
    let small = json!({"recipe": sample_recipe(), "max_w": 60, "max_h": 60, "crop": crop});
    let (s, h, _) = send(&app, post_json(&format!("/api/docs/{id}/preview"), small)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (
            h["x-width"].to_str().unwrap(),
            h["x-height"].to_str().unwrap()
        ),
        ("60", "20")
    );
    assert_eq!(h["x-crop"], "120,160,240,80");

    // Partial pixels round outward; nonsense is refused.
    let edge = json!({"original": true, "max_w": 4096, "max_h": 4096,
        "crop": {"x": 0.001, "y": 0.0, "w": 0.002, "h": 2.0}});
    let (_, h, _) = send(&app, post_json(&format!("/api/docs/{id}/preview"), edge)).await;
    assert_eq!(h["x-crop"], "0,0,2,320");
    let bad = json!({"original": true, "max_w": 99, "max_h": 99, "crop": {"x": 0, "y": 0, "w": 0, "h": 1}});
    let (s, _, _) = send(&app, post_json(&format!("/api/docs/{id}/preview"), bad)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
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

/// Decode an RGBA PNG into (width, height, pixels).
fn rgba(png_bytes: &[u8]) -> (usize, usize, Vec<u8>) {
    let dec = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    buf.truncate(info.buffer_size());
    (info.width as usize, info.height as usize, buf)
}

#[tokio::test]
async fn masks_travel_and_show() {
    let app = app(None, None);
    let doc = upload(&app, "baseline-420.jpg", fixture("baseline-420.jpg")).await;
    let id = doc["id"].as_str().unwrap().to_string();
    // A left-to-right gradient on the second step, at half opacity.
    let recipe = json!({"steps": [
        {"effect": "invert", "seed": "1", "params": {"hits": "0"}},
        {"effect": "bitcrush", "seed": "2", "params": {"opacity": "0.5"},
         "mask": {"op": [
            {"shape": "linear", "from": [0.0, 0.5], "to": [1.0, 0.5], "feather": 0.01}]}}]});

    // The mask survives TOML both ways.
    let (s, _, toml) = send(&app, post_json("/api/recipe/to-toml", recipe.clone())).await;
    assert_eq!(s, StatusCode::OK);
    let toml = String::from_utf8(toml).unwrap();
    assert!(
        toml.contains("[[step.mask.op]]") && toml.contains("version = 3"),
        "{toml}"
    );
    let req = Request::post("/api/recipe/from-toml")
        .body(Body::from(toml))
        .unwrap();
    let (s, _, back) = send(&app, req).await;
    assert_eq!(s, StatusCode::OK);
    // Defaults come back written out (`mode = "add"`), so they read clearly.
    let mut expected = recipe["steps"][1]["mask"].clone();
    expected["op"][0]["mode"] = json!("add");
    assert_eq!(json_of(&back)["steps"][1]["mask"], expected);

    // Preview renders with it.
    let (s, _, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/preview"),
            json!({"recipe": recipe, "max_w": 200, "max_h": 200}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));

    // The mask view: tinted, opaque on the left, clear on the right.
    let ask = |step: usize| {
        post_json(
            &format!("/api/docs/{id}/mask"),
            json!({"recipe": recipe, "step": step, "max_w": 200, "max_h": 200,
                   "tint": [10, 20, 30]}),
        )
    };
    let (s, h, b) = send(&app, ask(1)).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(h[header::CONTENT_TYPE], "image/png");
    let (w, hgt, px) = rgba(&b);
    assert!(w <= 200 && hgt <= 200);
    let alpha = |x: usize| px[((hgt / 2) * w + x) * 4 + 3];
    assert_eq!(&px[..3], &[10, 20, 30]);
    assert!(
        alpha(2) > 230 && alpha(w - 3) < 25,
        "{} {}",
        alpha(2),
        alpha(w - 3)
    );

    // A step without a mask shows everywhere; a missing step is an error.
    let (_, _, b) = send(&app, ask(0)).await;
    assert!(rgba(&b).2.chunks(4).all(|p| p[3] == 255));
    let (s, _, _) = send(&app, ask(5)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

fn put_json(uri: &str, v: Value) -> Request<Body> {
    Request::put(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(v.to_string()))
        .unwrap()
}

#[tokio::test]
async fn presets_save_list_replace_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("presets");
    let app = router(AppState::new(None, None).with_presets(dir.clone()));
    let get = |uri: &str| Request::get(uri).body(Body::empty()).unwrap();

    let (_, _, b) = send(&app, get("/api/config")).await;
    assert_eq!(json_of(&b)["presets"], true);
    let (s, _, b) = send(&app, get("/api/presets")).await;
    assert_eq!((s, json_of(&b)), (StatusCode::OK, json!([])));

    // Save; names may have spaces (URL-encoded in the path).
    let uri = "/api/presets/Melt%20down";
    let body = json!({"recipe": sample_recipe()});
    let (s, _, b) = send(&app, put_json(uri, body.clone())).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(json_of(&b)["name"], "Melt down");
    // It is an ordinary recipe file the CLI can replay.
    let text = fs::read_to_string(dir.join("Melt down.toml")).unwrap();
    assert!(Recipe::from_toml(&text).is_ok(), "{text}");

    // Taken names need `replace`.
    let (s, _, _) = send(&app, put_json(uri, body.clone())).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let one = json!({"recipe": {"steps": [{"effect": "invert", "seed": "3"}]}, "replace": true});
    let (s, _, _) = send(&app, put_json(uri, one)).await;
    assert_eq!(s, StatusCode::OK);

    let (_, _, b) = send(&app, get("/api/presets")).await;
    let list = json_of(&b);
    assert_eq!(list[0]["name"], "Melt down");
    assert_eq!(list[0]["effects"], json!(["invert"]));
    let (s, _, b) = send(&app, get(uri)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json_of(&b)["steps"][0]["seed"], "3");

    // Bad names and empty recipes are refused; nothing leaves the directory.
    for bad in ["/api/presets/..%2Fescape", "/api/presets/.hidden"] {
        let (s, _, _) = send(&app, put_json(bad, body.clone())).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    assert!(!tmp.path().join("escape.toml").exists());
    let (s, _, _) = send(&app, put_json("/api/presets/empty", json!({"recipe": {}}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let del = || Request::delete(uri).body(Body::empty()).unwrap();
    assert_eq!(send(&app, del()).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&app, del()).await.0, StatusCode::NOT_FOUND);

    // A server without a presets directory says so.
    let plain = app_without_presets();
    let (_, _, b) = send(&plain, get("/api/config")).await;
    assert_eq!(json_of(&b)["presets"], false);
    assert_eq!(
        send(&plain, get("/api/presets")).await.0,
        StatusCode::NOT_FOUND
    );
}

fn app_without_presets() -> Router {
    app(None, None)
}

/// A server with settings kept in a temporary home, never the real one.
fn app_with_settings(home: &Path, root: Option<PathBuf>) -> Router {
    let store = glitchrx_serve::settings::SettingsStore::open(
        home.join(".config/glitchrx/config.toml"),
        home,
    )
    .unwrap();
    router(AppState::new(root, None).with_settings(store))
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn settings_choose_the_library_and_export_folders() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join("Pictures/trips")).unwrap();
    fs::create_dir_all(home.join("Pictures/glitched")).unwrap();
    fs::create_dir_all(home.join(".hidden")).unwrap();
    fs::create_dir_all(tmp.path().join("outside")).unwrap();
    fs::write(
        home.join("Pictures/trips/a.jpg"),
        fixture("baseline-444.jpg"),
    )
    .unwrap();
    let app = app_with_settings(&home, None);

    // Nothing chosen yet: no browsing.
    let (_, _, b) = send(&app, get("/api/config")).await;
    assert_eq!(json_of(&b)["browse"], false);
    assert_eq!(json_of(&b)["settings"], true);
    let (_, _, b) = send(&app, get("/api/settings")).await;
    assert_eq!(
        json_of(&b),
        json!({"library": null, "export": null, "fixed_root": null})
    );

    // The folder picker lists folders under home, no hidden ones, and
    // never leaves home.
    let (s, _, b) = send(&app, get("/api/folders?path=")).await;
    assert_eq!(s, StatusCode::OK);
    let names: Vec<_> = json_of(&b)["dirs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].clone())
        .collect();
    assert_eq!(names, vec![json!("Pictures")]);
    let (_, _, b) = send(&app, get("/api/folders?path=Pictures")).await;
    assert_eq!(json_of(&b)["dirs"].as_array().unwrap().len(), 2);
    let (s, _, _) = send(&app, get("/api/folders?path=../outside")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Choose the folders: Open now browses the library.
    let (s, _, b) = send(
        &app,
        put_json(
            "/api/settings",
            json!({"library": "Pictures", "export": "Pictures/glitched"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(json_of(&b)["library"], "Pictures");
    let (_, _, b) = send(&app, get("/api/browse?path=trips")).await;
    assert_eq!(json_of(&b)["files"][0]["path"], "trips/a.jpg");
    let file = fs::read_to_string(home.join(".config/glitchrx/config.toml")).unwrap();
    assert!(file.contains("glitched"), "{file}");

    // Outside home, or not a folder: refused, nothing changed.
    for bad in ["../outside", "Pictures/trips/a.jpg", "missing"] {
        let (s, _, _) = send(
            &app,
            put_json("/api/settings", json!({"library": bad, "export": null})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    let (_, _, b) = send(&app, get("/api/settings")).await;
    assert_eq!(json_of(&b)["export"], "Pictures/glitched");

    // --root wins over the library.
    let fixed = tmp.path().join("fixed");
    fs::create_dir_all(&fixed).unwrap();
    let app = app_with_settings(&home, Some(fixed.canonicalize().unwrap()));
    let (_, _, b) = send(&app, get("/api/settings")).await;
    assert!(
        json_of(&b)["fixed_root"]
            .as_str()
            .unwrap()
            .ends_with("fixed")
    );
    send(
        &app,
        put_json(
            "/api/settings",
            json!({"library": "Pictures", "export": null}),
        ),
    )
    .await;
    let (_, _, b) = send(&app, get("/api/browse?path=")).await;
    assert_eq!(
        json_of(&b)["dirs"].as_array().unwrap().len(),
        0,
        "still the fixed root"
    );
}

#[tokio::test]
async fn save_as_into_a_folder_never_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join("out")).unwrap();
    let app = app_with_settings(&home, None);
    let original = fixture("baseline-444.jpg");
    let doc = upload(&app, "photo.jpg", original.clone()).await;
    let id = doc["id"].as_str().unwrap();

    let (_, _, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/suggest"),
            json!({"folder": "out", "format": "webp"}),
        ),
    )
    .await;
    assert_eq!(json_of(&b)["name"], "photo-glitch");

    let req = |name: &str| {
        json!({"recipe": sample_recipe(), "format": "webp", "lossless": true,
               "folder": "out", "name": name, "with_recipe": true})
    };
    let (s, _, b) = send(
        &app,
        post_json(&format!("/api/docs/{id}/save-to"), req("mine")),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(
        json_of(&b)["saved"],
        json!(["~/out/mine.webp", "~/out/mine.recipe.toml"])
    );
    let written = fs::read(home.join("out/mine.webp")).unwrap();
    assert_eq!(&written[8..12], b"WEBP");
    // Lossless: exactly the CLI's result, decoded.
    let mut expect = codec::decode(&original).unwrap();
    core_recipe().apply(&mut expect.raster).unwrap();
    let settings = codec::EncodeSettings {
        lossless: true,
        ..Default::default()
    };
    assert_eq!(
        written,
        codec::encode_with(
            &expect.raster,
            Format::Webp,
            expect.icc.as_deref(),
            &settings
        )
        .unwrap()
    );

    // The same name again: refused, file unchanged; the suggestion moves on.
    let (s, _, b) = send(
        &app,
        post_json(&format!("/api/docs/{id}/save-to"), req("mine.webp")),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "{}", String::from_utf8_lossy(&b));
    assert_eq!(fs::read(home.join("out/mine.webp")).unwrap(), written);

    // Names cannot reach other folders or hide.
    for bad in ["../escape", "sub/x", ".hidden", ""] {
        let (s, _, _) = send(
            &app,
            post_json(&format!("/api/docs/{id}/save-to"), req(bad)),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad:?}");
    }
    let (s, _, _) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/save-to"),
            json!({"recipe": sample_recipe(), "format": "png", "folder": "..", "name": "x"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(!tmp.path().join("x.png").exists());
}

#[tokio::test]
async fn webp_and_avif_exports_take_their_settings() {
    let app = app(None, None);
    let doc = upload(&app, "photo.jpg", fixture("baseline-444.jpg")).await;
    let id = doc["id"].as_str().unwrap();
    assert_eq!(doc["avif_keeps_profile"], true);
    let export = |v: Value| post_json(&format!("/api/docs/{id}/export"), v);
    let size = |q: u8| export(json!({"recipe": sample_recipe(), "format": "webp", "quality": q}));
    let (s, h, small) = send(&app, size(10)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "image/webp");
    let (_, _, big) = send(&app, size(95)).await;
    assert!(small.len() < big.len());
    let (s, h, avif) = send(
        &app,
        export(json!({"recipe": sample_recipe(), "format": "avif", "quality": 60, "speed": 10})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "image/avif");
    assert!(
        h[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains("photo-glitch.avif")
    );
    assert_eq!(&avif[4..12], b"ftypavif");
    let (s, _, _) = send(
        &app,
        export(json!({"recipe": sample_recipe(), "format": "avif", "speed": 11})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = send(
        &app,
        export(json!({"recipe": sample_recipe(), "format": "jpeg", "quality": 0})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

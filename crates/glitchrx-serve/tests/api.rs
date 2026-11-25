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
    assert_eq!(effects.as_array().unwrap().len(), 20);
    assert_eq!(effects[0]["id"], "echo");
    // The curve kind reaches the GUI, which draws an editor for it.
    let curves = effects
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "curves")
        .unwrap();
    assert_eq!(curves["group"], "adjust");
    assert!(
        curves["params"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == "curve")
    );

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
        toml.contains("[[step.mask.op]]") && toml.contains("version = 7"),
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

#[tokio::test]
async fn work_size_runs_the_steps_on_a_smaller_canvas() {
    let app = app(None, None);
    let original = fixture("baseline-444.jpg");
    let doc = upload(&app, "photo.jpg", original.clone()).await;
    let id = doc["id"].as_str().unwrap();
    let mut recipe = sample_recipe();
    recipe["work_size"] = json!(200);
    let export = |r: Value| {
        post_json(
            &format!("/api/docs/{id}/export"),
            json!({"recipe": r, "format": "png"}),
        )
    };

    // Exactly the CLI's run: shrink, bend, write at the work size.
    let (s, _, b) = send(&app, export(recipe.clone())).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let img = codec::decode(&original).unwrap();
    let core = Recipe {
        work_size: Some(200),
        ..core_recipe()
    };
    let want = core.render(&img.raster).unwrap();
    let got = codec::decode(&b).unwrap().raster;
    assert_eq!((got.width, got.height), (want.width, want.height));
    assert_eq!(got.bytes, want.bytes);
    assert_eq!(want.width.max(want.height), 200);

    // Scaled back: the full size again, in blocks.
    recipe["scale_back"] = json!(true);
    let (_, _, b) = send(&app, export(recipe.clone())).await;
    let big = codec::decode(&b).unwrap().raster;
    assert_eq!(
        (big.width, big.height),
        (img.raster.width, img.raster.height)
    );

    // The original view shows the canvas too, so before and after match.
    let (s, h, _) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/preview"),
            json!({"original": true, "recipe": recipe, "max_w": 4000, "max_h": 4000}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["x-width"].to_str().unwrap(), want.width.to_string());

    // Too small is refused with a message.
    recipe["work_size"] = json!(8);
    let (s, _, b) = send(&app, export(recipe)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(String::from_utf8_lossy(&b).contains("work_size"));
}

#[tokio::test]
async fn crop_comes_before_the_steps() {
    let app = app(None, None);
    let original = fixture("baseline-444.jpg");
    let doc = upload(&app, "photo.jpg", original.clone()).await;
    let id = doc["id"].as_str().unwrap();
    let mut recipe = sample_recipe();
    recipe["crop"] = json!({"x": 0.25, "y": 0.0, "w": 0.5, "h": 0.75});
    let export = |r: Value| {
        post_json(
            &format!("/api/docs/{id}/export"),
            json!({"recipe": r, "format": "png"}),
        )
    };

    // The same bytes as the CLI's run.
    let (s, _, b) = send(&app, export(recipe.clone())).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let img = codec::decode(&original).unwrap();
    let core = Recipe {
        crop: Some(glitchrx_core::recipe::Crop {
            x: 0.25,
            y: 0.0,
            w: 0.5,
            h: 0.75,
        }),
        ..core_recipe()
    };
    let want = core.render(&img.raster).unwrap();
    let got = codec::decode(&b).unwrap().raster;
    assert_eq!(
        (got.width, got.height),
        (img.raster.width / 2, img.raster.height * 3 / 4)
    );
    assert_eq!(got.bytes, want.bytes);

    // A crop outside the photo is refused with a message.
    recipe["crop"] = json!({"x": 0.75, "y": 0.0, "w": 0.5, "h": 1.0});
    let (s, _, b) = send(&app, export(recipe)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(String::from_utf8_lossy(&b).contains("crop"));
}

/// D-69: a crop of the result. The export is the whole render, framed;
/// previews, zoomed views and the mask view all show just the frame, and
/// a zoomed view is measured within it.
#[tokio::test]
async fn a_crop_of_the_result_frames_the_render() {
    let app = app(None, None);
    let original = fixture("baseline-420.jpg"); // 480 x 320
    let doc = upload(&app, "photo.jpg", original.clone()).await;
    let id = doc["id"].as_str().unwrap();
    let mut recipe = sample_recipe();
    recipe["crop"] = json!({"x": 0.25, "y": 0.0, "w": 0.5, "h": 0.5});
    recipe["crop_result"] = json!(true);

    let (s, _, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/export"),
            json!({"recipe": recipe, "format": "png"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let mut whole = codec::decode(&original).unwrap().raster;
    core_recipe().apply(&mut whole).unwrap();
    let framed = whole.crop(120, 0, 240, 160);
    assert_eq!(codec::decode(&b).unwrap().raster, framed);

    // A live preview shows the frame, filling the box.
    let (s, h, _) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/preview"),
            json!({"recipe": recipe, "max_w": 120, "max_h": 120}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (&h["x-width"], &h["x-height"]),
        (&"120".parse().unwrap(), &"80".parse().unwrap())
    );

    // A zoomed view: its shares and X-Crop are of the frame.
    let (s, h, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/preview"),
            json!({"recipe": recipe, "max_w": 4096, "max_h": 4096,
                   "crop": {"x": 0.5, "y": 0.5, "w": 0.5, "h": 0.5}}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["x-crop"], "120,80,120,80");
    let got = codec::decode(&b).unwrap().raster;
    let want = framed.crop(120, 80, 120, 80);
    let err = got
        .bytes
        .iter()
        .zip(&want.bytes)
        .map(|(a, b)| a.abs_diff(*b) as u64)
        .sum::<u64>()
        / got.bytes.len() as u64;
    assert!(err < 8, "mean error {err}");

    // The mask view lies on the preview: the same size.
    let (s, _, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/mask"),
            json!({"recipe": recipe, "step": 0, "max_w": 120, "max_h": 120}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let png = codec::decode(&b).unwrap().raster;
    assert_eq!((png.width, png.height), (120, 80));
}

/// Save As may update a project of the same name, once asked (Phil,
/// 2026-11-25): the first try answers 409 with `X-Replaceable`, the second
/// with `replace` writes over it. Pictures and non-projects are never
/// replaced.
#[tokio::test]
async fn save_as_updates_a_project_once_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join("out")).unwrap();
    let app = app_with_settings(&home, None);
    let doc = upload(&app, "photo.jpg", fixture("baseline-444.jpg")).await;
    let id = doc["id"].as_str().unwrap();
    let save = |name: &str, project: bool, replace: bool, recipe: Value| {
        post_json(
            &format!("/api/docs/{id}/save-to"),
            json!({"recipe": recipe, "format": "png", "project": project,
                   "replace": replace, "folder": "out", "name": name}),
        )
    };
    let (s, _, b) = send(&app, save("edit", true, false, sample_recipe())).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let first = fs::read(home.join("out/edit.glrx")).unwrap();

    // Taken: asked about, not written.
    let mut changed = sample_recipe();
    changed["steps"][0]["params"]["decay"] = json!("0.3");
    let (s, h, _) = send(&app, save("edit", true, false, changed.clone())).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(h["x-replaceable"], "1");
    assert_eq!(fs::read(home.join("out/edit.glrx")).unwrap(), first);

    // Asked: updated, and it opens with the new steps.
    let (s, _, b) = send(&app, save("edit", true, true, changed)).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let now = fs::read(home.join("out/edit.glrx")).unwrap();
    assert_ne!(now, first);
    let p = glitchrx_core::project::Project::from_bytes(&now).unwrap();
    assert_eq!(p.recipe.steps[0].params["decay"], "0.3");

    // A picture never replaces; nor does a project replace a file that is
    // not one, whatever its name.
    let (s, _, _) = send(&app, save("pic", false, false, sample_recipe())).await;
    assert_eq!(s, StatusCode::OK);
    let (s, h, _) = send(&app, save("pic", false, true, sample_recipe())).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(h.get("x-replaceable").is_none());
    fs::write(home.join("out/fake.glrx"), b"not a project").unwrap();
    let (s, h, _) = send(&app, save("fake", true, true, sample_recipe())).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(h.get("x-replaceable").is_none());
    assert_eq!(
        fs::read(home.join("out/fake.glrx")).unwrap(),
        b"not a project"
    );
}

#[tokio::test]
async fn saved_recipes_are_listed_read_and_offered_for_their_photo() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join("out")).unwrap();
    let app = app_with_settings(&home, None);
    let (s, _, _) = send(
        &app,
        put_json("/api/settings", json!({"library": null, "export": "out"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let original = fixture("baseline-444.jpg");
    let doc = upload(&app, "IMG_7.jpg", original.clone()).await;
    assert_eq!(doc["recipes"], json!([]));
    let id = doc["id"].as_str().unwrap();

    // Saved under a name of its own: the recipe names the photo.
    let mut recipe = sample_recipe();
    recipe["source"] = json!("../../somewhere/IMG_7.jpg");
    let (s, _, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/save-to"),
            json!({"recipe": recipe, "format": "png", "folder": "out",
                   "name": "beach", "with_recipe": true}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let text = fs::read_to_string(home.join("out/beach.recipe.toml")).unwrap();
    assert!(text.contains("source = \"IMG_7.jpg\""), "{text}");
    // An older recipe, matched by its export name; and one for another photo.
    fs::write(
        home.join("out/IMG_7-glitch-2.recipe.toml"),
        text.replace("source = \"IMG_7.jpg\"\n", ""),
    )
    .unwrap();
    fs::write(
        home.join("out/other.recipe.toml"),
        text.replace("IMG_7.jpg", "IMG_8.jpg"),
    )
    .unwrap();

    // The same photo opened again: both of its recipes are offered.
    let again = upload(&app, "IMG_7.jpg", original).await;
    let mut names: Vec<_> = again["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    assert_eq!(names, ["IMG_7-glitch-2.recipe.toml", "beach.recipe.toml"]);

    // All three are listed, and one reads back as the GUI's recipe.
    let (_, _, b) = send(&app, get("/api/recipes")).await;
    assert_eq!(json_of(&b).as_array().unwrap().len(), 3);
    let (s, _, b) = send(&app, get("/api/recipe?place=export&path=beach.recipe.toml")).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(json_of(&b)["steps"][1]["effect"], "shift");
    assert_eq!(json_of(&b)["steps"][1]["seed"], "42");
    assert_eq!(json_of(&b)["source"], "IMG_7.jpg");

    // Nothing else is read: other files, other folders, unset folders.
    fs::write(home.join("out/notes.txt"), "secret").unwrap();
    for bad in [
        "place=export&path=notes.txt",
        "place=export&path=../.config/glitchrx/config.toml",
        "place=library&path=beach.recipe.toml",
    ] {
        let (s, _, _) = send(&app, get(&format!("/api/recipe?{bad}"))).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{bad}");
    }
}

#[tokio::test]
async fn histogram_of_the_picture_reaching_a_step() {
    let app = app(None, None);
    let doc = upload(&app, "photo.jpg", fixture("baseline-444.jpg")).await;
    let id = doc["id"].as_str().unwrap();
    let ask = |step: usize| {
        post_json(
            &format!("/api/docs/{id}/histogram"),
            json!({"recipe": sample_recipe(), "step": step, "max_w": 120, "max_h": 120}),
        )
    };
    let (s, _, b) = send(&app, ask(0)).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let h0 = json_of(&b);
    // 480 x 320 fitted into 120 x 120.
    let pixels = 120 * 80;
    for ch in ["red", "green", "blue", "luma"] {
        let counts = h0[ch].as_array().unwrap();
        assert_eq!(counts.len(), 256);
        assert_eq!(
            counts.iter().map(|v| v.as_u64().unwrap()).sum::<u64>(),
            pixels
        );
    }
    // After the first step the picture differs; past the last step is refused.
    let (_, _, b) = send(&app, ask(2)).await;
    assert_ne!(json_of(&b)["red"], h0["red"]);
    let (s, _, _) = send(&app, ask(3)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn projects_download_open_and_save_over_themselves() {
    use glitchrx_core::project::{self, Project};
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("pics");
    fs::create_dir_all(&root).unwrap();
    let original = fixture("orient-6.jpg");
    fs::write(root.join("photo.jpg"), &original).unwrap();
    let app = app(Some(root.canonicalize().unwrap()), None);

    // A project downloaded from an uploaded photo carries it and the recipe.
    let doc = upload(&app, "photo.jpg", original.clone()).await;
    let id = doc["id"].as_str().unwrap();
    assert!(doc["project"].is_null());
    let (s, h, b) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/export"),
            json!({"recipe": sample_recipe(), "format": "tiff", "project": true}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(h[header::CONTENT_TYPE], project::MIMETYPE);
    assert!(
        h[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains("photo-glitch.glrx")
    );
    let p = Project::from_bytes(&b).unwrap();
    assert_eq!(p.name, "photo.jpg");
    assert_eq!(p.recipe.steps.len(), 2);
    assert!(p.preview.is_some());
    assert_eq!(
        codec::decode(&p.original).unwrap().raster,
        codec::decode(&original).unwrap().raster
    );

    // Uploaded back, it opens as its original with its recipe.
    let opened = upload(&app, "edit.glrx", b.clone()).await;
    assert_eq!(opened["name"], "photo.jpg");
    assert_eq!(opened["project"], "edit.glrx");
    assert_eq!(opened["recipe"]["steps"].as_array().unwrap().len(), 2);
    assert_eq!(opened["recipe"]["steps"][0]["effect"], "echo");

    // In the library: listed, with the preview as its thumbnail.
    fs::write(root.join("edit.glrx"), &b).unwrap();
    let (_, _, l) = send(
        &app,
        Request::get("/api/browse?path=")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let names: Vec<_> = json_of(&l)["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["edit.glrx", "photo.jpg"]);
    let (s, _, t) = send(
        &app,
        Request::get("/api/thumb?path=edit.glrx")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(t.starts_with(&[0xFF, 0xD8]));

    // Opened from the library, saved next to it as a new project, then
    // over itself when asked.
    let (_, _, o) = send(
        &app,
        post_json("/api/docs/open", json!({"path": "edit.glrx"})),
    )
    .await;
    let o = json_of(&o);
    let id = o["id"].as_str().unwrap();
    assert_eq!(o["path"], "edit.glrx");
    assert_eq!(o["recipes"], json!([]), "no continue-from for a project");
    let one_step = json!({"steps": [{"effect": "invert", "seed": "1", "params": {}}]});
    let (s, _, r) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/save"),
            json!({"recipe": one_step, "format": "tiff", "project": true}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&r));
    assert_eq!(json_of(&r)["saved"], json!(["edit-glitch.glrx"]));
    assert_eq!(
        fs::read(root.join("edit.glrx")).unwrap(),
        b,
        "not replaced unasked"
    );
    let (s, _, r) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/save"),
            json!({"recipe": one_step, "format": "tiff", "project": true, "replace": true}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&r));
    assert_eq!(json_of(&r)["saved"], json!(["edit.glrx"]));
    let replaced = Project::from_bytes(&fs::read(root.join("edit.glrx")).unwrap()).unwrap();
    assert_eq!(replaced.recipe.steps[0].effect, "invert");
    assert_eq!(
        replaced.original, p.original,
        "the photo inside is carried over"
    );

    // A photo is never replaced, and a picture never replaces a project.
    let (_, _, ph) = send(
        &app,
        post_json("/api/docs/open", json!({"path": "photo.jpg"})),
    )
    .await;
    let pid = json_of(&ph)["id"].as_str().unwrap().to_string();
    for (doc, body) in [
        (
            &pid,
            json!({"recipe": one_step, "format": "tiff", "project": true, "replace": true}),
        ),
        (
            &id.to_string(),
            json!({"recipe": one_step, "format": "tiff", "replace": true}),
        ),
    ] {
        let (s, _, _) = send(&app, post_json(&format!("/api/docs/{doc}/save"), body)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    assert_eq!(fs::read(root.join("photo.jpg")).unwrap(), original);

    // Not a project, though named like one.
    let (s, _, _) = send(
        &app,
        Request::post("/api/docs?name=x.glrx")
            .body(Body::from(&b"PK\x03\x04junk"[..]))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn the_launch_file_opens_once() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("edit.glrx");
    fs::write(&path, fixture("project-v1.glrx")).unwrap();
    let app = router(AppState::new(None, None).with_launch_file(path));
    let (s, _, b) = send(&app, post_json("/api/desktop/launch", json!({}))).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let doc = json_of(&b);
    assert_eq!(doc["project"], "edit.glrx");
    assert_eq!(doc["name"], "orient-6.jpg");
    assert_eq!(doc["recipe"]["steps"].as_array().unwrap().len(), 2);
    // Taken: a reload starts as usual.
    let (s, _, _) = send(&app, post_json("/api/desktop/launch", json!({}))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    // Without one, nothing.
    let plain = app_without_presets();
    let (s, _, _) = send(&plain, post_json("/api/desktop/launch", json!({}))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn new_folders_are_made_only_under_home_and_never_reused() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join("Pictures")).unwrap();
    fs::create_dir_all(tmp.path().join("outside")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(tmp.path().join("outside"), home.join("out")).unwrap();
    let home = home.canonicalize().unwrap();
    let app = app_with_settings(&home, None);
    let make =
        |path: &str, name: &str| post_json("/api/folders", json!({ "path": path, "name": name }));

    // Made, and listed so the browser can open it.
    let (s, _, b) = send(&app, make("Pictures", "projects")).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    assert_eq!(json_of(&b)["path"], "Pictures/projects");
    assert!(home.join("Pictures/projects").is_dir());

    // A taken name is refused, not reused.
    let (s, _, _) = send(&app, make("Pictures", "projects")).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Names that are not one plain name.
    for bad in ["", "  ", "a/b", "../escape", ".hidden", "a\\b"] {
        let (s, _, _) = send(&app, make("Pictures", bad)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad:?}");
    }
    // Places outside home, directly or through a symlink.
    for path in ["..", "../outside", "out", "missing"] {
        let (s, _, _) = send(&app, make(path, "x")).await;
        assert_ne!(s, StatusCode::OK, "{path:?}");
    }
    assert!(!tmp.path().join("outside/x").exists());
    assert!(!tmp.path().join("x").exists());
}

#[tokio::test]
async fn pull_strokes_travel_through_json_and_toml() {
    let app = app_without_presets();
    let recipe = json!({
        "encoding": "u8",
        "channels": 1,
        "steps": [{
            "effect": "pull",
            "seed": "0",
            "params": { "sampling": "smooth" },
            "pull": [
                { "size": 0.2, "strength": 0.7, "points": [0.3, 0.5, 0.6, 0.5], "brush": 1 },
                { "size": 0.1, "from": [0.4, 0.4], "to": [0.45, 0.4] },
                { "size": 0.3, "strength": 0.5, "points": [0.2, 0.2] }
            ]
        }]
    });
    let (s, _, toml) = send(&app, post_json("/api/recipe/to-toml", recipe.clone())).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&toml));
    let toml = String::from_utf8(toml).unwrap();
    assert!(
        toml.contains("[[step.pull]]") && toml.contains("version = 7"),
        "{toml}"
    );
    let (s, _, back) = send(
        &app,
        Request::post("/api/recipe/from-toml")
            .body(Body::from(toml))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let back = json_of(&back);
    assert_eq!(back["steps"][0]["pull"], recipe["steps"][0]["pull"]);
    // A page from before D-68 sends its pins apart; they run last.
    let mut old = recipe.clone();
    old["steps"][0]["pull"] = json!([recipe["steps"][0]["pull"][0]]);
    old["steps"][0]["pin"] = json!([recipe["steps"][0]["pull"][1]]);
    let (s, _, toml) = send(&app, post_json("/api/recipe/to-toml", old)).await;
    assert_eq!(s, StatusCode::OK);
    let toml = String::from_utf8(toml).unwrap();
    assert_eq!(toml.matches("[[step.pull]]").count(), 2, "{toml}");
    assert!(toml.find("from = ").unwrap() > toml.find("points = ").unwrap());
    // Strokes on another effect, or placement on a pull, are refused.
    let mut bad = recipe.clone();
    bad["steps"][0]["effect"] = json!("echo");
    bad["steps"][0]["params"] = json!({});
    let (s, _, _) = send(&app, post_json("/api/recipe/to-toml", bad)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

/// A picture made from a project is named after the project (as an editor
/// exports poster.psd to poster.jpg), wherever it goes; a new project from
/// it still takes the next -glitch name.
#[tokio::test]
async fn pictures_from_a_project_take_its_name() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join("jpeg")).unwrap();
    let app = app_with_settings(&home, None);
    let photo = upload(&app, "IMG_1.jpg", fixture("baseline-444.jpg")).await;
    let pid = photo["id"].as_str().unwrap();
    let (_, _, proj) = send(
        &app,
        post_json(
            &format!("/api/docs/{pid}/export"),
            json!({"recipe": sample_recipe(), "format": "tiff", "project": true}),
        ),
    )
    .await;
    let doc = upload(&app, "IMG_1-glitch-2.glrx", proj.to_vec()).await;
    let id = doc["id"].as_str().unwrap();
    let suggest = |body: serde_json::Value| {
        let app = app.clone();
        async move {
            let (_, _, b) = send(&app, post_json(&format!("/api/docs/{id}/suggest"), body)).await;
            json_of(&b)["name"].as_str().unwrap().to_string()
        }
    };
    assert_eq!(
        suggest(json!({"folder": "jpeg", "format": "jpeg"})).await,
        "IMG_1-glitch-2"
    );
    fs::write(home.join("jpeg/IMG_1-glitch-2.jpg"), b"x").unwrap();
    assert_eq!(
        suggest(json!({"folder": "jpeg", "format": "jpeg"})).await,
        "IMG_1-glitch-2-1"
    );
    assert_eq!(
        suggest(json!({"folder": "jpeg", "format": "jpeg", "project": true})).await,
        "IMG_1-glitch-3",
        "a new project: the photo's next -glitch number (any extension counts)"
    );
    // Download: the project's name too.
    let (s, h, _) = send(
        &app,
        post_json(
            &format!("/api/docs/{id}/export"),
            json!({"recipe": sample_recipe(), "format": "png"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        h[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains("IMG_1-glitch-2.png")
    );
}

/// Download links: the export waits behind `/api/downloads/{id}/{name}`,
/// named as typed, and the link needs no cookie (Firefox on iOS fetches
/// it outside the page). Everything else still needs the token.
#[tokio::test]
async fn download_links_carry_the_name_and_their_own_secret() {
    let app = app(None, Some("s3cret"));
    let cookie = "glitchrx_token=s3cret";
    let with_cookie = |mut r: Request<Body>| {
        r.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        r
    };
    let (s, _, b) = send(
        &app,
        with_cookie(
            Request::post("/api/docs?name=photo.jpg")
                .body(Body::from(fixture("baseline-444.jpg")))
                .unwrap(),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let id = json_of(&b)["id"].as_str().unwrap().to_string();
    let export = |body: Value| {
        let app = app.clone();
        let req = with_cookie(post_json(&format!("/api/docs/{id}/export"), body));
        async move { send(&app, req).await }
    };

    // The typed name, with the format's extension added.
    let (s, _, b) = export(json!({"recipe": sample_recipe(), "format": "png",
        "file_name": "my shot", "link": true}))
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let link = json_of(&b);
    assert_eq!(link["name"], "my shot.png");
    let url = link["url"].as_str().unwrap().to_string();
    assert!(url.starts_with("/api/downloads/") && url.ends_with("/my%20shot.png"));

    // Fetched without the cookie, twice.
    for _ in 0..2 {
        let (s, h, body) = send(&app, get(&url)).await;
        assert_eq!(s, StatusCode::OK);
        let cd = h[header::CONTENT_DISPOSITION].to_str().unwrap();
        assert!(cd.contains("filename*=UTF-8''my%20shot.png"), "{cd}");
        assert_eq!(h[header::CONTENT_TYPE], "image/png");
        assert!(body.starts_with(b"\x89PNG"));
    }
    // A made-up id is not found; the bare prefix gives nothing away.
    let (s, _, _) = send(
        &app,
        get("/api/downloads/0123456789abcdef0123456789abcdef/x.png"),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Making a link needs the token.
    let (s, _, _) = send(
        &app,
        Request::post("/api/downloads?name=r.recipe.toml")
            .body(Body::from("x"))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // No name typed: the automatic one. A name with folders: refused.
    let (_, _, b) =
        export(json!({"recipe": sample_recipe(), "format": "jpeg", "link": true})).await;
    assert_eq!(json_of(&b)["name"], "photo-glitch.jpg");
    let (s, _, _) = export(json!({"recipe": sample_recipe(), "format": "png",
        "file_name": "../up", "link": true}))
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Suggested for a download (no folder): the same automatic name.
    let (s, _, b) = send(
        &app,
        with_cookie(post_json(
            &format!("/api/docs/{id}/suggest"),
            json!({"format": "png"}),
        )),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json_of(&b)["name"], "photo-glitch");

    // A file made in the page (the recipe) gets a link of its own.
    let (s, _, b) = send(
        &app,
        with_cookie(
            Request::post("/api/downloads?name=photo-glitch.recipe.toml")
                .header(header::CONTENT_TYPE, "application/toml")
                .body(Body::from("version = 5\n"))
                .unwrap(),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let url = json_of(&b)["url"].as_str().unwrap().to_string();
    let (s, h, body) = send(&app, get(&url)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "application/toml");
    assert_eq!(body, b"version = 5\n");
}

/// Save next to original with a typed name: that name, then -1, -2, ...;
/// a recipe beside it must be free too. Nothing is overwritten.
#[tokio::test]
async fn save_next_to_original_takes_a_typed_name() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("pics");
    fs::create_dir_all(root.join("trip")).unwrap();
    fs::write(root.join("trip/photo.jpg"), fixture("baseline-444.jpg")).unwrap();
    // Taken by a recipe alone: the picture name is free but its recipe is not.
    fs::write(root.join("trip/mine-1.recipe.toml"), "x").unwrap();
    let app = app(Some(root.canonicalize().unwrap()), None);
    let (_, _, b) = send(
        &app,
        post_json("/api/docs/open", json!({"path": "trip/photo.jpg"})),
    )
    .await;
    let id = json_of(&b)["id"].as_str().unwrap().to_string();
    let save = json!({"recipe": sample_recipe(), "format": "png",
        "with_recipe": true, "file_name": "mine"});
    let mut saved = vec![];
    for _ in 0..2 {
        let (s, _, b) = send(
            &app,
            post_json(&format!("/api/docs/{id}/save"), save.clone()),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
        saved.push(json_of(&b)["saved"][0].as_str().unwrap().to_string());
    }
    assert_eq!(saved, ["trip/mine.png", "trip/mine-2.png"]);
    assert_eq!(
        fs::read_to_string(root.join("trip/mine-1.recipe.toml")).unwrap(),
        "x"
    );
}

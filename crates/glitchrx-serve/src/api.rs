//! HTTP routes.
//!
//! Everything under `/api` is JSON in, JSON or image bytes out. Image work
//! (decoding, bending, encoding) is CPU-bound, so it runs on tokio's
//! blocking pool and never stalls the async workers.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use glitchrx_core::codec;
use glitchrx_core::io::{
    WriteError, auto_output, base_stem, project_output, write_output, write_project,
};
use glitchrx_core::project::{self, Project};
use glitchrx_core::recipe::NEVER;
use glitchrx_core::{Error, Raster, Recipe};
use serde::{Deserialize, Serialize};

use crate::browse::{self, BrowseError};
use crate::dialogs::Dialogs;
use crate::docs::{Doc, Store, random_hex};
use crate::downloads::Downloads;
use crate::dto::{self, DocDto, RecipeDto};
use crate::presets::{PresetError, PresetInfo, Presets};
use crate::recipes;
use crate::render::{self, Format};
use crate::settings::{Settings, SettingsStore};

/// Largest upload accepted. Full-frame camera JPEGs are 10-40 MB, and
/// uncompressed TIFFs of the same photos 70-150 MB.
const MAX_UPLOAD: usize = 256 << 20;
/// How many photos stay open at once (least recently used is closed).
const MAX_DOCS: usize = 4;
/// Largest file the page itself may put behind a download link (a recipe).
const MAX_STASH: usize = 4 << 20;
/// Name of the cookie that carries the access token.
const TOKEN_COOKIE: &str = "glitchrx_token";

/// Shared server state.
pub struct AppState {
    pub store: Store,
    /// The canonical folder Open browses and Save next to original writes
    /// in: `--root`, else the library folder from Settings. None turns
    /// server browsing off.
    root: RwLock<Option<PathBuf>>,
    /// The root came from `--root` (or the desktop app), so Settings does
    /// not change it.
    root_fixed: bool,
    /// The library and export folders; off unless a settings file is given
    /// (tests never touch the user's real one).
    pub settings: Option<SettingsStore>,
    /// Shared access token, if one is required.
    pub token: Option<String>,
    /// Time limit for a live preview; an exact preview gets ten times this.
    pub preview_budget: Duration,
    /// Native file dialogs, when running inside the desktop app.
    pub dialogs: Option<Arc<dyn Dialogs>>,
    /// Saved presets; off unless a directory is given (tests never touch
    /// the user's real one).
    pub presets: Option<Presets>,
    /// The desktop app was started to open this file (a double-clicked
    /// project or photo). The page opens it once, at startup.
    launch: std::sync::Mutex<Option<PathBuf>>,
    /// Finished exports waiting behind a download link (see `downloads`).
    pub downloads: Downloads,
}

impl AppState {
    pub fn new(root: Option<PathBuf>, token: Option<String>) -> AppState {
        AppState {
            store: Store::new(MAX_DOCS),
            root_fixed: root.is_some(),
            root: RwLock::new(root),
            settings: None,
            token,
            preview_budget: PREVIEW_BUDGET,
            dialogs: None,
            presets: None,
            launch: std::sync::Mutex::new(None),
            downloads: Downloads::default(),
        }
    }

    /// Open `path` when the page starts (the desktop app, started from a
    /// file manager with a file).
    pub fn with_launch_file(self, path: PathBuf) -> AppState {
        *self.launch.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
        self
    }

    /// Keep presets in `dir` (created on the first save).
    pub fn with_presets(mut self, dir: PathBuf) -> AppState {
        self.presets = Some(Presets::new(dir));
        self
    }

    /// Keep the library and export folders in `store`. Without a fixed
    /// root, its library becomes the root.
    pub fn with_settings(mut self, store: SettingsStore) -> AppState {
        if !self.root_fixed {
            self.set_root(store.get().library);
        }
        self.settings = Some(store);
        self
    }

    /// The folder Open browses, if any.
    pub fn current_root(&self) -> Option<PathBuf> {
        self.root.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_root(&self, root: Option<PathBuf>) {
        *self.root.write().unwrap_or_else(|e| e.into_inner()) = root;
    }

    /// Enable the native Open and Save dialogs (the desktop app).
    pub fn with_dialogs(mut self, dialogs: Arc<dyn Dialogs>) -> AppState {
        self.dialogs = Some(dialogs);
        self
    }
}

type Shared = Arc<AppState>;

/// Build the router (also used directly by the tests).
pub fn router(state: AppState) -> Router {
    let state = Arc::new(state);
    let api = Router::new()
        .route("/config", get(config))
        .route("/effects", get(effects))
        .route("/docs", post(upload))
        .route("/docs/open", post(open))
        .route("/docs/{id}", get(doc_info))
        .route("/docs/{id}/preview", post(preview))
        .route("/docs/{id}/mask", post(mask_view))
        .route("/docs/{id}/histogram", post(histogram))
        .route("/docs/{id}/export", post(export))
        .route("/docs/{id}/save", post(save))
        .route("/docs/{id}/save-as", post(save_as))
        .route("/docs/{id}/save-to", post(save_to))
        .route("/docs/{id}/suggest", post(suggest_name))
        .route("/downloads", post(stash_download))
        .route("/downloads/{id}/{name}", get(fetch_download))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/folders", get(folders).post(new_folder))
        .route("/desktop/open", post(desktop_open))
        .route("/desktop/launch", post(desktop_launch))
        .route("/presets", get(list_presets))
        .route(
            "/presets/{name}",
            get(get_preset).put(put_preset).delete(delete_preset),
        )
        .route("/recipe/from-toml", post(recipe_from_toml))
        .route("/recipes", get(list_recipes))
        .route("/recipe", get(read_recipe))
        .route("/recipe/to-toml", post(recipe_to_toml))
        .route("/browse", get(browse_dir))
        .route("/thumb", get(thumb))
        .fallback(|| async { ApiError::new(StatusCode::NOT_FOUND, "no such API route") });
    Router::new()
        .nest("/api", api)
        .fallback(crate::assets::serve)
        .layer(DefaultBodyLimit::max(MAX_UPLOAD))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Errors

/// An error answered as `{"error": "..."}` with a status code.
#[derive(Debug)]
pub struct ApiError(StatusCode, String);

impl ApiError {
    fn new(code: StatusCode, msg: impl Into<String>) -> ApiError {
        ApiError(code, msg.into())
    }
    fn bad(msg: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::BAD_REQUEST, msg)
    }
    fn internal(msg: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        #[derive(Serialize)]
        struct Body {
            error: String,
        }
        (self.0, Json(Body { error: self.1 })).into_response()
    }
}

impl From<Error> for ApiError {
    fn from(e: Error) -> ApiError {
        let code = match e {
            Error::UnknownFormat | Error::Unsupported(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Error::Malformed(_) | Error::Project(_) => StatusCode::UNPROCESSABLE_ENTITY,
            _ => StatusCode::BAD_REQUEST,
        };
        ApiError(code, e.to_string())
    }
}

impl From<BrowseError> for ApiError {
    fn from(e: BrowseError) -> ApiError {
        match e {
            BrowseError::NotFound => ApiError::new(StatusCode::NOT_FOUND, "not found"),
            BrowseError::Io(m) => ApiError::internal(m),
        }
    }
}

impl From<PresetError> for ApiError {
    fn from(e: PresetError) -> ApiError {
        let code = match e {
            PresetError::BadName(_) | PresetError::Invalid(_) => StatusCode::BAD_REQUEST,
            PresetError::NotFound(_) => StatusCode::NOT_FOUND,
            PresetError::Exists(_) => StatusCode::CONFLICT,
            PresetError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError::new(code, e.to_string())
    }
}

type ApiResult<T> = Result<T, ApiError>;

/// Run CPU-heavy work off the async workers.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::internal(format!("worker failed: {e}")))?
}

fn find(st: &AppState, id: &str) -> ApiResult<Arc<Doc>> {
    st.store.get(id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "this photo is no longer open on the server; open it again",
        )
    })
}

fn root(st: &AppState) -> ApiResult<PathBuf> {
    st.current_root().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "server browsing is off (choose a library folder in Settings, or start with --root DIR)",
        )
    })
}

fn doc_dto(st: &AppState, doc: &Doc) -> DocDto {
    // Only a photo inside the current root can be saved next to.
    let path = match (st.current_root(), &doc.source) {
        (Some(r), Some(s)) if s.starts_with(&r) => Some(browse::relative(&r, s)),
        _ => None,
    };
    let mut dto = DocDto::new(&doc.id, &doc.name, path, doc.file_len, &doc.image);
    dto.project = doc.project.clone();
    // A project brings its own recipe: nothing to offer to continue from.
    if doc.project.is_none() {
        dto.recipes = recipes::scan(&recipe_roots(st))
            .into_iter()
            .filter(|f| recipes::matches(f, &doc.name))
            .collect();
    }
    dto
}

/// The folders recipes are looked for in: the library (or `--root`) and
/// the export folder, when they are set.
fn recipe_roots(st: &AppState) -> Vec<(recipes::Place, PathBuf)> {
    let mut roots = Vec::new();
    if let Some(r) = st.current_root() {
        roots.push((recipes::Place::Library, r));
    }
    if let Some(e) = st
        .settings
        .as_ref()
        .and_then(|s| s.get().export)
        .and_then(|p| p.canonicalize().ok())
    {
        roots.push((recipes::Place::Export, e));
    }
    roots
}

/// The recipes saved in the library and export folders, newest first.
async fn list_recipes(State(st): State<Shared>) -> ApiResult<Json<Vec<recipes::Found>>> {
    let roots = recipe_roots(&st);
    blocking(move || Ok(Json(recipes::scan(&roots)))).await
}

#[derive(Deserialize)]
struct RecipeQuery {
    place: recipes::Place,
    path: String,
}

/// One saved recipe, read for the GUI. Only `.recipe.toml` files inside
/// the library or export folder.
async fn read_recipe(
    State(st): State<Shared>,
    Query(q): Query<RecipeQuery>,
) -> ApiResult<Json<RecipeDto>> {
    let root = recipe_roots(&st)
        .into_iter()
        .find(|(p, _)| *p == q.place)
        .map(|(_, r)| r)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "that folder is not set"))?;
    blocking(move || {
        let not_found = || ApiError::new(StatusCode::NOT_FOUND, "no such recipe");
        let path = browse::resolve(&root, &q.path).map_err(|_| not_found())?;
        let ok = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(recipes::SUFFIX));
        let meta = std::fs::metadata(&path).map_err(|_| not_found())?;
        if !ok || !meta.is_file() || meta.len() > recipes::MAX_BYTES {
            return Err(not_found());
        }
        let text = std::fs::read_to_string(&path).map_err(|_| not_found())?;
        let r = Recipe::from_toml(&text)?;
        Ok(Json(RecipeDto::from_recipe(&r)))
    })
    .await
}

// ---------------------------------------------------------------------------
// Access token

/// Constant-time comparison, so the token cannot be guessed byte by byte
/// from response timings.
pub(crate) fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == TOKEN_COOKIE)
        .map(|(_, v)| v.to_string())
}

fn query_token(req: &Request) -> Option<String> {
    req.uri()
        .query()?
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == "token")
        .map(|(_, v)| v.to_string())
}

/// Files that need no token: the web app manifest and its icons. Browsers
/// fetch a manifest without cookies, and iOS fetches the home-screen icon
/// on its own, so these must be public. They are static and say nothing
/// about the photos.
/// Where download links live: `/api/downloads/{id}/{name}`.
const DOWNLOAD_PREFIX: &str = "/api/downloads/";

const PUBLIC: &[&str] = &[
    "/manifest.webmanifest",
    "/favicon.svg",
    "/icon.svg",
    "/icon-192.png",
    "/icon-512.png",
    "/icon-maskable-512.png",
    "/apple-touch-icon.png",
];

/// The page a browser gets instead of the app when it has no token: one
/// field to paste it into. Submitting loads `?token=...`, which sets the
/// cookie like the printed URL. This matters for an app added to the iOS
/// home screen, which may start with its own, empty cookie jar. No script,
/// and `action=""` posts back to the same page, so nothing from the request
/// is echoed into the HTML.
const SIGN_IN: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="color-scheme" content="dark light">
<title>glitchrx: sign in</title>
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<style>
:root{--bg:#f4efe0;--card:#fbf7ec;--fg:#3c3836;--dim:#928374;--fill:rgb(60 56 54/.07);--accent:#79740e;--on:#fbf7ec}
@media (prefers-color-scheme:dark){:root{--bg:#1d2021;--card:#282828;--fg:#ebdbb2;--fill:rgb(235 219 178/.08);--accent:#b8bb26;--on:#1d2021}}
*{box-sizing:border-box}
body{margin:0;min-height:100dvh;display:grid;place-items:center;padding:24px;background:var(--bg);color:var(--fg);
font:15px/1.4 -apple-system,BlinkMacSystemFont,system-ui,'Segoe UI',Roboto,sans-serif;-webkit-font-smoothing:antialiased}
main{width:min(360px,100%);background:var(--card);border-radius:14px;padding:24px 20px;text-align:center}
img{width:64px;height:64px;border-radius:14px}
h1{font:400 1.4rem Georgia,serif;margin:12px 0 4px}
p{color:var(--dim);margin:0 0 18px;font-size:.9rem}
code{white-space:nowrap}
input,button{font:inherit;width:100%;min-height:44px;border:none;border-radius:10px}
input{background:var(--fill);color:var(--fg);padding:0 12px;font-size:16px}
button{margin-top:10px;background:var(--accent);color:var(--on);font-weight:600}
</style></head>
<body><main>
<img src="/apple-touch-icon.png" alt="">
<h1>glitchrx</h1>
<p>Paste the access token: the part after <code>?token=</code> in the address <code>glitchrx serve</code> printed.</p>
<form method="get" action="">
<input name="token" type="password" autocomplete="current-password" autocapitalize="off" spellcheck="false" placeholder="Access token" required autofocus>
<button type="submit">Open</button>
</form>
</main></body></html>
"#;

/// Whether the request is a browser loading a page (not a script call).
fn wants_html(req: &Request) -> bool {
    req.method() == axum::http::Method::GET
        && req
            .headers()
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|a| a.contains("text/html"))
}

/// When a token is configured, every request needs it: either as the
/// cookie, or once as `?token=...` (the URL printed at startup), which sets
/// the cookie and redirects to the clean URL. The manifest and icons are
/// exempt (`PUBLIC`). Also adds security headers to every response.
async fn guard(State(st): State<Shared>, req: Request, next: Next) -> Response {
    let mut res = check_token(&st, req, next).await;
    let h = res.headers_mut();
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // Everything the page needs comes from this server; the CSP makes that
    // a browser-enforced promise (no fonts, scripts or calls from elsewhere).
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' blob: data:; style-src 'self' 'unsafe-inline'; \
             object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    res
}

async fn check_token(st: &Shared, req: Request, next: Next) -> Response {
    if let Some(token) = &st.token {
        let from_cookie = cookie_token(req.headers()).is_some_and(|t| same_secret(&t, token));
        // A download link carries its own secret (see `downloads`), so a
        // download manager that fetches it without the cookie still can.
        let link = req.method() == axum::http::Method::GET
            && req.uri().path().starts_with(DOWNLOAD_PREFIX);
        let public = link || PUBLIC.contains(&req.uri().path());
        if !from_cookie && !public {
            if query_token(&req).is_some_and(|t| same_secret(&t, token)) {
                let cookie = format!(
                    "{TOKEN_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=31536000"
                );
                let mut res = Response::new(Body::empty());
                *res.status_mut() = StatusCode::SEE_OTHER;
                let location = req.uri().path().to_string();
                res.headers_mut().insert(
                    header::LOCATION,
                    HeaderValue::from_str(&location).unwrap_or(HeaderValue::from_static("/")),
                );
                if let Ok(v) = HeaderValue::from_str(&cookie) {
                    res.headers_mut().insert(header::SET_COOKIE, v);
                }
                return res;
            }
            if wants_html(&req) {
                return (
                    StatusCode::UNAUTHORIZED,
                    [
                        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                        (header::CACHE_CONTROL, "no-store"),
                    ],
                    SIGN_IN,
                )
                    .into_response();
            }
            return (
                StatusCode::UNAUTHORIZED,
                "glitchrx: access token required. Open the URL printed by `glitchrx serve` \
                 (it ends in ?token=...).\n",
            )
                .into_response();
        }
    }
    next.run(req).await
}

// ---------------------------------------------------------------------------
// Handlers

#[derive(Serialize)]
struct ConfigDto {
    version: &'static str,
    /// Whether server-side browsing (`--root`) is available.
    browse: bool,
    /// Name of the root directory, for display.
    root_name: Option<String>,
    max_preview_side: u32,
    /// Sample encodings for the encoding pickers.
    encodings: Vec<dto::EncodingDto>,
    /// Running inside the desktop app: native Open and Save dialogs.
    desktop: bool,
    /// Saved presets are available.
    presets: bool,
    /// The Settings sheet can choose library and export folders.
    settings: bool,
}

async fn config(State(st): State<Shared>) -> Json<ConfigDto> {
    let root = st.current_root();
    Json(ConfigDto {
        version: env!("CARGO_PKG_VERSION"),
        browse: root.is_some(),
        root_name: root.as_ref().map(|r| {
            r.file_name().map_or_else(
                || r.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        }),
        max_preview_side: render::MAX_PREVIEW_SIDE,
        encodings: dto::encodings(),
        desktop: st.dialogs.is_some(),
        presets: st.presets.is_some(),
        settings: st.settings.is_some(),
    })
}

async fn effects() -> Json<Vec<dto::EffectDto>> {
    Json(dto::effects())
}

#[derive(Deserialize)]
struct UploadQuery {
    name: Option<String>,
}

/// Upload a photo as the raw request body (`?name=photo.jpg`).
async fn upload(
    State(st): State<Shared>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> ApiResult<Json<DocDto>> {
    let name = q
        .name
        .map(|n| sanitize_name(&n))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "photo.jpg".into());
    let st2 = st.clone();
    blocking(move || Ok(Json(open_doc(&st2, name, None, &body)?))).await
}

/// Open a photo or a project and answer with its description; a project's
/// recipe comes along, for the GUI to load.
fn open_doc(
    st: &AppState,
    file_name: String,
    source: Option<PathBuf>,
    bytes: &[u8],
) -> ApiResult<DocDto> {
    let (doc, recipe) = Doc::open(random_hex(8), file_name, source, bytes)?;
    let doc = st.store.insert(doc);
    let mut dto = doc_dto(st, &doc);
    dto.recipe = recipe.as_ref().map(RecipeDto::from_recipe);
    Ok(dto)
}

/// Keep only the final path component and drop control characters, so a
/// name from the browser can only ever name a download, never a path.
fn sanitize_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("");
    last.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

#[derive(Deserialize)]
struct OpenRequest {
    path: String,
}

/// Open a photo or a project from `--root`.
async fn open(State(st): State<Shared>, Json(req): Json<OpenRequest>) -> ApiResult<Json<DocDto>> {
    let path = browse::resolve(&root(&st)?, &req.path)?;
    if !path.is_file() || !browse::is_openable(&path) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "not a photo"));
    }
    let st2 = st.clone();
    blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| ApiError::internal(e.to_string()))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Json(open_doc(&st2, name, Some(path), &bytes)?))
    })
    .await
}

async fn doc_info(State(st): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<DocDto>> {
    let doc = find(&st, &id)?;
    Ok(Json(doc_dto(&st, &doc)))
}

#[derive(Deserialize)]
struct PreviewRequest {
    #[serde(default)]
    recipe: RecipeDto,
    /// Box to cover, in device pixels.
    max_w: u32,
    max_h: u32,
    /// Show the untouched original instead (before/after).
    #[serde(default)]
    original: bool,
    /// Bend at full resolution, then shrink: exactly the export, slower.
    #[serde(default)]
    exact: bool,
    /// Only this part of the picture (a zoomed view). Always exact: a crop
    /// is cut from the full-resolution render, since effects run along the
    /// whole raster and no part can be bent on its own.
    #[serde(default)]
    crop: Option<Crop>,
}

/// Part of the picture, as fractions of its width and height.
#[derive(Deserialize, Clone, Copy, Debug)]
struct Crop {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Crop {
    /// The whole pixels covering it in a `width` x `height` picture, as
    /// `(x, y, w, h)`: at least one pixel, never past the edges.
    fn pixels(&self, width: usize, height: usize) -> ApiResult<(usize, usize, usize, usize)> {
        let finite = [self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite());
        if !finite || self.w <= 0.0 || self.h <= 0.0 {
            return Err(ApiError::bad(
                "a crop needs a finite position and a positive size",
            ));
        }
        // Outward to whole pixels, so the crop covers everything asked for.
        let span = |start: f64, len: f64, n: usize| {
            let s = ((start.clamp(0.0, 1.0) * n as f64).floor() as usize).min(n - 1);
            let e = (((start + len).clamp(0.0, 1.0) * n as f64).ceil() as usize).clamp(s + 1, n);
            (s, e - s)
        };
        let (x, w) = span(self.x, self.w, width);
        let (y, h) = span(self.y, self.h, height);
        Ok((x, y, w, h))
    }
}

/// How long a live preview may run before it gives up (SPEC 9.2). Export
/// has no limit; an exact preview gets longer, since it was asked for.
pub const PREVIEW_BUDGET: Duration = Duration::from_secs(3);

/// Render a preview. Answers `image/jpeg` with timing in `X-Render-Ms`, or
/// 204 when a newer request for the same photo has arrived (latest wins):
/// a render in progress notices within a block of samples and stops. A
/// render over its time budget stops the same way and answers 503 with
/// `X-Too-Heavy: 1`, so the GUI can say so instead of showing an error.
async fn preview(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<PreviewRequest>,
) -> ApiResult<Response> {
    let doc = find(&st, &id)?;
    // The recipe comes with an original request too: its work size decides
    // the canvas, so before and after show the same pixels.
    let framing = req.recipe.to_recipe()?;
    let (cw, ch) = {
        let c = doc.canvas(&framing);
        (c.width, c.height)
    };
    // The picture shown: the canvas, or the part a crop of the result
    // frames (D-69). A zoomed crop is a part of that.
    let frame = framing.result_rect(cw, ch).unwrap_or((0, 0, cw, ch));
    let crop = match req.crop {
        Some(c) => Some(c.pixels(frame.2, frame.3)?),
        None => None,
    };
    let recipe = (!req.original).then(|| framing.clone());
    // Only bend renders compete; an original (before/after) request must
    // not cancel the render it is being compared with. Crops compete only
    // with crops.
    let zoom = crop.is_some();
    let generation = recipe
        .as_ref()
        .map(|_| if zoom { doc.begin_zoom() } else { doc.begin() });
    let current = move |doc: &Doc, g: u64| {
        if zoom {
            doc.is_current_zoom(g)
        } else {
            doc.is_current(g)
        }
    };
    let max_w = req.max_w.clamp(16, render::MAX_PREVIEW_SIDE) as usize;
    let max_h = req.max_h.clamp(16, render::MAX_PREVIEW_SIDE) as usize;
    let exact = req.exact || zoom;
    let budget = st.preview_budget * if exact { 10 } else { 1 };
    // Cut the frame and the crop (if any) from a full-resolution canvas,
    // then shrink it.
    let shown = move |full: &Raster| {
        let (fx, fy, fw, fh) = frame;
        match crop {
            Some((x, y, w, h)) => full.crop(fx + x, fy + y, w, h).downscale(max_w, max_h),
            None => {
                let framed;
                let full = if (fw, fh) == (full.width, full.height) {
                    full
                } else {
                    framed = full.crop(fx, fy, fw, fh);
                    &framed
                };
                let (w, h) = full.fit(max_w, max_h);
                full.resize(w, h)
            }
        }
    };
    blocking(move || {
        let t0 = Instant::now();
        let raster = match (&recipe, generation) {
            (Some(r), Some(g)) => {
                let stop = || !current(&doc, g) || t0.elapsed() > budget;
                let result = if exact {
                    doc.render_full(r, &stop).map(|full| shown(&full))
                } else {
                    doc.preview_framed(r, max_w, max_h, &stop)
                };
                match result {
                    Ok(raster) => raster,
                    Err(Error::Cancelled) if !current(&doc, g) => {
                        return Ok(StatusCode::NO_CONTENT.into_response());
                    }
                    Err(Error::Cancelled) => {
                        let mut res = ApiError::new(
                            StatusCode::SERVICE_UNAVAILABLE,
                            format!(
                                "too heavy to preview {}(over {} s); export still works",
                                if exact { "exactly " } else { "live " },
                                budget.as_secs()
                            ),
                        )
                        .into_response();
                        res.headers_mut()
                            .insert("x-too-heavy", HeaderValue::from_static("1"));
                        return Ok(res);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            _ => shown(&doc.canvas(&framing)),
        };
        let jpeg = render::preview_jpeg(&raster, 88).map_err(ApiError::internal)?;
        let ms = t0.elapsed().as_millis();
        let mut res = (
            [
                (header::CONTENT_TYPE, "image/jpeg".to_string()),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
            jpeg,
        )
            .into_response();
        let h = res.headers_mut();
        let mut set = |k: &'static str, v: String| {
            if let Ok(v) = HeaderValue::from_str(&v) {
                h.insert(k, v);
            }
        };
        set("x-render-ms", ms.to_string());
        set("x-width", raster.width.to_string());
        set("x-height", raster.height.to_string());
        if let Some((x, y, w, h)) = crop {
            // The full-resolution pixels it shows, so the GUI can place it
            // (of the picture shown: within the frame of a cropped result).
            set("x-crop", format!("{x},{y},{w},{h}"));
        }
        Ok(res)
    })
    .await
}

#[derive(Deserialize)]
struct MaskRequest {
    recipe: RecipeDto,
    /// Which step's mask, from 0.
    step: usize,
    max_w: u32,
    max_h: u32,
    /// Overlay color; the GUI passes its theme's.
    #[serde(default = "mask_tint")]
    tint: [u8; 3],
}

fn mask_tint() -> [u8; 3] {
    [204, 36, 29]
}

/// Show a step's mask: an RGBA PNG at preview size, the tint with the
/// mask's coverage as alpha. It reuses the preview's cache for the picture
/// arriving at the step, but does not take part in the preview's
/// latest-wins rule: the GUI asks for both at once after each stroke, and
/// they must not cancel each other.
async fn mask_view(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<MaskRequest>,
) -> ApiResult<Response> {
    let doc = find(&st, &id)?;
    let recipe = req.recipe.to_recipe()?;
    let max_w = req.max_w.clamp(16, render::MAX_PREVIEW_SIDE) as usize;
    let max_h = req.max_h.clamp(16, render::MAX_PREVIEW_SIDE) as usize;
    let budget = st.preview_budget;
    blocking(move || {
        let t0 = Instant::now();
        let stop = || t0.elapsed() > budget;
        let (w, h, coverage) = match doc.mask(&recipe, req.step, max_w, max_h, &stop) {
            Ok(m) => m,
            Err(Error::Cancelled) => {
                return Err(ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "too heavy to show the mask live".to_string(),
                ));
            }
            Err(e) => return Err(e.into()),
        };
        let png = render::mask_png(w, h, &coverage, req.tint).map_err(ApiError::internal)?;
        Ok((
            [
                (header::CONTENT_TYPE, "image/png".to_string()),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
            png,
        )
            .into_response())
    })
    .await
}

#[derive(Deserialize)]
struct HistogramRequest {
    recipe: RecipeDto,
    /// The step whose input is measured, from 0.
    step: usize,
    max_w: u32,
    max_h: u32,
}

#[derive(Serialize)]
struct Histogram {
    red: Vec<u32>,
    green: Vec<u32>,
    blue: Vec<u32>,
    luma: Vec<u32>,
}

/// The picture as it reaches a step, as histograms (for the curves editor).
async fn histogram(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<HistogramRequest>,
) -> ApiResult<Json<Histogram>> {
    let doc = find(&st, &id)?;
    let recipe = req.recipe.to_recipe()?;
    let max_w = req.max_w.clamp(16, render::MAX_PREVIEW_SIDE) as usize;
    let max_h = req.max_h.clamp(16, render::MAX_PREVIEW_SIDE) as usize;
    let budget = st.preview_budget;
    blocking(move || {
        let t0 = Instant::now();
        let stop = || t0.elapsed() > budget;
        match doc.histogram(&recipe, req.step, max_w, max_h, &stop) {
            Ok([red, green, blue, luma]) => Ok(Json(Histogram {
                red,
                green,
                blue,
                luma,
            })),
            Err(Error::Cancelled) => Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "too heavy to measure live".to_string(),
            )),
            Err(e) => Err(e.into()),
        }
    })
    .await
}

#[derive(Deserialize)]
struct ExportRequest {
    recipe: RecipeDto,
    format: Format,
    /// JPEG, lossy WebP and AVIF quality, 1..=100 (default per format).
    #[serde(default)]
    quality: Option<u8>,
    /// WebP: lossless.
    #[serde(default)]
    lossless: bool,
    /// AVIF: 1 (slowest, smallest) to 10 (fastest).
    #[serde(default)]
    speed: Option<u8>,
    /// Save modes only: also write the recipe as TOML next to the output.
    #[serde(default)]
    with_recipe: bool,
    /// A project file (`.glrx`, D-63) instead of a picture: the original,
    /// the recipe and a preview. `format` and the encoder settings are
    /// then unused.
    #[serde(default)]
    project: bool,
    /// Save next to the original: write the project over the one the photo
    /// was opened from. Save As into a folder: update the project already
    /// there under that name. The GUI asks first either way.
    #[serde(default)]
    replace: bool,
    /// Download and Save next to original: the name typed in the Export
    /// sheet, without folders (the extension is added). None keeps the
    /// automatic name.
    #[serde(default)]
    file_name: Option<String>,
    /// Download only: hold the file behind a download link and answer with
    /// the link, instead of the bytes (see `downloads`).
    #[serde(default)]
    link: bool,
}

impl ExportRequest {
    /// The file extension of what is written.
    fn ext(&self) -> &'static str {
        if self.project {
            project::EXTENSION
        } else {
            codec::Format::from(self.format).extension()
        }
    }

    fn mime(&self) -> &'static str {
        if self.project {
            project::MIMETYPE
        } else {
            codec::Format::from(self.format).mime()
        }
    }
}

impl ExportRequest {
    fn settings(&self) -> ApiResult<codec::EncodeSettings> {
        if self.quality.is_some_and(|q| !(1..=100).contains(&q)) {
            return Err(ApiError::bad("quality must be 1 to 100"));
        }
        if self.speed.is_some_and(|q| !(1..=10).contains(&q)) {
            return Err(ApiError::bad("speed must be 1 to 10"));
        }
        Ok(codec::EncodeSettings {
            quality: self.quality,
            lossless: self.lossless,
            speed: self.speed,
        })
    }
}

/// File name for a download: `photo.jpg` -> `photo-glitch.tif`.
/// The download name for an export of `doc`. A picture made from a
/// project takes the project's name (see [`project_output`]); everything
/// else gets the photo's `-glitch` name.
fn download_name(doc: &Doc, project: bool, ext: &str) -> String {
    match &doc.project {
        Some(p) if !project => {
            let stem = std::path::Path::new(p)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("{stem}.{ext}")
        }
        _ => export_name(&doc.name, ext),
    }
}

fn export_name(doc_name: &str, ext: &str) -> String {
    let stem = std::path::Path::new(doc_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{}-glitch.{ext}", base_stem(&stem))
}

/// What an export writes: the picture, or a project holding the original,
/// the recipe and a preview of the same render.
fn output_bytes(
    doc: &Doc,
    recipe: &Recipe,
    req: &ExportRequest,
    settings: &codec::EncodeSettings,
) -> ApiResult<Vec<u8>> {
    if !req.project {
        return export_bytes(doc, recipe, req.format, settings);
    }
    let bent = doc.render_full(recipe, NEVER)?;
    let full = (doc.image.raster.width, doc.image.raster.height);
    let raster = recipe.finish(bent.as_ref().clone(), full);
    let preview = project::preview(&raster, doc.image.icc.as_deref())?;
    Ok(Project::new(recipe.clone(), &doc.name, &doc.file, Some(preview))?.to_bytes()?)
}

/// Bend the full image and encode it, keeping the color profile.
fn export_bytes(
    doc: &Doc,
    recipe: &Recipe,
    format: Format,
    settings: &codec::EncodeSettings,
) -> ApiResult<Vec<u8>> {
    let bent = doc.render_full(recipe, NEVER)?;
    let full = (doc.image.raster.width, doc.image.raster.height);
    let raster = recipe.finish(bent.as_ref().clone(), full);
    Ok(codec::encode_with(
        &raster,
        format.into(),
        doc.image.icc.as_deref(),
        settings,
    )?)
}

/// `Content-Disposition` for a download named `name`. RFC 6266: an ASCII
/// fallback plus the exact UTF-8 name.
fn attachment(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
        percent_encode(name)
    )
}

#[derive(Serialize)]
struct Link {
    /// Fetch this (a plain GET) to download the file.
    url: String,
    name: String,
}

/// Hold a finished file behind a download link.
fn link_to(st: &AppState, name: String, mime: String, bytes: Vec<u8>) -> Link {
    let id = st.downloads.put(name.clone(), mime, bytes);
    Link {
        url: format!("{DOWNLOAD_PREFIX}{id}/{}", percent_encode(&name)),
        name,
    }
}

/// Full-resolution export as a download. Runs the recipe on the full image
/// with [`Recipe::apply`], the same call the CLI makes. With `link`, the
/// answer is a download link (JSON) rather than the file.
async fn export(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<ExportRequest>,
) -> ApiResult<Response> {
    let doc = find(&st, &id)?;
    let recipe = req.recipe.to_recipe()?;
    let settings = req.settings()?;
    let name = match &req.file_name {
        Some(n) => file_name(n, req.ext())?,
        None => download_name(&doc, req.project, req.ext()),
    };
    blocking(move || {
        let bytes = output_bytes(&doc, &recipe, &req, &settings)?;
        if req.link {
            let link = link_to(&st, name, req.mime().to_string(), bytes);
            return Ok(Json(link).into_response());
        }
        Ok((
            [
                (header::CONTENT_TYPE, req.mime().to_string()),
                (header::CONTENT_DISPOSITION, attachment(&name)),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
            bytes,
        )
            .into_response())
    })
    .await
}

#[derive(Deserialize)]
struct StashQuery {
    name: String,
}

/// Small files made in the page (a recipe) get a download link too: the
/// body is the file, `?name=` its name, and its type is kept.
async fn stash_download(
    State(st): State<Shared>,
    Query(q): Query<StashQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Link>> {
    if body.len() > MAX_STASH {
        return Err(ApiError::bad("too large for a download link"));
    }
    let clean = sanitize_name(&q.name);
    if clean.is_empty() || clean.starts_with('.') || clean != q.name.trim() {
        return Err(ApiError::bad(
            "a file name, without folders, not starting with a dot",
        ));
    }
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    Ok(Json(link_to(&st, clean, mime, body.to_vec())))
}

/// Fetch a download link. Needs no token: the id is the secret.
async fn fetch_download(
    State(st): State<Shared>,
    Path((id, _name)): Path<(String, String)>,
) -> ApiResult<Response> {
    let Some(file) = st.downloads.get(&id) else {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "this download link has expired; export again",
        ));
    };
    Ok((
        [
            (header::CONTENT_TYPE, file.mime),
            (header::CONTENT_DISPOSITION, attachment(&file.name)),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        file.bytes.as_ref().clone(),
    )
        .into_response())
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[derive(Serialize)]
struct Saved {
    /// Root-relative paths of the files written.
    saved: Vec<String>,
}

/// Save next to the original on the server (photos opened from `--root`
/// only), with the CLI's automatic naming. Never overwrites, never touches
/// the original.
async fn save(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<ExportRequest>,
) -> ApiResult<Json<Saved>> {
    let root = root(&st)?;
    let doc = find(&st, &id)?;
    let Some(source) = doc.source.clone() else {
        return Err(ApiError::bad(
            "this photo was uploaded from the browser; use download instead",
        ));
    };
    let recipe = req.recipe.to_recipe()?;
    let settings = req.settings()?;
    if req.replace && (!req.project || doc.project.is_none()) {
        return Err(ApiError::bad(
            "only a project can be saved over the one it was opened from",
        ));
    }
    // Checked before the long render.
    let typed = req
        .file_name
        .as_deref()
        .map(|n| file_name(n, req.ext()))
        .transpose()?;
    blocking(move || {
        let bytes = output_bytes(&doc, &recipe, &req, &settings)?;
        let conflict = |e: WriteError| ApiError::new(StatusCode::CONFLICT, e.to_string());
        if req.replace {
            // Over the project itself (D-63); the GUI asked first.
            write_project(&source, &source, &bytes, true).map_err(conflict)?;
            return Ok(Json(Saved {
                saved: vec![browse::relative(&root, &source)],
            }));
        }
        // A name typed in the sheet, with -1, -2, ... if taken; else a
        // picture from a project is named after the project, and anything
        // else takes the next free -glitch name.
        let with_recipe = req.with_recipe && !req.project;
        let out = if let Some(typed) = typed {
            let dir = source.parent().unwrap_or(std::path::Path::new("."));
            free_name(dir, &typed, with_recipe)
        } else if doc.project.is_some() && !req.project {
            project_output(&source, req.ext())
        } else {
            auto_output(&source, req.ext()).map_err(|e| ApiError::internal(e.to_string()))?
        };
        let write = |path: &std::path::Path, data: &[u8]| {
            write_output(&source, path, data, false).map_err(conflict)
        };
        write(&out, &bytes)?;
        let mut saved = vec![browse::relative(&root, &out)];
        if with_recipe {
            let rpath = out.with_extension("recipe.toml");
            write(&rpath, recipe.to_toml().as_bytes())?;
            saved.push(browse::relative(&root, &rpath));
        }
        Ok(Json(Saved { saved }))
    })
    .await
}

// ---------------------------------------------------------------------------
// Desktop app: native dialogs

fn dialogs(st: &AppState) -> ApiResult<Arc<dyn Dialogs>> {
    st.dialogs
        .clone()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "only in the desktop app"))
}

/// The user closed a dialog without choosing: nothing to do.
fn cancelled() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

/// Where dialogs start when there is no original to start next to.
fn pictures_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let pictures = home.join("Pictures");
    Some(if pictures.is_dir() { pictures } else { home })
}

/// Show the native Open dialog and open the chosen photo. Unlike
/// `/docs/open`, the path is not confined to `--root`: the user picked it
/// on their own machine.
async fn desktop_open(State(st): State<Shared>) -> ApiResult<Response> {
    let dialogs = dialogs(&st)?;
    let st2 = st.clone();
    blocking(move || {
        let Some(path) = dialogs.pick_photo(pictures_dir().as_deref()) else {
            return Ok(cancelled());
        };
        let bytes =
            std::fs::read(&path).map_err(|e| ApiError::bad(format!("{}: {e}", path.display())))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Json(open_doc(&st2, name, Some(path), &bytes)?).into_response())
    })
    .await
}

/// The file the desktop app was started with, opened once (204 when there
/// is none, or it was already taken). Like the native Open dialog, the path
/// is the user's own choice on their own machine, not confined to a root.
async fn desktop_launch(State(st): State<Shared>) -> ApiResult<Response> {
    let Some(path) = st.launch.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return Ok(cancelled());
    };
    let st2 = st.clone();
    blocking(move || {
        let bytes =
            std::fs::read(&path).map_err(|e| ApiError::bad(format!("{}: {e}", path.display())))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Json(open_doc(&st2, name, Some(path), &bytes)?).into_response())
    })
    .await
}

#[derive(Serialize)]
struct SavedAs {
    /// Full paths of the files written.
    saved: Vec<String>,
    /// Files not written, with the reason (a recipe file that exists).
    skipped: Vec<String>,
}

/// Show the native Save dialog, suggesting the next free `-glitch` name
/// next to the original, then render and write there. Replacing a file is
/// allowed only because the dialog asked first; the original is never
/// written. The recipe, if wanted, goes beside the image and is never
/// replaced, since the dialog asked about the image only.
async fn save_as(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<ExportRequest>,
) -> ApiResult<Response> {
    let dialogs = dialogs(&st)?;
    let doc = find(&st, &id)?;
    let recipe = req.recipe.to_recipe()?;
    let settings = req.settings()?;
    blocking(move || {
        let ext = req.ext();
        let (dir, name) = match &doc.source {
            Some(src) => {
                let next = if doc.project.is_some() && !req.project {
                    Some(project_output(src, ext))
                } else {
                    auto_output(src, ext).ok()
                };
                let suggested = next
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| download_name(&doc, req.project, ext));
                (src.parent().map(|p| p.to_path_buf()), suggested)
            }
            None => (pictures_dir(), download_name(&doc, req.project, ext)),
        };
        let Some(out) = dialogs.save_as(dir.as_deref(), &name) else {
            return Ok(cancelled());
        };
        // An uploaded photo has no file on disk; an empty path matches
        // nothing, so only the "never the input" check is skipped.
        let source = doc.source.clone().unwrap_or_default();
        let bytes = output_bytes(&doc, &recipe, &req, &settings)?;
        // The dialog asked before replacing; a project may even replace
        // the one it came from (D-63), a photo never.
        let written = if req.project {
            write_project(&source, &out, &bytes, true)
        } else {
            write_output(&source, &out, &bytes, true)
        };
        written.map_err(|e| ApiError::new(StatusCode::CONFLICT, e.to_string()))?;
        let mut saved = vec![out.display().to_string()];
        let mut skipped = Vec::new();
        if req.with_recipe && !req.project {
            let rpath = out.with_extension("recipe.toml");
            match write_output(&source, &rpath, recipe.to_toml().as_bytes(), false) {
                Ok(()) => saved.push(rpath.display().to_string()),
                Err(e) => skipped.push(e.to_string()),
            }
        }
        Ok(Json(SavedAs { saved, skipped }).into_response())
    })
    .await
}

// ---------------------------------------------------------------------------
// Settings (SPEC 4c): the library and export folders, and Save As into a
// folder. Folders travel as paths relative to the home directory.

fn settings(st: &AppState) -> ApiResult<&SettingsStore> {
    st.settings
        .as_ref()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "settings are off on this server"))
}

#[derive(Serialize, Deserialize)]
struct SettingsDto {
    /// Relative to home ("" is home itself); null when not chosen.
    library: Option<String>,
    export: Option<String>,
    /// When `--root` decides what Open browses: its name, for display.
    #[serde(default, skip_deserializing)]
    fixed_root: Option<String>,
}

fn settings_dto(st: &AppState, store: &SettingsStore) -> SettingsDto {
    let v = store.get();
    SettingsDto {
        library: v.library.as_deref().map(|p| store.relative(p)),
        export: v.export.as_deref().map(|p| store.relative(p)),
        fixed_root: st
            .root_fixed
            .then(|| st.current_root().map(|r| r.display().to_string()))
            .flatten(),
    }
}

async fn get_settings(State(st): State<Shared>) -> ApiResult<Json<SettingsDto>> {
    Ok(Json(settings_dto(&st, settings(&st)?)))
}

/// Choose the folders. Each must exist inside home; the file is written
/// and, unless `--root` is fixed, Open browses the new library at once.
async fn put_settings(
    State(st): State<Shared>,
    Json(req): Json<SettingsDto>,
) -> ApiResult<Json<SettingsDto>> {
    let store = settings(&st)?;
    let pick = |rel: &Option<String>| -> ApiResult<Option<PathBuf>> {
        rel.as_deref()
            .map(|r| {
                store
                    .folder(r)
                    .map_err(|_| ApiError::bad(format!("{r}: not a folder in your home folder")))
            })
            .transpose()
    };
    let values = Settings {
        library: pick(&req.library)?,
        export: pick(&req.export)?,
    };
    store
        .set(values.clone())
        .map_err(|e| ApiError::internal(format!("could not save the settings: {e}")))?;
    if !st.root_fixed {
        st.set_root(values.library);
    }
    Ok(Json(settings_dto(&st, store)))
}

#[derive(Serialize)]
struct Folders {
    /// This folder, relative to home.
    path: String,
    parent: Option<String>,
    dirs: Vec<browse::Entry>,
}

/// The folders inside a folder under home, for picking the library and
/// export folders. Folders only; hidden ones are left out.
async fn folders(State(st): State<Shared>, Query(q): Query<PathQuery>) -> ApiResult<Json<Folders>> {
    let home = settings(&st)?.home().to_path_buf();
    blocking(move || {
        let l = browse::list(&home, &q.path)?;
        Ok(Json(Folders {
            path: l.path,
            parent: l.parent,
            dirs: l.dirs,
        }))
    })
    .await
}

#[derive(Deserialize)]
struct NewFolderRequest {
    /// The folder to make it in, relative to home ("" is home itself).
    path: String,
    /// The new folder's name: one path component, not hidden.
    name: String,
}

/// Make a folder inside a folder under home (D-65), and list it, so the
/// browser can open it at once. Only ever one new, empty folder: a taken
/// name is refused (409) rather than reused, and nothing else changes.
async fn new_folder(
    State(st): State<Shared>,
    Json(req): Json<NewFolderRequest>,
) -> ApiResult<Json<Folders>> {
    let home = settings(&st)?.home().to_path_buf();
    // The same rule as a Save As file name: what was typed must already be
    // a single, plain name, so nothing is quietly changed or escaped.
    let name = req.name.trim();
    if sanitize_name(name) != name || name.is_empty() || name.starts_with('.') || name.len() > 200 {
        return Err(ApiError::bad(
            "a folder name, without slashes, not starting with a dot",
        ));
    }
    let name = name.to_owned();
    blocking(move || {
        // `resolve` canonicalizes, so `..` and symlinks leading out of
        // home are refused before anything is made.
        let parent = browse::resolve(&home, &req.path)?;
        let dir = parent.join(&name);
        std::fs::create_dir(&dir).map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => {
                ApiError::new(StatusCode::CONFLICT, format!("{name} is already there"))
            }
            std::io::ErrorKind::PermissionDenied => ApiError::new(
                StatusCode::FORBIDDEN,
                format!("no permission to make a folder in ~/{}", req.path),
            ),
            _ => ApiError::internal(format!("cannot make {name}: {e}")),
        })?;
        let l = browse::list(&home, &browse::relative(&home, &dir))?;
        Ok(Json(Folders {
            path: l.path,
            parent: l.parent,
            dirs: l.dirs,
        }))
    })
    .await
}

#[derive(Deserialize)]
struct SaveToRequest {
    #[serde(flatten)]
    export: ExportRequest,
    /// Relative to home.
    folder: String,
    /// The file name; the format's extension is added unless it is there.
    name: String,
}

/// A file name typed in the browser, made safe: one path component, no
/// hidden files, with the format's extension.
fn file_name(name: &str, ext: &str) -> ApiResult<String> {
    let clean = sanitize_name(name);
    if clean.is_empty() || clean.starts_with('.') || clean != name.trim() || clean.len() > 200 {
        return Err(ApiError::bad(
            "a file name, without folders, not starting with a dot",
        ));
    }
    // The extension itself, or another spelling of the same format (`.jpeg`).
    let has_ext = std::path::Path::new(&clean).extension().is_some_and(|e| {
        let e = e.to_string_lossy();
        e.eq_ignore_ascii_case(ext)
            || codec::Format::parse(&e).is_some_and(|f| codec::Format::parse(ext) == Some(f))
    });
    Ok(if has_ext {
        clean
    } else {
        format!("{clean}.{ext}")
    })
}

/// `name` in `dir`, or `stem-1.ext`, `stem-2.ext`, ... if it is taken. With
/// a recipe beside it, its `.recipe.toml` must be free too.
fn free_name(dir: &std::path::Path, name: &str, with_recipe: bool) -> PathBuf {
    let path = std::path::Path::new(name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    let taken = |p: &std::path::Path| std::fs::symlink_metadata(p).is_ok();
    (0..)
        .map(|n| {
            let s = if n == 0 {
                stem.to_string()
            } else {
                format!("{stem}-{n}")
            };
            dir.join(format!("{s}.{ext}"))
        })
        .find(|p| !(taken(p) || (with_recipe && taken(&p.with_extension("recipe.toml")))))
        .expect("an unbounded search finds a free name")
}

/// Save As into a folder under home, with a name chosen in the browser.
/// Never overwrites (409, so the dialog can ask for another name) and
/// never writes the original, with one exception: a project may update an
/// existing project of that name, once asked (`replace`; Phil, 2026-11-25).
/// Its 409 says so with `X-Replaceable: 1`, so the GUI knows to offer it.
async fn save_to(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<SaveToRequest>,
) -> ApiResult<Response> {
    let store = settings(&st)?;
    let doc = find(&st, &id)?;
    let folder = store
        .folder(&req.folder)
        .map_err(|_| ApiError::bad(format!("{}: not a folder in your home folder", req.folder)))?;
    let ext = req.export.ext();
    let out = folder.join(file_name(&req.name, ext)?);
    let recipe = req.export.recipe.to_recipe()?;
    let encode = req.export.settings()?;
    let with_recipe = req.export.with_recipe && !req.export.project;
    let export = req.export;
    let source = doc.source.clone().unwrap_or_default();
    let shown = |p: &std::path::Path| format!("~/{}", store.relative(p));
    let (out_shown, rpath) = (shown(&out), out.with_extension("recipe.toml"));
    let recipe_shown = shown(&rpath);
    // Refuse before the long render when the name is taken, unless it is a
    // project being updated (a project only: never a photo, nor another
    // kind of file that happens to end in .glrx).
    let taken = std::fs::symlink_metadata(&out).is_ok();
    let replaceable = taken && export.project && glitchrx_core::io::is_project_file(&out);
    let update = replaceable && export.replace;
    if taken && !update {
        let mut res = ApiError::new(
            StatusCode::CONFLICT,
            if replaceable {
                format!("{out_shown} already exists")
            } else {
                format!("{out_shown} already exists; choose another name")
            },
        )
        .into_response();
        if replaceable {
            res.headers_mut()
                .insert("x-replaceable", HeaderValue::from_static("1"));
        }
        return Ok(res);
    }
    blocking(move || {
        let bytes = output_bytes(&doc, &recipe, &export, &encode)?;
        let conflict = |e: WriteError| ApiError::new(StatusCode::CONFLICT, e.to_string());
        let write = |path: &std::path::Path, data: &[u8]| {
            write_output(&source, path, data, false).map_err(conflict)
        };
        if update {
            write_project(&source, &out, &bytes, true).map_err(conflict)?;
        } else {
            write(&out, &bytes)?;
        }
        let mut saved = vec![out_shown];
        if with_recipe {
            write(&rpath, recipe.to_toml().as_bytes())?;
            saved.push(recipe_shown);
        }
        Ok(Json(Saved { saved }).into_response())
    })
    .await
}

#[derive(Deserialize)]
struct SuggestRequest {
    /// Relative to home. None: the name a download would get.
    #[serde(default)]
    folder: Option<String>,
    format: Format,
    /// A project file's name rather than a picture's.
    #[serde(default)]
    project: bool,
}

#[derive(Serialize)]
struct Suggested {
    /// The next free `-glitch` name in the folder, without the extension.
    name: String,
}

/// The name Save As suggests: the photo's next free `-glitch` name in the
/// chosen folder, as the CLI would pick it there; for a picture made from
/// a project, the project's own name (with `-1`, ... if taken).
async fn suggest_name(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<SuggestRequest>,
) -> ApiResult<Json<Suggested>> {
    let doc = find(&st, &id)?;
    let ext = if req.project {
        project::EXTENSION
    } else {
        codec::Format::from(req.format).extension()
    };
    let stem = |p: &std::path::Path| {
        p.file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let Some(folder) = &req.folder else {
        let name = download_name(&doc, req.project, ext);
        return Ok(Json(Suggested {
            name: stem(std::path::Path::new(&name)),
        }));
    };
    let store = settings(&st)?;
    let folder = store
        .folder(folder)
        .map_err(|_| ApiError::bad(format!("{folder}: not a folder in your home folder")))?;
    // A picture from a project takes the project's name (D-63); otherwise
    // the photo's next free -glitch name.
    let out = match &doc.project {
        Some(p) if !req.project => project_output(&folder.join(p), ext),
        _ => auto_output(&folder.join(&doc.name), ext)
            .map_err(|e| ApiError::internal(e.to_string()))?,
    };
    Ok(Json(Suggested { name: stem(&out) }))
}

// ---------------------------------------------------------------------------
// Presets: the user's saved recipes (see `presets`).

fn presets(st: &Shared) -> ApiResult<&Presets> {
    st.presets
        .as_ref()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "presets are off on this server"))
}

async fn list_presets(State(st): State<Shared>) -> ApiResult<Json<Vec<PresetInfo>>> {
    Ok(Json(presets(&st)?.list()?))
}

async fn get_preset(
    State(st): State<Shared>,
    Path(name): Path<String>,
) -> ApiResult<Json<RecipeDto>> {
    Ok(Json(RecipeDto::from_recipe(&presets(&st)?.get(&name)?)))
}

#[derive(Deserialize)]
struct PresetRequest {
    recipe: RecipeDto,
    /// Replace a preset of the same name (after the user said so).
    #[serde(default)]
    replace: bool,
}

/// Save a preset: 200 with its name, or 409 if the name is taken and
/// `replace` was not given.
async fn put_preset(
    State(st): State<Shared>,
    Path(name): Path<String>,
    Json(req): Json<PresetRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let recipe = req.recipe.to_recipe()?;
    if recipe.steps.is_empty() {
        return Err(ApiError::bad("a preset needs at least one step"));
    }
    let saved = presets(&st)?.put(&name, &recipe, req.replace)?;
    Ok(Json(serde_json::json!({ "name": saved })))
}

async fn delete_preset(
    State(st): State<Shared>,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    presets(&st)?.delete(&name)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Parse a recipe file into the GUI's JSON form.
async fn recipe_from_toml(body: String) -> ApiResult<Json<RecipeDto>> {
    let r = Recipe::from_toml(&body)?;
    Ok(Json(RecipeDto::from_recipe(&r)))
}

/// Write the GUI's recipe as a TOML file (validated first).
async fn recipe_to_toml(Json(dto): Json<RecipeDto>) -> ApiResult<Response> {
    let r = dto.to_recipe()?;
    Ok((
        [(header::CONTENT_TYPE, "application/toml; charset=utf-8")],
        r.to_toml(),
    )
        .into_response())
}

#[derive(Deserialize)]
struct PathQuery {
    #[serde(default)]
    path: String,
}

async fn browse_dir(
    State(st): State<Shared>,
    Query(q): Query<PathQuery>,
) -> ApiResult<Json<browse::Listing>> {
    let root = root(&st)?;
    blocking(move || Ok(Json(browse::list(&root, &q.path)?))).await
}

/// A small JPEG thumbnail of a photo under `--root`, or of a project's
/// preview (its original, if it has none).
async fn thumb(State(st): State<Shared>, Query(q): Query<PathQuery>) -> ApiResult<Response> {
    let path = browse::resolve(&root(&st)?, &q.path)?;
    if !browse::is_openable(&path) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "not a photo"));
    }
    blocking(move || {
        let mut bytes =
            std::fs::read(&path).map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        if project::is_project(&bytes) {
            let p = Project::from_bytes(&bytes)?;
            bytes = p.preview.unwrap_or(p.original);
        }
        let img = codec::decode_scaled(&bytes, Some((240, 240)))?;
        let jpeg = render::preview_jpeg(&img.raster, 75).map_err(ApiError::internal)?;
        Ok((
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "private, max-age=600"),
            ],
            jpeg,
        )
            .into_response())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(sanitize_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_name("C:\\x\\photo.JPG"), "photo.JPG");
        assert_eq!(export_name("photo.JPG", "tif"), "photo-glitch.tif");
        assert_eq!(export_name("photo-glitch-3.jpg", "png"), "photo-glitch.png");
        assert_eq!(export_name("a b.tiff", "jpg"), "a b-glitch.jpg");
        assert_eq!(export_name("IMG_1.jpg", "glrx"), "IMG_1-glitch.glrx");
        // A typed name keeps its own spelling of the format, and gets the
        // extension otherwise.
        assert_eq!(file_name("x.jpeg", "jpg").unwrap(), "x.jpeg");
        assert_eq!(file_name("x", "glrx").unwrap(), "x.glrx");
        assert_eq!(file_name("x.GLRX", "glrx").unwrap(), "x.GLRX");
        assert_eq!(file_name("x.bar", "glrx").unwrap(), "x.bar.glrx");
        assert_eq!(percent_encode("a b.jpg"), "a%20b.jpg");
    }

    #[test]
    fn secret_comparison() {
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "ab"));
    }
}

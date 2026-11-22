//! HTTP routes.
//!
//! Everything under `/api` is JSON in, JSON or image bytes out. Image work
//! (decoding, bending, encoding) is CPU-bound, so it runs on tokio's
//! blocking pool and never stalls the async workers.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use glitchrx_core::codec;
use glitchrx_core::io::{auto_output, base_stem, write_output};
use glitchrx_core::recipe::NEVER;
use glitchrx_core::{Error, Recipe};
use serde::{Deserialize, Serialize};

use crate::browse::{self, BrowseError};
use crate::dialogs::Dialogs;
use crate::docs::{Doc, Store, random_hex};
use crate::dto::{self, DocDto, RecipeDto};
use crate::render::{self, Format};

/// Largest upload accepted. Full-frame camera JPEGs are 10-40 MB, and
/// uncompressed TIFFs of the same photos 70-150 MB.
const MAX_UPLOAD: usize = 256 << 20;
/// How many photos stay open at once (least recently used is closed).
const MAX_DOCS: usize = 4;
/// Name of the cookie that carries the access token.
const TOKEN_COOKIE: &str = "glitchrx_token";

/// Shared server state.
pub struct AppState {
    pub store: Store,
    /// Canonical `--root`, if browsing server files is enabled.
    pub root: Option<PathBuf>,
    /// Shared access token, if one is required.
    pub token: Option<String>,
    /// Time limit for a live preview; an exact preview gets ten times this.
    pub preview_budget: Duration,
    /// Native file dialogs, when running inside the desktop app.
    pub dialogs: Option<Arc<dyn Dialogs>>,
}

impl AppState {
    pub fn new(root: Option<PathBuf>, token: Option<String>) -> AppState {
        AppState {
            store: Store::new(MAX_DOCS),
            root,
            token,
            preview_budget: PREVIEW_BUDGET,
            dialogs: None,
        }
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
        .route("/docs/{id}/export", post(export))
        .route("/docs/{id}/save", post(save))
        .route("/docs/{id}/save-as", post(save_as))
        .route("/desktop/open", post(desktop_open))
        .route("/recipe/from-toml", post(recipe_from_toml))
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
            Error::Malformed(_) => StatusCode::UNPROCESSABLE_ENTITY,
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

fn root(st: &AppState) -> ApiResult<&std::path::Path> {
    st.root.as_deref().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "server browsing is off (start with --root DIR)",
        )
    })
}

fn doc_dto(st: &AppState, doc: &Doc) -> DocDto {
    let path = match (&st.root, &doc.source) {
        (Some(r), Some(s)) => Some(browse::relative(r, s)),
        _ => None,
    };
    DocDto::new(&doc.id, &doc.name, path, doc.file_len, &doc.image)
}

// ---------------------------------------------------------------------------
// Access token

/// Constant-time comparison, so the token cannot be guessed byte by byte
/// from response timings.
fn same_secret(a: &str, b: &str) -> bool {
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
        let public = PUBLIC.contains(&req.uri().path());
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
}

async fn config(State(st): State<Shared>) -> Json<ConfigDto> {
    Json(ConfigDto {
        version: env!("CARGO_PKG_VERSION"),
        browse: st.root.is_some(),
        root_name: st.root.as_ref().map(|r| {
            r.file_name().map_or_else(
                || r.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        }),
        max_preview_side: render::MAX_PREVIEW_SIDE,
        encodings: dto::encodings(),
        desktop: st.dialogs.is_some(),
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
    blocking(move || {
        let doc = Doc::new(random_hex(8), name, None, &body)?;
        let doc = st2.store.insert(doc);
        Ok(Json(doc_dto(&st2, &doc)))
    })
    .await
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

/// Open a photo from `--root`.
async fn open(State(st): State<Shared>, Json(req): Json<OpenRequest>) -> ApiResult<Json<DocDto>> {
    let path = browse::resolve(root(&st)?, &req.path)?;
    if !path.is_file() || !browse::is_photo(&path) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "not a photo"));
    }
    let st2 = st.clone();
    blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| ApiError::internal(e.to_string()))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let doc = Doc::new(random_hex(8), name, Some(path), &bytes)?;
        let doc = st2.store.insert(doc);
        Ok(Json(doc_dto(&st2, &doc)))
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
    let recipe = if req.original {
        None
    } else {
        Some(req.recipe.to_recipe()?)
    };
    // Only bend renders compete; an original (before/after) request must
    // not cancel the render it is being compared with.
    let generation = recipe.as_ref().map(|_| doc.begin());
    let max_w = req.max_w.clamp(16, render::MAX_PREVIEW_SIDE) as usize;
    let max_h = req.max_h.clamp(16, render::MAX_PREVIEW_SIDE) as usize;
    let exact = req.exact;
    let budget = st.preview_budget * if exact { 10 } else { 1 };
    blocking(move || {
        let t0 = Instant::now();
        let raster = match (&recipe, generation) {
            (Some(r), Some(g)) => {
                let stop = || !doc.is_current(g) || t0.elapsed() > budget;
                let result = if exact {
                    doc.render_full(r, &stop).map(|full| {
                        let (w, h) = full.fit(max_w, max_h);
                        full.resize(w, h)
                    })
                } else {
                    doc.preview(r, max_w, max_h, &stop)
                };
                match result {
                    Ok(raster) => raster,
                    Err(Error::Cancelled) if !doc.is_current(g) => {
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
            _ => doc.original(max_w, max_h),
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
    /// Overlay colour; the GUI passes its theme's.
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
struct ExportRequest {
    recipe: RecipeDto,
    format: Format,
    /// Save mode only: also write the recipe as TOML next to the output.
    #[serde(default)]
    with_recipe: bool,
}

/// File name for a download: `photo.jpg` -> `photo-glitch.tif`.
fn export_name(doc_name: &str, format: Format) -> String {
    let stem = std::path::Path::new(doc_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = codec::Format::from(format).extension();
    format!("{}-glitch.{ext}", base_stem(&stem))
}

/// Bend the full image and encode it, keeping the colour profile.
fn export_bytes(doc: &Doc, recipe: &Recipe, format: Format) -> ApiResult<Vec<u8>> {
    let raster = doc.render_full(recipe, NEVER)?;
    Ok(codec::encode(
        &raster,
        format.into(),
        doc.image.icc.as_deref(),
    )?)
}

/// Full-resolution export as a download. Runs the recipe on the full image
/// with [`Recipe::apply`], the same call the CLI makes.
async fn export(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<ExportRequest>,
) -> ApiResult<Response> {
    let doc = find(&st, &id)?;
    let recipe = req.recipe.to_recipe()?;
    blocking(move || {
        let bytes = export_bytes(&doc, &recipe, req.format)?;
        let name = export_name(&doc.name, req.format);
        // RFC 6266: an ASCII fallback plus the exact UTF-8 name.
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
        let disposition = format!(
            "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
            percent_encode(&name)
        );
        Ok((
            [
                (
                    header::CONTENT_TYPE,
                    codec::Format::from(req.format).mime().to_string(),
                ),
                (header::CONTENT_DISPOSITION, disposition),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
            bytes,
        )
            .into_response())
    })
    .await
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
    let root = root(&st)?.to_path_buf();
    let doc = find(&st, &id)?;
    let Some(source) = doc.source.clone() else {
        return Err(ApiError::bad(
            "this photo was uploaded from the browser; use download instead",
        ));
    };
    let recipe = req.recipe.to_recipe()?;
    blocking(move || {
        let bytes = export_bytes(&doc, &recipe, req.format)?;
        let ext = codec::Format::from(req.format).extension();
        let out = auto_output(&source, ext).map_err(|e| ApiError::internal(e.to_string()))?;
        let write = |path: &std::path::Path, data: &[u8]| {
            write_output(&source, path, data, false)
                .map_err(|e| ApiError::new(StatusCode::CONFLICT, e.to_string()))
        };
        write(&out, &bytes)?;
        let mut saved = vec![browse::relative(&root, &out)];
        if req.with_recipe {
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
        let doc = Doc::new(random_hex(8), name, Some(path), &bytes)?;
        let doc = st2.store.insert(doc);
        Ok(Json(doc_dto(&st2, &doc)).into_response())
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
    blocking(move || {
        let ext = codec::Format::from(req.format).extension();
        let (dir, name) = match &doc.source {
            Some(src) => {
                let suggested = auto_output(src, ext)
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| export_name(&doc.name, req.format));
                (src.parent().map(|p| p.to_path_buf()), suggested)
            }
            None => (pictures_dir(), export_name(&doc.name, req.format)),
        };
        let Some(out) = dialogs.save_as(dir.as_deref(), &name) else {
            return Ok(cancelled());
        };
        // An uploaded photo has no file on disk; an empty path matches
        // nothing, so only the "never the input" check is skipped.
        let source = doc.source.clone().unwrap_or_default();
        let bytes = export_bytes(&doc, &recipe, req.format)?;
        write_output(&source, &out, &bytes, true)
            .map_err(|e| ApiError::new(StatusCode::CONFLICT, e.to_string()))?;
        let mut saved = vec![out.display().to_string()];
        let mut skipped = Vec::new();
        if req.with_recipe {
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
    let root = root(&st)?.to_path_buf();
    blocking(move || Ok(Json(browse::list(&root, &q.path)?))).await
}

/// A small JPEG thumbnail of a photo under `--root`.
async fn thumb(State(st): State<Shared>, Query(q): Query<PathQuery>) -> ApiResult<Response> {
    let path = browse::resolve(root(&st)?, &q.path)?;
    if !browse::is_photo(&path) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "not a photo"));
    }
    blocking(move || {
        let bytes =
            std::fs::read(&path).map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
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
        assert_eq!(export_name("photo.JPG", Format::Tiff), "photo-glitch.tif");
        assert_eq!(
            export_name("photo-glitch-3.jpg", Format::Png),
            "photo-glitch.png"
        );
        assert_eq!(export_name("a b.tiff", Format::Jpeg), "a b-glitch.jpg");
        assert_eq!(percent_encode("a b.jpg"), "a%20b.jpg");
    }

    #[test]
    fn secret_comparison() {
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "ab"));
    }
}

//! HTTP routes.
//!
//! Everything under `/api` is JSON in, JSON or image bytes out. Image work
//! (parsing, glitching, decoding) is CPU-bound, so it runs on tokio's
//! blocking pool and never stalls the async workers.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use glitchrx_core::io::{Suffix, auto_output, base_stem, write_output};
use glitchrx_core::turbo::{self, Scale};
use glitchrx_core::{Error, Recipe};
use serde::{Deserialize, Serialize};

use crate::browse::{self, BrowseError};
use crate::docs::{Doc, Store, random_hex};
use crate::dto::{self, DocDto, RecipeDto};
use crate::render::{self, Format};

/// Largest upload accepted. Full-frame camera JPEGs are 10-40 MB.
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
}

impl AppState {
    pub fn new(root: Option<PathBuf>, token: Option<String>) -> AppState {
        AppState {
            store: Store::new(MAX_DOCS),
            root,
            token,
        }
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
        .route("/docs/{id}/export", post(export))
        .route("/docs/{id}/save", post(save))
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
            Error::NotJpeg | Error::Unsupported(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
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
    DocDto::new(&doc.id, &doc.name, path, &doc.info)
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

/// When a token is configured, every request needs it: either as the
/// cookie, or once as `?token=...` (the URL printed at startup), which sets
/// the cookie and redirects to the clean URL. Also adds security headers.
async fn guard(State(st): State<Shared>, req: Request, next: Next) -> Response {
    if let Some(token) = &st.token {
        let from_cookie = cookie_token(req.headers()).is_some_and(|t| same_secret(&t, token));
        if !from_cookie {
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
            return (
                StatusCode::UNAUTHORIZED,
                "glitchrx: access token required. Open the URL printed by `glitchrx serve` \
                 (it ends in ?token=...).\n",
            )
                .into_response();
        }
    }
    let mut res = next.run(req).await;
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
        let doc = Doc::new(random_hex(8), name, None, body.to_vec())?;
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
        let doc = Doc::new(random_hex(8), name, Some(path), bytes)?;
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
}

/// Render a preview. Answers `image/jpeg` with the decode status in
/// `X-Decode` / `X-Decode-Message` and timing in `X-Render-Ms`, or 204 when
/// a newer request for the same photo has already arrived (latest wins).
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
    // Only glitch renders compete; an original (before/after) request must
    // not cancel the glitch render it is being compared with.
    let generation = recipe.as_ref().map(|_| doc.begin());
    blocking(move || {
        let t0 = Instant::now();
        let glitched = match &recipe {
            None => doc.original.to_vec(),
            Some(r) => {
                // A newer request queued behind us: skip the work entirely.
                if generation.is_some_and(|g| !doc.is_current(g)) {
                    return Ok(StatusCode::NO_CONTENT.into_response());
                }
                doc.glitch(r)?
            }
        };
        let p = render::preview(&glitched, &doc.info.frame, req.max_w, req.max_h)
            .map_err(|m| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, m))?;
        let ms = t0.elapsed().as_millis();
        let mut res = (
            [
                (header::CONTENT_TYPE, "image/jpeg".to_string()),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
            p.jpeg,
        )
            .into_response();
        let h = res.headers_mut();
        let mut set = |k: &'static str, v: String| {
            if let Ok(v) = HeaderValue::from_str(&v) {
                h.insert(k, v);
            }
        };
        set("x-decode", p.status.word().into());
        // Header values must be visible ASCII; decoder messages are.
        if let Some(m) = p.status.message() {
            set(
                "x-decode-message",
                m.replace(|c: char| !c.is_ascii() || c.is_control(), " "),
            );
        }
        set("x-render-ms", ms.to_string());
        set("x-width", p.width.to_string());
        set("x-height", p.height.to_string());
        Ok(res)
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

/// File name for a download: `photo.jpg` -> `photo-glitch.jpg`.
fn export_name(doc_name: &str, format: Format) -> String {
    let p = std::path::Path::new(doc_name);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = format
        .extension()
        .map(str::to_string)
        .or_else(|| p.extension().map(|e| e.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "jpg".into());
    format!("{}-glitch.{ext}", base_stem(&stem))
}

/// Full-resolution export as a download. Runs the recipe from scratch with
/// [`Recipe::apply`], the same call the CLI makes.
async fn export(
    State(st): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<ExportRequest>,
) -> ApiResult<Response> {
    let doc = find(&st, &id)?;
    let recipe = req.recipe.to_recipe()?;
    blocking(move || {
        let glitched = recipe.apply(doc.original.to_vec())?;
        let bytes = render::export(glitched, req.format).map_err(ApiError::internal)?;
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
                (header::CONTENT_TYPE, req.format.mime().to_string()),
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
        let glitched = recipe.apply(doc.original.to_vec())?;
        let bytes = render::export(glitched, req.format).map_err(ApiError::internal)?;
        let auto =
            auto_output(&source, Suffix::Glitch).map_err(|e| ApiError::internal(e.to_string()))?;
        let out = match req.format.extension() {
            Some(ext) => auto.with_extension(ext),
            None => auto,
        };
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
        let frame = glitchrx_core::Jpeg::parse(bytes.clone(), glitchrx_core::Mode::Safe)?
            .info()
            .frame
            .clone();
        let scale = Scale::to_fit(frame.width as u32, frame.height as u32, 240, 240);
        let (img, status) = turbo::decode(&bytes, scale);
        let img = img.ok_or_else(|| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                status.message().unwrap_or("decode failed").to_string(),
            )
        })?;
        let jpeg = turbo::encode(
            &img,
            turbo::EncodeOptions {
                quality: 75,
                ..Default::default()
            },
        )
        .ok_or_else(|| ApiError::internal("thumbnail encode failed"))?;
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
        assert_eq!(export_name("photo.JPG", Format::Jpeg), "photo-glitch.JPG");
        assert_eq!(
            export_name("photo-glitch-3.jpg", Format::Png),
            "photo-glitch.png"
        );
        assert_eq!(export_name("a b.jpeg", Format::JpegBaked), "a b-glitch.jpg");
        assert_eq!(percent_encode("a b.jpg"), "a%20b.jpg");
    }

    #[test]
    fn secret_comparison() {
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "ab"));
    }
}

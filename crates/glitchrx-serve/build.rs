//! Embed the built Svelte frontend (`web/dist`) into the binary.
//!
//! Generates `$OUT_DIR/assets.rs` with one `include_bytes!` per file, so the
//! server needs nothing on disk at runtime and makes no external requests.
//! When the frontend has not been built (a fresh clone, or a Rust-only gate
//! run), a small placeholder page is embedded instead, so `cargo build` and
//! `cargo test` never depend on Node.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let web = manifest.join("../../web");
    let dist = web.join("dist");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("assets.rs");

    // `web/` always exists in the repo; its mtime changes when `dist` is
    // created or removed. Watching a path that does not exist would make
    // cargo rerun this script on every build.
    println!("cargo:rerun-if-changed={}", web.display());

    let mut files = Vec::new();
    if dist.join("index.html").is_file() {
        println!("cargo:rerun-if-changed={}", dist.display());
        collect(&dist, &dist, &mut files);
        files.sort();
    }

    let mut code = String::new();
    code.push_str("/// Files of the built frontend: (path, mime type, contents).\n");
    code.push_str("pub static ASSETS: &[(&str, &str, &[u8])] = &[\n");
    if files.is_empty() {
        let placeholder = PathBuf::from(env::var("OUT_DIR").unwrap()).join("placeholder.html");
        fs::write(&placeholder, PLACEHOLDER).unwrap();
        code.push_str(&format!(
            "    (\"index.html\", \"text/html; charset=utf-8\", include_bytes!({:?})),\n",
            placeholder.display().to_string()
        ));
    }
    for rel in &files {
        let abs = dist.join(rel);
        println!("cargo:rerun-if-changed={}", abs.display());
        code.push_str(&format!(
            "    ({:?}, {:?}, include_bytes!({:?})),\n",
            rel,
            mime(rel),
            abs.display().to_string()
        ));
    }
    code.push_str("];\n");
    code.push_str(&format!(
        "/// False when the placeholder page is embedded.\npub const BUILT: bool = {};\n",
        !files.is_empty()
    ));
    fs::write(out, code).unwrap();
}

/// Paths relative to `base`, with `/` separators.
fn collect(base: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(base, &path, out);
        } else {
            let rel = path.strip_prefix(base).unwrap();
            let parts: Vec<_> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push(parts.join("/"));
        }
    }
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>glitchrx</title>
<style>body{font-family:"Courier New",monospace;background:#282828;color:#ebdbb2;margin:2rem;line-height:1.5}
code{color:#b8bb26}</style></head>
<body><h1>glitchrx serve</h1>
<p>The server is running, but this binary was built without the web interface.</p>
<p>Build it with <code>just web</code> (or <code>cd web &amp;&amp; npm ci &amp;&amp; npm run build</code>),
then rebuild glitchrx.</p></body></html>
"#;

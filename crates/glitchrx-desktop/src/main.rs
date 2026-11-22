//! glitchrx desktop: the web GUI in a native window (SPEC phase 2e).
//!
//! The app is the same server as `glitchrx serve`, started inside this
//! process on `127.0.0.1` at a free port with a fresh random token, and a
//! native window (Tauri, on the system's web view) pointed at it. So the
//! desktop app and the LAN GUI are one code path: previews, export and
//! undo behave identically.
//!
//! What the desktop adds is native file dialogs. The server calls them
//! through [`Dialogs`] from its blocking pool; the page itself gets no
//! access to Tauri (no IPC), it only talks HTTP to its own server.

// Release builds on Windows should not open a console; harmless elsewhere.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glitchrx_serve::dialogs::{Dialogs, PHOTO_EXTENSIONS};
use glitchrx_serve::docs::random_hex;
use glitchrx_serve::{AppState, router};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

/// Label of the one window.
const MAIN: &str = "main";

/// The system's Open and Save dialogs, via Tauri's dialog plugin. The
/// `blocking_*` calls hop to the UI thread and wait; the server calls them
/// from its blocking pool, never from the UI thread itself.
struct NativeDialogs(AppHandle);

impl NativeDialogs {
    /// A file dialog attached to the main window (so it is modal to it).
    fn builder(&self, dir: Option<&Path>) -> tauri_plugin_dialog::FileDialogBuilder<tauri::Wry> {
        let mut d = self.0.dialog().file();
        if let Some(w) = self.0.get_webview_window(MAIN) {
            d = d.set_parent(&w);
        }
        if let Some(dir) = dir {
            d = d.set_directory(dir);
        }
        d
    }
}

impl Dialogs for NativeDialogs {
    fn pick_photo(&self, dir: Option<&Path>) -> Option<PathBuf> {
        self.builder(dir)
            .set_title("Open a photo")
            .add_filter("Photos", PHOTO_EXTENSIONS)
            .blocking_pick_file()?
            .into_path()
            .ok()
    }

    fn save_as(&self, dir: Option<&Path>, name: &str) -> Option<PathBuf> {
        // The system dialog asks before replacing a file (see `Dialogs`).
        self.builder(dir)
            .set_title("Save the bent photo")
            .set_file_name(name)
            .blocking_save_file()?
            .into_path()
            .ok()
    }
}

/// The home folder, for the in-app browser (the "server's folders" tab).
fn home() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    home.canonicalize().ok().filter(|p| p.is_dir())
}

fn main() {
    // Bind before the window exists: the socket listens from here on, so
    // the first page load queues instead of failing, however fast it is.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("cannot listen on 127.0.0.1");
    listener
        .set_nonblocking(true)
        .expect("cannot make the listener non-blocking");
    let port = listener.local_addr().expect("no local address").port();
    // Other users on this machine can reach 127.0.0.1 too; the token keeps
    // them out. A new one each launch, so nothing to store or leak.
    let token = random_hex(16);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let dialogs = Arc::new(NativeDialogs(app.handle().clone()));
            let state = AppState::new(home(), Some(token.clone())).with_dialogs(dialogs);
            let service = router(state);
            // Tauri runs on tokio, so the server shares its runtime.
            tauri::async_runtime::spawn(async move {
                match tokio::net::TcpListener::from_std(listener) {
                    Ok(l) => {
                        if let Err(e) = axum::serve(l, service).await {
                            eprintln!("glitchrx: server stopped: {e}");
                        }
                    }
                    Err(e) => eprintln!("glitchrx: cannot start the server: {e}"),
                }
            });

            // `?token=` sets the cookie and redirects to the clean URL.
            let url = format!("http://127.0.0.1:{port}/?token={token}");
            // Debug builds say where, so the same session can be opened in
            // a browser with its developer tools.
            if cfg!(debug_assertions) {
                eprintln!("glitchrx desktop: serving {url}");
            }
            let url = url.parse()?;
            WebviewWindowBuilder::new(app, MAIN, WebviewUrl::External(url))
                .title("glitchrx")
                .inner_size(1280.0, 840.0)
                .min_inner_size(640.0, 480.0)
                // Let the page handle dropped files (it accepts photos)
                // instead of Tauri swallowing the drop.
                .disable_drag_drop_handler()
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("glitchrx desktop failed to start");
}

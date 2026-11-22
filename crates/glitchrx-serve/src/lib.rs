//! glitchrx-serve: the web GUI behind `glitchrx serve` (SPEC section 10).
//!
//! An axum server that wraps `glitchrx-core` and serves the Svelte frontend,
//! which is compiled ahead of time and embedded in the binary (`build.rs`).
//! Photos only ever travel between the browser and this server: an upload
//! from a phone goes to *your* machine, and no page asset comes from
//! anywhere else.
//!
//! Module map:
//! - [`api`]: routes, access token, error mapping
//! - [`docs`]: open photos and the per-step render cache
//! - [`render`]: preview decode/re-encode and full-resolution export
//! - [`browse`]: listing photos under `--root`, confined to it
//! - [`dto`]: the JSON shapes the frontend uses
//! - [`dialogs`]: native file dialogs, supplied by the desktop app

pub mod api;
mod assets;
pub mod browse;
pub mod dialogs;
pub mod docs;
pub mod dto;
pub mod presets;
pub mod render;

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

pub use api::{AppState, router};

/// Default port. Arbitrary, unprivileged, unlikely to clash.
pub const DEFAULT_PORT: u16 = 7878;

/// Server settings from the command line.
#[derive(Debug, Clone)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    /// Directory the GUI may browse and save into.
    pub root: Option<PathBuf>,
    /// Shared access token, if required.
    pub token: Option<String>,
    /// Where saved presets live; none turns presets off.
    pub presets: Option<PathBuf>,
}

/// True when the frontend was built into this binary (not the placeholder).
pub fn frontend_built() -> bool {
    assets::BUILT
}

/// Run the server until Ctrl-C. Prints the URL(s) to open.
pub fn serve(config: Config) -> io::Result<()> {
    let root = match &config.root {
        Some(r) => {
            let c = r
                .canonicalize()
                .map_err(|e| io::Error::new(e.kind(), format!("--root {}: {e}", r.display())))?;
            if !c.is_dir() {
                return Err(io::Error::other(format!(
                    "--root {} is not a directory",
                    r.display()
                )));
            }
            Some(c)
        }
        None => None,
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let addr = SocketAddr::new(config.host, config.port);
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| io::Error::new(e.kind(), format!("cannot listen on {addr}: {e}")))?;
        let port = listener.local_addr()?.port();
        announce(&config, root.as_deref(), port);
        let mut state = AppState::new(root, config.token.clone());
        if let Some(dir) = &config.presets {
            state = state.with_presets(dir.clone());
        }
        let app = router(state);
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
                eprintln!("\nglitchrx: stopped");
            })
            .await
    })
}

/// Print where to point a browser, plus warnings about exposure.
fn announce(config: &Config, root: Option<&std::path::Path>, port: u16) {
    let suffix = config
        .token
        .as_ref()
        .map_or(String::new(), |t| format!("/?token={t}"));
    let url = |ip: IpAddr| match ip {
        IpAddr::V6(v6) => format!("http://[{v6}]:{port}{suffix}"),
        IpAddr::V4(v4) => format!("http://{v4}:{port}{suffix}"),
    };

    eprintln!("glitchrx serve {}", env!("CARGO_PKG_VERSION"));
    if config.host.is_unspecified() {
        // Listening everywhere: list this machine's addresses (LAN, VPN).
        let mut addrs: Vec<IpAddr> = if_addrs::get_if_addrs()
            .map(|v| v.into_iter().map(|i| i.ip()).collect())
            .unwrap_or_default();
        addrs.retain(|ip| ip.is_ipv4() || config.host.is_ipv6());
        addrs.sort_by_key(|ip| (ip.is_loopback(), *ip));
        addrs.dedup();
        for ip in addrs {
            eprintln!("  open {}", url(ip));
        }
    } else {
        eprintln!("  open {}", url(config.host));
    }
    match root {
        Some(r) => eprintln!("  browsing and saving in {}", r.display()),
        None => eprintln!("  server browsing off (add --root DIR to open and save files there)"),
    }
    if !config.host.is_loopback() && config.token.is_none() {
        eprintln!(
            "  warning: reachable from other machines with no access token; \
             anyone who can reach this port can use it (add --token)"
        );
    }
    if !frontend_built() {
        eprintln!("  note: built without the web UI; run `just web` and rebuild");
    }
    eprintln!("  Ctrl-C to stop");
}

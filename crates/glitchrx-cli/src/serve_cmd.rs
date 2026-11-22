//! `glitchrx serve`: start the web GUI.
//!
//! Dispatched before the normal argument parsing when the first argument is
//! the word `serve`. (A photo literally named `serve` can still be glitched
//! as `./serve`.)

use std::ffi::OsString;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use glitchrx_serve::{Config, DEFAULT_PORT, docs::random_hex, serve};

/// Start the glitchrx web GUI.
#[derive(Debug, Parser)]
#[command(
    name = "glitchrx serve",
    version,
    about = "Start the glitchrx web GUI. Photos stay on this machine: the browser sends them \
             to this server and nowhere else.",
    after_help = "Examples:\n  glitchrx serve\n  glitchrx serve --root ~/Pictures\n  \
                  glitchrx serve --host 0.0.0.0 --token --root ~/Pictures"
)]
struct ServeCli {
    /// Address to listen on. Default: this machine only. Use 0.0.0.0 (or a
    /// LAN or WireGuard address) to reach it from a phone.
    #[arg(long, default_value = "127.0.0.1")]
    host: IpAddr,

    /// Port to listen on (0 picks a free one).
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Directory the GUI may browse, open from and save into.
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Require a shared access token. Without a value, one is generated and
    /// printed as part of the URL.
    #[arg(long, value_name = "SECRET", num_args = 0..=1, require_equals = true)]
    token: Option<Option<String>>,

    /// Directory for saved presets (your own recipes, shared by every device
    /// using this server). Default: ~/.config/glitchrx/presets.
    #[arg(long, value_name = "DIR")]
    presets: Option<PathBuf>,
}

pub fn main(args: Vec<OsString>) -> ExitCode {
    let cli = ServeCli::parse_from(std::iter::once(OsString::from("glitchrx serve")).chain(args));
    let token = match cli.token {
        None => None,
        Some(Some(t)) if !t.is_empty() => {
            if !t
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.~".contains(c))
            {
                eprintln!("glitchrx serve: --token may only use letters, digits and -_.~");
                return ExitCode::from(2);
            }
            Some(t)
        }
        Some(_) => Some(random_hex(16)),
    };
    let config = Config {
        host: cli.host,
        port: cli.port,
        root: cli.root,
        token,
        presets: cli.presets.or_else(glitchrx_serve::presets::default_dir),
    };
    match serve(config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("glitchrx serve: {e}");
            ExitCode::from(1)
        }
    }
}

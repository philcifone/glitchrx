//! Tauri's build step: reads `tauri.conf.json`, embeds the icons and the
//! (tiny) loading page.
fn main() {
    tauri_build::build()
}

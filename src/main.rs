//! DockCV — application entry point.
//!
//! Primary target is macOS; Linux and Windows support is wired through
//! `gpui_platform` feature selection in `Cargo.toml` and should build without
//! source changes.

// On macOS, detach from the terminal so a double-clicked .app does not keep a
// console window around. Harmless when launched from a shell.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod config;
mod import;
mod render;
mod resume;
mod theme;
mod typst_engine;
mod vault;
mod views;

fn main() {
    app::run();
}

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
// Test-only until the ATS screen lands (B1): what is here today is the
// conformance harness and the readers it measures with, and a module the
// binary cannot reach is dead code to the compiler however useful it is.
#[cfg(test)]
mod ats;
mod config;
mod import;
mod logging;
mod render;
mod theme;
mod update;
mod vault;
mod views;

// The engine lives in `dockcv-core` now — see that crate's manifest for why.
// Re-exported under the names it had here so every `crate::resume::…` path in
// the app keeps resolving: the move is a change of *where the code lives*, and
// making it also a change of what everything is called would bury it.
pub use dockcv_core::{resume, typst_engine};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .any(|a| a == "--version" || a == "-v" || a == "-V")
    {
        println!("DockCV {}", app::APP_VERSION);
        return;
    }

    // First, and before any window: everything after this point can fail, and
    // a failure nobody can read is the thing this exists to end.
    logging::init();
    app::run();
}

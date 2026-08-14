//! Application configuration (not user data): remembers the active vault so
//! the app can skip setup on subsequent launches.
//!
//! Stored as readable TOML at `~/.config/dockcv/config.toml`. This is app
//! config, distinct from the user's cvault (which holds the résumés).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;

#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    /// The vault directory the user last opened, if any.
    pub vault: Option<PathBuf>,
    /// The palette the user last chose. Defaults to Slate Dark.
    #[serde(default)]
    pub theme: ThemeMode,
    /// Whether the library's one-line helper has been dismissed. App config
    /// rather than vault data: it records what this *person* has read, not
    /// anything about their documents, so it should not travel with a vault
    /// copied to another machine.
    #[serde(default)]
    pub library_helper_dismissed: bool,
}

fn config_dir() -> PathBuf {
    crate::vault::user_home_dir().join(".config").join("dockcv")
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Load the config, or a default if it doesn't exist / can't be parsed.
pub fn load() -> Config {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// Persist the config (best effort).
pub fn save(config: &Config) {
    let dir = config_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(text) = toml::to_string_pretty(config) {
        let _ = fs::write(config_path(), text);
    }
}

/// Convenience: record the active vault and persist, leaving the rest of the
/// config alone.
pub fn set_vault(vault: PathBuf) {
    let mut config = load();
    config.vault = Some(vault);
    save(&config);
}

/// Convenience: record the chosen palette and persist.
pub fn set_theme(mode: ThemeMode) {
    let mut config = load();
    config.theme = mode;
    save(&config);
}

/// Convenience: remember that the library helper line has been dismissed.
pub fn dismiss_library_helper() {
    let mut config = load();
    config.library_helper_dismissed = true;
    save(&config);
}

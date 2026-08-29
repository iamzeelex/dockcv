//! Application configuration (not user data): remembers the active vault so
//! the app can skip setup on subsequent launches.
//!
//! Stored as readable TOML at `~/.config/dockcv/config.toml`. This is app
//! config, distinct from the user's cvault (which holds the résumés).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;
use crate::update::Channel;

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
    /// How often DockCV may ask whether a newer version exists — by word, so
    /// the file stays readable and says what it does.
    ///
    /// Absent means [`Channel::Manual`]: the button works, nothing runs on its
    /// own. Somebody who never opens Settings never makes a network request
    /// they did not press a button for (US-10, `update.rs`).
    #[serde(default)]
    pub update_check: String,
    /// `YYYY-MM-DD` of the last completed check, so the weekly one knows
    /// whether it is due. A date and not a timestamp: the question is "was it
    /// this week", and a clock reading is more about the user than the answer
    /// needs.
    #[serde(default)]
    pub update_last_checked: String,
    /// A version the user chose to skip. Suppresses the banner for that one
    /// release and nothing else — the next one is announced normally.
    #[serde(default)]
    pub update_skipped: String,
    /// Whether the one-time offer to turn the weekly check on has been
    /// answered. Asked once, in a line, never again — a first-run modal about
    /// updates is exactly the interruption this product exists not to be.
    #[serde(default)]
    pub update_asked: bool,
}

impl Config {
    /// The channel this config selects, defaulting safely.
    pub fn update_channel(&self) -> Channel {
        Channel::from_word(&self.update_check)
    }
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

/// Convenience: record the update channel and persist.
pub fn set_update_channel(channel: Channel) {
    let mut config = load();
    config.update_check = channel.word().to_string();
    // Asking again after the setting has been touched would be asking someone
    // to answer a question they have just answered more precisely.
    config.update_asked = true;
    save(&config);
}

/// Convenience: remember that a check completed today, and what it found.
///
/// The date is written whether or not there was anything new — the weekly
/// check is about how often we ask, not about how often the answer changes.
pub fn record_update_check(today: &str) {
    let mut config = load();
    config.update_last_checked = today.to_string();
    save(&config);
}

/// Convenience: never mention this particular version again.
pub fn skip_update(version: &str) {
    let mut config = load();
    config.update_skipped = version.to_string();
    save(&config);
}

/// Convenience: the one-time offer has been answered, either way.
pub fn mark_update_asked() {
    let mut config = load();
    config.update_asked = true;
    save(&config);
}

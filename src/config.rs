//! Application configuration (not user data): remembers the active vault so
//! the app can skip setup on subsequent launches.
//!
//! Stored as readable TOML at `~/.config/dockcv/config.toml`. This is app
//! config, distinct from the user's cvault (which holds the résumés).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;
use crate::update::Channel;

#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    /// The vault directory the user last opened, if any.
    pub vault: Option<PathBuf>,
    /// Where each document was last exported to.
    ///
    /// A11 wants the second export of a document to default to the first one's
    /// folder, and per document rather than per vault — two CVs are for
    /// different jobs and go to different places. It lives here rather than in
    /// the document because a folder is a fact about *this machine*: a vault
    /// copied to another laptop would carry `/Users/somebody/Downloads`, which
    /// is not a folder there. Newest last, and capped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub export_destinations: Vec<ExportDestination>,
    /// How the gallery is ordered, by word.
    ///
    /// A preference about *looking*, not a fact about any document, so it lives
    /// here rather than in the vault. One order for the person rather than one
    /// per vault: a second vault is not a reason to want a different sort, and
    /// if it ever becomes one, a keyed list is a free addition where changing
    /// this field would not be.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gallery_sort: String,
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

/// One document's last export folder. An array of tables rather than a map,
/// because a TOML key holding an absolute path is unreadable and needs quoting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportDestination {
    /// The document this is about.
    pub document: PathBuf,
    /// The folder its last export went to.
    pub folder: PathBuf,
}

/// How many documents' export folders are remembered.
///
/// Long past a vault anyone has, and the entries that fall off the front are
/// the ones least recently exported — whose folder is the least likely to still
/// be the right guess.
const MAX_EXPORT_DESTINATIONS: usize = 100;

impl Config {
    /// The folder `document` was last exported to, if this machine remembers.
    pub fn export_destination(&self, document: &Path) -> Option<&Path> {
        self.export_destinations
            .iter()
            .rev()
            .find(|row| row.document == document)
            .map(|row| row.folder.as_path())
    }

    /// Remember where `document` was just exported, replacing any earlier
    /// answer for it rather than stacking a second one.
    pub fn remember_export_destination(&mut self, document: &Path, folder: &Path) {
        self.export_destinations
            .retain(|row| row.document != document);
        self.export_destinations.push(ExportDestination {
            document: document.to_path_buf(),
            folder: folder.to_path_buf(),
        });
        let overflow = self
            .export_destinations
            .len()
            .saturating_sub(MAX_EXPORT_DESTINATIONS);
        self.export_destinations.drain(..overflow);
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

//! The vault's own notebooks: the library, the diary, the board, the profiles.
//!
//! Split from `vault.rs` by C15. Each is one atomic file beside the documents,
//! each has a reserved name `list_documents` skips, and each carries a
//! migration for the shape it used to have — which is the whole reason they
//! belong together rather than beside the code that reads a CV.
//!
//! Snapshots are here too. They are not a notebook, but they are the other
//! thing the vault holds that is not a document, and the only one with bytes
//! rather than TOML in it.

use std::fs;
use std::path::{Path, PathBuf};

use crate::resume::model::{ApplicationStatus, Applications, Diary, Library, ProfileCatalog};

use crate::vault::{
    atomic_write, list_documents, load, OnDisk, SaveError, APPLICATIONS_FILE, DIARY_FILE,
    LIBRARY_FILE, PROFILES_FILE, SNAPSHOTS_DIR,
};

fn save_notebook<T: serde::Serialize>(
    path: &Path,
    data: &T,
    seen: Option<OnDisk>,
) -> Result<OnDisk, SaveError> {
    let text =
        toml::to_string_pretty(data).map_err(|e| SaveError::Failed(format!("serialize: {e}")))?;
    let current = OnDisk::read(path);
    if let Some(seen) = seen {
        if current != seen {
            return Err(SaveError::Conflict);
        }
    }
    let written = OnDisk::of(&text);
    if written == current {
        return Ok(current);
    }
    atomic_write(path, &text).map_err(SaveError::Failed)?;
    Ok(written)
}

/// Path of the vault's block library file.
pub fn library_path(vault_dir: &Path) -> PathBuf {
    vault_dir.join(LIBRARY_FILE)
}

/// Load the vault's block library, or an empty one if absent / unreadable.
pub fn load_library(vault_dir: &Path) -> Library {
    load_library_seen(vault_dir).0
}

/// Load the library and what was on disk, for checked writing later.
pub fn load_library_seen(vault_dir: &Path) -> (Library, OnDisk) {
    let path = library_path(vault_dir);
    let on_disk = OnDisk::read(&path);
    let library = fs::read_to_string(&path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    (library, on_disk)
}

/// Atomically save the block library without checking seen disk state.
pub fn save_library(vault_dir: &Path, library: &Library) -> Result<(), String> {
    save_notebook(&library_path(vault_dir), library, None)
        .map(|_| ())
        .map_err(|e| e.message())
}

/// Atomically save the block library, failing with [`SaveError::Conflict`] if the
/// file on disk changed since `seen`.
pub fn save_library_seen(
    vault_dir: &Path,
    library: &Library,
    seen: OnDisk,
) -> Result<OnDisk, SaveError> {
    save_notebook(&library_path(vault_dir), library, Some(seen))
}

/// Path of the vault's diary file.
pub fn diary_path(vault_dir: &Path) -> PathBuf {
    vault_dir.join(DIARY_FILE)
}

/// Load the diary, or an empty one if absent / unreadable.
pub fn load_diary(vault_dir: &Path) -> Diary {
    load_diary_seen(vault_dir).0
}

/// Load the diary and what was on disk, for checked writing later.
pub fn load_diary_seen(vault_dir: &Path) -> (Diary, OnDisk) {
    let path = diary_path(vault_dir);
    let on_disk = OnDisk::read(&path);
    let diary = fs::read_to_string(&path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    (diary, on_disk)
}

/// Atomically save the diary without checking seen disk state.
pub fn save_diary(vault_dir: &Path, diary: &Diary) -> Result<(), String> {
    save_notebook(&diary_path(vault_dir), diary, None)
        .map(|_| ())
        .map_err(|e| e.message())
}

/// Atomically save the diary, failing with [`SaveError::Conflict`] if the
/// file on disk changed since `seen`.
pub fn save_diary_seen(vault_dir: &Path, diary: &Diary, seen: OnDisk) -> Result<OnDisk, SaveError> {
    save_notebook(&diary_path(vault_dir), diary, Some(seen))
}

/// Path of the vault's applications file.
pub fn applications_path(vault_dir: &Path) -> PathBuf {
    vault_dir.join(APPLICATIONS_FILE)
}

/// Load the applications board, or an empty one if absent / unreadable.
pub fn load_applications(vault_dir: &Path) -> Applications {
    load_applications_seen(vault_dir).0
}

/// Load the applications board and what was on disk, for checked writing later.
pub fn load_applications_seen(vault_dir: &Path) -> (Applications, OnDisk) {
    let path = applications_path(vault_dir);
    let on_disk = OnDisk::read(&path);
    let mut applications: Applications = fs::read_to_string(&path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    for entry in &mut applications.entries {
        if entry.status_word.is_empty() {
            entry.status_word = ApplicationStatus::Wishlist.word().to_string();
        }
    }
    for entry in &applications.entries {
        if !entry.status_is_recognised() {
            log::warn!(
                "applications.toml: status \"{}\" is not a word this build knows — \
                 the card reads as Wishlist and the word is kept as written",
                entry.status_word
            );
        }
    }
    (applications, on_disk)
}

/// Atomically save the applications board without checking seen disk state.
pub fn save_applications(vault_dir: &Path, applications: &Applications) -> Result<(), String> {
    save_notebook(&applications_path(vault_dir), applications, None)
        .map(|_| ())
        .map_err(|e| e.message())
}

/// Atomically save the applications board, failing with [`SaveError::Conflict`] if the
/// file on disk changed since `seen`.
pub fn save_applications_seen(
    vault_dir: &Path,
    applications: &Applications,
    seen: OnDisk,
) -> Result<OnDisk, SaveError> {
    save_notebook(&applications_path(vault_dir), applications, Some(seen))
}

/// Path of the vault's reusable layout profiles.
pub fn profiles_path(vault_dir: &Path) -> PathBuf {
    vault_dir.join(PROFILES_FILE)
}

/// Load user profiles. Built-ins live in code, so a missing or unreadable
/// file is exactly an empty user catalog and never prevents the vault opening.
pub fn load_profiles(vault_dir: &Path) -> ProfileCatalog {
    load_profiles_seen(vault_dir).0
}

/// Load user profiles and what was on disk, for checked writing later.
pub fn load_profiles_seen(vault_dir: &Path) -> (ProfileCatalog, OnDisk) {
    let path = profiles_path(vault_dir);
    let on_disk = OnDisk::read(&path);
    let catalog = fs::read_to_string(&path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    (catalog, on_disk)
}

/// Atomically save user profiles without checking seen disk state.
#[allow(dead_code)]
pub fn save_profiles(vault_dir: &Path, profiles: &ProfileCatalog) -> Result<(), String> {
    save_notebook(&profiles_path(vault_dir), profiles, None)
        .map(|_| ())
        .map_err(|e| e.message())
}

/// Atomically save user profiles, failing with [`SaveError::Conflict`] if the
/// file on disk changed since `seen`.
pub fn save_profiles_seen(
    vault_dir: &Path,
    profiles: &ProfileCatalog,
    seen: OnDisk,
) -> Result<OnDisk, SaveError> {
    save_notebook(&profiles_path(vault_dir), profiles, Some(seen))
}

/// Document stems whose working copy or any preset names `profile`.
///
/// Distinct documents, not reference count: `Update ATS-safe (2 CVs)` tells a
/// person how far the write reaches, and one CV with three ATS presets is still
/// one CV.
pub fn documents_using_profile(vault_dir: &Path, profile: &str) -> Vec<String> {
    list_documents(vault_dir)
        .into_iter()
        .filter_map(|path| {
            let doc = load(&path).ok()?;
            let used = doc.layout_profile.as_deref() == Some(profile)
                || doc
                    .presets
                    .iter()
                    .any(|preset| preset.profile.as_deref() == Some(profile));
            used.then(|| {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            })
        })
        .collect()
}

/// Directory PDF snapshots are stored in — `<vault>/snapshots/`.
pub fn snapshots_dir(vault_dir: &Path) -> PathBuf {
    vault_dir.join(SNAPSHOTS_DIR)
}

/// Turn arbitrary user text into a filesystem-safe, lowercase-hyphenated
/// fragment. Shared by `new_doc_path`'s `slugify` in spirit but kept local:
/// a snapshot file name has its own suffix shape (`-vN.pdf`), collapses runs
/// of separators (a company name like `"Bramble Tech / EU"` has adjacent
/// non-alphanumeric characters that would otherwise leave `---` in the file
/// name), and has its own "never produce an empty fragment" fallback
/// (`"application"`, not `"resume"`).
fn slugify_for_snapshot(s: &str) -> String {
    let mut slug = String::with_capacity(s.len());
    let mut last_was_dash = false;
    for c in s.trim().chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "application".to_string()
    } else {
        slug
    }
}

/// Write a snapshot PDF's bytes into `<vault>/snapshots/`, deriving a
/// sanitized file name from the company name and version, and return the
/// file name to store in `Snapshot::file`.
///
/// Does not generate the PDF — producing the bytes is the Typst pipeline's
/// job; this only stores what it is handed.
pub fn save_snapshot(
    vault_dir: &Path,
    bytes: &[u8],
    company: &str,
    version: u32,
) -> Result<String, String> {
    let dir = snapshots_dir(vault_dir);
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let file_name = format!("{}-v{version}.pdf", slugify_for_snapshot(company));
    let path = dir.join(&file_name);
    fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(file_name)
}

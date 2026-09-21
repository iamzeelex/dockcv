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
    list_documents, load, APPLICATIONS_FILE, DIARY_FILE, LIBRARY_FILE, PROFILES_FILE,
    SNAPSHOTS_DIR,
};

/// Path of the vault's block library file.
pub fn library_path(vault_dir: &Path) -> PathBuf {
    vault_dir.join(LIBRARY_FILE)
}

/// Load the vault's block library, or an empty one if absent / unreadable.
pub fn load_library(vault_dir: &Path) -> Library {
    fs::read_to_string(library_path(vault_dir))
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// Atomically save the block library.
pub fn save_library(vault_dir: &Path, library: &Library) -> Result<(), String> {
    let text = toml::to_string_pretty(library).map_err(|e| format!("serialize: {e}"))?;
    let path = library_path(vault_dir);
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename {}: {e}", path.display()))?;
    Ok(())
}

/// Path of the vault's diary file.
pub fn diary_path(vault_dir: &Path) -> PathBuf {
    vault_dir.join(DIARY_FILE)
}

/// Load the diary, or an empty one if absent / unreadable.
pub fn load_diary(vault_dir: &Path) -> Diary {
    fs::read_to_string(diary_path(vault_dir))
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// Atomically save the diary.
pub fn save_diary(vault_dir: &Path, diary: &Diary) -> Result<(), String> {
    let text = toml::to_string_pretty(diary).map_err(|e| format!("serialize: {e}"))?;
    let path = diary_path(vault_dir);
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename {}: {e}", path.display()))?;
    Ok(())
}

/// Path of the vault's applications file.
pub fn applications_path(vault_dir: &Path) -> PathBuf {
    vault_dir.join(APPLICATIONS_FILE)
}

/// Load the applications board, or an empty one if absent / unreadable.
///
/// A missing `applications.toml` (every vault written before this feature)
/// is exactly the empty-board case — never an error. `Applications::normalize`
/// is the migration for files written before `Application::furthest` existed:
/// it raises each entry's `furthest` to at least its current `status`, so an
/// old row's conversion funnel isn't silently zeroed just because the field
/// wasn't there to deserialize.
pub fn load_applications(vault_dir: &Path) -> Applications {
    let mut applications: Applications = fs::read_to_string(applications_path(vault_dir))
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    for entry in &mut applications.entries {
        // An empty status is ours, not the author's.
        //
        // The word is otherwise kept exactly as written, and deliberately: a
        // hand-edited `status = "ofer"` must not be silently rewritten to
        // `wishlist`, because then an offer stops having ever existed. `""` is
        // the one value that rule does not protect — nobody types it. It came
        // from `Application::default` disagreeing with serde's own default
        // (fixed at the source), and every card the app made carried it. So it
        // is filled in rather than warned about for the life of the vault.
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
    applications
}

/// Atomically save the applications board.
pub fn save_applications(vault_dir: &Path, applications: &Applications) -> Result<(), String> {
    let text = toml::to_string_pretty(applications).map_err(|e| format!("serialize: {e}"))?;
    let path = applications_path(vault_dir);
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename {}: {e}", path.display()))?;
    Ok(())
}

/// Path of the vault's reusable layout profiles.
pub fn profiles_path(vault_dir: &Path) -> PathBuf {
    vault_dir.join(PROFILES_FILE)
}

/// Load user profiles. Built-ins live in code, so a missing or unreadable
/// file is exactly an empty user catalog and never prevents the vault opening.
pub fn load_profiles(vault_dir: &Path) -> ProfileCatalog {
    fs::read_to_string(profiles_path(vault_dir))
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// Atomically save user profiles.
pub fn save_profiles(vault_dir: &Path, profiles: &ProfileCatalog) -> Result<(), String> {
    let text = toml::to_string_pretty(profiles).map_err(|e| format!("serialize: {e}"))?;
    let path = profiles_path(vault_dir);
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename {}: {e}", path.display()))?;
    Ok(())
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



//! `cvault` — File-over-App local storage.
//!
//! A cvault is a plain directory (`~/cvault`) of human-readable TOML documents,
//! one file per resume. No hidden database, no lock-in: open a file in any text
//! editor, hand-edit it, sync it via iCloud/Dropbox/git. The app just reads and
//! writes these files.

use std::fs;
use std::path::{Path, PathBuf};

use std::time::{SystemTime, UNIX_EPOCH};

use crate::resume::model::{Applications, Diary, Library, ResumeDoc};

const VAULT_DIR_NAME: &str = "cvault";
const LIBRARY_FILE: &str = "library.toml";
const DIARY_FILE: &str = "diary.toml";
const APPLICATIONS_FILE: &str = "applications.toml";
const SNAPSHOTS_DIR: &str = "snapshots";
/// Reserved files that live in the vault but are NOT CV documents.
const RESERVED_FILES: [&str; 3] = [LIBRARY_FILE, DIARY_FILE, APPLICATIONS_FILE];

/// Cross-platform resolution of the current user's home directory.
pub fn user_home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(
            |_| match (std::env::var("HOMEDRIVE"), std::env::var("HOMEPATH")) {
                (Ok(drive), Ok(path)) => Ok(format!("{drive}{path}")),
                _ => Err(std::env::VarError::NotPresent),
            },
        )
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Whether `dir` can be opened as a vault at all.
///
/// Deliberately just "is a directory": the startup path calls this on the
/// remembered vault, and a stricter test here would hide an existing vault from
/// its owner the first time they upgraded. [`vault_shape`] is the judgement;
/// this is the precondition.
pub fn is_vault(dir: &Path) -> bool {
    dir.is_dir()
}

/// How much a directory looks like a vault, for the picker to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultShape {
    /// Carries the marker file DockCV writes when it adopts a folder.
    Marked,
    /// No marker, but the contents are a vault's: notebooks, a `.trash`, or
    /// `.toml` files that parse as documents. Every vault made before the
    /// marker existed looks like this.
    Recognized,
    /// Empty, or as good as. A fresh folder is a perfectly good new vault.
    Empty,
    /// A directory full of things that are not CVs. Picking `~/` or
    /// `~/Library` lands here — and used to be accepted silently, after which
    /// the gallery parsed every unrelated `.toml` on the machine and drew the
    /// failures as cards reading "unreadable file".
    Unrecognized {
        /// How many `.toml` files are in it that are not documents. The number
        /// is what makes the warning concrete rather than vague.
        stray_toml: usize,
    },
}

/// Marker file written when DockCV adopts a folder as a vault.
///
/// Dotfile, TOML, one line — File-over-App all the way down: the folder says
/// what it is in a format the user can read, and nothing depends on a database
/// entry somewhere else.
const MARKER_FILE: &str = ".cvault";

/// Classify a directory before making it the user's vault.
pub fn vault_shape(dir: &Path) -> VaultShape {
    if !dir.is_dir() {
        return VaultShape::Unrecognized { stray_toml: 0 };
    }
    if dir.join(MARKER_FILE).exists() {
        return VaultShape::Marked;
    }
    if dir.join(LIBRARY_FILE).exists()
        || dir.join(DIARY_FILE).exists()
        || dir.join(APPLICATIONS_FILE).exists()
        || dir.join(".trash").is_dir()
    {
        return VaultShape::Recognized;
    }

    let documents = list_documents(dir);
    if documents.is_empty() {
        // No `.toml` at all. Whatever else is in there, nothing will be
        // mistaken for a CV, so this is a usable — if unusual — new vault.
        return VaultShape::Empty;
    }

    // Parse rather than count: a folder of *documents* is a vault whoever made
    // it, and a folder of unrelated TOML is not. Bounded by the same 10 MB
    // per-file guard `load` applies, and only reached for a folder that has no
    // marker and no notebooks — which is once, at the picker.
    let parses = documents.iter().filter(|p| load(p).is_ok()).count();
    if parses > 0 {
        VaultShape::Recognized
    } else {
        VaultShape::Unrecognized {
            stray_toml: documents.len(),
        }
    }
}

/// Write the marker, so the folder answers for itself next time.
///
/// Best effort: a vault on a read-only volume is still a vault to read, and
/// refusing to open it over a missing marker would be the tail wagging the dog.
pub fn mark_as_vault(dir: &Path) {
    let path = dir.join(MARKER_FILE);
    if path.exists() {
        return;
    }
    let _ = fs::write(
        &path,
        format!(
            "# This folder is a DockCV vault.\ncreated = \"{}\"\n",
            today_iso()
        ),
    );
}

/// Create a fresh `cvault` directory inside `parent` and return its path.
pub fn create_vault(parent: &Path) -> Result<PathBuf, String> {
    let dir = parent.join(VAULT_DIR_NAME);
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    mark_as_vault(&dir);
    Ok(dir)
}

/// Human-readable summary of a document, for the gallery cards.
///
/// `Clone` because the cache owns one copy and the cards take theirs by value —
/// a few small string clones per visible card, in place of re-parsing the vault.
#[derive(Clone)]
pub struct DocMeta {
    pub path: PathBuf,
    /// The document's file stem — its name on disk, and the only thing that
    /// tells two CVs apart when both belong to the same person with the same
    /// job title. Shown on the card for that reason, and because it answers
    /// "where are my files" in passing (review P-11).
    pub stem: String,
    /// Person name, or the file stem if the document can't be read.
    pub name: String,
    pub label: String,
    pub presets: usize,
    /// Preset names, in document order. A count answers "how many"; the names
    /// answer "which", which is the question a returning user actually has —
    /// and P-01's complaint is precisely that presets are invisible.
    pub preset_names: Vec<String>,
    /// True when the file couldn't be parsed as a document.
    pub unreadable: bool,
    /// File's last-modified time, seconds since the UNIX epoch. `None` if the
    /// file couldn't be stat'd. Feed to `relative_time` (with the current
    /// time) for the gallery's "updated N ago" line — this field is
    /// deliberately a raw timestamp, not a formatted string, so formatting
    /// stays pure and testable at the boundary.
    pub modified_secs: Option<u64>,
}

impl DocMeta {
    /// A document is a "draft" when it hasn't been organised into any preset
    /// yet. Derived, not stored: a stored flag would drift from `presets` the
    /// moment a preset is added or removed elsewhere.
    pub fn is_draft(&self) -> bool {
        self.presets == 0
    }
}

/// Derive a document's card summary from an already-parsed document, or from
/// nothing when it failed to parse. Split out so `load_all` can hand over the
/// document it just read instead of provoking a second read.
fn meta_from(path: &Path, doc: Option<&ResumeDoc>) -> DocMeta {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string();

    let modified_secs = modified_epoch_secs(path);

    match doc {
        Some(doc) => {
            let basics = doc.profile.active();
            DocMeta {
                name: if basics.name.trim().is_empty() {
                    stem.clone()
                } else {
                    basics.name.clone()
                },
                label: basics.label.clone(),
                presets: doc.presets.len(),
                preset_names: doc.presets.iter().map(|p| p.name.clone()).collect(),
                unreadable: false,
                path: path.to_path_buf(),
                stem,
                modified_secs,
            }
        }
        None => DocMeta {
            name: stem.clone(),
            label: String::new(),
            presets: 0,
            preset_names: Vec::new(),
            unreadable: true,
            path: path.to_path_buf(),
            stem,
            modified_secs,
        },
    }
}

/// Metadata **and** the parsed document for every file, from one read each.
///
/// `read_meta` parses a document to derive its summary and then drops it, so
/// anything that needs the document itself would otherwise parse the whole
/// vault a second time. `None` is a file that did not parse — the gallery
/// still draws it (as "unreadable file"), so it stays in the list rather than
/// being filtered out here.
pub fn load_all(vault_dir: &Path) -> Vec<(DocMeta, Option<ResumeDoc>)> {
    list_documents(vault_dir)
        .into_iter()
        .map(|path| {
            let doc = load(&path).ok();
            (meta_from(&path, doc.as_ref()), doc)
        })
        .collect()
}

/// Pick a free `<base>.toml` path in the vault (deduping with `-2`, `-3`, …).
pub fn new_doc_path(vault_dir: &Path, base: &str) -> PathBuf {
    let slug = slugify(base);
    let mut candidate = vault_dir.join(format!("{slug}.toml"));
    let mut n = 2;
    while candidate.exists() {
        candidate = vault_dir.join(format!("{slug}-{n}.toml"));
        n += 1;
    }
    candidate
}

/// Create a new document file from `doc` and return its path.
pub fn create_document(vault_dir: &Path, doc: &ResumeDoc, base: &str) -> Result<PathBuf, String> {
    let path = new_doc_path(vault_dir, base);
    save(doc, &path)?;
    Ok(path)
}

/// The file's last-modified time, seconds since the UNIX epoch, if the file
/// can be stat'd. Feeds `DocMeta::modified_secs`.
fn modified_epoch_secs(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Format an elapsed duration as the gallery's compact relative-time copy
/// ("just now", "3m ago", "2h ago", "6d ago", "3w ago", "2mo ago", "1y ago").
///
/// Deliberately pure — takes both timestamps as arguments rather than reading
/// the clock itself, so the boundary behavior is testable without freezing
/// time. `then_secs` and `now_secs` are both seconds since the UNIX epoch.
///
/// Months and years are calendar-approximate (30-day month, 365-day year) —
/// exact enough for "how stale is this document", not a calendar library.
///
/// If `then_secs` is after `now_secs` (clock skew, or a file copied with a
/// future mtime), the elapsed time is clamped to zero and this returns
/// "just now" rather than underflowing or panicking.
pub fn relative_time(then_secs: u64, now_secs: u64) -> String {
    let diff = now_secs.saturating_sub(then_secs);

    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    const WEEK: u64 = 7 * DAY;
    const MONTH: u64 = 30 * DAY;
    const YEAR: u64 = 365 * DAY;

    if diff < MINUTE {
        "just now".to_string()
    } else if diff < HOUR {
        format!("{}m ago", diff / MINUTE)
    } else if diff < DAY {
        format!("{}h ago", diff / HOUR)
    } else if diff < WEEK {
        format!("{}d ago", diff / DAY)
    } else if diff < MONTH {
        format!("{}w ago", diff / WEEK)
    } else if diff < YEAR {
        format!("{}mo ago", diff / MONTH)
    } else {
        format!("{}y ago", diff / YEAR)
    }
}

/// Rename a document's file, returning its new path.
///
/// File-over-App: a document's name **is** its file name, so renaming is a
/// filesystem move rather than a field somewhere. The new name is slugified
/// the same way `new_doc_path` slugifies a template name, so a user typing
/// "FAANG concise" gets `faang-concise.toml` and never a name the shell has
/// to quote.
///
/// Refuses to overwrite: if the target exists, the caller is told rather than
/// silently losing a document.
pub fn rename_document(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    let vault_dir = path.parent().ok_or("document has no parent directory")?;
    // Checked on the *input*, not the slug: `slugify` substitutes "resume"
    // for anything that reduces to nothing, which is right for a template
    // name and wrong here — a user who cleared the field meant to cancel, not
    // to name their CV "resume".
    if new_name.trim().is_empty() {
        return Err("a document needs a name".to_string());
    }
    let slug = slugify(new_name);
    let dest = vault_dir.join(format!("{slug}.toml"));
    if dest == path {
        return Ok(dest);
    }
    if dest.exists() {
        return Err(format!("“{slug}” already exists in this vault"));
    }
    fs::rename(path, &dest).map_err(|e| format!("rename: {e}"))?;
    Ok(dest)
}

/// Duplicate a document, returning the new path.
pub fn duplicate_document(path: &Path) -> Result<PathBuf, String> {
    let vault_dir = path.parent().ok_or("document has no parent directory")?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("cv");
    let dest = new_doc_path(vault_dir, &format!("{stem}-copy"));
    fs::copy(path, &dest).map_err(|e| format!("copy: {e}"))?;
    Ok(dest)
}

/// Number of items currently in the vault's trash.
pub fn trash_count(vault_dir: &Path) -> usize {
    fs::read_dir(vault_dir.join(".trash"))
        .map(|entries| entries.flatten().count())
        .unwrap_or(0)
}

/// Permanently empty the vault's trash (user-triggered from Settings).
pub fn empty_trash(vault_dir: &Path) -> Result<(), String> {
    let trash = vault_dir.join(".trash");
    if trash.exists() {
        fs::remove_dir_all(&trash).map_err(|e| format!("empty trash: {e}"))?;
    }
    Ok(())
}

/// "Delete" a document by moving it into the vault's `.trash` folder — a
/// reversible delete rather than a destructive one.
pub fn delete_document(path: &Path) -> Result<(), String> {
    let vault_dir = path.parent().ok_or("document has no parent directory")?;
    let trash = vault_dir.join(".trash");
    fs::create_dir_all(&trash).map_err(|e| format!("create .trash: {e}"))?;
    let name = path.file_name().ok_or("document has no file name")?;

    // Never onto an occupied name. `fs::rename` replaces the destination
    // silently on Unix, so deleting `cv.toml`, creating a new one, and deleting
    // that too destroyed the first — inside the folder whose entire job is to
    // make deletion reversible.
    let dest = free_trash_path(&trash, name);
    fs::rename(path, &dest).map_err(|e| format!("move to .trash: {e}"))?;
    Ok(())
}

/// A path in `trash` that nothing occupies, deduping with `-2`, `-3`, … before
/// the extension. Mirrors `new_doc_path`'s scheme so a trashed file still looks
/// like the document it came from.
fn free_trash_path(trash: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let candidate = trash.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let name = Path::new(name);
    let stem = name.file_stem().and_then(|s| s.to_str()).unwrap_or("cv");
    let ext = name.extension().and_then(|s| s.to_str()).unwrap_or("toml");
    // Bounded: a user who has trashed the same name a thousand times has a
    // different problem, and an unbounded loop here would be a hang.
    for n in 2..1000 {
        let candidate = trash.join(format!("{stem}-{n}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    trash.join(format!("{stem}-{}.{ext}", today_iso()))
}

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
    let applications: Applications = fs::read_to_string(applications_path(vault_dir))
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
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

/// Today's date as `YYYY-MM-DD`, computed from the system clock without any
/// date-library dependency (Howard Hinnant's civil-from-days algorithm).
pub fn today_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The date `days` ago, as `YYYY-MM-DD`.
///
/// Same clock and same algorithm as [`today_iso`], so a window counted back
/// from today and the dates it is compared against cannot disagree about what
/// day it is.
pub fn iso_days_ago(days: i64) -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64 - days);
    format!("{y:04}-{m:02}-{d:02}")
}

pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

fn slugify(s: &str) -> String {
    let slug: String = s
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "resume".to_string()
    } else {
        slug
    }
}

/// List the document files (`*.toml`) in a vault.
pub fn list_documents(vault_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(vault_dir) else {
        return Vec::new();
    };
    let mut docs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
        .filter(|p| {
            // Exclude reserved notebooks (library/diary) — they are not CVs.
            !p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| RESERVED_FILES.contains(&n))
        })
        .collect();
    docs.sort();
    docs
}

const MAX_VAULT_DOC_SIZE: u64 = 10 * 1024 * 1024; // 10 MB

/// Load a document from a TOML file.
pub fn load(path: &Path) -> Result<ResumeDoc, String> {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > MAX_VAULT_DOC_SIZE {
            return Err(format!(
                "document exceeds size limit of {} MB",
                MAX_VAULT_DOC_SIZE / (1024 * 1024)
            ));
        }
    }
    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Serialize a document to its TOML representation.
pub fn to_toml(doc: &ResumeDoc) -> Result<String, String> {
    toml::to_string_pretty(doc).map_err(|e| format!("serialize: {e}"))
}

/// Atomically save a document to a TOML file (write to a temp file, then
/// rename) so a crash mid-write can't corrupt the existing file.
pub fn save(doc: &ResumeDoc, path: &Path) -> Result<(), String> {
    let text = to_toml(doc)?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::resume::model::{Diary, Library, Work};
    use crate::resume::{altacv, model::ResumeDoc};

    #[test]
    fn list_documents_excludes_reserved_notebooks() {
        let dir = std::env::temp_dir().join(format!("dockcv-list-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "alpha.toml",
            "library.toml",
            "diary.toml",
            "applications.toml",
        ] {
            std::fs::write(dir.join(name), "x = 1\n").unwrap();
        }
        let docs = super::list_documents(&dir);
        let names: Vec<String> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            .collect();
        assert!(names.contains(&"alpha.toml".to_string()));
        assert!(!names.contains(&"library.toml".to_string()));
        assert!(!names.contains(&"diary.toml".to_string()));
        assert!(!names.contains(&"applications.toml".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A document that will not parse must come back as an error and be left
    /// **byte for byte** as it was.
    ///
    /// The editor used to answer a failed load by seeding the bundled AltaCV
    /// sample and writing it to that same path, so a stray character in a file
    /// the product tells people to hand-edit destroyed the CV on one click.
    /// Reading is now `Shell::open_doc`'s job and `Root::new` takes a document
    /// it cannot have failed to load — but the property the fix rests on is
    /// this one, and it belongs where the loading lives.
    #[test]
    fn a_document_that_will_not_parse_is_left_exactly_as_it_is() {
        let dir = std::env::temp_dir().join(format!("dockcv-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp vault");
        let path = dir.join("hand-edited.toml");

        // A real document with one line mangled the way a person would.
        let doc = ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
        let good = super::to_toml(&doc).expect("serializes");
        let broken = good.replace("[profile]", "[profile");
        assert_ne!(broken, good, "the fixture must actually be broken");
        std::fs::write(&path, &broken).expect("write");

        assert!(
            super::load(&path).is_err(),
            "a broken document must not load"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("still there"),
            broken,
            "a failed load must not rewrite, repair or replace the file"
        );

        // And the vault still lists it, so the gallery can show it and say so
        // rather than pretending the document does not exist.
        let listed = super::list_documents(&dir);
        assert_eq!(listed, vec![path.clone()]);
        assert!(super::meta_from(&path, super::load(&path).ok().as_ref()).unreadable);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The trash exists to make deletion reversible, so a delete must never
    /// destroy something already in it. `fs::rename` replaces the destination
    /// silently on Unix, so this was a real way to lose a document permanently
    /// through the reversible path.
    /// L-8: a typo in a hand-edited `status` used to cost the record it named.
    /// The word read as `Wishlist` — the right trade, one bad word costing one
    /// card rather than the board — and was then **written back** as
    /// `"wishlist"` on the next save, so `status = "ofer"` quietly turned an
    /// offer into a wishlist entry with nothing left to notice.
    #[test]
    fn a_status_this_build_cannot_read_survives_a_save() {
        let dir = std::env::temp_dir().join(format!("dockcv-status-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp vault");

        std::fs::write(
            super::applications_path(&dir),
            "[[entries]]\ncompany = \"Acme\"\nrole = \"Engineer\"\nstatus = \"ofer\"\n",
        )
        .expect("write board");

        let applications = super::load_applications(&dir);
        assert_eq!(
            applications.entries[0].status(),
            crate::resume::model::ApplicationStatus::Wishlist,
            "an unknown word still reads as Wishlist, so the board stays usable"
        );

        super::save_applications(&dir, &applications).expect("save");
        let text = std::fs::read_to_string(super::applications_path(&dir)).expect("read back");
        assert!(
            text.contains("status = \"ofer\""),
            "the user's own word must come back out; got:\n{text}"
        );

        // And a word we *do* understand is still written from the enum, so a
        // card moved on the board writes where it was moved to.
        let mut applications = super::load_applications(&dir);
        applications.entries[0]
            .advance_to(crate::resume::model::ApplicationStatus::Offer, "2026-08-21");
        super::save_applications(&dir, &applications).expect("save");
        let text = std::fs::read_to_string(super::applications_path(&dir)).expect("read back");
        assert!(text.contains("status = \"offer\""), "got:\n{text}");
        assert!(
            !text.contains("ofer\""),
            "the typo is gone once it is corrected"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A card built in code sets the enum and leaves the word at its default;
    /// the file must say what the enum means, not what the default was.
    #[test]
    fn a_card_created_in_code_writes_the_status_it_was_given() {
        let dir = std::env::temp_dir().join(format!("dockcv-status-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp vault");

        let mut applications = crate::resume::model::Applications::default();
        applications
            .entries
            .push(crate::resume::model::Application {
                company: "Acme".into(),
                status_word: crate::resume::model::ApplicationStatus::Applied
                    .word()
                    .into(),
                ..Default::default()
            });
        super::save_applications(&dir, &applications).expect("save");

        let text = std::fs::read_to_string(super::applications_path(&dir)).expect("read back");
        assert!(text.contains("status = \"applied\""), "got:\n{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every schema change needs a forward migration and a round-trip test
    /// (CLAUDE.md). `LayoutSettings::skills` is `#[serde(default)]`, so a
    /// document written before it existed must still load — and must load as
    /// the arrangement it was actually printed with, not as whichever variant
    /// happens to be listed first.
    #[test]
    fn a_document_without_a_skills_style_loads_as_the_one_it_was_printed_with() {
        use crate::resume::model::SkillsStyle;

        let dir = std::env::temp_dir().join(format!("dockcv-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp vault");
        let path = dir.join("cv.toml");

        // Save with the default, then strip the key the way a file written
        // before this field existed would not have had it at all.
        let doc = crate::resume::model::ResumeDoc::from_resume(
            crate::resume::model::Resume::default(),
            "Base",
        );
        super::save(&doc, &path).expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        let without: String = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("skills = "))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!without.contains("skills = "), "the key really is gone");
        std::fs::write(&path, &without).expect("write");

        let loaded = super::load(&path).expect("an older document still loads");
        assert_eq!(
            loaded.layout.skills.style,
            SkillsStyle::Rows,
            "an older file must keep rendering the way it did"
        );

        // And a chosen style round-trips.
        let mut doc = loaded;
        doc.layout.skills.style = SkillsStyle::Bubbles;
        super::save(&doc, &path).expect("save");
        assert_eq!(
            super::load(&path).expect("reload").layout.skills.style,
            SkillsStyle::Bubbles
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trashing_the_same_name_twice_keeps_both() {
        let dir = std::env::temp_dir().join(format!("dockcv-trash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp vault");

        let mut first = ResumeDoc::from_resume(crate::resume::model::Resume::default(), "Base");
        first.profile.active_mut().name = "The first one".into();
        let path = super::create_document(&dir, &first, "cv").expect("create");
        super::delete_document(&path).expect("delete");

        // Same name again, different contents.
        let mut second = ResumeDoc::from_resume(crate::resume::model::Resume::default(), "Base");
        second.profile.active_mut().name = "The second one".into();
        let path = super::create_document(&dir, &second, "cv").expect("recreate");
        assert_eq!(
            path.file_name().unwrap(),
            "cv.toml",
            "the name is free again"
        );
        super::delete_document(&path).expect("delete again");

        assert_eq!(super::trash_count(&dir), 2, "both must survive");
        let names: Vec<String> = std::fs::read_dir(dir.join(".trash"))
            .expect("trash")
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .collect();
        assert!(names.contains(&"cv.toml".to_string()), "{names:?}");
        assert!(names.contains(&"cv-2.toml".to_string()), "{names:?}");

        let mut recovered: Vec<String> = std::fs::read_dir(dir.join(".trash"))
            .expect("trash")
            .flatten()
            .filter_map(|e| super::load(&e.path()).ok())
            .map(|d| d.profile.active().name.clone())
            .collect();
        recovered.sort();
        assert_eq!(recovered, vec!["The first one", "The second one"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The picker used to accept any directory, so choosing `~/` made the
    /// gallery parse every unrelated `.toml` on the machine and draw the
    /// failures as cards. These are the four answers that decides between.
    #[test]
    fn a_directory_is_classified_before_it_becomes_a_vault() {
        use super::VaultShape;

        let root = std::env::temp_dir().join(format!("dockcv-shape-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let make = |name: &str| {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).expect("temp dir");
            dir
        };

        // A folder with nothing in it is a perfectly good new vault.
        assert_eq!(super::vault_shape(&make("empty")), VaultShape::Empty);

        // A real vault, made the normal way, carries the marker.
        let created = super::create_vault(&make("parent")).expect("create");
        assert_eq!(super::vault_shape(&created), VaultShape::Marked);

        // A vault from before the marker existed: no `.cvault`, but its
        // contents give it away. This is the case that must not regress —
        // failing it would hide an existing vault from its owner.
        let legacy = make("legacy");
        let doc = ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
        super::create_document(&legacy, &doc, "cv").expect("create");
        assert_eq!(super::vault_shape(&legacy), VaultShape::Recognized);

        // …and one recognised by its notebooks alone, with no documents yet.
        let notebooks = make("notebooks");
        super::save_diary(&notebooks, &Diary::default()).expect("save");
        assert_eq!(super::vault_shape(&notebooks), VaultShape::Recognized);

        // A folder of TOML that is not CVs — a config directory, say.
        let strays = make("strays");
        for name in ["Cargo.toml", "settings.toml", "rust-toolchain.toml"] {
            std::fs::write(strays.join(name), "[package]\nname = \"x\"\n").expect("write");
        }
        assert_eq!(
            super::vault_shape(&strays),
            VaultShape::Unrecognized { stray_toml: 3 }
        );

        // Adopting it anyway leaves the marker, so it stops asking.
        super::mark_as_vault(&strays);
        assert_eq!(super::vault_shape(&strays), VaultShape::Marked);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The marker is a readable file, not a magic byte — and it is not a
    /// document, so it must never show up in the gallery.
    #[test]
    fn the_marker_is_plain_text_and_is_not_a_document() {
        let dir = std::env::temp_dir().join(format!("dockcv-marker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp vault");

        super::mark_as_vault(&dir);
        let text = std::fs::read_to_string(dir.join(".cvault")).expect("marker exists");
        assert!(text.contains("DockCV vault"), "{text}");
        assert!(
            toml::from_str::<toml::Value>(&text).is_ok(),
            "the marker must parse as TOML: {text}"
        );
        assert!(super::list_documents(&dir).is_empty());

        // Writing it twice must not clobber the original creation date.
        super::mark_as_vault(&dir);
        assert_eq!(std::fs::read_to_string(dir.join(".cvault")).unwrap(), text);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn civil_from_days_known_values() {
        assert_eq!(super::civil_from_days(0), (1970, 1, 1));
        assert_eq!(super::civil_from_days(18_628), (2021, 1, 1));
    }

    #[test]
    fn relative_time_boundaries() {
        use super::relative_time as rt;

        assert_eq!(rt(1000, 1000), "just now"); // 0s
        assert_eq!(rt(1000, 1000 + 59), "just now"); // 59s
        assert_eq!(rt(1000, 1000 + 60), "1m ago"); // 60s
        assert_eq!(rt(1000, 1000 + 59 * 60), "59m ago"); // 59m
        assert_eq!(rt(1000, 1000 + 60 * 60), "1h ago"); // 60m
        assert_eq!(rt(1000, 1000 + 23 * 3600), "23h ago"); // 23h
        assert_eq!(rt(1000, 1000 + 24 * 3600), "1d ago"); // 24h
        assert_eq!(rt(1000, 1000 + 6 * 86_400), "6d ago"); // 6d
        assert_eq!(rt(1000, 1000 + 7 * 86_400), "1w ago"); // 7d
        assert_eq!(rt(1000, 1000 + 27 * 86_400), "3w ago"); // 27d
        assert_eq!(rt(1000, 1000 + 28 * 86_400), "4w ago"); // 28d

        // Clock skew / copied file with a future mtime: must not underflow
        // or panic. Decision: treat as elapsed-zero, same as "0s".
        assert_eq!(rt(2000, 1000), "just now");
    }

    #[test]
    fn is_draft_derives_from_presets() {
        let with_presets = super::DocMeta {
            path: std::path::PathBuf::new(),
            stem: "cv".into(),
            preset_names: Vec::new(),
            name: "A".into(),
            label: String::new(),
            presets: 1,
            unreadable: false,
            modified_secs: None,
        };
        assert!(!with_presets.is_draft());
        let no_presets = super::DocMeta {
            presets: 0,
            ..with_presets
        };
        assert!(no_presets.is_draft());
    }

    #[test]
    fn read_meta_populates_modified_secs() {
        let dir =
            std::env::temp_dir().join(format!("dockcv-meta-mtime-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.toml");
        std::fs::write(&path, "").unwrap();

        let meta = super::meta_from(&path, super::load(&path).ok().as_ref());
        assert!(meta.modified_secs.is_some());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Diaries written before `source_doc` existed must still load. A new field
    /// that breaks old vaults is a data-loss bug, not a schema change.
    #[test]
    fn diary_without_source_doc_still_loads() {
        let old = "[[entries]]\ndate = \"2026-06-18\"\ntext = \"Cut p99 latency in half\"\n";
        let diary: Diary = toml::from_str(old).expect("a pre-source_doc diary must load");
        assert_eq!(diary.entries.len(), 1);
        assert!(diary.entries[0].source_doc.is_none());

        // And the new field round-trips when it is set.
        let mut with_source = diary;
        with_source.entries[0].source_doc = Some("sofiia-senior-swe".into());
        let text = toml::to_string_pretty(&with_source).expect("serializes");
        let back: Diary = toml::from_str(&text).expect("round-trips");
        assert_eq!(
            back.entries[0].source_doc.as_deref(),
            Some("sofiia-senior-swe")
        );
    }

    /// `used_in` arrived with `Use in a CV` (US-06). A diary written before it
    /// existed must load with the list empty, and an entry that has never been
    /// promoted must not gain a key — the vault is the user's own files, often
    /// under git, and a diff full of `used_in = []` is noise.
    #[test]
    fn a_diary_without_used_in_still_loads_and_gains_no_key() {
        let old = "[[entries]]\ndate = \"2026-06-18\"\ntext = \"Cut p99 latency in half\"\n";
        let diary: Diary = toml::from_str(old).expect("a pre-used_in diary must load");
        assert!(diary.entries[0].used_in.is_empty());

        let untouched = toml::to_string_pretty(&diary).expect("serializes");
        assert!(
            !untouched.contains("used_in"),
            "an entry that was never promoted must not gain the key:\n{untouched}"
        );

        // …and it round-trips once a win actually goes into a CV.
        let mut promoted = diary;
        promoted.entries[0].used_in = vec!["sofiia-senior-swe".into(), "sofiia-em".into()];
        let text = toml::to_string_pretty(&promoted).expect("serializes");
        let back: Diary = toml::from_str(&text).expect("round-trips");
        assert_eq!(
            back.entries[0].used_in,
            vec!["sofiia-senior-swe", "sofiia-em"]
        );
    }

    /// The confidential mark (US-36) is additive, and an unmarked entry must
    /// gain no key — the vault is the user's own files, often under git.
    #[test]
    fn a_diary_without_the_confidential_mark_still_loads() {
        let old = "[[entries]]\ndate = \"2026-06-18\"\ntext = \"Cut p99 latency in half\"\n";
        let diary: Diary = toml::from_str(old).expect("a pre-confidential diary must load");
        assert!(!diary.entries[0].confidential);

        let untouched = toml::to_string_pretty(&diary).expect("serializes");
        assert!(
            !untouched.contains("confidential"),
            "an unmarked entry must not gain the key:\n{untouched}"
        );

        let mut marked = diary;
        marked.entries[0].confidential = true;
        let text = toml::to_string_pretty(&marked).expect("serializes");
        assert!(text.contains("confidential = true"));
        let back: Diary = toml::from_str(&text).expect("round-trips");
        assert!(back.entries[0].confidential);
    }

    /// Role and tags arrived after entries were already on disk. A diary
    /// written before they existed must load with both empty, and neither may
    /// reach the file until it holds something — a `role = ""` line in every
    /// entry would be noise in a format whose point is being readable.
    #[test]
    fn diary_without_role_or_tags_still_loads() {
        let old = "[[entries]]\ndate = \"2026-06-18\"\ntext = \"Cut p99 latency in half\"\n";
        let diary: Diary = toml::from_str(old).expect("a pre-role diary must load");
        assert_eq!(diary.entries[0].role, "");
        assert!(diary.entries[0].tags.is_empty());

        let empty = toml::to_string_pretty(&diary).expect("serializes");
        assert!(!empty.contains("role"), "an empty role must not be written");
        assert!(!empty.contains("tags"), "empty tags must not be written");

        let mut tagged = diary;
        tagged.entries[0].role = "Acme Corp · Senior SWE".into();
        tagged.entries[0].tags = vec!["performance".into(), "architecture".into()];
        let text = toml::to_string_pretty(&tagged).expect("serializes");
        let back: Diary = toml::from_str(&text).expect("round-trips");
        assert_eq!(back.entries[0].role, "Acme Corp · Senior SWE");
        assert_eq!(back.entries[0].tags, vec!["performance", "architecture"]);
    }

    /// O-13: visibility is part of what a preset selects. A document written
    /// before it existed must load with everything visible, an empty list must
    /// not reach the file, and — the part that matters — hiding has to reach
    /// the *rendered* document, not just the sidebar.
    #[test]
    fn hiding_a_section_survives_a_round_trip_and_reaches_the_render() {
        use crate::resume::model::SectionKind;

        let mut doc =
            ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
        assert!(
            !doc.compose().certificates.is_empty(),
            "fixture must have certificates"
        );

        doc.set_hidden(SectionKind::Certificates, true);
        assert!(doc.is_hidden(SectionKind::Certificates));
        // The whole point: a hidden section leaves the composed document.
        assert!(doc.compose().certificates.is_empty());
        // …and the sections that were not hidden are untouched.
        assert!(!doc.compose().work.is_empty());

        let text = super::to_toml(&doc).expect("serializes");
        let back: ResumeDoc = toml::from_str(&text).expect("round-trips");
        assert!(back.is_hidden(SectionKind::Certificates));

        // Profile is not hideable — a résumé without a name is broken, not short.
        let mut doc = back;
        doc.set_hidden(SectionKind::Profile, true);
        assert!(!doc.is_hidden(SectionKind::Profile));

        // Showing it again empties the list, and an empty list is not written.
        doc.set_hidden(SectionKind::Certificates, false);
        let text = super::to_toml(&doc).expect("serializes");
        assert!(
            !text.contains("hidden_sections"),
            "an empty list must not be written"
        );
    }

    /// The first per-section override has to survive the disk, and — because
    /// it is the first row of a table the rest of the per-section settings
    /// will land in — an untouched document has to gain no key at all.
    #[test]
    fn a_sections_own_layout_survives_a_round_trip_and_is_absent_until_used() {
        use crate::resume::model::SectionKind;

        let mut doc =
            ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
        let text = super::to_toml(&doc).expect("serializes");
        assert!(
            !text.contains("section_overrides"),
            "a document nobody customised must carry no overrides table"
        );

        doc.set_heading_printed(SectionKind::Profile, false);
        assert!(!doc.prints_heading(SectionKind::Profile));
        assert!(
            doc.prints_heading(SectionKind::Work),
            "one section, not all of them"
        );

        let text = super::to_toml(&doc).expect("serializes");
        let back: ResumeDoc = toml::from_str(&text).expect("round-trips");
        assert!(!back.prints_heading(SectionKind::Profile));
        assert!(back.prints_heading(SectionKind::Work));

        // A per-field departure survives the disk too, and only the field that
        // departed is written — the rest stay absent so they keep following.
        let mut doc = back;
        doc.set_section_overrides(
            SectionKind::Skills,
            crate::resume::model::SectionOverrides {
                heading_style: Some(crate::resume::model::HeadingStyle::Boxed),
                ..doc.section_overrides(SectionKind::Skills)
            },
        );
        let text = super::to_toml(&doc).expect("serializes");
        assert!(text.contains("heading_style"), "{text}");
        assert!(
            !text.contains("heading_case"),
            "a field nobody set was written, so it has stopped following the document"
        );
        let back: ResumeDoc = toml::from_str(&text).expect("round-trips");
        assert_eq!(
            back.headings_for(SectionKind::Skills).style,
            crate::resume::model::HeadingStyle::Boxed
        );
        assert_eq!(
            back.headings_for(SectionKind::Skills).case,
            back.layout.headings.case,
            "the section stopped following the document on a field it never set"
        );

        // Following the document again removes the row rather than storing a
        // row of defaults — "follow the document" is the absence of an entry.
        let mut doc = back;
        doc.set_section_overrides(SectionKind::Skills, Default::default());
        doc.set_heading_printed(SectionKind::Profile, true);
        assert!(doc.section_overrides.is_empty());
        let text = super::to_toml(&doc).expect("serializes");
        assert!(
            !text.contains("section_overrides"),
            "an emptied table must not be written"
        );
    }

    /// Stage history has to survive the disk, and a board written before it
    /// existed has to load without gaining an empty one in every card.
    #[test]
    fn stage_history_round_trips_and_old_boards_gain_no_key() {
        use crate::resume::model::{Application, ApplicationStatus, Applications};

        let mut board = Applications {
            entries: vec![Application {
                company: "Acme".into(),
                created: "2026-06-01".into(),
                ..Default::default()
            }],
        };
        let text = toml::to_string_pretty(&board).expect("serializes");
        assert!(
            !text.contains("history"),
            "a card that has never moved must carry no history table"
        );

        board.entries[0].advance_to(ApplicationStatus::Applied, "2026-06-10");
        board.entries[0].advance_to(ApplicationStatus::Interviewing, "2026-07-02");
        let text = toml::to_string_pretty(&board).expect("serializes");
        let back: Applications = toml::from_str(&text).expect("round-trips");

        let history = &back.entries[0].history;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].at, "2026-06-10");
        assert_eq!(history[0].to, "applied");
        assert_eq!(history[1].to, "interviewing");
        // The word is what round-trips, exactly as `status` does — so a stage
        // this build does not know is not silently rewritten on the way back.
        assert_eq!(
            back.entries[0].stage_before(1),
            ApplicationStatus::Applied,
            "the stage before a move is the one the previous move landed on"
        );
        assert_eq!(back.entries[0].stage_before(0), ApplicationStatus::Wishlist);
    }

    /// Rounds and the closure have to survive the disk, and a board written
    /// before either existed must gain no keys.
    #[test]
    fn rounds_and_closure_round_trip_and_old_boards_gain_no_keys() {
        use crate::resume::model::{Application, Applications, Closure, InterviewRound};

        let mut board = Applications {
            entries: vec![Application {
                company: "Acme".into(),
                ..Default::default()
            }],
        };
        let text = toml::to_string_pretty(&board).expect("serializes");
        assert!(!text.contains("rounds"), "{text}");
        assert!(!text.contains("closed_as"), "{text}");

        board.entries[0].rounds.push(InterviewRound {
            at: "2026-07-02".into(),
            label: "Technical screen".into(),
        });
        board.entries[0].rounds.push(InterviewRound {
            at: "2026-07-16".into(),
            // A round nobody named is still a round that happened.
            label: String::new(),
        });
        board.entries[0].closed_as = Some(Closure::Ghosted);

        let text = toml::to_string_pretty(&board).expect("serializes");
        let back: Applications = toml::from_str(&text).expect("round-trips");
        assert_eq!(back.entries[0].rounds.len(), 2);
        assert_eq!(back.entries[0].rounds[0].label, "Technical screen");
        assert!(back.entries[0].rounds[1].label.is_empty());
        assert_eq!(back.entries[0].closed_as, Some(Closure::Ghosted));
    }

    /// Renaming is a file move, so the two failure modes that matter are
    /// clobbering an existing document and accepting a name the filesystem
    /// cannot hold.
    #[test]
    fn renaming_a_document_never_overwrites_another() {
        let dir = std::env::temp_dir().join(format!("dockcv-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp vault");

        let doc = ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
        let a = super::create_document(&dir, &doc, "first").expect("create a");
        let b = super::create_document(&dir, &doc, "second").expect("create b");

        // A name the user types is slugified, exactly like a new document's.
        let renamed = super::rename_document(&a, "FAANG concise").expect("rename");
        assert!(renamed.ends_with("faang-concise.toml"), "got {renamed:?}");
        assert!(!a.exists(), "the old file is gone");
        assert!(super::load(&renamed).is_ok(), "the document still parses");

        // Onto an occupied name: refused, and both files survive.
        let err = super::rename_document(&renamed, "second").expect_err("must refuse");
        assert!(err.contains("already exists"), "got {err}");
        assert!(renamed.exists() && b.exists());

        // Empty is refused rather than producing `.toml`.
        assert!(super::rename_document(&renamed, "   ").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The gallery's card reads these three facts to tell one CV from
    /// another; a count alone cannot, since two CVs for the same person carry
    /// the same name and the same job title.
    #[test]
    fn doc_meta_carries_what_distinguishes_two_cvs() {
        use crate::resume::model::SectionKind;
        let dir = std::env::temp_dir().join(format!("dockcv-meta-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp vault");

        let mut doc =
            ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
        doc.add_variant(SectionKind::Work);
        doc.add_variant(SectionKind::Skills);
        doc.add_preset("FAANG · concise");
        doc.add_preset("Infra-heavy");
        let path = super::create_document(&dir, &doc, "sean senior swe").expect("create");

        let meta = super::meta_from(&path, super::load(&path).ok().as_ref());
        assert_eq!(meta.stem, "sean-senior-swe");
        assert_eq!(
            meta.preset_names,
            vec!["FAANG · concise".to_string(), "Infra-heavy".to_string()],
            "the card shows preset names, not a count (P-01)"
        );
        assert!(!meta.is_draft());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Section order is data now. A document written before it existed must load,
    /// and a stored order that has gone stale must be repaired rather than
    /// silently dropping sections from the editor.
    #[test]
    fn section_order_defaults_and_repairs_itself() {
        use crate::resume::model::SectionKind;

        let mut doc = ResumeDoc::from_resume(crate::resume::model::Resume::default(), "Base");
        assert_eq!(
            doc.sections(),
            ResumeDoc::SECTIONS.to_vec(),
            "empty means default"
        );

        // A partial order (as an older or hand-edited file might hold) keeps the
        // listed sections first and appends the rest — nothing disappears.
        doc.section_order = vec![SectionKind::Skills, SectionKind::Work];
        let order = doc.sections();
        assert_eq!(&order[..2], &[SectionKind::Skills, SectionKind::Work]);
        assert_eq!(order.len(), ResumeDoc::SECTIONS.len());

        // Duplicates are dropped rather than rendering a section twice.
        doc.section_order = vec![SectionKind::Work, SectionKind::Work];
        assert_eq!(doc.sections().len(), ResumeDoc::SECTIONS.len());

        // Moving is clamped at both ends.
        doc.section_order = Vec::new();
        doc.move_section(SectionKind::Profile, -1);
        assert_eq!(
            doc.sections()[0],
            SectionKind::Profile,
            "cannot move past the top"
        );
        doc.move_section(SectionKind::Profile, 1);
        assert_eq!(doc.sections()[1], SectionKind::Profile);
    }

    /// A document with no custom sections must serialize exactly as it did
    /// before D-9 — no new keys. Vaults are the user's real files under git.
    #[test]
    fn a_document_without_custom_sections_gains_no_new_keys() {
        let doc = ResumeDoc::from_resume(crate::resume::model::Resume::default(), "Base");
        let text = super::to_toml(&doc).expect("serializes");
        assert!(
            !text.contains("custom_sections"),
            "an untouched document should not gain a custom_sections table:\n{text}"
        );
        assert!(
            !text.contains("next_custom_section_id"),
            "an untouched document should not gain the id counter:\n{text}"
        );
    }

    /// A new custom section must actually change the rendered document.
    ///
    /// The renderer skips empty sections, so an unseeded one produced identical
    /// Typst source, the recompile was skipped as a no-op, and "+ Add" looked
    /// like it had done nothing.
    #[test]
    fn a_new_custom_section_changes_the_generated_document() {
        use crate::resume::template;

        let mut doc = ResumeDoc::from_resume(crate::resume::model::Resume::default(), "Base");
        let before = template::generate_for(&doc);

        doc.add_custom_section("Publications");
        let after = template::generate_for(&doc);

        assert_ne!(
            before, after,
            "adding a section must change what gets rendered"
        );
        assert!(
            after.contains("Publications"),
            "the new section's title should reach the source"
        );
    }

    #[test]
    fn library_round_trip() {
        let dir = std::env::temp_dir().join(format!("dockcv-lib-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut library = Library::default();
        library.work.push(Work {
            position: "Engineer".into(),
            name: "Acme".into(),
            ..Default::default()
        });
        super::save_library(&dir, &library).expect("save library");

        let back = super::load_library(&dir);
        assert_eq!(back.work.len(), 1);
        assert_eq!(back.work[0].position, "Engineer");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn applications_round_trip() {
        use crate::resume::model::{Application, ApplicationStatus, Applications};

        let dir = std::env::temp_dir().join(format!("dockcv-apps-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut applications = Applications::default();
        applications.entries.push(Application {
            company: "Bramble Tech".into(),
            role: "Staff Engineer".into(),
            status_word: ApplicationStatus::Interviewing.word().into(),
            sent_as: Some(crate::resume::model::SentCv {
                document: "sofiia-senior-swe".into(),
                preset: "FAANG · concise".into(),
            }),
            ..Default::default()
        });
        super::save_applications(&dir, &applications).expect("save applications");

        let back = super::load_applications(&dir);
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].company, "Bramble Tech");
        assert_eq!(back.entries[0].status(), ApplicationStatus::Interviewing);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// No `applications.toml` on disk at all (every vault written before this
    /// feature) must load as an empty board, never an error.
    #[test]
    fn missing_applications_file_loads_as_empty() {
        let dir = std::env::temp_dir().join(format!("dockcv-apps-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let applications = super::load_applications(&dir);
        assert!(applications.entries.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_snapshot_writes_bytes_and_sanitizes_the_file_name() {
        let dir = std::env::temp_dir().join(format!("dockcv-snapshot-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let file_name = super::save_snapshot(&dir, b"%PDF-fake", "Bramble Tech / EU", 1)
            .expect("save snapshot");
        assert_eq!(file_name, "bramble-tech-eu-v1.pdf");
        let bytes = std::fs::read(super::snapshots_dir(&dir).join(&file_name)).unwrap();
        assert_eq!(bytes, b"%PDF-fake");

        // A company name that is entirely punctuation must still produce a
        // usable, non-empty file name rather than a hidden dotfile or an
        // empty stem.
        let punctuation_only = super::save_snapshot(&dir, b"x", "!!!", 2).expect("save snapshot");
        assert_eq!(punctuation_only, "application-v2.pdf");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn toml_round_trip_preserves_document() {
        let resume = altacv::import(altacv::ALTACV_SAMPLE).unwrap();
        let mut doc = ResumeDoc::from_resume(resume, "Base");
        doc.add_variant(crate::resume::model::SectionKind::Work);
        doc.add_preset("Tailored");

        let text = super::to_toml(&doc).expect("serialize to TOML");
        let back: ResumeDoc = toml::from_str(&text).expect("deserialize from TOML");

        assert_eq!(back.profile.active().name, doc.profile.active().name);
        assert_eq!(back.work.variants.len(), doc.work.variants.len());
        assert_eq!(back.work.active().len(), doc.work.active().len());
        assert_eq!(back.presets.len(), 1);
        assert_eq!(back.presets[0].name, "Tailored");
    }

    /// Documents written before `layout` existed (page size, margins, text
    /// scale) must still load — with defaults that reproduce exactly the
    /// values `resume/template.rs`'s old hard-coded `PREAMBLE` used, so an
    /// existing vault's CVs render unchanged (US-07/C1).
    #[test]
    fn layout_defaults_and_old_docs_still_load() {
        use crate::resume::model::{LayoutSettings, Margins, PageSize, TypeSizes};

        let resume = altacv::import(altacv::ALTACV_SAMPLE).unwrap();
        let doc = ResumeDoc::from_resume(resume, "Base");
        let text = super::to_toml(&doc).expect("serialize to TOML");

        // Simulate a pre-`layout` file: strip the `[layout]` table (and its
        // `[layout.margins]` sub-table) that this version now appends.
        let cut = text
            .find("\n[layout]")
            .expect("layout is present in a fresh doc");
        let old_shape = &text[..cut];
        assert!(!old_shape.contains("[layout"));

        let back: ResumeDoc =
            toml::from_str(old_shape).expect("a pre-layout document must still load");
        assert_eq!(back.layout, LayoutSettings::default());
        assert_eq!(back.layout.page_size, PageSize::A4);
        assert_eq!(back.layout.margins.x_mm, 16.0);

        // And the field round-trips for real once it's set.
        let mut with_layout = back;
        with_layout.layout = LayoutSettings {
            page_size: PageSize::Letter,
            font: Default::default(),
            date_format: Default::default(),
            skills: Default::default(),
            entries: Default::default(),
            header: Default::default(),
            headings: Default::default(),
            sizes: TypeSizes {
                name_pt: 8.5,
                title_pt: 2.0,
                heading_pt: -1.0,
                entry_pt: 1.0,
            },
            text_scale_pct: 90,
            leading_em: 0.65,
            margins: Margins {
                x_mm: 18.0,
                top_mm: 15.0,
                bottom_mm: 15.0,
            },
        };
        let text2 = super::to_toml(&with_layout).expect("serialize with layout set");
        let back2: ResumeDoc = toml::from_str(&text2).expect("round-trips");
        assert_eq!(back2.layout, with_layout.layout);

        // The shape every document written between the two features is in: a
        // `[layout]` table that predates `[layout.sizes]`. It has to keep the
        // sizes the template hard-coded, not zeroes.
        let mut without_sizes = String::new();
        let mut in_sizes = false;
        for line in text2.lines() {
            if line.starts_with('[') {
                in_sizes = line.starts_with("[layout.sizes]");
            }
            if !in_sizes {
                without_sizes.push_str(line);
                without_sizes.push('\n');
            }
        }
        assert!(
            !without_sizes.contains("name_pt"),
            "the cut missed the table"
        );
        let back3: ResumeDoc = toml::from_str(&without_sizes).expect("loads without sizes");
        assert_eq!(back3.layout.sizes, TypeSizes::default());
        assert_eq!(
            back3.layout.text_scale_pct, 90,
            "the rest of the table survived the cut"
        );
    }

    /// A document written before custom sections existed (D-9) — no
    /// `custom_sections` table, no `next_custom_section_id` key — must still
    /// load through the real `vault::load`/`vault::save` path, rendering the
    /// same document it always did.
    #[test]
    fn custom_sections_default_and_old_docs_still_load() {
        use crate::resume::model::SectionKind;

        let resume = altacv::import(altacv::ALTACV_SAMPLE).unwrap();
        let doc = ResumeDoc::from_resume(resume, "Base");
        let text = super::to_toml(&doc).expect("serialize to TOML");
        assert!(!text.contains("custom_sections"));

        let back: ResumeDoc = toml::from_str(&text).expect("a pre-D-9 document must still load");
        assert!(back.custom_sections.is_empty());
        assert_eq!(back.next_custom_section_id, 0);
        assert_eq!(back.sections(), ResumeDoc::SECTIONS.to_vec());

        // And a custom section round-trips for real once one is added, via
        // the actual save/load functions (not just `toml::to_string`/`from_str`).
        let dir =
            std::env::temp_dir().join(format!("dockcv-custom-section-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.toml");

        let mut with_custom = back;
        let id = with_custom.add_custom_section("Languages");
        super::save(&with_custom, &path).expect("save");
        let reloaded = super::load(&path).expect("load");
        assert_eq!(reloaded.custom_sections.len(), 1);
        assert_eq!(reloaded.custom_sections[0].title, "Languages");
        assert_eq!(reloaded.sections().last(), Some(&SectionKind::Custom(id)));

        std::fs::remove_dir_all(&dir).ok();
    }
}

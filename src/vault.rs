//! `cvault` — File-over-App local storage.
//!
//! A cvault is a plain directory (`~/cvault`) of human-readable TOML documents,
//! one file per resume. No hidden database, no lock-in: open a file in any text
//! editor, hand-edit it, sync it via iCloud/Dropbox/git. The app just reads and
//! writes these files.

use std::fs;
use std::path::{Path, PathBuf};

use std::time::{SystemTime, UNIX_EPOCH};

use crate::resume::model::ResumeDoc;

mod atomic;
mod notebooks;
mod write;

pub use atomic::atomic_write;
/// Writing a document against the version that was read — the lost-update
/// guard and the decision about edits made outside. Its own file for the
/// reason `notebooks` is: the paths stay `vault::save` and `vault::OnDisk`.
pub use write::*;

/// The vault's notebooks, re-exported so `vault::load_applications` and its
/// eighty-odd siblings keep working.
///
/// The same move `model.rs` makes after C14: the code moves, the path does
/// not. A split that renames every call site is a split nobody can review,
/// and the paths are what the rest of the app reads this module by.
pub use notebooks::*;

const VAULT_DIR_NAME: &str = "cvault";
pub(crate) const LIBRARY_FILE: &str = "library.toml";
pub(crate) const DIARY_FILE: &str = "diary.toml";
pub(crate) const APPLICATIONS_FILE: &str = "applications.toml";
pub(crate) const PROFILES_FILE: &str = "profiles.toml";
pub(crate) const SNAPSHOTS_DIR: &str = "snapshots";
/// Reserved files that live in the vault but are NOT CV documents.
const RESERVED_FILES: [&str; 4] = [LIBRARY_FILE, DIARY_FILE, APPLICATIONS_FILE, PROFILES_FILE];

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
        || dir.join(PROFILES_FILE).exists()
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

/// Which of a document's names a search matched.
///
/// Search returning a flat list of documents is the defect: the box matched
/// `name` alone — the *person's* name, identical on every card in a vault of
/// one person's CVs — so it found everything or nothing and never said why.
/// A result has to name what carried the query, or the answer is not usable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchKind {
    /// The file stem. First, because it is what the card leads with and the
    /// only field that tells two CVs of one person with one title apart.
    Stem,
    Person,
    Role,
    Preset,
    Variant,
}

impl MatchKind {
    /// The word shown beside a result. `None` for the fields already on the
    /// card — saying "stem" over the title it just matched is noise.
    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::Stem | Self::Person => None,
            Self::Role => Some("role"),
            Self::Preset => Some("preset"),
            Self::Variant => Some("variant"),
        }
    }
}

/// One searchable name belonging to a document.
#[derive(Clone, Debug)]
pub struct SearchEntry {
    pub kind: MatchKind,
    /// As the user wrote it, for showing back.
    pub text: String,
    /// Lower-cased once here rather than on every keystroke.
    folded: String,
}

/// Human-readable summary of a document, for the gallery cards.
///
/// One version of a document, as the front door needs to list it.
#[derive(Clone, Debug)]
pub struct PresetMeta {
    pub name: String,
    /// The version this one was made from, by name — see
    /// [`crate::resume::model::Preset::based_on`].
    pub based_on: Option<String>,
    /// What this version is for, when its author said — see
    /// [`crate::resume::model::Preset::description`]. The front door's row
    /// subtitle, and nothing when absent.
    pub description: Option<String>,
}

/// `Clone` because the cache owns one copy and each row takes its own — a few
/// small string clones per visible row, in place of re-parsing the vault.
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
    /// Preset names, in document order. A count answers "how many"; the names
    /// answer "which", which is the question a returning user actually has —
    /// and P-01's complaint is precisely that presets are invisible.
    pub presets: Vec<PresetMeta>,
    /// True when the file couldn't be parsed as a document.
    pub unreadable: bool,
    /// File's last-modified time, seconds since the UNIX epoch. `None` if the
    /// file couldn't be stat'd. Feed to `relative_time` (with the current
    /// time) for the gallery's "updated N ago" line — this field is
    /// deliberately a raw timestamp, not a formatted string, so formatting
    /// stays pure and testable at the boundary.
    pub modified_secs: Option<u64>,
    /// Every name in this document a search can match, folded once.
    ///
    /// Built here rather than walked per keystroke: `vault_cache.rs` exists so
    /// the vault is parsed once per change, and a search that reopens every
    /// `ResumeDoc` on every character undoes exactly that.
    pub search: Vec<SearchEntry>,
}

impl DocMeta {
    /// What in this document carries `folded`, or `None` if nothing does.
    ///
    /// `folded` must already be lower-cased — the caller does it once per
    /// keystroke rather than this doing it once per document. The kinds are
    /// ordered so the stem answers first: it is what the card leads with, so a
    /// result explaining itself as a preset when the title already matched
    /// would be explaining the wrong thing.
    pub fn best_match(&self, folded: &str) -> Option<&SearchEntry> {
        if folded.is_empty() {
            return None;
        }
        self.search
            .iter()
            .filter(|entry| entry.folded.contains(folded))
            .min_by_key(|entry| entry.kind)
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
            let name = if basics.name.trim().is_empty() {
                stem.clone()
            } else {
                basics.name.clone()
            };
            let presets: Vec<PresetMeta> = doc
                .presets
                .iter()
                .map(|p| PresetMeta {
                    name: p.name.clone(),
                    based_on: p.based_on.clone(),
                    description: p.description.clone(),
                })
                .collect();

            let mut search = SearchIndex::default();
            search.add(MatchKind::Stem, &stem);
            search.add(MatchKind::Person, &name);
            search.add(MatchKind::Role, &basics.label);
            for preset in presets.iter().map(|p| p.name.as_str()) {
                search.add(MatchKind::Preset, preset);
            }
            // One name per variant, not one per section that happens to carry
            // it: every section usually has a `Base`, and seven copies of it
            // would be seven identical results for one word.
            for section in doc.sections() {
                for variant in doc.variant_names(section) {
                    search.add(MatchKind::Variant, &variant);
                }
            }

            DocMeta {
                name,
                label: basics.label.clone(),
                presets,
                unreadable: false,
                path: path.to_path_buf(),
                stem,
                modified_secs,
                search: search.into_entries(),
            }
        }
        // A file that will not parse still gets its stem searched: it is the
        // only thing known about it, and it is how the user finds it to fix it.
        None => {
            let mut search = SearchIndex::default();
            search.add(MatchKind::Stem, &stem);
            DocMeta {
                name: stem.clone(),
                label: String::new(),
                presets: Vec::new(),
                unreadable: true,
                path: path.to_path_buf(),
                stem,
                modified_secs,
                search: search.into_entries(),
            }
        }
    }
}

/// Collects a document's searchable names, skipping blanks and repeats.
#[derive(Default)]
struct SearchIndex(Vec<SearchEntry>);

impl SearchIndex {
    fn add(&mut self, kind: MatchKind, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let folded = text.to_lowercase();
        if self.0.iter().any(|e| e.kind == kind && e.folded == folded) {
            return;
        }
        self.0.push(SearchEntry {
            kind,
            text: text.to_string(),
            folded,
        });
    }

    fn into_entries(self) -> Vec<SearchEntry> {
        self.0
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
    // Nothing there yet — and if something is, the name was taken between
    // choosing it and writing it, which is a conflict like any other.
    save(doc, &path, OnDisk::ABSENT).map_err(|e| e.message())?;
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
    load_seen(path).map(|(doc, _)| doc)
}

/// A document's text, refused past the size guard rather than read whole.
fn read_document_text(path: &Path) -> Result<String, String> {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > MAX_VAULT_DOC_SIZE {
            return Err(format!(
                "document exceeds size limit of {} MB",
                MAX_VAULT_DOC_SIZE / (1024 * 1024)
            ));
        }
    }
    fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
}

/// Turn a document's text into a [`ResumeDoc`].
///
/// **The only way a document enters memory from a file**, and the reason it is
/// one function rather than a `toml::from_str` at each call site: a document
/// written before 0.4.0 does not deserialize at all until
/// [`crate::resume::parse_document_toml`] has run over it. The parser lives in
/// `dockcv-core` because the browser engine accepts the same files.
pub fn parse(text: &str) -> Result<ResumeDoc, toml::de::Error> {
    crate::resume::parse_document_toml(text)
}

/// Serialize a document to its TOML representation.
pub fn to_toml(doc: &ResumeDoc) -> Result<String, String> {
    toml::to_string_pretty(doc).map_err(|e| format!("serialize: {e}"))
}

// Four test modules, one per question asked of the store. They sat beside this
// file under `#[path]` attributes until the directory they wanted existed.
#[cfg(test)]
mod notebooks_tests;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod shape_tests;
#[cfg(test)]
mod write_tests;

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

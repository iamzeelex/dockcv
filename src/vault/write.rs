//! Writing a document against the version that was read.
//!
//! Split from `vault.rs` in 0.4.1 so the module could take the fixes for the
//! false "changed outside DockCV" without growing past its size. Everything
//! here answers one question — may this write replace what is on disk? — and
//! `write_tests.rs` beside it is where that question is tested.

use std::fs;
use std::path::Path;

use crate::resume::model::ResumeDoc;

use super::{atomic_write, parse, read_document_text, to_toml};

/// The contents of a document's file, as whoever holds it in memory last
/// agreed with them.
///
/// The promise this exists to keep is the one on the front of the README: the
/// vault is plain text you can read without this app and edit in any editor.
/// A held document plus an unconditional write breaks it silently — DockCV
/// keeps a document in memory for as long as it is open and writes the whole
/// of it on a 600 ms debounce, so an edit made in another editor, by a `git
/// checkout`, or by a sync client between two of those writes was replaced by
/// whatever the app happened to be holding, with nothing said. That is the
/// lost update, and the fix is the ordinary one: remember what you read, and
/// refuse to replace anything else.
///
/// A hash rather than an mtime because an mtime answers a different question.
/// Filesystem timestamps have a granularity, clocks move, and a sync client
/// can restore a file's old timestamp along with its contents; the bytes
/// cannot be wrong about themselves. It never leaves memory, so the hasher's
/// instability across Rust releases costs nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OnDisk(Option<u64>);

impl OnDisk {
    /// No file — a document made in the app and not yet written. The first
    /// save creates it, and finding a file where this says there is none is
    /// itself a conflict: something else wrote one.
    pub const ABSENT: Self = Self(None);

    /// What is at `path` now.
    ///
    /// A file that cannot be read counts as absent, deliberately: this is the
    /// same call the holder made when it took its copy, so the two agree and
    /// no phantom conflict is raised over a file neither of them can see. A
    /// document whose file is unreadable never reached an editor in the first
    /// place — `Shell` refuses to open it (`report_unreadable`).
    pub fn read(path: &Path) -> Self {
        fs::read_to_string(path)
            .map(|text| Self::of(&text))
            .unwrap_or(Self::ABSENT)
    }

    /// What `doc` would leave on disk if it were written now.
    ///
    /// Comparing this against what a holder last agreed with is how it answers
    /// "do I have anything unsaved" without keeping a dirty flag — a flag is a
    /// second copy of the truth, and it drifts.
    pub fn of_document(doc: &ResumeDoc) -> Self {
        to_toml(doc)
            .map(|text| Self::of(&text))
            .unwrap_or(Self::ABSENT)
    }

    pub fn of(text: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        Self(Some(hasher.finish()))
    }
}

/// Why a document was not written.
#[derive(Debug)]
pub enum SaveError {
    /// The file is not the one the caller read. **Nothing was written**, and
    /// both versions still exist: the file's, on disk, and the caller's, in
    /// memory. Only a person can say which one is wanted.
    Conflict,
    /// The write itself failed — a full disk, a read-only folder, a volume
    /// that has been unmounted.
    Failed(String),
}

impl SaveError {
    pub fn message(&self) -> String {
        match self {
            Self::Conflict => "the file changed on disk since DockCV read it".to_string(),
            Self::Failed(message) => message.clone(),
        }
    }
}

/// Atomically save a document to a TOML file (write to a temp file, then
/// rename) so a crash mid-write can't corrupt the existing file.
///
/// `seen` is what the caller believes is on disk — from [`load_seen`], or from
/// the [`OnDisk`] this returned the last time it wrote. A file that no longer
/// matches it is left alone; see [`OnDisk`] for why that matters.
///
/// Returns what is on disk afterwards, for the caller to hold until next time.
pub fn save(doc: &ResumeDoc, path: &Path, seen: OnDisk) -> Result<OnDisk, SaveError> {
    let text = to_toml(doc).map_err(SaveError::Failed)?;
    let current = OnDisk::read(path);
    if current != seen {
        return Err(SaveError::Conflict);
    }

    // Already what we would write. Returning early keeps DockCV from touching
    // a file it has no changes for — opening a document and leaving used to
    // rewrite it, which is a modification as far as git, a sync client or a
    // file watcher is concerned.
    let written = OnDisk::of(&text);
    if written == current {
        return Ok(current);
    }

    atomic_write(path, &text).map_err(SaveError::Failed)?;
    Ok(written)
}

/// What a holder should do about the file under the document it is keeping.
#[derive(Debug, PartialEq, Eq)]
pub enum ExternalChange {
    /// The file is still the one we agreed with. Nothing happened, or we wrote
    /// it ourselves.
    None,
    /// The file changed and the holder has nothing unsaved, so taking the
    /// file's version costs nothing and settles it.
    Adopt,
    /// Both changed. Two versions of the document exist and neither is a
    /// superset of the other, so only a person can say which is wanted.
    Conflict,
}

/// Decide what an open document should do about its file.
///
/// Its own function, and tested, because the two mistakes it could make are
/// both silent. Adopting when the holder has unsaved edits throws away work
/// nobody was warned about; treating an ordinary external edit as a conflict
/// leaves the app showing a version of a file that no longer exists and says
/// the wrong thing about why.
///
/// `seen` is the file as the holder last agreed with it, and `agreed` is what
/// the holder's *document* serialized to at that same moment. For a file
/// DockCV wrote they are the same hash. They differ for one it did not — typed
/// by hand, written by an older version, carrying a comment — because that file
/// parses to a document DockCV would write differently. Asking "is there
/// anything unsaved" of the file's bytes would then answer yes for a document
/// nobody has touched, and every edit made outside would arrive as a conflict
/// instead of a reload.
pub fn external_change(
    doc: &ResumeDoc,
    path: &Path,
    seen: OnDisk,
    agreed: OnDisk,
) -> ExternalChange {
    let current = OnDisk::read(path);
    if current == seen {
        // Nothing outside has touched it — including while the holder is
        // mid-edit with a write still pending, which is most of a typing
        // session.
        return ExternalChange::None;
    }

    let ours = OnDisk::of_document(doc);
    if current == ours {
        // The file already says what this document says: there are not two
        // versions, whoever wrote this one. A second DockCV on the same vault
        // saving the same keystrokes, or an edit outside that was undone.
        return ExternalChange::None;
    }

    if ours == agreed {
        ExternalChange::Adopt
    } else {
        ExternalChange::Conflict
    }
}

/// Read a document and what its file held, for a caller that is going to keep
/// the document and write it back later.
///
/// One read for both, and that is the point of it. A holder that parses the
/// file and then hashes it again has two looks at a file that can change in
/// between, and pairs a document with a hash that does not describe it.
pub fn load_seen(path: &Path) -> Result<(ResumeDoc, OnDisk), String> {
    let text = read_document_text(path)?;
    let doc = parse(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok((doc, OnDisk::of(&text)))
}

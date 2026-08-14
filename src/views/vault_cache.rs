//! A parsed view of the vault, refreshed only when the vault has actually
//! changed.
//!
//! Every vault screen used to read and parse its data **inside `render`**. The
//! gallery did it twice a frame — `list_metadata` is a directory scan plus a
//! full TOML parse of every document, once for the header's aggregate and again
//! for the grid — so typing one character into the search box re-parsed the
//! whole vault, and the cost grew with the number of CVs the user owned. The
//! library, diary and applications board each did the same with their own file,
//! and the diary paid twice because the rail renders separately from the pane.
//!
//! ## Why a fingerprint rather than manual invalidation
//!
//! The obvious fix — a cache plus an `invalidate()` call at every write — has
//! two problems, and the second is fatal for this product. The first is that
//! one forgotten call shows the user stale data. The second is that a vault is
//! **plain TOML the user is invited to hand-edit**, and files also arrive from
//! iCloud, Dropbox and `git pull`. A cache that only listens to DockCV's own
//! writes would stop noticing the very thing File-over-App promises.
//!
//! So the cache re-derives a cheap **fingerprint** of the directory every frame
//! — one `read_dir`, and a `stat` per file, no parsing — and reloads only when
//! it differs. The expensive half (parsing N documents) is what disappears; the
//! syscall that notices someone else's edit is what stays.
//!
//! A revision counter, bumped by [`super::save_status::record`], sits beside it
//! for the one case a fingerprint cannot see: DockCV's own write landing in the
//! same filesystem timestamp tick as the previous one, with the file ending up
//! exactly the same length. Rare, but it is our own write, so we know about it
//! for free.

use std::path::{Path, PathBuf};

use crate::resume::model::{Applications, Diary, Library};
use crate::vault::{self, DocMeta};

/// What the directory looked like: one entry per `.toml`, sorted by name.
///
/// Modification time is taken in nanoseconds where the platform offers them;
/// length is carried alongside because a one-second-granularity filesystem
/// would otherwise hide a same-second edit.
#[derive(PartialEq, Eq, Default)]
struct Fingerprint(Vec<(PathBuf, u128, u64)>);

impl Fingerprint {
    fn of(dir: &Path) -> Self {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Self::default();
        };
        let mut files: Vec<(PathBuf, u128, u64)> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().is_none_or(|ext| ext != "toml") {
                    return None;
                }
                let meta = entry.metadata().ok()?;
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                Some((path, modified, meta.len()))
            })
            .collect();
        // `read_dir` yields in whatever order the filesystem likes, and two
        // orderings of the same directory are the same directory.
        files.sort();
        Self(files)
    }
}

/// The vault, parsed once per change.
#[derive(Default)]
pub(super) struct VaultCache {
    /// `None` until the first load. Also the "which directory is this?" key —
    /// switching vaults has to reload even if the two happen to fingerprint
    /// identically.
    loaded_from: Option<PathBuf>,
    fingerprint: Fingerprint,
    revision: u64,

    metadata: Vec<DocMeta>,
    library: Library,
    diary: Diary,
    applications: Applications,
}

impl VaultCache {
    /// Bring the cache in line with the directory. Cheap — one `read_dir` and a
    /// `stat` per document — unless something changed, in which case it is the
    /// same work the screens were doing every frame anyway.
    ///
    /// `revision` is [`super::save_status::vault_revision`]; see the module
    /// docs for why both signals exist.
    pub(super) fn refresh(&mut self, vault: Option<&Path>, revision: u64) {
        let Some(dir) = vault else {
            // No vault open: drop whatever the last one left behind rather than
            // letting the welcome screen answer questions about it.
            *self = Self::default();
            return;
        };

        let fingerprint = Fingerprint::of(dir);
        let same_vault = self.loaded_from.as_deref() == Some(dir);
        if same_vault && self.fingerprint == fingerprint && self.revision == revision {
            return;
        }

        self.metadata = vault::list_metadata(dir);
        self.library = vault::load_library(dir);
        self.diary = vault::load_diary(dir);
        self.applications = vault::load_applications(dir);

        self.loaded_from = Some(dir.to_path_buf());
        self.fingerprint = fingerprint;
        self.revision = revision;
    }

    pub(super) fn metadata(&self) -> &[DocMeta] {
        &self.metadata
    }

    /// The document paths, in the order `vault::list_documents` would give
    /// them — derived from the metadata rather than re-scanning the directory.
    pub(super) fn document_paths(&self) -> Vec<PathBuf> {
        self.metadata.iter().map(|m| m.path.clone()).collect()
    }

    pub(super) fn library(&self) -> &Library {
        &self.library
    }

    pub(super) fn diary(&self) -> &Diary {
        &self.diary
    }

    pub(super) fn applications(&self) -> &Applications {
        &self.applications
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dockcv-cache-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp vault");
        dir
    }

    fn sample() -> crate::resume::model::ResumeDoc {
        use crate::resume::{altacv, model::ResumeDoc};
        ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base")
    }

    /// The whole point: an edit made by something other than DockCV — a text
    /// editor, a sync client, `git pull` — must still be picked up, because the
    /// format being hand-editable is the product.
    #[test]
    fn an_edit_from_outside_the_app_is_noticed() {
        let dir = temp_vault("external");
        let path = vault::create_document(&dir, &sample(), "cv").expect("create");

        let mut cache = VaultCache::default();
        cache.refresh(Some(&dir), 0);
        assert_eq!(cache.metadata().len(), 1);
        let before = cache.metadata()[0].name.clone();

        // Somebody else writes the file. No revision bump — DockCV never knew.
        let mut doc = vault::load(&path).expect("load");
        doc.profile.active_mut().name = "Someone Else Entirely".into();
        vault::save(&doc, &path).expect("save");

        cache.refresh(Some(&dir), 0);
        assert_ne!(cache.metadata()[0].name, before);
        assert_eq!(cache.metadata()[0].name, "Someone Else Entirely");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A new document appearing, and an old one going away, are both directory
    /// changes rather than file changes — the fingerprint has to see them too.
    #[test]
    fn documents_appearing_and_disappearing_are_noticed() {
        let dir = temp_vault("listing");
        let a = vault::create_document(&dir, &sample(), "first").expect("create");

        let mut cache = VaultCache::default();
        cache.refresh(Some(&dir), 0);
        assert_eq!(cache.metadata().len(), 1);

        vault::create_document(&dir, &sample(), "second").expect("create");
        cache.refresh(Some(&dir), 0);
        assert_eq!(cache.metadata().len(), 2);

        std::fs::remove_file(&a).expect("remove");
        cache.refresh(Some(&dir), 0);
        assert_eq!(cache.metadata().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Switching vaults must reload even when the two directories are
    /// indistinguishable by fingerprint — which two fresh vaults holding the
    /// same starter document very nearly are.
    #[test]
    fn switching_vaults_reloads_even_when_they_look_alike() {
        let one = temp_vault("switch-a");
        let two = temp_vault("switch-b");
        vault::create_document(&one, &sample(), "cv").expect("create");

        let mut cache = VaultCache::default();
        cache.refresh(Some(&one), 0);
        assert_eq!(cache.metadata().len(), 1);

        cache.refresh(Some(&two), 0);
        assert!(
            cache.metadata().is_empty(),
            "the second vault is empty; the first one's documents must not persist"
        );

        let _ = std::fs::remove_dir_all(&one);
        let _ = std::fs::remove_dir_all(&two);
    }

    /// The notebooks are not `.toml` documents in the listing sense, but they
    /// are `.toml` files in the directory — so the fingerprint sees them, and a
    /// diary edit refreshes the diary.
    #[test]
    fn a_diary_write_is_noticed() {
        use crate::resume::model::DiaryEntry;

        let dir = temp_vault("diary");
        let mut cache = VaultCache::default();
        cache.refresh(Some(&dir), 0);
        assert!(cache.diary().entries.is_empty());

        let mut diary = Diary::default();
        diary.entries.push(DiaryEntry {
            date: "2026-08-13".into(),
            text: "Cut p99 latency in half".into(),
            ..Default::default()
        });
        vault::save_diary(&dir, &diary).expect("save");

        cache.refresh(Some(&dir), 0);
        assert_eq!(cache.diary().entries.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Closing the vault must not leave the previous one's contents readable
    /// behind the welcome screen.
    #[test]
    fn closing_the_vault_empties_the_cache() {
        let dir = temp_vault("closed");
        vault::create_document(&dir, &sample(), "cv").expect("create");

        let mut cache = VaultCache::default();
        cache.refresh(Some(&dir), 0);
        assert_eq!(cache.metadata().len(), 1);

        cache.refresh(None, 0);
        assert!(cache.metadata().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The revision counter is the second signal, for our own writes landing in
    /// the same timestamp tick at the same length. Bumping it must reload even
    /// when the directory looks untouched.
    #[test]
    fn a_revision_bump_reloads_on_its_own() {
        let dir = temp_vault("revision");
        let path = vault::create_document(&dir, &sample(), "cv").expect("create");

        let mut cache = VaultCache::default();
        cache.refresh(Some(&dir), 0);

        // Rewrite the file preserving both length and — as far as the test can
        // arrange — the appearance of not having changed.
        let text = std::fs::read_to_string(&path).expect("read");
        std::fs::write(&path, text).expect("write");

        let before = cache.metadata().len();
        cache.refresh(Some(&dir), 1);
        assert_eq!(cache.metadata().len(), before, "still one document");
        assert_eq!(cache.revision, 1, "the bump was taken");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

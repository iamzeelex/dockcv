//! The version constructor: what a draft is, and the four things you can do to it.
//!
//! Between "there is a job" and "there is a version for it" the product used to
//! put the Preset Matrix — a section × variant grid. That is the right surface
//! for auditing a document with five readings in it and the wrong one for
//! making the first: it asks which *variant* of each section, when the question
//! in the person's head is which *changes* to make.
//!
//! So this screen asks that instead. The answers are not generated: a change is
//! a cut of a section that already exists, under the name and the one-line
//! description its author gave it (`version_changes.rs`). The matrix is still
//! there, one click away, for when three named changes are not enough.
//!
//! **Nothing is written until `Save version`.** The draft is a `ResumeDoc` in
//! memory with the source preset applied and the toggles on top; discarding it
//! leaves the vault exactly as it was. That is a change from the flow this
//! replaces, which created the preset and an application card the moment you
//! typed a company name — and would have left a card pinned to a preset that
//! no longer existed the first time somebody changed their mind.

use std::path::PathBuf;

use gpui::{Context, Task};

use crate::render::Rendered;
use crate::resume::model::{Application, ResumeDoc, SectionKind, SentCv};
use crate::typst_engine::PageGeometry;
use crate::vault;

use super::save_status;
use super::shell::Shell;
use super::version_changes::{self, Change};

/// A version being built, and nothing on disk yet.
pub(super) struct VersionDraft {
    pub path: PathBuf,
    /// The document as this version would read: the source preset applied,
    /// then whatever has been toggled since.
    pub doc: ResumeDoc,
    pub company: String,
    pub role: String,
    /// Which preset it started from.
    pub source: usize,
    /// The compiled page, once there is one. `None` is "not yet", never "no
    /// page" — a blank sheet would be a claim about the document.
    pub page: Option<Rendered>,
    pub geometry: Option<PageGeometry>,
    pub compiling: bool,
    pub task: Option<Task<()>>,
}

/// How large the preview page is rasterized. Fixed rather than
/// resolution-matched: this is a reading of the shape of a page, not a proof
/// sheet, and the editor is where somebody goes to read the words.
const PREVIEW_SCALE: f32 = 2.0;

impl VersionDraft {
    /// The changes available against the source, with the working copy's
    /// answers already marked.
    pub(super) fn changes(&self) -> Vec<Change> {
        version_changes::available(&self.doc, self.source)
    }

    /// What the source preset is called.
    pub(super) fn source_name(&self) -> String {
        self.doc
            .presets
            .get(self.source)
            .map(|p| p.name.clone())
            .unwrap_or_default()
    }

    /// The file stem, which is how the applications board keys a send.
    pub(super) fn stem(&self) -> String {
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

impl Shell {
    /// Start building a version of `path` from preset `source`.
    pub(super) fn open_version_draft(
        &mut self,
        path: PathBuf,
        source: usize,
        company: String,
        role: String,
        cx: &mut Context<Self>,
    ) {
        let Ok(mut doc) = vault::load(&path) else {
            save_status::record(
                cx,
                "this CV",
                Err(format!("{} could not be read", path.display())),
            );
            cx.notify();
            return;
        };
        if doc.presets.get(source).is_none() {
            return;
        }
        // Stand the working copy in the source's reading, so every change the
        // list offers is a change *from* the thing it says it is based on.
        doc.apply_preset(source);

        self.drafting = Some(Box::new(VersionDraft {
            path,
            doc,
            company,
            role,
            source,
            page: None,
            geometry: None,
            compiling: false,
            task: None,
        }));
        self.compile_draft(cx);
        cx.notify();
    }

    /// Throw the draft away. Nothing was written, so nothing is undone.
    pub(super) fn discard_version_draft(&mut self, cx: &mut Context<Self>) {
        self.drafting = None;
        cx.notify();
    }

    /// Turn one change on or off.
    ///
    /// Off does not mean "the document's default" — it means *what the source
    /// reads*, which is the only answer that makes the count honest. A change
    /// you switched off is not a change.
    pub(super) fn toggle_draft_change(&mut self, change: &Change, cx: &mut Context<Self>) {
        let Some(draft) = self.drafting.as_mut() else {
            return;
        };
        let Some(source) = draft.doc.presets.get(draft.source).cloned() else {
            return;
        };
        version_changes::toggle(&mut draft.doc, &source, change);
        self.compile_draft(cx);
        cx.notify();
    }

    /// Start from a different version instead.
    ///
    /// Every toggle is dropped, and deliberately: a change is defined against
    /// the source, so carrying them over would carry names that mean something
    /// else now.
    pub(super) fn set_draft_source(&mut self, source: usize, cx: &mut Context<Self>) {
        let Some(draft) = self.drafting.as_mut() else {
            return;
        };
        if draft.source == source || draft.doc.presets.get(source).is_none() {
            return;
        }
        draft.source = source;
        draft.doc.apply_preset(source);
        self.compile_draft(cx);
        cx.notify();
    }

    /// Write the version, file the application, and go back to the list.
    ///
    /// The preset is captured by [`ResumeDoc::add_preset`] rather than
    /// assembled here: it records the whole reading — selection, hidden, order,
    /// headings, language, profile — and a call site that lists the fields it
    /// knows about is how a new one gets silently dropped.
    pub(super) fn save_version_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.drafting.take() else {
            return;
        };
        let Some(vault_dir) = self.vault.clone() else {
            return;
        };
        let stem = draft.stem();
        let mut doc = draft.doc;
        let name = super::tailor::unique_preset_name(&doc, &draft.company);
        let based_on = doc.presets.get(draft.source).map(|p| p.name.clone());

        doc.add_preset(name.clone());
        if let Some(preset) = doc.presets.last_mut() {
            preset.based_on = based_on;
        }

        let on_disk = vault::OnDisk::read(&draft.path);
        let result = vault::save(&doc, &draft.path, on_disk);
        let failed = result.is_err();
        // Always recorded, not only on failure: the banner is one mechanism for
        // every vault write, and the revision it bumps is what makes the cache
        // — and so the list this returns to — show the new version.
        save_status::record_document(cx, &draft.path, on_disk, result);
        if failed {
            cx.notify();
            return;
        }

        // The card arrives with the version rather than before it. It goes to
        // the wishlist, so nothing counts as sent until it moves — which is
        // what `furthest` is for.
        let mut applications = vault::load_applications(&vault_dir);
        applications.entries.push(Application {
            company: draft.company.clone(),
            role: draft.role.clone(),
            created: vault::today_iso(),
            sent_as: Some(SentCv {
                document: stem,
                preset: name,
            }),
            ..Default::default()
        });
        save_status::record(
            cx,
            "applications board",
            vault::save_applications(&vault_dir, &applications),
        );

        self.reading_pages.retain(|(p, _), _| *p != draft.path);
        self.load_matrix_records();
        cx.notify();
    }

    /// Save it, then open the matrix on it.
    ///
    /// Named for both halves. The matrix reads a document from disk, so there
    /// is no version of this that adjusts a draft — and silently writing one
    /// behind a button called `Adjust sections` is the kind of surprise this
    /// screen exists to remove.
    pub(super) fn save_draft_and_open_matrix(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.drafting.as_ref().map(|d| d.path.clone()) else {
            return;
        };
        self.save_version_draft(cx);
        self.open_preset_matrix(path, cx);
    }

    /// Lay the draft out and rasterize one page, off the UI thread.
    pub(super) fn compile_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.drafting.as_mut() else {
            return;
        };
        let source = crate::resume::template::generate_for_with_profiles(
            &draft.doc,
            self.cache.profiles(),
        );
        draft.compiling = true;

        let engine = self
            .thumb_engine
            .get_or_insert_with(|| {
                std::sync::Arc::new(std::sync::Mutex::new(
                    crate::typst_engine::TypstEngine::new(String::new()),
                ))
            })
            .clone();
        let executor = cx.background_executor().clone();

        let task = cx.spawn(async move |this, cx| {
            let compiled = executor
                .spawn(async move {
                    let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
                    engine.set_source(source);
                    engine.compile_to_pixels(PREVIEW_SCALE).ok()
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                let Some(draft) = this.drafting.as_mut() else {
                    return;
                };
                draft.compiling = false;
                // A reading that will not compile keeps the last page it had
                // rather than blanking: the change you just made is the thing
                // to look at, and an empty sheet says the document is empty.
                if let Some((pixels, geometry)) = compiled {
                    if let Ok(page) = crate::render::pixels_to_render_image(pixels, PREVIEW_SCALE) {
                        draft.page = Some(page);
                    }
                    draft.geometry = Some(geometry);
                }
                cx.notify();
            });
        });

        if let Some(draft) = self.drafting.as_mut() {
            draft.task = Some(task);
        }
    }

    /// Why this source, in evidence rather than opinion.
    ///
    /// The board knows how many applications went out under each reading and
    /// how many came back. That is a fact about this person's own history, and
    /// it is the whole of what this panel is allowed to say until there is a
    /// job description to read — see the note in `tailor.rs`.
    pub(super) fn draft_source_evidence(&self) -> Option<(String, String)> {
        let draft = self.drafting.as_ref()?;
        let name = draft.source_name();
        let record = self
            .cache
            .applications()
            .record_for(&draft.stem(), &name);

        Some(match (record.sent, record.interviewed) {
            (0, _) => (
                format!("{name} is where you left this CV"),
                "Nothing has gone out under any version of it yet, so there is no record to \
                 choose by. Start from the one that is closest and change what does not fit."
                    .to_string(),
            ),
            (sent, 0) => (
                format!("{name} is the one you send"),
                format!(
                    "{sent} application{} went out under it. None have come back as an \
                     interview yet.",
                    if sent == 1 { "" } else { "s" }
                ),
            ),
            (sent, interviewed) => (
                format!("{name} is the one that has been working"),
                format!(
                    "{sent} application{} went out under it, and {interviewed} came back as \
                     an interview.",
                    if sent == 1 { "" } else { "s" }
                ),
            ),
        })
    }
}

/// How many of the offered changes are on.
pub(super) fn selected(changes: &[Change]) -> usize {
    version_changes::applied_count(changes)
}

/// What the row of sections a change touches is called, for the row's eyebrow.
pub(super) fn section_label(doc: &ResumeDoc, section: SectionKind) -> String {
    doc.section_title(section)
}

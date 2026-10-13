use gpui::{prelude::*, Context, Entity, Window};

use dockcv_ui_components::TextFieldState;

use crate::resume::model::{DiaryEntry, SectionKind};
use crate::vault;
use crate::views::confirm;
use crate::views::root::editor_state::{self, EditorMode, EditorSelection};
use crate::views::save_status;

use super::Root;

/// State for the D-7 quick-capture sheet: just the note field. Not committed
/// to `Root::diary` until the user clicks Save (`Root::commit_capture`).
pub(crate) struct CaptureSheet {
    pub(crate) text: Entity<TextFieldState>,
    /// The role this win belongs to, empty for none (O-15).
    pub(crate) role: String,
}

/// Push a clone of `item` (if present) onto a library pool.
fn clone_into<T: Clone>(item: Option<&T>, pool: &mut Vec<T>) {
    if let Some(value) = item {
        pool.push(value.clone());
    }
}

/// Push a clone of a library `item` (if present) onto a document section.
fn clone_from<T: Clone>(item: Option<&T>, target: &mut Vec<T>) {
    if let Some(value) = item {
        target.push(value.clone());
    }
}

/// Join two labels with an em dash, skipping empties.
fn join_em(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (false, false) => format!("{a} — {b}"),
        (false, true) => a.to_string(),
        (true, false) => b.to_string(),
        (true, true) => "(untitled)".to_string(),
    }
}

impl Root {
    // --- block library ("me") ---

    /// Copy the entry at `index` of `section`'s active variant into the vault
    /// library, so it can be reused in other résumés.
    pub(super) fn save_block_to_library(
        &mut self,
        section: SectionKind,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        use SectionKind::*;
        match section {
            Work => clone_into(self.doc.work.active().get(index), &mut self.library.work),
            Education => clone_into(
                self.doc.education.active().get(index),
                &mut self.library.education,
            ),
            Skills => clone_into(
                self.doc.skills.active().get(index),
                &mut self.library.skills,
            ),
            Certificates => clone_into(
                self.doc.certificates.active().get(index),
                &mut self.library.certificates,
            ),
            Organizations => clone_into(
                self.doc.volunteer.active().get(index),
                &mut self.library.volunteer,
            ),
            // The vault library is a copy pool for the six built-in section
            // shapes only (`Library` has no custom-section pool) — out of
            // scope for D-9's model-only task.
            Profile | Custom(_) => {}
        }
        save_status::record(
            cx,
            "library",
            vault::save_library(&self.vault_dir, &self.library),
        );
        cx.notify();
    }

    /// Insert a copy of library block `index` into `section`'s active variant.
    pub(super) fn insert_library_block(
        &mut self,
        section: SectionKind,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        use SectionKind::*;
        match section {
            Work => clone_from(self.library.work.get(index), self.doc.work.active_mut()),
            Education => clone_from(
                self.library.education.get(index),
                self.doc.education.active_mut(),
            ),
            Skills => clone_from(self.library.skills.get(index), self.doc.skills.active_mut()),
            Certificates => clone_from(
                self.library.certificates.get(index),
                self.doc.certificates.active_mut(),
            ),
            Organizations => clone_from(
                self.library.volunteer.get(index),
                self.doc.volunteer.active_mut(),
            ),
            // See `save_block_to_library`: no library pool for custom sections.
            Profile | Custom(_) => {}
        }
        self.selection = EditorSelection {
            section,
            item: editor_state::item_count(&self.doc, section).checked_sub(1),
        };
        self.focused_section = section;
        self.expanded.insert(section);
        self.editor_mode = EditorMode::Content;
        self.library_picker = None;
        self.fields_stale = true;
        cx.notify();
        self.schedule_recompile(window, cx);
        self.schedule_save(cx);
    }

    pub(super) fn open_library_picker(&mut self, section: SectionKind, cx: &mut Context<Self>) {
        self.library_picker = Some(section);
        cx.notify();
    }

    pub(super) fn open_diary_picker(&mut self, work_index: usize, cx: &mut Context<Self>) {
        self.diary_picker = Some(work_index);
        cx.notify();
    }

    /// Insert diary entry `entry_index`'s text as a new highlight on the work
    /// entry at `work_index` of the active variant.
    pub(super) fn insert_diary_highlight(
        &mut self,
        work_index: usize,
        entry_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        if let Some(entry) = self.diary.entries.get(entry_index) {
            let text = entry.text.clone();
            if let Some(work) = self.doc.work.active_mut().get_mut(work_index) {
                work.highlights.push(text);
            }
        }
        self.diary_picker = None;
        self.fields_stale = true;
        cx.notify();
        self.schedule_recompile(window, cx);
        self.schedule_save(cx);
    }

    // --- diary quick-capture (D-7, design doc §8) ---

    /// The document's own name — its file stem, which is what a document is
    /// called under File-over-App and the only thing that distinguishes two
    /// CVs belonging to the same person.
    pub(super) fn document_file_name(&self) -> String {
        self.doc_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled".to_string())
    }

    /// What the capture sheet offers as roles.
    ///
    /// This document's own jobs first — a win captured while editing a CV is
    /// almost always about one of the jobs in it — then any role the diary
    /// already uses, so a capture can join an existing bucket rather than
    /// starting a near-duplicate. Same `employer · position` spelling the
    /// Diary screen uses, or the two would not group together.
    pub(super) fn capture_roles(&self) -> Vec<String> {
        let mut roles: Vec<String> = Vec::new();
        let mut push = |role: String| {
            if !role.is_empty() && !roles.contains(&role) {
                roles.push(role);
            }
        };
        for work in self.doc.work.active() {
            let (employer, position) = (work.name.trim(), work.position.trim());
            match (employer.is_empty(), position.is_empty()) {
                (false, false) => push(format!("{employer} · {position}")),
                (false, true) => push(employer.to_string()),
                (true, false) => push(position.to_string()),
                (true, true) => {}
            }
        }
        for entry in &self.diary.entries {
            push(entry.role.clone());
        }
        roles
    }

    pub(super) fn document_identity(&self) -> String {
        let profile = self.doc.profile.active();
        let name = profile.name.trim();
        let label = profile.label.trim();
        match (name.is_empty(), label.is_empty()) {
            (false, false) => format!("{name} — {label}"),
            (false, true) => name.to_string(),
            (true, false) => label.to_string(),
            (true, true) => "Untitled".to_string(),
        }
    }

    /// Open the quick-capture sheet. Building its `TextFieldState` needs a
    /// `Window`, which the toolbar's `Capture` click handler has.
    pub(super) fn open_capture_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = cx.new(|cx| TextFieldState::auto_grow(3, 10, window, cx));
        self.capture_sheet = Some(CaptureSheet {
            text,
            role: String::new(),
        });
        cx.notify();
    }

    pub(super) fn close_capture_sheet(&mut self, cx: &mut Context<Self>) {
        self.capture_sheet = None;
        cx.notify();
    }

    /// Write the sheet's text as a new `DiaryEntry`, exactly as
    /// `Shell::commit_diary_entry` does — reload, insert at the front, save —
    /// except `source_doc` is set to this document's file stem (P-05: the
    /// link back to the CV a capture came from is the point of this feature,
    /// not optional metadata).
    pub(super) fn commit_capture(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = &self.capture_sheet else {
            return;
        };
        let text = sheet.text.read(cx).value(cx).trim().to_string();
        if text.is_empty() {
            return;
        }
        let source_doc = self
            .doc_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned());

        let mut diary = vault::load_diary(&self.vault_dir);
        diary.entries.insert(
            0,
            DiaryEntry {
                date: vault::today_iso(),
                text,
                // O-15: chosen in the sheet, from this document's own jobs.
                // Never guessed — a CV holds several roles and putting the
                // wrong employer on a real achievement is worse than leaving
                // it untagged, which is why "No role" is still the default and
                // still a legitimate answer.
                role: sheet.role.clone(),
                tags: Vec::new(),
                confidential: false,
                used_in: Vec::new(),
                source_doc,
            },
        );
        save_status::record(cx, "diary", vault::save_diary(&self.vault_dir, &diary));
        self.diary = diary;
        self.capture_sheet = None;
        cx.notify();
    }

    /// Number of library blocks available for a section.
    pub(super) fn library_count(&self, section: SectionKind) -> usize {
        use SectionKind::*;
        match section {
            Work => self.library.work.len(),
            Education => self.library.education.len(),
            Skills => self.library.skills.len(),
            Certificates => self.library.certificates.len(),
            Organizations => self.library.volunteer.len(),
            Profile | Custom(_) => 0,
        }
    }

    /// Display labels for the library blocks of a section.
    pub(super) fn library_labels(&self, section: SectionKind) -> Vec<String> {
        use SectionKind::*;
        match section {
            Work => self
                .library
                .work
                .iter()
                .map(|w| join_em(&w.position, &w.name))
                .collect(),
            Education => self
                .library
                .education
                .iter()
                .map(|e| join_em(&e.study_type, &e.institution))
                .collect(),
            Skills => self.library.skills.iter().map(|s| s.name.clone()).collect(),
            Certificates => self
                .library
                .certificates
                .iter()
                .map(|c| join_em(&c.name, &c.issuer))
                .collect(),
            Organizations => self
                .library
                .volunteer
                .iter()
                .map(|v| join_em(&v.position, &v.organization))
                .collect(),
            Profile | Custom(_) => Vec::new(),
        }
    }

    /// Diary entries marked confidential whose wording may have reached *this*
    /// document.
    ///
    /// US-36 asks for a warning at export if a bullet came from a confidential
    /// note. A bullet is a bare `String`, so it carries no provenance — but the
    /// entry does: `Use in a CV` records the document it was promoted into
    /// (`DiaryEntry::used_in`). Reading it from that side is what makes the
    /// warning possible at all without changing every bullet in the model.
    ///
    /// It is honest about being a *may*: a promotion made before this field
    /// existed is not recorded, and a bullet typed by hand from a confidential
    /// note was never promoted at all. The dialog says so rather than claiming
    /// a certainty it does not have.
    fn confidential_sources(&self) -> Vec<String> {
        let Some(stem) = self.doc_path.file_stem().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        self.diary
            .entries
            .iter()
            .filter(|e| e.confidential && e.used_in.iter().any(|d| d == stem))
            .map(|e| e.text.clone())
            .collect()
    }

    /// Export, warning first if a confidential note may have reached this CV.
    pub(super) fn export_pdf_checked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sources = self.confidential_sources();
        if sources.is_empty() {
            self.open_export_sheet(window, cx);
            return;
        }

        let listed: String = sources
            .iter()
            .take(3)
            .map(|text| {
                let mut line: String = text.chars().take(80).collect();
                if text.chars().count() > 80 {
                    line.push('…');
                }
                format!("\n• {line}")
            })
            .collect();
        let more = sources.len().saturating_sub(3);

        confirm::caution(
            format!(
                "This CV uses {} confidential {}.",
                sources.len(),
                if sources.len() == 1 { "note" } else { "notes" }
            ),
            format!(
                "Check the bullets are abstracted before this leaves your machine — \
                 the outcome and the number, not the client, the system or the \
                 incident.{listed}{}",
                if more > 0 {
                    format!("\n…and {more} more.")
                } else {
                    String::new()
                }
            ),
            "Export anyway",
            window,
            cx,
            |this, window, cx| this.open_export_sheet(window, cx),
        );
    }
}

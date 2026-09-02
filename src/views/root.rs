//! The root view: the resume editor shell.
//!
//! Left: a sections navigator built from the recognized [`Resume`] model.
//! Each section is a collapsible card whose body holds **editable fields**. Every
//! field addressed by a [`FieldId`] owns a live [`TextFieldState`]; the editor
//! keeps them in [`Root::fields`] and writes each change straight back into the
//! string the id points at. Right: the live Typst preview, rerendered from the
//! model on every edit.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{
    actions, div, prelude::*, px, App, Context, Entity, EventEmitter, FocusHandle, KeyBinding,
    Subscription, Task, Window,
};

use dockcv_ui_components::{
    h_resizable, resizable_panel, SliderState, TextFieldEvent, TextFieldState,
};

use crate::render::{self, Rendered};
use crate::resume::diagnostics::{describe_all, CompileMessage};
use crate::resume::edit::FieldId;
use crate::resume::model::{Diary, DiaryEntry, Library, ResumeDoc, SectionKind};
use crate::resume::template;
use crate::theme::ActiveTheme;
use crate::typst_engine::{PageGeometry, Severity, TypstEngine};
use crate::vault;

use super::confirm;
use super::save_status;

use super::root_section_rename::SectionRename;
use super::root_section_variants::VariantRename;

/// Idle time before the quick draft re-render fires (16ms = 1 frame @ 60 FPS).
const DRAFT_DEBOUNCE: Duration = Duration::from_millis(16);
/// Further idle time before the crisp, full-resolution re-render fires. Both
/// passes live in one debounced task, so the crisp pass is cancelled by the
/// next edit — you get cheap draft frames while typing and a sharp frame once
/// you pause.
const CRISP_DELAY: Duration = Duration::from_millis(150);

/// One frame at 60 Hz. A full-resolution compile faster than this is not worth
/// showing a soft page to avoid.
const FRAME_BUDGET: Duration = Duration::from_millis(16);
/// Ceiling on the preview's rasterization scale, in pixels per typographic
/// point. Bounds memory: an A4 page at 6× is ~3600×5000 px, and nothing on a
/// screen can show more detail than the display's own pixels anyway.
const MAX_RENDER_SCALE: f32 = 6.0;
/// Idle time after the last change before the document is written to its vault
/// file. Debounced so rapid edits coalesce into one write.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(600);

// --- keyboard navigation (P-17) ---
//
// Bound under the "Root" key context (`Render for Root` sets
// `.key_context(EDITOR_CONTEXT)` on the screen's outer div) rather than
// globally. GPUI resolves a keystroke against every binding whose context
// matches something on the *focused element's* ancestor chain, most specific
// first, and only tries the next match if the handler that ran calls
// `cx.propagate()`. Every field in this screen is a `gpui_component` text
// input, which claims a large set of chords under its own "Input" context
// (see `.research/gpui-component/crates/ui/src/input/state.rs::init`) and
// does **not** propagate most of them — any chord on that list would be
// swallowed by whichever field currently has focus and would never reach a
// binding here. Every chord below was checked against that list and avoids
// all of it. `escape` is the one deliberate exception: `InputState::escape`
// propagates once it has nothing of its own to do (no completion menu open,
// no IME composition, `clean_on_escape` unset — true of every field in this
// screen), so it still reaches [`CloseOverlay`] normally.
actions!(
    dockcv_editor,
    [
        /// Move the section-navigation cursor to the next of the six fixed
        /// sections (wraps).
        FocusNextSection,
        /// Move the section-navigation cursor to the previous section (wraps).
        FocusPrevSection,
        /// Expand/collapse the section the navigation cursor is on.
        ToggleFocusedSection,
        /// Switch the focused section to its next variant (wraps).
        NextVariant,
        /// Switch the focused section to its previous variant (wraps).
        PrevVariant,
        /// Apply the document's next preset (wraps; starts at the first if
        /// none is applied yet).
        NextPreset,
        /// Apply the document's previous preset (wraps; starts at the last
        /// if none is applied yet).
        PrevPreset,
        /// Move input focus to the next field of the focused, expanded
        /// section (wraps; expands the section if it was collapsed).
        FocusNextField,
        /// Move input focus to the previous field of the focused section
        /// (wraps).
        FocusPrevField,
        /// Step the document back to before the last structural change —
        /// a delete, an add, a reorder, an applied preset. Not text: while
        /// the caret is in a field, that field's own undo takes `cmd-z`
        /// first (see `root_undo`).
        UndoDocument,
        /// Step forward again after [`UndoDocument`].
        RedoDocument,
        /// Export the composed document to PDF (same action as the toolbar
        /// button).
        ExportPdf,
        /// Open the D-7 quick-capture sheet (same action as the toolbar
        /// button).
        OpenCapture,
        /// Close whichever of the capture sheet / library picker / diary
        /// picker overlay is open.
        CloseOverlay,
    ]
);

/// The key context [`Render for Root`] sets, and every [`KeyBinding`] below
/// is scoped to. Exposed so sibling files (`root_overlays.rs`) can build
/// `Button::tooltip_with_action` hints against the same context string.
///
/// **Not** `"Root"`, which is what it used to be — and which is the exact
/// string `gpui-component`'s own window `Root` declares
/// (`crates/ui/src/root.rs`), where it binds `tab`, `shift-tab` and `cmd-c`.
/// Two different elements answering to one context name meant the editor's
/// bindings were dispatchable from every screen in the app, not just the
/// editor, and that any chord upstream adds under `"Root"` would silently
/// start fighting one of ours. Nothing collides today; the name did.
pub(super) const EDITOR_CONTEXT: &str = "DockCvEditor";

/// Chord literals, in `KeyBinding::new`'s own `-`-joined syntax. Centralized
/// so `init_keybindings` and every on-screen hint read the same literal and
/// can never drift apart. `pub(super)` so the variant pills' tooltips can
/// name the chord they are driven by.
pub(super) mod keys {
    pub const FOCUS_NEXT_SECTION: &str = "ctrl-down";
    pub const FOCUS_PREV_SECTION: &str = "ctrl-up";
    pub const TOGGLE_SECTION: &str = "cmd-j";
    pub const NEXT_VARIANT: &str = "ctrl-shift-down";
    pub const PREV_VARIANT: &str = "ctrl-shift-up";
    pub const NEXT_PRESET: &str = "alt-shift-down";
    pub const PREV_PRESET: &str = "alt-shift-up";
    pub const FOCUS_NEXT_FIELD: &str = "alt-down";
    pub const FOCUS_PREV_FIELD: &str = "alt-up";
    pub const EXPORT_PDF: &str = "cmd-e";
    pub const OPEN_CAPTURE: &str = "cmd-k";
    pub const CLOSE_OVERLAY: &str = "escape";
    /// The usual pair. Dispatch is what keeps them from fighting the text
    /// input's identical bindings: a focused field is deeper in the tree, so
    /// its `Undo` wins while the caret is in it, and these take over once it
    /// is not.
    pub const UNDO_DOCUMENT: &str = "cmd-z";
    pub const REDO_DOCUMENT: &str = "cmd-shift-z";
}

/// Register the editor's keybindings. A one-time, process-wide call —
/// `cx.bind_keys` is not per-render — made once from `app.rs::init`, the same
/// place `Quit` is bound. Only *dispatch* is scoped, by [`EDITOR_CONTEXT`].
pub fn init_keybindings(cx: &mut App) {
    use keys::*;
    cx.bind_keys([
        KeyBinding::new(FOCUS_NEXT_SECTION, FocusNextSection, Some(EDITOR_CONTEXT)),
        KeyBinding::new(FOCUS_PREV_SECTION, FocusPrevSection, Some(EDITOR_CONTEXT)),
        KeyBinding::new(TOGGLE_SECTION, ToggleFocusedSection, Some(EDITOR_CONTEXT)),
        KeyBinding::new(NEXT_VARIANT, NextVariant, Some(EDITOR_CONTEXT)),
        KeyBinding::new(PREV_VARIANT, PrevVariant, Some(EDITOR_CONTEXT)),
        KeyBinding::new(NEXT_PRESET, NextPreset, Some(EDITOR_CONTEXT)),
        KeyBinding::new(PREV_PRESET, PrevPreset, Some(EDITOR_CONTEXT)),
        KeyBinding::new(FOCUS_NEXT_FIELD, FocusNextField, Some(EDITOR_CONTEXT)),
        KeyBinding::new(FOCUS_PREV_FIELD, FocusPrevField, Some(EDITOR_CONTEXT)),
        KeyBinding::new(EXPORT_PDF, ExportPdf, Some(EDITOR_CONTEXT)),
        KeyBinding::new(OPEN_CAPTURE, OpenCapture, Some(EDITOR_CONTEXT)),
        KeyBinding::new(CLOSE_OVERLAY, CloseOverlay, Some(EDITOR_CONTEXT)),
        KeyBinding::new(UNDO_DOCUMENT, UndoDocument, Some(EDITOR_CONTEXT)),
        KeyBinding::new(REDO_DOCUMENT, RedoDocument, Some(EDITOR_CONTEXT)),
    ]);
}

/// Events the editor emits to its host shell.
pub enum EditorEvent {
    BackToGallery,
    /// P-01: the toolbar's preset menu must be a door into the Preset
    /// Matrix, scoped to this document — `Shell` owns that screen, so the
    /// editor asks for it rather than navigating there itself.
    OpenPresetMatrix,
}

/// The compile state the UI can always show something for — "compiling /
/// ready / error", not just "error" (US-07's own acceptance text). A failed
/// or in-flight compile never touches [`Root::rendered`]: the last good
/// frame stays on screen exactly as before; this only tracks what to *say*
/// about the most recent attempt.
///
/// The design doc's warning-styled banner with `Jump to section →` (the Typst-controls spec)
/// is a separate, later task — this type carries what that UI needs
/// ([`CompileMessage`]'s severity and optional section), it just isn't
/// rendered here.
#[derive(Debug, Clone, PartialEq)]
pub enum CompileState {
    /// A recompile is in flight and hasn't produced a frame yet — true only
    /// before the very first render lands, since after that the debounced
    /// draft pass (`schedule_recompile`) always lands quickly enough that
    /// there is no separate "still compiling" gap the user would see beyond
    /// the existing draft→crisp progression.
    Compiling,
    /// The most recent compile succeeded. `warnings` is every diagnostic
    /// that attempt produced — always `Severity::Warning`, never empty *and*
    /// `Error` at once, since an error would have produced `Error` instead.
    Ready { warnings: Vec<CompileMessage> },
    /// The most recent compile failed. `messages` is every diagnostic from
    /// that attempt (errors, plus any warnings emitted before the error),
    /// each already a full sentence naming a section when that's honest.
    Error { messages: Vec<CompileMessage> },
}

pub struct Root {
    /// Shared so the debounced recompile can run on a background thread without
    /// blocking the UI. `TypstEngine` is `Send` (its `World` is `Send + Sync`).
    pub(super) engine: Arc<Mutex<TypstEngine>>,
    pub(super) doc: ResumeDoc,
    pub(super) rendered: Option<Rendered>,
    /// Visible compile status — see [`CompileState`].
    pub(super) compile_state: CompileState,
    /// The composed source last sent to the renderer; used to skip recompiles
    /// when an edit doesn't change the rendered output (e.g. renaming a
    /// variant or preset).
    pub(super) last_source: String,
    /// Identities of expanded section cards. Keyed by `SectionKind` rather
    /// than a title string — a custom section's title is user-editable
    /// (D-9), and keying expand/collapse state by text that can change on
    /// every keystroke would both collide (two custom sections both titled
    /// "New Section" until renamed) and reset on every edit.
    pub(super) expanded: HashSet<SectionKind>,
    /// The section keyboard navigation (`FocusNextSection`/`FocusPrevSection`)
    /// currently points at — always one of the six fixed `SectionKind`s, never
    /// empty, so every other keyboard action (`ToggleFocusedSection`,
    /// `NextVariant`, `FocusNextField`, …) always has a section to act on.
    pub(super) focused_section: SectionKind,
    /// One live text-input state per addressable field, plus the subscription
    /// that writes its changes back into `doc`. Rebuilt whenever the model
    /// changes underneath — see [`Root::sync_fields`].
    pub(super) fields: HashMap<FieldId, FieldBinding>,
    /// Set when something other than typing changed the model (variant switch,
    /// preset applied, entry added or removed), so the next frame re-seeds every
    /// field from the document.
    pub(super) fields_stale: bool,
    pub(super) focus_handle: FocusHandle,
    /// The in-flight debounced recompile. Replacing it cancels the previous one
    /// (GPUI tasks abort on drop), which is what makes the debounce work.
    pub(super) recompile_task: Option<Task<()>>,
    /// Where this document lives in the cvault, and the in-flight debounced save.
    pub(super) doc_path: PathBuf,
    pub(super) save_task: Option<Task<()>>,
    /// Structural history — see [`super::root_undo`]. Whole-document
    /// snapshots, taken before a change rather than after it.
    pub(super) undo_stack: Vec<ResumeDoc>,
    pub(super) redo_stack: Vec<ResumeDoc>,
    /// The vault's reusable block library ("me") and its directory.
    pub(super) vault_dir: PathBuf,
    pub(super) library: Library,
    /// When set, the "add from library" picker is open for this section.
    pub(super) library_picker: Option<SectionKind>,
    /// The vault's professional diary.
    pub(super) diary: Diary,
    /// When set, the "from diary" picker is open for this work-entry index.
    pub(super) diary_picker: Option<usize>,
    /// D-7: the toolbar's `Capture` quick-capture sheet. `Some` while it's open;
    /// holds the one text field it needs, built lazily (creating a
    /// `TextFieldState` needs a `Window`, which the click handler that opens
    /// this has, same as `Root::fields`).
    pub(super) capture_sheet: Option<CaptureSheet>,
    /// The export sheet overlay (`root_export_sheet.rs`): `Some` while the user
    /// is choosing a format and a preset. One at a time, like `capture_sheet`.
    pub(super) export_sheet: Option<super::root_export_sheet::ExportSheetState>,
    /// The section header's rename control (`root_section_rename.rs`): `Some`
    /// while a section's printed heading is being edited inline. One at a
    /// time, and not part of `Root::fields` — it never addresses a `FieldId`,
    /// since a built-in section's heading has no addressable field (only
    /// `ResumeDoc::set_section_title` does) and a custom section's does, but
    /// the whole point of this control is that the two kinds share one UI.
    pub(super) renaming_section: Option<SectionRename>,
    /// The active-variant rename control (`root_section_variants.rs`,
    /// editor-comfort.md C-2): `Some` while a section's active chip is being
    /// renamed inline. Same one-at-a-time shape as `renaming_section`, kept
    /// separate because it addresses a different field (`FieldId::VariantName`
    /// vs. a section's printed heading).
    pub(super) renaming_variant: Option<VariantRename>,
    /// The preset last applied (or saved) from the toolbar's preset menu, for
    /// display only (`Preset  <name>  ▾`) — `ResumeDoc` has no notion of a
    /// "current" preset (a preset is just a named selection, per
    /// the editor spec §9), so this is ephemeral view state, not
    /// persisted with the document.
    pub(super) active_preset: Option<usize>,
    /// Preview rasters replaced by a newer one, waiting to be released.
    ///
    /// `Window::drop_image` evicts an atlas tile **immediately**, and GPUI frees
    /// the whole texture slot once its last key goes — so releasing the outgoing
    /// image the moment a new one arrives can pull a tile out from under a frame
    /// that still references it, and `metal_atlas::texture()` then unwraps a
    /// `None` and aborts the process. Retiring instead of dropping, and releasing
    /// at the top of the next `render`, guarantees a full frame has been built
    /// without the old image before its tile goes.
    ///
    /// Not optional: the atlas has no time-based eviction, so never releasing
    /// would grow it by one raster per recompile — a leak on every keystroke.
    pub(super) retired_images: Vec<std::sync::Arc<gpui::RenderImage>>,
    /// First-frame compile is deferred to `render` (needs a `Window`).
    pub(super) initialized: bool,
    /// Whether the layout rail (C2) is showing.
    ///
    /// A toggle rather than permanent chrome, per `typst-controls.md`'s
    /// ruling: the review describes layout fiddling as what you do in the last
    /// forty minutes before sending and then not again until the next wave. A
    /// surface used that way earns space *while in use*.
    /// How long the last full-resolution compile took.
    ///
    /// The draft pass exists to put *something* on screen fast, and it costs a
    /// visibly soft page for as long as the user keeps typing. Whether that
    /// trade is worth making depends on the document, the zoom, the machine
    /// and the build — a constant chosen from one benchmark would be wrong
    /// everywhere else. So it is measured here and re-read on the next edit.
    pub(super) last_crisp: Option<Duration>,

    /// Preview zoom, in percent. **Ephemeral view state**, deliberately not
    /// stored with the document: how close you are looking is not a property
    /// of the CV (the Typst-controls spec §8).
    /// Percent, continuous rather than one of the steps: a trackpad pinch
    /// reports a fractional delta and snapping it to the nearest step on every
    /// frame would make the gesture stutter. `+`/`-` still land exactly on a
    /// step — a user who asks for 100% gets 100%.
    pub(super) zoom_pct: f32,
    /// The scale the last render was rasterized at, so a zoom change can tell
    /// whether it needs a sharper pass.
    pub(super) last_scale: f32,
    /// The compiler's last measurement of the laid-out pages.
    pub(super) geometry: Option<PageGeometry>,
    pub(super) layout_rail_open: bool,
    /// Which of the rail's groups is expanded. One at a time: the rail is
    /// 220px of a window someone is reading a document in, and four headings
    /// with one open is a shorter thing to scan than nine controls in a
    /// column. Typography first, because type is what people change first.
    pub(super) layout_group: usize,
    /// The rail's two sliders. Built lazily — a `SliderState` needs a
    /// `Window`, which `Root::new` does not have, same as `Root::fields`.
    pub(super) margin_slider: Option<Entity<SliderState>>,
    pub(super) scale_slider: Option<Entity<SliderState>>,
    /// Kept alive so the sliders keep reporting movement.
    pub(super) slider_subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for Root {}

impl Root {
    /// Build an editor over a document that has **already been read**.
    ///
    /// Taking `doc` rather than a path is the fix for the worst thing this file
    /// did. It used to load the file itself and answer a parse failure by
    /// seeding the bundled AltaCV sample and writing it straight to `doc_path`
    /// — destroying a real CV whose only problem was a typo in a format the
    /// product tells people to hand-edit. Reading is now `Shell::open_doc`'s
    /// job, which can refuse and say so; by the time the editor exists there is
    /// a document, so there is no failure left here to answer badly.
    ///
    /// New documents arrive the same way: `Shell::create_doc` writes the file
    /// through `vault::create_document` and then opens it, so the "seed if
    /// absent" branch had no legitimate caller either.
    pub fn new(doc_path: PathBuf, doc: ResumeDoc, cx: &mut Context<Self>) -> Self {
        let vault_dir = doc_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let library = vault::load_library(&vault_dir);
        let diary = vault::load_diary(&vault_dir);

        let engine = Arc::new(Mutex::new(TypstEngine::new(template::generate(
            &doc.compose(),
        ))));

        Self {
            engine,
            doc,
            rendered: None,
            compile_state: CompileState::Compiling,
            last_source: String::new(),
            expanded: HashSet::from([SectionKind::Profile]),
            focused_section: SectionKind::Profile,
            fields: HashMap::new(),
            fields_stale: false,
            focus_handle: cx.focus_handle(),
            recompile_task: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            doc_path,
            save_task: None,
            vault_dir,
            library,
            library_picker: None,
            diary,
            diary_picker: None,
            capture_sheet: None,
            export_sheet: None,
            renaming_section: None,
            renaming_variant: None,
            active_preset: None,
            retired_images: Vec::new(),
            initialized: false,
            last_crisp: None,
            zoom_pct: 100.0,
            last_scale: 0.0,
            geometry: None,
            layout_rail_open: false,
            layout_group: 0,
            margin_slider: None,
            scale_slider: None,
            slider_subscriptions: Vec::new(),
        }
    }

    // --- presets (toolbar preset menu, design doc §8) ---

    /// Switch every section to the variants recorded in preset `index` and
    /// remember it as the toolbar's displayed preset.
    pub(super) fn apply_preset(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        self.doc.apply_preset(index);
        self.active_preset = Some(index);
        self.schedule_save(cx);
        self.fields_stale = true;
        cx.notify();
        self.schedule_recompile(window, cx);
    }

    /// Save the document's current section×variant selection as a new named
    /// preset (`ResumeDoc::add_preset`) and make it the toolbar's displayed
    /// preset. Not "capture" — see `ResumeDoc::add_preset`'s own doc comment
    /// for why that word is reserved for the Diary's quick-capture (D-7).
    pub(super) fn save_current_as_preset(&mut self, cx: &mut Context<Self>) {
        self.checkpoint();
        let n = self.doc.presets.len() + 1;
        self.doc.add_preset(format!("Preset {n}"));
        self.active_preset = Some(self.doc.presets.len() - 1);
        self.schedule_save(cx);
        self.fields_stale = true;
        cx.notify();
    }

    /// Delete the preset currently shown in the toolbar. The old preset bar
    /// exposed this per-chip (✕); the merged toolbar (design doc §3) has no
    /// room to draw it per-preset, so it moves into the menu as a single
    /// action on whichever preset is selected.
    pub(super) fn remove_active_preset(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.active_preset else {
            return;
        };
        self.checkpoint();
        self.doc.remove_preset(index);
        self.active_preset = None;
        self.schedule_save(cx);
        self.fields_stale = true;
        cx.notify();
    }

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

    /// The document's identity as shown in the titlebar breadcrumb
    /// (`Albert — Senior SWE`) — reused by the capture sheet so the user can
    /// see which document a captured entry will be linked to.
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

    /// Debounced background write of the document to its cvault file.
    ///
    /// The outcome is recorded rather than discarded: this is the write that
    /// runs on every keystroke, so it is also the one whose silent failure cost
    /// the most — a read-only vault meant an afternoon of edits existed only on
    /// screen.
    pub(super) fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let doc = self.doc.clone();
        let path = self.doc_path.clone();
        let executor = cx.background_executor().clone();

        self.save_task = Some(cx.spawn(async move |_this, cx| {
            executor.timer(SAVE_DEBOUNCE).await;
            let result = executor
                .spawn(async move { vault::save(&doc, &path) })
                .await;
            cx.update(|cx| {
                save_status::record(cx, "document", result);
                // The banner lives on `Shell`'s frame, which nothing else here
                // touches, so this write needs its own repaint request.
                cx.refresh_windows();
            });
        }));
    }

    /// Write the document out now, ignoring the debounce.
    ///
    /// Called when the editor is about to stop existing — the app is quitting,
    /// the window is closing, or `Shell` is swapping the screen out from under
    /// it. Dropping the entity cancels [`Root::save_task`], so without this the
    /// last 600 ms of typing goes nowhere.
    pub fn flush_save(&self) -> Result<(), String> {
        vault::save(&self.doc, &self.doc_path)
    }

    /// Synchronous compile, used once for the first frame so the preview is not
    /// blank on launch.
    /// Pixels per typographic point the preview should rasterize at, derived
    /// from the size the sheet is **actually drawn**.
    ///
    /// This used to be `window.scale_factor()` — 2.0 on a retina display, and
    /// nothing to do with how large the page was on screen. At 100% zoom that
    /// happened to be roughly right; at 200% it left the same raster stretched
    /// across twice the pixels, which is why zooming in made the page softer
    /// instead of sharper, and why the whole preview "felt like a half
    /// measure". Zoom now *re-renders* rather than magnifying a bitmap.
    pub(super) fn crisp_scale(&self, window: &Window) -> f32 {
        let page_width_pt = self.doc.layout.page_size.width_pt();
        if page_width_pt <= 0.0 {
            return window.scale_factor();
        }
        // Device pixels the sheet occupies, divided by the points it is wide.
        let device_px = self.preview_width() * window.scale_factor();
        (device_px / page_width_pt).clamp(1.0, MAX_RENDER_SCALE)
    }

    pub(super) fn recompile_now(&mut self, window: &mut Window) {
        let source = template::generate_for(&self.doc);
        let scale = self.crisp_scale(window);
        self.last_scale = scale;
        self.last_source = source.clone();
        self.compile_state = CompileState::Compiling;
        let outcome = compile(&self.engine, source, scale);
        self.apply_render(outcome, window);
    }

    /// Debounced, off-thread recompile. Call this on every edit: the UI has
    /// already updated the model/caret, and this catches the preview up shortly
    /// after the user stops typing — without ever blocking the UI thread.
    pub(super) fn schedule_recompile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = template::generate_for(&self.doc);
        let crisp_scale = self.crisp_scale(window);

        // Skip work entirely when neither the document nor the resolution it
        // needs has changed — editing a variant name changes no output, and
        // re-rendering at a scale we already have would be a compile for
        // nothing.
        if source == self.last_source && (crisp_scale - self.last_scale).abs() < 0.01 {
            return;
        }
        self.last_source = source.clone();
        self.last_scale = crisp_scale;
        self.compile_state = CompileState::Compiling;

        // The draft is half the final resolution rather than a fixed 1×: at
        // 1× the raster was *narrower than the sheet on screen* and the page
        // read as soft the whole time you were typing.
        let draft_scale = (crisp_scale * 0.5).max(1.0);

        // **Skip the draft when the sharp pass is quick enough to wait for.**
        //
        // The draft buys latency to first pixel and pays for it in a page that
        // is visibly soft for as long as the user keeps typing — at 100% on a
        // 2× display the draft is stretched 1.6×. When the full-resolution
        // compile lands inside a frame nobody perceives the wait, so the blur
        // is bought with nothing. The first edit of a session has no
        // measurement yet and takes the draft, which is also the compile most
        // likely to be slow: nothing is cached.
        let skip_draft =
            crisp_scale <= draft_scale + 0.01 || self.last_crisp.is_some_and(|d| d <= FRAME_BUDGET);

        let engine = self.engine.clone();
        let executor = cx.background_executor().clone();

        self.recompile_task = Some(cx.spawn_in(window, async move |this, cx| {
            if !skip_draft {
                executor.timer(DRAFT_DEBOUNCE).await;
                let draft = {
                    let engine = engine.clone();
                    let source = source.clone();
                    executor
                        .spawn(async move { compile(&engine, source, draft_scale) })
                        .await
                };
                if this
                    .update_in(cx, |this, window, _cx| this.apply_render(draft, window))
                    .is_err()
                {
                    return;
                }
                // The draft already is the final resolution — nothing to sharpen.
                if crisp_scale <= draft_scale + 0.01 {
                    return;
                }
                // Only if no new edit cancels us first.
                executor.timer(CRISP_DELAY).await;
            } else {
                executor.timer(DRAFT_DEBOUNCE).await;
            }

            let started = Instant::now();
            let crisp = executor
                .spawn(async move { compile(&engine, source, crisp_scale) })
                .await;
            let elapsed = started.elapsed();
            let _ = this.update_in(cx, |this, window, _cx| {
                this.last_crisp = Some(elapsed);
                this.apply_render(crisp, window);
            });
        }));
    }

    /// Export the current composition to PDF via a native Save dialog.
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

    /// Store a freshly rendered page (freeing the previous GPU texture) and
    /// update [`Root::compile_state`] from what the attempt produced. A
    /// failed compile never touches `self.rendered` — the last good frame
    /// stays on screen exactly as before this change.
    pub(super) fn apply_render(&mut self, outcome: RenderOutcome, _window: &mut Window) {
        match outcome.rendered {
            Some(rendered) => {
                if let Some(previous) = self.rendered.take() {
                    // Retired, not dropped — see `retired_images`.
                    self.retired_images.push(previous.image);
                }
                self.rendered = Some(rendered);
                // Only on success: a failed compile leaves the last good
                // measurement beside the last good frame, so the page counter
                // does not blank out while you fix a typo.
                if outcome.geometry.is_some() {
                    self.geometry = outcome.geometry;
                }
                self.compile_state = CompileState::Ready {
                    warnings: outcome.messages,
                };
            }
            None => {
                self.compile_state = CompileState::Error {
                    messages: outcome.messages,
                }
            }
        }
    }

    /// Every message from the most recent failed compile, joined into one
    /// string — feeds `render_preview`'s existing plain-text error box. The
    /// design doc's warning-styled banner with section attribution and a
    /// "Jump to section →" action (the Typst-controls spec) is a
    /// separate, later task; this just keeps the current display correct
    /// under the new state shape.
    pub(super) fn compile_error_text(&self) -> Option<String> {
        match &self.compile_state {
            CompileState::Error { messages } => Some(
                messages
                    .iter()
                    .map(|m| m.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        }
    }

    /// Bring [`Root::fields`] in line with the document.
    ///
    /// Runs once per frame. Fields keep their state — and therefore their caret,
    /// selection and undo history — across renders; they are rebuilt only when
    /// `fields_stale` says the model moved under them.
    pub(super) fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.fields_stale {
            self.fields.clear();
            self.fields_stale = false;
            self.focus_handle.focus(window, cx);
        }

        let wanted = FieldId::addressable(&self.doc);
        for field in &wanted {
            if self.fields.contains_key(field) {
                continue;
            }
            let Some(value) = field.get(&self.doc).cloned() else {
                continue;
            };
            self.fields
                .insert(*field, self.bind_field(*field, value, window, cx));
        }

        // Retire states for fields the document no longer has, so a deleted
        // bullet cannot leave a live input behind.
        if self.fields.len() > wanted.len() {
            let live: HashSet<FieldId> = wanted.into_iter().collect();
            self.fields.retain(|field, _| live.contains(field));
        }
    }

    /// Create one field's state, seed it from the model, and wire its changes back.
    fn bind_field(
        &self,
        field: FieldId,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> FieldBinding {
        let state = cx.new(|cx| {
            let state = if field.multiline() {
                TextFieldState::auto_grow(2, 12, window, cx)
            } else {
                TextFieldState::single_line(window, cx)
            };
            state.seed(value, window, cx);
            state
        });

        let subscription = cx.subscribe_in(
            &state,
            window,
            move |this, state, event: &TextFieldEvent, window, cx| {
                if !matches!(event, TextFieldEvent::Changed) {
                    return;
                }
                let value = state.read(cx).value(cx).to_string();
                let Some(target) = field.get_mut(&mut this.doc) else {
                    return;
                };
                if *target == value {
                    return;
                }
                *target = value;
                this.schedule_save(cx);
                this.schedule_recompile(window, cx);
                cx.notify();
            },
        );

        FieldBinding {
            state,
            _subscription: subscription,
        }
    }

    // --- keyboard navigation action handlers (P-17) ---

    pub(super) fn on_focus_next_section(
        &mut self,
        _: &FocusNextSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_section(1, window, cx);
    }

    pub(super) fn on_focus_prev_section(
        &mut self,
        _: &FocusPrevSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_section(-1, window, cx);
    }

    /// Move [`Root::focused_section`] to the next/previous of the six fixed
    /// sections, wrapping, and blur whatever field currently has OS focus —
    /// moving the navigation cursor away from a field you're mid-edit in
    /// should stop reading as "still editing here".
    fn step_section(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let sections = ResumeDoc::SECTIONS;
        let current = sections
            .iter()
            .position(|&s| s == self.focused_section)
            .unwrap_or(0);
        let len = sections.len() as isize;
        let next = (current as isize + delta).rem_euclid(len) as usize;
        self.focused_section = sections[next];
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(super) fn on_toggle_focused_section(
        &mut self,
        _: &ToggleFocusedSection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.expanded.insert(self.focused_section) {
            self.expanded.remove(&self.focused_section);
        }
        cx.notify();
    }

    pub(super) fn on_next_variant(
        &mut self,
        _: &NextVariant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_variant(1, window, cx);
    }

    pub(super) fn on_prev_variant(
        &mut self,
        _: &PrevVariant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_variant(-1, window, cx);
    }

    /// Switch [`Root::focused_section`] to its next/previous variant, wrapping
    /// — the same side effects `root_sidebar.rs`'s variant pills trigger on
    /// click.
    fn step_variant(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let section = self.focused_section;
        let count = self.doc.variant_names(section).len();
        if count == 0 {
            return;
        }
        let current = self.doc.active_variant(section) as isize;
        let next = (current + delta).rem_euclid(count as isize) as usize;
        self.checkpoint();
        self.doc.set_active_variant(section, next);
        self.schedule_save(cx);
        self.fields_stale = true;
        cx.notify();
        self.schedule_recompile(window, cx);
    }

    pub(super) fn on_next_preset(
        &mut self,
        _: &NextPreset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_preset(1, window, cx);
    }

    pub(super) fn on_prev_preset(
        &mut self,
        _: &PrevPreset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_preset(-1, window, cx);
    }

    /// Apply the document's next/previous preset, wrapping; starts at the
    /// first (next) or last (previous) preset if none is applied yet.
    fn step_preset(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.doc.presets.len();
        if count == 0 {
            return;
        }
        let next = match self.active_preset {
            Some(i) => (i as isize + delta).rem_euclid(count as isize) as usize,
            None if delta >= 0 => 0,
            None => count - 1,
        };
        self.apply_preset(next, window, cx);
    }

    pub(super) fn on_focus_next_field(
        &mut self,
        _: &FocusNextField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_field(1, window, cx);
    }

    pub(super) fn on_focus_prev_field(
        &mut self,
        _: &FocusPrevField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_field(-1, window, cx);
    }

    /// Move OS input focus to the next/previous field of
    /// [`Root::focused_section`], wrapping. Expands the section first — field
    /// navigation into a collapsed card would move focus somewhere the user
    /// can't see.
    fn step_field(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let section = self.focused_section;
        self.expanded.insert(section);

        let ids: Vec<FieldId> = FieldId::addressable(&self.doc)
            .into_iter()
            .filter(|id| id.section() == Some(section))
            .collect();
        if ids.is_empty() {
            cx.notify();
            return;
        }

        let current = ids.iter().position(|id| {
            self.fields
                .get(id)
                .map(|binding| binding.state.read(cx).is_focused(window, cx))
                .unwrap_or(false)
        });
        let next_index = match current {
            Some(i) => (i as isize + delta).rem_euclid(ids.len() as isize) as usize,
            None if delta >= 0 => 0,
            None => ids.len() - 1,
        };

        if let Some(binding) = self.fields.get(&ids[next_index]) {
            let handle = binding.state.read(cx).focus_handle(cx);
            handle.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn on_undo_document(
        &mut self,
        _: &UndoDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.undo_document(window, cx);
    }

    pub(super) fn on_redo_document(
        &mut self,
        _: &RedoDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.redo_document(window, cx);
    }

    pub(super) fn on_input_undo(
        &mut self,
        _: &dockcv_ui_components::input::Undo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.undo_document(window, cx);
    }

    pub(super) fn on_input_redo(
        &mut self,
        _: &dockcv_ui_components::input::Redo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.redo_document(window, cx);
    }

    pub(super) fn on_export_pdf_action(
        &mut self,
        _: &ExportPdf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_pdf_checked(window, cx);
    }

    pub(super) fn on_open_capture_action(
        &mut self,
        _: &OpenCapture,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_capture_sheet(window, cx);
    }

    /// Closes whichever overlay is open, in priority order — only one of
    /// these is ever open at once in normal use, but the order is defensive
    /// rather than assumed.
    pub(super) fn on_close_overlay(
        &mut self,
        _: &CloseOverlay,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.capture_sheet.is_some() {
            self.close_capture_sheet(cx);
        } else if self.diary_picker.is_some() {
            self.diary_picker = None;
            cx.notify();
        } else if self.library_picker.is_some() {
            self.library_picker = None;
            cx.notify();
        } else if self.renaming_section.is_some() {
            self.cancel_rename(cx);
        } else if self.renaming_variant.is_some() {
            self.cancel_variant_rename(cx);
        }
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Release last frame's outgoing rasters, now that a frame has been built
        // without them.
        for image in self.retired_images.drain(..) {
            let _ = window.drop_image(image);
        }

        if !self.initialized {
            self.initialized = true;
            self.focus_handle.focus(window, cx);
            self.recompile_now(window);
        }
        self.sync_fields(window, cx);
        self.ensure_layout_sliders(window, cx);

        let theme = *cx.theme();

        div()
            .id("root")
            .key_context(EDITOR_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_focus_next_section))
            .on_action(cx.listener(Self::on_focus_prev_section))
            .on_action(cx.listener(Self::on_toggle_focused_section))
            .on_action(cx.listener(Self::on_next_variant))
            .on_action(cx.listener(Self::on_prev_variant))
            .on_action(cx.listener(Self::on_next_preset))
            .on_action(cx.listener(Self::on_prev_preset))
            .on_action(cx.listener(Self::on_focus_next_field))
            .on_action(cx.listener(Self::on_focus_prev_field))
            .on_action(cx.listener(Self::on_undo_document))
            .on_action(cx.listener(Self::on_redo_document))
            .on_action(cx.listener(Self::on_input_undo))
            .on_action(cx.listener(Self::on_input_redo))
            .on_action(cx.listener(Self::on_export_pdf_action))
            .on_action(cx.listener(Self::on_open_capture_action))
            .on_action(cx.listener(Self::on_close_overlay))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(self.render_toolbar(cx))
            .child(
                // Draggable split rather than a fixed column. How much room a
                // CV's fields need is a property of the document — a long
                // summary and a one-line job title want different widths — so
                // it is the user's to set, not a constant's to guess.
                //
                // `flex_none()` on the **sized** panel is not decoration — it is
                // the usage `ResizablePanel`'s own doc example shows. The panel
                // applies `flex_none` for itself, but only while the state has
                // not yet recorded a size; after the first measurement the
                // unconditional `flex_grow_1()` above it wins and the panel
                // grows past its own maximum. The second panel must *not* have
                // it: that one takes the remainder.
                // The group must take the **remainder** of the column, not all
                // of it: `ResizablePanelGroup` renders itself `size_full()`, so
                // as a direct child of the column it claimed the window's whole
                // height and sat 56px below the chrome bar — pushing its own
                // bottom, and the preview toolbar anchored to it, past the
                // window edge. This wrapper is what makes `size_full()` mean
                // "the space left over".
                div().flex_1().min_h_0().flex().child(
                    h_resizable("editor-panes")
                        .child(
                            resizable_panel()
                                .flex_none()
                                .size(px(392.0))
                                // Below ~320 the two-column form collapses to one
                                // useful column; above ~680 the preview stops being
                                // a preview.
                                .size_range(px(320.0)..px(680.0))
                                .child(self.render_sidebar(cx)),
                        )
                        .child(resizable_panel().child(self.render_preview(cx))),
                ),
            )
            .children(
                self.library_picker
                    .map(|sec| self.render_library_overlay(cx, sec)),
            )
            .children(
                self.diary_picker
                    .map(|idx| self.render_diary_overlay(cx, idx)),
            )
            .children(
                self.capture_sheet
                    .is_some()
                    .then(|| self.render_capture_sheet(cx)),
            )
            .children(
                self.export_sheet
                    .is_some()
                    .then(|| self.render_export_sheet(cx)),
            )
    }
}

/// A field's live input state together with the subscription that writes its
/// changes into the document. Dropping the pair unsubscribes.
pub(super) struct FieldBinding {
    pub(super) state: Entity<TextFieldState>,
    _subscription: Subscription,
}

/// State for the D-7 quick-capture sheet: just the note field. Not committed
/// to `Root::diary` until the user clicks Save (`Root::commit_capture`).
pub(super) struct CaptureSheet {
    pub(super) text: Entity<TextFieldState>,
    /// The role this win belongs to, empty for none (O-15).
    pub(super) role: String,
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

/// What one compile attempt produced, for [`Root::apply_render`] to fold
/// into [`CompileState`]. `rendered` is `Some` exactly when the compile
/// succeeded *and* its pixels converted to a GPUI image; `messages` is every
/// diagnostic from the attempt, humanized and section-attributed via
/// `resume::diagnostics::describe_all`.
pub(super) struct RenderOutcome {
    pub(super) rendered: Option<Rendered>,
    pub(super) messages: Vec<CompileMessage>,
    /// What the compiler measured about the laid-out pages — the page count
    /// the toolbar shows (US-08's `1 / 2`) and the overflow the chip reports.
    /// `None` when the compile failed, so the last good measurement stays on
    /// screen beside the last good render rather than blanking.
    pub(super) geometry: Option<PageGeometry>,
}

/// Lock the engine, set the source, compile, and rasterize at `scale`. Runs
/// on a background thread; every type crossing back (`RenderOutcome`) is
/// `Send`.
///
/// `source` doubles as the text `resume::diagnostics::describe_all` needs
/// for section attribution — the same generated document a diagnostic's
/// byte offset points into.
fn compile(engine: &Arc<Mutex<TypstEngine>>, source: String, scale: f32) -> RenderOutcome {
    let attempt = {
        let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
        engine.set_source(source.clone());
        engine.compile_with_diagnostics(scale)
    };

    let messages = describe_all(&attempt.diagnostics, &source);
    let mut measured: Option<PageGeometry> = None;

    let rendered = match attempt.result {
        Ok((pixels, geometry)) => match render::pixels_to_render_image(pixels, scale) {
            Ok(rendered) => {
                measured = Some(geometry);
                Some(rendered)
            }
            Err(message) => {
                // Rasterization itself failed (not a Typst diagnostic) —
                // report it the same way, rather than silently keeping the
                // stale `messages` from a compile that actually succeeded.
                return RenderOutcome {
                    geometry: None,
                    rendered: None,
                    messages: vec![CompileMessage {
                        severity: Severity::Error,
                        section: None,
                        text: format!("Couldn't prepare the preview image: {message}."),
                    }],
                };
            }
        },
        Err(()) => None,
    };

    RenderOutcome {
        rendered,
        messages,
        geometry: measured,
    }
}

#[cfg(test)]
mod draft_policy_tests {
    use super::{FRAME_BUDGET, MAX_RENDER_SCALE};
    use std::time::Duration;

    /// The rule `schedule_recompile` applies, isolated from the task it runs
    /// in: the draft is skipped when it would buy nothing.
    fn skips_draft(crisp_scale: f32, last_crisp: Option<Duration>) -> bool {
        let draft_scale = (crisp_scale * 0.5).max(1.0);
        crisp_scale <= draft_scale + 0.01 || last_crisp.is_some_and(|d| d <= FRAME_BUDGET)
    }

    #[test]
    fn a_sharp_pass_inside_a_frame_is_worth_waiting_for() {
        // 100% zoom on a 2× display: the crisp scale a `PageSize::A4` needs.
        assert!(skips_draft(1.61, Some(Duration::from_millis(8))));
        // Slow enough to notice: the draft earns its blur.
        assert!(!skips_draft(3.23, Some(Duration::from_millis(40))));
    }

    /// The first edit of a session has nothing measured — and is the compile
    /// most likely to be slow, since nothing is cached yet.
    #[test]
    fn the_first_edit_takes_the_draft() {
        assert!(!skips_draft(3.23, None));
    }

    /// At low zoom the draft and the sharp pass are the same raster, so there
    /// is no second pass to wait for whatever the timing says.
    #[test]
    fn a_draft_equal_to_the_sharp_pass_is_skipped_regardless() {
        assert!(skips_draft(1.0, None));
        assert!(skips_draft(1.0, Some(Duration::from_secs(1))));
        // And the ceiling is still the ceiling.
        assert!(!skips_draft(
            MAX_RENDER_SCALE,
            Some(Duration::from_millis(40))
        ));
    }
}

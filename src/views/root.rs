//! The root view: the resume editor shell.
//!
//! Left: a sections navigator built from the recognized [`ResumeDoc`] model.
//! Each section is a collapsible card whose body holds **editable fields**. Every
//! field addressed by a [`FieldId`] owns a live [`TextFieldState`]; the editor
//! keeps them in [`Root::fields`] and writes each change straight back into the
//! string the id points at. Right: the live Typst preview, rerendered from the
//! model on every edit.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    div, prelude::*, px, Context, Entity, EventEmitter, FocusHandle, Subscription, Task, Window,
};

use dockcv_ui_components::{h_resizable, resizable_panel, SliderState, TextFieldState};

use crate::render::Rendered;
use crate::resume::ats::Finding;
use crate::resume::edit::FieldId;
use crate::resume::model::{Diary, Library, ProfileCatalog, ResumeDoc, SectionKind};
use crate::resume::template;
use crate::theme::ActiveTheme;
use crate::typst_engine::{PageGeometry, TypstEngine};
use crate::vault;

pub(crate) mod ats_lint;
pub(crate) mod custom_sections;
pub(crate) mod dates;
pub(crate) mod editor_state;
pub(crate) mod export_rail;
pub(crate) mod export_sheet;
pub(crate) mod export_write;
pub(crate) mod fields;
pub(crate) mod highlights;
pub(crate) mod keybindings;
pub(crate) mod layout_rail;
pub(crate) mod layout_rows;
pub(crate) mod library_capture;
pub(crate) mod overlays;
pub(crate) mod preset_control;
pub(crate) mod preview_chrome;
pub(crate) mod recompile;
pub(crate) mod save;
pub(crate) mod section_chrome;
pub(crate) mod section_drag;
pub(crate) mod section_layout;
pub(crate) mod section_rename;
pub(crate) mod section_variants;
pub(crate) mod sidebar;
pub(crate) mod undo;

pub use keybindings::{init_keybindings, keys, ExportPdf, EDITOR_CONTEXT};
pub(crate) use keybindings::{NextPreset, OpenCapture, RedoDocument, UndoDocument};
pub use recompile::CompileState;

use self::editor_state::{EditorMode, EditorSelection, LayoutCategory};
pub(super) use self::fields::FieldBinding;
use self::library_capture::CaptureSheet;
use self::section_rename::SectionRename;
use self::section_variants::VariantRename;

/// Events the editor emits to its host shell.
pub enum EditorEvent {
    BackToGallery,
    /// P-01: the toolbar's preset menu must be a door into the Preset
    /// Matrix, scoped to this document — `Shell` owns that screen, so the
    /// editor asks for it rather than navigating there itself.
    OpenPresetMatrix,
}

pub struct Root {
    /// Shared so the debounced recompile can run on a background thread without
    /// blocking the UI. `TypstEngine` is `Send` (its `World` is `Send + Sync`).
    pub(super) engine: Arc<Mutex<TypstEngine>>,
    pub(super) doc: ResumeDoc,
    /// User profiles from `<vault>/profiles.toml`. Built-ins are supplied by
    /// `ProfileCatalog` itself and therefore exist even when this is empty.
    pub(super) profiles: ProfileCatalog,
    pub(super) profiles_on_disk: vault::OnDisk,
    /// Why a shared profile was detached by the first manual layout edit.
    /// Ephemeral interaction state; the durable result is `layout_profile =
    /// None` plus the copied-and-edited `layout` in the document.
    pub(super) profile_detachment: Option<ProfileDetachment>,
    /// Inline name field opened by “Save as a new profile…”.
    pub(super) profile_fork: Option<ProfileFork>,
    /// Deterministic B2 findings for the current working copy, refreshed once
    /// per frame and shared by every section card.
    pub(super) ats_findings: Vec<Finding>,
    pub(super) rendered: Option<Rendered>,
    /// Visible compile status — see [`CompileState`].
    pub(super) compile_state: CompileState,
    /// The composed source last sent to the renderer; used to skip recompiles
    /// when an edit doesn't change the rendered output (e.g. renaming a
    /// variant or preset).
    pub(super) last_source: String,
    /// Which navigator branches show their entries. Only the selected
    /// section's branch is drawn, so at most one of these is visible at a
    /// time; the set remembers the rest, so returning to a section finds it
    /// the way you left it.
    ///
    /// Keyed by `SectionKind` rather than a title string — a custom section's
    /// title is user-editable (D-9), and keying expand/collapse state by text
    /// that can change on every keystroke would both collide (two custom
    /// sections both titled "New Section" until renamed) and reset on every
    /// edit.
    pub(super) expanded: HashSet<SectionKind>,
    /// The section keyboard navigation (`FocusNextSection`/`FocusPrevSection`)
    /// currently points at — always a section the document actually has,
    /// custom ones included, and never empty, so every other keyboard action
    /// (`ToggleFocusedSection`,
    /// `NextVariant`, `FocusNextField`, …) always has a section to act on.
    pub(super) focused_section: SectionKind,
    pub(super) selection: EditorSelection,
    pub(super) editor_mode: EditorMode,
    /// Whether the document on screen differs from the one on disk.
    ///
    /// True from the edit until the debounced write lands, and it *stays* true
    /// when that write fails — which is the whole point. The banner explains a
    /// failure; this says, at all times and without being asked, whether the
    /// file has what you are looking at. In a product whose entire promise is
    /// that the data is yours in a folder you own, that is the one fact that
    /// may never be absent from the screen.
    pub(super) unsaved: bool,
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
    /// What `doc_path` held when this editor took its copy, or when it last
    /// wrote one. An open document is held in memory for as long as the screen
    /// is up, and every write replaces the whole file — so without this, an
    /// edit made in another editor between two debounced saves was overwritten
    /// with no sign that it had ever been there. See [`vault::OnDisk`].
    pub(super) on_disk: vault::OnDisk,
    /// Set by the vault watcher when `doc_path` changed on disk. Acted on in
    /// `render` rather than where it is set, because taking the file's version
    /// means recompiling the preview and that needs a `Window` — the same
    /// reason `fields_stale` is a flag and not a call.
    pub(super) external_change_pending: bool,
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
    pub(super) export_sheet: Option<export_sheet::ExportSheetState>,
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
    /// How the sheet is sized against the pane — and therefore whether
    /// `zoom_pct` is in force at all. Ephemeral like the zoom itself.
    pub(super) preview_fit: preview_chrome::PreviewFit,
    /// The preview pane's own size in display pixels, written each paint by the
    /// canvas in `render_preview`. A fit mode is a question about the pane, and
    /// GPUI only answers it at layout time.
    pub(super) preview_pane: Option<(f32, f32)>,
    /// The scale the last render was rasterized at, so a zoom change can tell
    /// whether it needs a sharper pass.
    pub(super) last_scale: f32,
    /// The compiler's last measurement of the laid-out pages.
    pub(super) geometry: Option<PageGeometry>,
    /// Which category Layout mode is showing. One at a time, listed down the
    /// panel's own left column — the same navigator-and-inspector shape
    /// Content mode uses, so switching modes changes what the two columns hold
    /// and not how the panel is read. Typography first, because type is what
    /// people change first.
    pub(super) layout_category: LayoutCategory,
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
        let (profiles, profiles_on_disk) = vault::load_profiles_seen(&vault_dir);

        let engine = Arc::new(Mutex::new(TypstEngine::new(
            template::generate_for_with_profiles(&doc, &profiles),
        )));

        // Read here rather than taken as an argument: `Root::new` already owns
        // the path, and every caller that builds an editor would otherwise have
        // to remember to carry the fingerprint alongside the document.
        let on_disk = vault::OnDisk::read(&doc_path);

        Self {
            on_disk,
            external_change_pending: false,
            engine,
            doc,
            profiles,
            profiles_on_disk,
            profile_detachment: None,
            profile_fork: None,
            ats_findings: Vec::new(),
            rendered: None,
            compile_state: CompileState::Compiling,
            last_source: String::new(),
            expanded: HashSet::from([SectionKind::Profile]),
            focused_section: SectionKind::Profile,
            selection: EditorSelection::initial(),
            editor_mode: EditorMode::Content,
            unsaved: false,
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
            retired_images: Vec::new(),
            initialized: false,
            last_crisp: None,
            zoom_pct: 100.0,
            // A CV is read at column width, and a sheet floating small in the
            // middle of a wide pane is the state nobody would have chosen.
            preview_fit: preview_chrome::PreviewFit::Width,
            preview_pane: None,
            last_scale: 0.0,
            geometry: None,
            layout_category: LayoutCategory::Typography,
            margin_slider: None,
            scale_slider: None,
            slider_subscriptions: Vec::new(),
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
        if self.external_change_pending {
            self.external_change_pending = false;
            self.adopt_external_change(window, cx);
        }
        self.selection.normalize(&self.doc);
        self.focused_section = self.selection.section;
        self.sync_fields(window, cx);
        self.ensure_layout_sliders(window, cx);
        self.refresh_ats_findings();

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
            .children(
                self.profile_detachment
                    .as_ref()
                    .map(|_| self.render_profile_detachment(cx)),
            )
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
                                .size(px(520.0))
                                // Below ~320 the two-column form collapses to one
                                // useful column; above ~680 the preview stops being
                                // a preview.
                                .size_range(px(440.0)..px(680.0))
                                .child(match self.editor_mode {
                                    EditorMode::Content => {
                                        self.render_sidebar(cx).into_any_element()
                                    }
                                    EditorMode::Layout => {
                                        self.render_layout_panel(cx).into_any_element()
                                    }
                                }),
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

/// A manual change made while a shared profile was selected.
#[derive(Clone)]
pub(super) struct ProfileDetachment {
    pub(super) profile: String,
    pub(super) changed: String,
    /// Distinct CVs the profile currently reaches. Captured before an update
    /// is offered, so the button says the write's scope before it happens.
    pub(super) affected: Vec<String>,
}

/// Live field for naming a detached layout as a new profile.
pub(super) struct ProfileFork {
    pub(super) field: Entity<TextFieldState>,
    pub(super) _subscription: Subscription,
}


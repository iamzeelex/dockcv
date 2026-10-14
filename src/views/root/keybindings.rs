use gpui::{actions, App, Context, KeyBinding, Window};

use crate::resume::edit::FieldId;
use crate::views::root::editor_state::{EditorMode, EditorSelection};

use super::Root;

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
pub const EDITOR_CONTEXT: &str = "DockCvEditor";

/// Chord literals, in `KeyBinding::new`'s own `-`-joined syntax. Centralized
/// so `init_keybindings` and every on-screen hint read the same literal and
/// can never drift apart. `pub` so the variant pills' tooltips can
/// name the chord they are driven by.
pub mod keys {
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

impl Root {
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

    /// Move through the document's actual section order, including custom
    /// sections, and blur whatever field currently has OS focus —
    /// moving the navigation cursor away from a field you're mid-edit in
    /// should stop reading as "still editing here".
    fn step_section(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let sections = self.doc.sections();
        let current = sections
            .iter()
            .position(|&s| s == self.focused_section)
            .unwrap_or(0);
        let len = sections.len() as isize;
        let next = (current as isize + delta).rem_euclid(len) as usize;
        self.focused_section = sections[next];
        self.selection = EditorSelection::for_section(&self.doc, sections[next]);
        self.expanded.insert(sections[next]);
        self.editor_mode = EditorMode::Content;
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
        self.selection.normalize(&self.doc);
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
        let next = match self.active_preset() {
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

        self.selection = EditorSelection {
            section,
            item: ids[next_index].item_index(),
        };
        self.editor_mode = EditorMode::Content;
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
        if self.assistant_sheet.is_some() {
            self.close_assistant_sheet(cx);
        } else if self.capture_sheet.is_some() {
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

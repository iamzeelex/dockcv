//! What you can do to one **version**, from its row on the front door.
//!
//! Split from the document's own actions deliberately. Both were reached
//! through a `···` and both opened the *same* menu, so `Delete` on a version
//! row deleted the whole CV. Splitting the menus fixed the wiring;
//! `front_door/menus.rs` then had to fix the appearance, because two identical
//! `···` at the same right edge still told a reader nothing about which object
//! they were about to act on.

use gpui::prelude::*;
use gpui::{Context, Window};
use std::path::{Path, PathBuf};

use dockcv_ui_components::{TextFieldEvent, TextFieldState};

use crate::resume::edit::FieldId;
use crate::vault;

use crate::views::save_status;
use crate::views::shell::Shell;

/// A version being renamed in place: which one, and the live field.
pub(crate) struct VersionRename {
    pub path: PathBuf,
    pub index: usize,
    pub field: gpui::Entity<TextFieldState>,
    /// Keeps the `TextFieldEvent` → commit translation alive for the rename.
    pub _subscription: gpui::Subscription,
}

impl Shell {
    /// Begin naming a version from its row.
    ///
    /// The point of marking a generated name as a placeholder is that the mark
    /// leads somewhere. Without this, `Preset 1` was labelled as unnamed by a
    /// screen that offered no way to name it.
    pub(crate) fn start_version_rename(
        &mut self,
        path: &Path,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(doc) = vault::load(path) else {
            return;
        };
        let Some(current) = doc.presets.get(index).map(|p| p.name.clone()) else {
            return;
        };
        let field = cx.new(|cx| {
            let state = TextFieldState::single_line(window, cx);
            state.seed(current, window, cx);
            state
        });
        // Enter or clicking away commits — the same gesture the matrix and the
        // editor already use, so a person who has renamed one thing in this
        // product does not have to learn a second way.
        let subscription = cx.subscribe_in(
            &field,
            window,
            move |this, _state, event: &TextFieldEvent, _window, cx| match event {
                TextFieldEvent::Submitted | TextFieldEvent::Blurred => {
                    this.commit_version_rename(cx)
                }
                TextFieldEvent::Changed | TextFieldEvent::Focused => {}
            },
        );

        let handle = field.read(cx).focus_handle(cx);
        self.renaming_version = Some(VersionRename {
            path: path.to_path_buf(),
            index,
            field,
            _subscription: subscription,
        });
        handle.focus(window, cx);
        cx.notify();
    }

    /// Write the typed name back, or keep the old one if it was left blank.
    ///
    /// Blank is refused rather than stored, the same as everywhere else a name
    /// is edited in this product: a version with no name is a row with nothing
    /// printed on it, and the list has no other way to tell its rows apart.
    pub(crate) fn commit_version_rename(&mut self, cx: &mut Context<Self>) {
        let Some(rename) = self.renaming_version.take() else {
            return;
        };
        let value = rename.field.read(cx).value(cx).trim().to_string();
        if !value.is_empty() {
            if let Ok(mut doc) = vault::load(&rename.path) {
                // Through the addressing layer, never at the field: reaching
                // past `FieldId` is how a value ends up in the model and
                // nowhere else (E-42).
                if let Some(slot) = FieldId::PresetName(rename.index).get_mut(&mut doc) {
                    *slot = value;
                }
                let on_disk = vault::OnDisk::read(&rename.path);
                let result = vault::save(&doc, &rename.path, on_disk);
                save_status::record_document(cx, &rename.path, on_disk, result);
                // The name is part of the key the board records a send under,
                // so what this row has done may read differently now.
                self.load_matrix_records();
            }
        }
        cx.notify();
    }

    /// Copy a version into a second one of the same document.
    pub(crate) fn duplicate_version(&mut self, path: &Path, index: usize, cx: &mut Context<Self>) {
        let Ok(mut doc) = vault::load(path) else {
            return;
        };
        let Some(name) = doc.presets.get(index).map(|p| format!("{} copy", p.name)) else {
            return;
        };
        if doc.add_preset_from(index, name).is_none() {
            return;
        }
        let on_disk = vault::OnDisk::read(path);
        let result = vault::save(&doc, path, on_disk);
        save_status::record_document(cx, path, on_disk, result);
        self.reading_pages.retain(|(p, _), _| p != path);
        cx.notify();
    }

    /// Remove one version. The document and every other version stay.
    ///
    /// Not behind a confirmation, and that is a decision rather than an
    /// omission: a version owns no content — it is a set of selections over
    /// variants that all still exist — so what this destroys is the naming of a
    /// combination, not any of the writing. The document's own `Delete`, which
    /// does destroy writing, keeps its confirmation.
    pub(crate) fn delete_version(&mut self, path: &Path, index: usize, cx: &mut Context<Self>) {
        let Ok(mut doc) = vault::load(path) else {
            return;
        };
        if index >= doc.presets.len() {
            return;
        }
        doc.remove_preset(index);
        let on_disk = vault::OnDisk::read(path);
        let result = vault::save(&doc, path, on_disk);
        save_status::record_document(cx, path, on_disk, result);
        self.reading_pages.retain(|(p, _), _| p != path);
        self.load_matrix_records();
        cx.notify();
    }
}

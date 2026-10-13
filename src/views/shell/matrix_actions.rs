use std::path::PathBuf;

use gpui::{prelude::*, Context, Window};

use dockcv_ui_components::{TextFieldEvent, TextFieldState};

use crate::config;
use crate::resume::export::names::{plan_batch, OnCollision};
use crate::typst_engine::TypstEngine;
use crate::vault;
use crate::views::preset_matrix::export::BatchExportSheet;
use crate::views::save_status;

use super::gallery_actions::{first_path, pick_dir};
use super::navigation::Screen;
use super::Shell;

impl Shell {
    /// Begin renaming the preset in column `idx`.
    ///
    /// `FieldId::PresetName` was addressable from the day presets existed and
    /// no view drew it, so a preset created as `Preset 2` kept that name for
    /// life (G-14). The gesture copies the editor's section rename — pen,
    /// inline field, Enter or clicking away commits — because a user who has
    /// renamed one should not have to learn a second way.
    pub(crate) fn start_preset_rename(
        &mut self,
        idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Screen::PresetMatrix(ref mut pm) = self.screen else {
            return;
        };
        let Some(current) = pm.doc.presets.get(idx).map(|p| p.name.clone()) else {
            return;
        };

        let field = cx.new(|cx| {
            let state = TextFieldState::single_line(window, cx);
            state.seed(current, window, cx);
            state
        });
        let subscription = cx.subscribe_in(
            &field,
            window,
            move |this, _state, event: &TextFieldEvent, window, cx| match event {
                TextFieldEvent::Submitted | TextFieldEvent::Blurred => {
                    this.commit_preset_rename(window, cx)
                }
                TextFieldEvent::Changed | TextFieldEvent::Focused => {}
            },
        );

        let handle = field.read(cx).focus_handle(cx);
        if let Screen::PresetMatrix(ref mut pm) = self.screen {
            pm.renaming_preset = Some(crate::views::preset_matrix::PresetRename {
                idx,
                field,
                _subscription: subscription,
            });
        }
        handle.focus(window, cx);
        cx.notify();
    }

    /// Write the typed name back and close the control. A blank name is
    /// refused rather than stored: an unnamed preset is a column with no
    /// header, and the matrix has no other way to tell its columns apart.
    pub(crate) fn commit_preset_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Screen::PresetMatrix(ref mut pm) = self.screen else {
            return;
        };
        let Some(rename) = pm.renaming_preset.take() else {
            return;
        };
        let value = rename.field.read(cx).value(cx).trim().to_string();
        if !value.is_empty() {
            // Written through the addressing layer rather than at the field:
            // `FieldId::PresetName` has been addressable since presets existed
            // and reaching past it would leave the variant dead, which is how
            // a field ends up in the model and nowhere else (E-42).
            if let Some(slot) =
                crate::resume::edit::FieldId::PresetName(rename.idx).get_mut(&mut pm.doc)
            {
                *slot = value;
            }
            let result = vault::save(&pm.doc, &pm.path, pm.on_disk);
            let (path, seen) = (pm.path.clone(), pm.on_disk);
            pm.on_disk = save_status::record_document(cx, &path, seen, result);
            // What the board recorded was sent under the *old* name, and
            // history keeps the name it was sent under. So a rename usually
            // empties this column's record, and saying so beats carrying the
            // old count under a new heading.
            self.load_matrix_records();
        }
        cx.notify();
    }

    /// Save current section variant configuration in the Preset Matrix as a new preset.
    pub(crate) fn save_matrix_as_preset(&mut self, cx: &mut Context<Self>) {
        let Screen::PresetMatrix(ref mut pm) = self.screen else {
            return;
        };
        // The same placeholder the editor hands out — see
        // `save_current_as_preset` for why it is "Version".
        let new_preset_name = format!("Version {}", pm.doc.presets.len() + 1);
        // The model captures the complete reading: custom-section variants,
        // visibility, order, and headings. Reassembling that here is how a new
        // preset quietly falls behind the next dimension the model gains.
        pm.doc.add_preset(new_preset_name);

        let result = vault::save(&pm.doc, &pm.path, pm.on_disk);
        let (path, seen) = (pm.path.clone(), pm.on_disk);
        pm.on_disk = save_status::record_document(cx, &path, seen, result);
        cx.notify();
        // A new column, with no evidence behind it until it is asked for.
        self.load_matrix_records();
        self.measure_matrix_pages(cx);
    }

    /// Step one of exporting every preset: choose the folder, then show what
    /// would be written into it.
    ///
    /// The folder comes first because a collision is a fact about a folder —
    /// there is no honest list of filenames to show until one is picked.
    pub(crate) fn export_all_matrix_presets(&mut self, cx: &mut Context<Self>) {
        let Screen::PresetMatrix(pm) = &self.screen else {
            return;
        };
        if pm.doc.presets.is_empty() {
            save_status::record(
                cx,
                "export",
                Err("This document has no presets yet, so there is nothing to export.".to_string()),
            );
            return;
        }
        let receiver = cx.prompt_for_paths(pick_dir());

        cx.spawn(async move |this, cx| {
            let Some(folder) = first_path(receiver.await) else {
                return; // cancelled or dialog error
            };
            let _ = this.update(cx, |this, cx| {
                let Screen::PresetMatrix(pm) = &this.screen else {
                    return;
                };
                // `KeepBoth` is the default because it is the answer that
                // cannot lose somebody's file. Replacing is available, and it
                // is a thing the user has to say.
                let plan = plan_batch(
                    &pm.doc,
                    &folder,
                    "pdf",
                    &crate::views::root_export_sheet::today(),
                    OnCollision::KeepBoth,
                );
                let ats_counts = (0..pm.doc.presets.len())
                    .map(|index| {
                        crate::views::root_ats_lint::findings_for_view(&pm.doc, Some(index)).len()
                    })
                    .collect();
                this.batch_export = Some(BatchExportSheet {
                    folder,
                    on_collision: OnCollision::KeepBoth,
                    plan,
                    ats_counts,
                    writing: false,
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// Step two: write the files the sheet listed, and only those.
    ///
    /// The compile happens off the main thread and returns the paths it actually
    /// wrote, which is the only thing export history may record — a history that
    /// points at a name a collision pushed aside is worse than none.
    pub(crate) fn run_batch_export(&mut self, cx: &mut Context<Self>) {
        let Screen::PresetMatrix(pm) = &self.screen else {
            return;
        };
        let Some(sheet) = &mut self.batch_export else {
            return;
        };
        if sheet.writing {
            return;
        }
        sheet.writing = true;
        let doc = pm.doc.clone();
        let profiles = pm.profiles.clone();
        let folder = sheet.folder.clone();
        let plan = sheet.plan.clone();
        let executor = cx.background_executor().clone();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = executor
                .spawn(async move {
                    let mut written: Vec<(String, PathBuf)> = Vec::new();
                    for step in &plan {
                        let mut preset_doc = doc.clone();
                        preset_doc.apply_preset(step.preset_index);
                        let source = crate::resume::template::generate_for_with_profiles(
                            &preset_doc,
                            &profiles,
                        );
                        let pdf_bytes = TypstEngine::new(source).compile_to_pdf()?;
                        std::fs::write(&step.destination.target, pdf_bytes).map_err(|e| {
                            format!("write to {} failed: {e}", step.destination.target.display())
                        })?;
                        written.push((step.preset.clone(), step.destination.target.clone()));
                    }
                    Ok::<Vec<(String, PathBuf)>, String>(written)
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.batch_export = None;
                match outcome {
                    Ok(written) => {
                        log::info!("exported {} presets to {}", written.len(), folder.display());
                        if let Screen::PresetMatrix(pm) = &mut this.screen {
                            let now = chrono::Local::now();
                            let date = now.format("%Y-%m-%d").to_string();
                            let time = now.format("%H:%M").to_string();
                            for (preset, path) in &written {
                                pm.doc
                                    .record_export(&date, &time, "PDF", preset, path.clone());
                            }
                            let mut config = config::load();
                            config.remember_export_destination(&pm.path, &folder);
                            config::save(&config);
                            let result = vault::save(&pm.doc, &pm.path, pm.on_disk);
                            let (path, seen) = (pm.path.clone(), pm.on_disk);
                            pm.on_disk = save_status::record_document(cx, &path, seen, result);
                        }
                    }
                    Err(message) => {
                        // Some presets may already be on disk. Say so, rather
                        // than leaving a half-written folder and a silent log.
                        //
                        // The sheet closes on the way out either way: after a
                        // partial write its list is a description of a folder
                        // that no longer exists, and asking again re-plans
                        // against what is actually there now.
                        log::error!("batch export to {} failed: {message}", folder.display());
                        save_status::record(cx, "export", Err(message));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

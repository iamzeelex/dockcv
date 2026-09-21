//! The Preset Matrix's side of `Shell`: setting a cell, and gathering the
//! evidence its column headers carry.
//!
//! Split out rather than piled onto `shell.rs`, which is past 1900 lines
//! against a ~800 limit and has been split once already. `preset_matrix.rs`
//! holds the state, `preset_matrix/grid.rs` draws it, and the actions that
//! reach the vault and the compiler are here.

use std::sync::{Arc, Mutex};

use gpui::Context;

use crate::resume::model::SectionKind;
use crate::typst_engine::TypstEngine;
use crate::vault;

use super::Choice;
use crate::views::save_status;
use crate::views::shell::{Screen, Shell};

impl Shell {
    /// Pin one matrix cell, or hide the section in that preset.
    ///
    /// Writing straight to disk rather than debouncing: a preset is one line of
    /// TOML and this is a deliberate choice from a menu, not typing — there is
    /// nothing to coalesce.
    ///
    /// `Choice::Unpinned` is not offered by the menu and is not accepted here.
    /// A preset names every section (`reconcile_presets`), so un-pinning one
    /// would be a way to make a preset incomplete on purpose, and the answer to
    /// "this preset should not show Skills" is `Hidden`, which says so.
    pub(crate) fn set_matrix_cell(
        &mut self,
        preset: usize,
        section: SectionKind,
        choice: Choice,
        cx: &mut Context<Self>,
    ) {
        let Screen::PresetMatrix(ref mut pm) = self.screen else {
            return;
        };
        let Some(entry) = pm.doc.presets.get_mut(preset) else {
            return;
        };
        match choice {
            Choice::Pin(id) => {
                entry.hidden.retain(|s| *s != section);
                entry.set(section, id);
            }
            Choice::Hidden => {
                if !entry.hidden.contains(&section) {
                    entry.hidden.push(section);
                }
            }
            Choice::Unpinned => return,
        }

        pm.forget_measurements();
        let result = vault::save(&pm.doc, &pm.path, pm.on_disk);
        let (path, seen) = (pm.path.clone(), pm.on_disk);
        pm.on_disk = save_status::record_document(cx, &path, seen, result);
        cx.notify();
        self.measure_matrix_pages(cx);
    }

    /// Measure how many pages each column lays out into.
    ///
    /// The column headers are the reason the matrix is worth opening — a grid
    /// that only names variants tells you what you pinned, not what it costs.
    /// So every reading gets measured, including the ones the person has never
    /// opened, which is the whole difficulty: the editor only ever compiles
    /// the document it is showing.
    ///
    /// Sources are generated here (pure string work over a cloned document)
    /// and laid out on the background executor, through the same shared engine
    /// the snapshot compiler uses so the fonts are loaded once. Nothing is
    /// rasterized: `TypstEngine::measure` stops after layout, which is the part
    /// that answers the question.
    pub(crate) fn measure_matrix_pages(&mut self, cx: &mut Context<Self>) {
        let Screen::PresetMatrix(ref mut pm) = self.screen else {
            return;
        };
        if pm.measuring {
            return;
        }
        let jobs: Vec<(Option<usize>, String)> = pm
            .columns()
            .iter()
            .map(|column| column.preset)
            .filter(|key| !pm.pages.contains_key(key))
            .map(|key| (key, pm.source_for(key)))
            .collect();
        if jobs.is_empty() {
            return;
        }
        pm.measuring = true;

        let engine = self
            .thumb_engine
            .get_or_insert_with(|| Arc::new(Mutex::new(TypstEngine::new(String::new()))))
            .clone();
        let executor = cx.background_executor().clone();

        cx.spawn(async move |this, cx| {
            let measured = executor
                .spawn(async move {
                    let mut engine = match engine.lock() {
                        Ok(engine) => engine,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let mut out = Vec::new();
                    for (key, source) in jobs {
                        engine.set_source(source);
                        // A reading that will not compile gets no page line
                        // rather than a zero. The editor is where a broken
                        // document is reported; a column header inventing
                        // "0 pages" would be the matrix lying about it.
                        if let Ok(geometry) = engine.measure() {
                            out.push((key, geometry));
                        }
                    }
                    out
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                let Screen::PresetMatrix(ref mut pm) = this.screen else {
                    return;
                };
                pm.measuring = false;
                for (key, geometry) in measured {
                    pm.pages.insert(key, geometry);
                }
                cx.notify();
                // A pin moved while this pass was in flight: its own trigger
                // found `measuring` set and did nothing, so the work it asked
                // for would otherwise never happen. Asking again costs nothing
                // when everything is already measured.
                this.measure_matrix_pages(cx);
            });
        })
        .detach();
    }

    /// Read what the board says about each of this document's readings.
    ///
    /// Once, when the screen opens: the applications board cannot change while
    /// the matrix is in front of it, and doing this per frame would re-read the
    /// vault cache for every row of the grid.
    pub(crate) fn load_matrix_records(&mut self) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let Screen::PresetMatrix(ref mut pm) = self.screen else {
            return;
        };
        let Some(stem) = pm
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
        else {
            return;
        };
        let applications = vault::load_applications(&vault);
        pm.records = pm
            .doc
            .presets
            .iter()
            .enumerate()
            .map(|(index, preset)| (index, applications.record_for(&stem, &preset.name)))
            .collect();
    }

    /// Select the working copy's profile, or the profile one preset applies.
    /// Existing profiles are references; this writes only the name into the
    /// document and never duplicates `LayoutSettings` into a matrix cell.
    pub(crate) fn set_matrix_profile(
        &mut self,
        preset: Option<usize>,
        profile: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Screen::PresetMatrix(ref mut pm) = self.screen else {
            return;
        };
        let slot = match preset {
            None => &mut pm.doc.layout_profile,
            Some(index) => {
                let Some(preset) = pm.doc.presets.get_mut(index) else {
                    return;
                };
                &mut preset.profile
            }
        };
        if *slot == profile {
            return;
        }
        *slot = profile;

        let result = vault::save(&pm.doc, &pm.path, pm.on_disk);
        let (path, seen) = (pm.path.clone(), pm.on_disk);
        pm.on_disk = save_status::record_document(cx, &path, seen, result);
        cx.notify();
    }


    /// Show every section, or only the ones some preset disagrees about.
    ///
    /// View state, deliberately: it is about looking rather than about the
    /// document, so it does not belong in the vault, and it is cheap enough to
    /// re-decide on every visit that it does not belong in `config.toml`
    /// either (the three-homes table).
    pub(crate) fn toggle_matrix_differences_only(&mut self, cx: &mut Context<Self>) {
        if let Screen::PresetMatrix(ref mut pm) = self.screen {
            pm.differences_only = !pm.differences_only;
            cx.notify();
        }
    }
}

//! Presets, from the editor's side: the toolbar selector and every action
//! behind it.
//!
//! Split out of `root.rs` rather than added to it. The preset methods are one
//! subject — which preset the working copy reads as, and the four things a
//! person can do about that — and `root.rs` is one of the two files CLAUDE.md
//! names as already over the size limit and not to be grown.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, FontWeight, IntoElement, Window};

use dockcv_ui_components::{Button, ButtonExt, DropdownMenu, PopupMenu, PopupMenuItem, SANS};

use crate::resume::model::DocumentLanguage;
use crate::theme::ActiveTheme;

use super::root::{NextPreset, EDITOR_CONTEXT};
use super::{EditorEvent, Root};

impl Root {
    /// The first preset that exactly describes the working copy.
    ///
    /// This is derived every time it is needed. Keeping the last clicked index
    /// here used to let the toolbar claim a preset was active after the person
    /// had already changed a section away from it.
    pub(super) fn active_preset(&self) -> Option<usize> {
        self.doc.active_preset_index()
    }

    /// The preset the working copy is closest to, with document order as the
    /// tie-break. Used only while no preset is an exact match: it gives
    /// `Update` and `Revert` an honest, deterministic target.
    pub(super) fn nearest_preset(&self) -> Option<usize> {
        self.doc.nearest_preset_index()
    }

    /// Switch every section to the variants recorded in preset `index`.
    pub(super) fn apply_preset(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.doc.presets.len() || self.doc.is_preset_active(index) {
            return;
        }
        self.checkpoint();
        self.doc.apply_preset(index);
        self.profile_detachment = None;
        self.profile_fork = None;
        self.reset_layout_sliders();
        self.schedule_save(cx);
        self.fields_stale = true;
        cx.notify();
        self.schedule_recompile(window, cx);
    }

    /// Show this document at preset `index`, on arrival from the gallery.
    ///
    /// The same edit as picking the preset from the toolbar menu — applying a
    /// preset moves `active` on every section it names, and that is stored, so
    /// there is no honest way for this gesture to mean something weaker. Two
    /// paths to one state that differ in whether they persist is the confusing
    /// thing, not the writing.
    ///
    /// Except when the document is already in that state, which is the common
    /// case for a card whose chip you clicked because it is what you want. Then
    /// this only names the preset in the toolbar: no checkpoint, no save, no
    /// recompile of a page that has not changed.
    pub(super) fn open_at_preset(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.doc.presets.len() {
            return;
        }
        self.apply_preset(index, window, cx);
    }

    /// Replace a preset's pins with the working copy. The checkpoint is the
    /// old preset, so document Undo restores it and immediately puts the
    /// toolbar back into `EDITED` without any separate view-state repair.
    pub(super) fn update_preset(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.doc.presets.len() || self.doc.is_preset_active(index) {
            return;
        }
        self.checkpoint();
        if !self.doc.update_preset(index) {
            return;
        }
        self.schedule_save(cx);
        cx.notify();
    }

    /// Save the document's current section×variant selection as a new named
    /// preset (`ResumeDoc::add_preset`) and make it the toolbar's displayed
    /// preset. Not "capture" — see `ResumeDoc::add_preset`'s own doc comment
    /// for why that word is reserved for the Diary's quick-capture (D-7).
    pub(super) fn save_current_as_preset(&mut self, cx: &mut Context<Self>) {
        self.checkpoint();
        // "Version", not "Preset": every surface a person reads says version,
        // and a placeholder that uses the internal word is a title they then
        // have to translate. Vaults written before this keep their `Preset N`
        // — `front_door::is_generated_name` recognises both and rewrites
        // neither, because a stored name is the key the applications board and
        // the export filename refer to.
        let n = self.doc.presets.len() + 1;
        self.doc.add_preset(format!("Version {n}"));
        self.schedule_save(cx);
        self.fields_stale = true;
        cx.notify();
    }

    /// Delete the preset currently shown in the toolbar. The old preset bar
    /// exposed this per-chip (✕); the merged toolbar (design doc §3) has no
    /// room to draw it per-preset, so it moves into the menu as a single
    /// action on whichever preset is selected.
    pub(super) fn remove_active_preset(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.active_preset() else {
            return;
        };
        self.checkpoint();
        self.doc.remove_preset(index);
        self.schedule_save(cx);
        self.fields_stale = true;
        cx.notify();
    }

    /// Change the language of the working reading.
    ///
    /// Like changing a heading or moving a section, this intentionally makes
    /// an active preset `EDITED`; `Update` or `Save as preset` is the explicit
    /// gesture that pins the new language to a named reading.
    pub(super) fn set_document_language(
        &mut self,
        language: DocumentLanguage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.lang.as_deref() == language.stored() {
            return;
        }
        self.checkpoint();
        if !self.doc.set_language(language) {
            return;
        }
        self.schedule_save(cx);
        cx.notify();
        self.schedule_recompile(window, cx);
    }

    /// Derive the label from the working copy. An exact match shows the preset
    /// name; otherwise the nearest reading is named and marked `EDITED`, with
    /// explicit Update and Revert actions.
    pub(super) fn render_preset_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let active_preset = self.active_preset();
        let nearest_preset = active_preset.or_else(|| self.nearest_preset());
        let edited = active_preset.is_none() && nearest_preset.is_some();
        let language = self.doc.language();
        let value = nearest_preset
            .and_then(|i| self.doc.preset_name(i))
            .map(|name| {
                if edited {
                    format!("{name} · EDITED")
                } else {
                    name.clone()
                }
            })
            .unwrap_or_else(|| "No preset".to_string());
        let presets: Vec<String> = self.doc.presets.iter().map(|p| p.name.clone()).collect();
        let preset_languages: Vec<DocumentLanguage> = (0..self.doc.presets.len())
            .filter_map(|index| self.doc.language_for_preset(index))
            .collect();
        let root = cx.weak_entity();

        Button::new("preset-control")
            .selector()
            // P-17: cycling presets from the keyboard has no other visible
            // control to hang a hint on but this one; `PrevPreset`'s chord
            // (Alt+Shift+Up) is the mirror of the one shown here and is named
            // in the text since a tooltip only resolves one action's binding.
            .tooltip_with_action(
                "Cycle to the next preset (Alt+Shift+Up for the previous)",
                &NextPreset,
                Some(EDITOR_CONTEXT),
            )
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(10.5))
                    .text_color(theme.text_subtle)
                    .child("PRESET"),
            )
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(format!("{value} · {}", language.badge())),
            )
            .dropdown_menu(move |menu, window, cx| {
                let mut menu = menu;
                if presets.is_empty() {
                    menu = menu.item(PopupMenuItem::label("No presets yet"));
                } else {
                    for (i, name) in presets.iter().enumerate() {
                        let root = root.clone();
                        let language = preset_languages.get(i).copied().unwrap_or_default();
                        menu = menu.item(
                            PopupMenuItem::new(format!("{name} · {}", language.badge()))
                                .checked(active_preset == Some(i))
                                .on_click(move |_ev, window, cx| {
                                    let _ = root.update(cx, |this, cx| {
                                        this.apply_preset(i, window, cx);
                                    });
                                }),
                        );
                    }
                }
                if edited {
                    if let Some(index) = nearest_preset {
                        if let Some(name) = presets.get(index).cloned() {
                            let update_root = root.clone();
                            menu = menu.separator().item(
                                PopupMenuItem::new(format!("Update {name}")).on_click(
                                    move |_ev, _window, cx| {
                                        let _ = update_root.update(cx, |this, cx| {
                                            this.update_preset(index, cx);
                                        });
                                    },
                                ),
                            );
                            let revert_root = root.clone();
                            menu = menu.item(
                                PopupMenuItem::new(format!("Revert to {name}")).on_click(
                                    move |_ev, window, cx| {
                                        let _ = revert_root.update(cx, |this, cx| {
                                            this.apply_preset(index, window, cx);
                                        });
                                    },
                                ),
                            );
                        }
                    }
                }
                let language_root = root.clone();
                let language_menu =
                    PopupMenu::build(window, cx, move |mut submenu, _window, _cx| {
                        for option in DocumentLanguage::ALL {
                            let root = language_root.clone();
                            submenu = submenu.item(
                                PopupMenuItem::new(option.label())
                                    .checked(option == language)
                                    .on_click(move |_ev, window, cx| {
                                        let _ = root.update(cx, |this, cx| {
                                            this.set_document_language(option, window, cx);
                                        });
                                    }),
                            );
                        }
                        submenu
                    });
                menu = menu.separator().item(PopupMenuItem::submenu(
                    format!("Language · {}", language.badge()),
                    language_menu,
                ));
                if active_preset.is_some() {
                    menu = menu.item(PopupMenuItem::new("Remove current preset").on_click({
                        let root = root.clone();
                        move |_ev, _window, cx| {
                            let _ = root.update(cx, |this, cx| {
                                this.remove_active_preset(cx);
                            });
                        }
                    }));
                }
                menu = menu.separator();
                menu = menu.item(PopupMenuItem::new("＋ Save as preset").on_click({
                    let root = root.clone();
                    move |_ev, _window, cx| {
                        let _ = root.update(cx, |this, cx| {
                            this.save_current_as_preset(cx);
                        });
                    }
                }));
                menu = menu.separator();
                menu.item(PopupMenuItem::new("Preset Matrix…").on_click({
                    let root = root.clone();
                    move |_ev, _window, cx| {
                        let _ = root.update(cx, |_this, cx| {
                            cx.emit(EditorEvent::OpenPresetMatrix);
                        });
                    }
                }))
            })
            .into_any_element()
    }
}

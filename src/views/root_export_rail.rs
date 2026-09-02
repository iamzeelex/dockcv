//! The layout rail's Export group: how this document names its files, and the
//! last few it actually wrote.
//!
//! Split out of `root_layout_rows.rs` because it is a different subject — every
//! other row in that file changes what the page *looks* like, and none of them
//! touches the filesystem.

use gpui::prelude::*;
use gpui::{div, px, Context, IntoElement, SharedString};

use dockcv_ui_components::{Button, ButtonExt, DropdownMenu, PopupMenuItem};

use crate::resume::model::ExportSettings;
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::root::Root;

impl Root {
    /// The filename pattern, with a live example of what it names *this*
    /// document — the date picker's argument: a worked example beats a pattern.
    pub(super) fn filename_pattern_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let root = cx.weak_entity();
        let preset_name = self
            .active_preset
            .and_then(|idx| self.doc.presets.get(idx))
            .map(|p| p.name.as_str());
        let example_name = format!(
            "{}.pdf",
            self.doc
                .export_filename_stem(preset_name, None, &super::root_export_sheet::today())
        );

        let current_pattern = &self.doc.export.filename_pattern;
        let active_label = ExportSettings::PRESETS
            .iter()
            .find(|(_, pat)| *pat == current_pattern.as_str())
            .map(|(lbl, _)| *lbl)
            .unwrap_or("Custom pattern");

        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(self.rail_label(cx, "Filename pattern"))
            .child(
                Button::new("layout-filename-pattern")
                    .selector_inline()
                    .w_full()
                    .label(active_label)
                    .tooltip("Pattern used for naming exported files.")
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        for (label, pattern) in ExportSettings::PRESETS {
                            let root = root.clone();
                            let pattern_str = pattern.to_string();
                            menu = menu.item(PopupMenuItem::new(*label).on_click(
                                move |_ev, window, cx| {
                                    let pattern_to_set = pattern_str.clone();
                                    let _ = root.update(cx, |this, cx| {
                                        this.doc.export.filename_pattern = pattern_to_set;
                                        this.after_layout_change(window, cx);
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().text_muted)
                    .child(format!("Example: {example_name}")),
            )
    }

    /// The last few exports of this document, newest first (A5).
    ///
    /// Only rendered when the Export group is open, because the "has this file
    /// moved?" mark is a `stat` per row and the rail redraws every frame.
    pub(super) fn export_history_rows(
        &self,
        cx: &mut Context<Self>,
        open: bool,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        if !open {
            return div().into_any_element();
        }
        if self.doc.export_history.is_empty() {
            return div()
                .text_style(TextStyle::meta())
                .text_color(theme.text_muted)
                .child("No exports recorded yet.")
                .into_any_element();
        }

        let mut list = div().flex().flex_col().gap(px(6.0));

        for (row, record) in self.doc.export_history.iter().rev().take(5).enumerate() {
            // A batch export writes one row per preset within the same minute,
            // so the timestamp is not unique enough to be an element id.
            let exists = record.path.exists();
            let file_name = record
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("document");
            let path = record.path.clone();

            list = list.child(
                div()
                    .p_2()
                    .rounded(theme.radius_sm())
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_style(TextStyle::chip())
                                    .text_color(theme.accent)
                                    .child(format!(
                                        "{} · {}",
                                        super::root_export_sheet::format_label(&record.format),
                                        record.preset
                                    )),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_muted)
                                    .child(format!("{} {}", record.date, record.time)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_style(TextStyle::code())
                                    .text_color(if exists { theme.text } else { theme.text_muted })
                                    .child(file_name.to_string()),
                            )
                            .child(if exists {
                                Button::new(SharedString::from(format!("export-reveal-{row}")))
                                    .quiet()
                                    .text_xs()
                                    .label("Reveal")
                                    .on_click(move |_ev, _window, cx: &mut gpui::App| {
                                        cx.reveal_path(&path);
                                    })
                                    .into_any_element()
                            } else {
                                div()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.danger)
                                    .child("(moved or deleted)")
                                    .into_any_element()
                            }),
                    ),
            );
        }

        list.into_any_element()
    }
}

//! Drawing the Preset Matrix: the chrome, the control bar and the grid.
//!
//! Split from `preset_matrix.rs` because C7 turned two fixed columns into as
//! many as the document has, and the table grew with it — `root.rs` and
//! `model.rs` are the standing lesson about letting one file absorb every
//! addition.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, Div, FontWeight, SharedString};

use dockcv_ui_components::{
    Button, ButtonExt, DockIcon, DropdownMenu, IconName, PopupMenuItem,
    ScrollableElement, Sizable, TextField, CHROME_HEIGHT, MONO, SANS,
};

use crate::resume::model::SectionKind;
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::preset_matrix::{Choice, Column, PresetMatrix};
use super::shell::Shell;

/// The section-label column. Fixed, because it is the one column whose content
/// does not vary with how many presets exist.
const LABEL_WIDTH: f32 = 170.0;
/// What a preset column will shrink to before the grid starts scrolling
/// sideways. Past four or five columns a comparison table needs the scrollbar
/// rather than columns too narrow to read (the design doc's §2).
const COLUMN_MIN_WIDTH: f32 = 168.0;

impl PresetMatrix {
    pub fn render_matrix(&self, cx: &mut Context<Shell>) -> Div {
        let theme = *cx.theme();
        let columns = self.columns();
        let rows = self.rows();

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(self.render_chrome(cx))
            .child(
                div()
                    .id("preset-matrix-grid")
                    .flex_1()
                    .p(px(26.0))
                    .overflow_scrollbar()
                    .flex()
                    .flex_col()
                    .child(self.render_control_bar(cx, &rows))
                    .child(self.render_table(cx, &columns, &rows)),
            )
    }

    /// Breadcrumb out, and the two actions that act on the whole document.
    fn render_chrome(&self, cx: &mut Context<Shell>) -> Div {
        let theme = *cx.theme();

        div()
            .flex()
            .items_center()
            .justify_between()
            .h(CHROME_HEIGHT)
            .pl(px(80.0))
            .pr(px(20.0))
            .bg(theme.chrome)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .child(
                        Button::new("matrix-back")
                            .quiet()
                            .gap(px(4.0))
                            .icon(IconName::ChevronLeft)
                            .tooltip("Back to the document")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.leave_preset_matrix(cx);
                            }))
                            .child(self.identity()),
                    )
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(13.5))
                            .text_color(theme.border_strong)
                            .child("/"),
                    )
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child("Presets"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Button::new("export-all-presets-btn")
                            .toolbar()
                            .icon(IconName::ArrowDown)
                            .tooltip("Export every preset to PDF into a folder")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.export_all_matrix_presets(cx);
                            }))
                            .child("Export all presets"),
                    )
                    .child(
                        Button::new("save-new-preset-btn")
                            .quiet()
                            .text_color(theme.accent)
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.save_matrix_as_preset(cx);
                            }))
                            .icon(IconName::Plus)
                            .child("Save current as new preset"),
                    ),
            )
    }

    /// The `differences only` toggle and the legend.
    ///
    /// Where the `COMPARING A vs B` bar used to be. Its `+ compare a third`
    /// prompt was drawn in the mockup, never built, and is not missed: every
    /// preset is a column now, so there is no third to add.
    fn render_control_bar(&self, cx: &mut Context<Shell>, rows: &[SectionKind]) -> Div {
        let theme = *cx.theme();
        let hidden_rows = self.doc.sections().len().saturating_sub(rows.len());
        let differences_only = self.differences_only;

        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .mb(px(22.0))
            .child(
                Button::new("matrix-differences-only")
                    .chip(differences_only, &theme)
                    .tooltip(if differences_only {
                        "Show every section"
                    } else {
                        "Hide the sections every preset agrees on"
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.toggle_matrix_differences_only(cx);
                    }))
                    .child("Differences only"),
            )
            .when(differences_only && hidden_rows > 0, |bar| {
                bar.child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.5))
                        .text_color(theme.text_subtle)
                        .child(format!("{hidden_rows} agreeing")),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .font_family(SANS)
                    .text_size(px(12.0))
                    .text_color(theme.text_subtle)
                    .child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(px(2.0))
                            .bg(theme.warning),
                    )
                    .child("Differs from the working copy"),
            )
    }

    fn render_table(
        &self,
        cx: &mut Context<Shell>,
        columns: &[Column],
        rows: &[SectionKind],
    ) -> Div {
        let theme = *cx.theme();

        let mut table_rows = vec![self.render_header_row(cx, columns)];
        for section in rows {
            table_rows.push(self.render_row(cx, columns, *section));
        }
        if rows.is_empty() {
            table_rows.push(
                div()
                    .w_full()
                    .bg(theme.elevated)
                    .px(px(16.0))
                    .py(px(18.0))
                    .text_style(TextStyle::body())
                    .text_color(theme.text_muted)
                    .child(if self.differences_only {
                        "Every preset agrees on every section."
                    } else {
                        "This document has no sections."
                    }),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(px(1.0))
            .bg(theme.border)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md())
            .overflow_hidden()
            .children(table_rows)
    }

    /// `SECTION | Now | <preset> | …`, each preset header carrying its rename
    /// pen and whichever mark the working copy earns it.
    fn render_header_row(&self, cx: &mut Context<Shell>, columns: &[Column]) -> Div {
        let theme = *cx.theme();

        let mut row = div().flex().w_full().gap(px(1.0)).child(
            div()
                .w(px(LABEL_WIDTH))
                .flex_none()
                .bg(theme.surface)
                .px(px(16.0))
                .py(px(12.0))
                .font_family(MONO)
                .text_size(px(11.0))
                .text_color(theme.text_subtle)
                .child("SECTION"),
        );

        for column in columns {
            row = row.child(self.render_header_cell(cx, column));
        }
        row
    }

    fn render_header_cell(&self, cx: &mut Context<Shell>, column: &Column) -> Div {
        let theme = *cx.theme();
        let working_copy = column.preset.is_none();
        let renaming = column
            .preset
            .and_then(|index| self.renaming_preset.as_ref().filter(|r| r.idx == index));

        let name: AnyElement = match renaming {
            Some(rename) => div()
                .w(px(150.0))
                .child(TextField::new(&rename.field).small())
                .into_any_element(),
            None => div()
                .font_family(SANS)
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if working_copy {
                    theme.text_muted
                } else {
                    theme.text
                })
                .child(column.name.clone())
                .into_any_element(),
        };

        // The two lines that make a column worth reading: what this reading
        // costs in paper, and how it has actually done. Both are blank until
        // they are known — a header that guessed would be worse than one that
        // waits (C11).
        let evidence = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .when_some(self.pages_line(column.preset), |line, text| {
                line.child(
                    div()
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .text_color(if self.overflows(column.preset) {
                            theme.warning
                        } else {
                            theme.text_subtle
                        })
                        .child(text),
                )
            })
            .when_some(self.record_line(column.preset), |line, text| {
                line.child(
                    div()
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .text_color(theme.text_subtle)
                        .child(text),
                )
            });

        div()
            .flex_1()
            .min_w(px(COLUMN_MIN_WIDTH))
            .bg(theme.surface)
            .px(px(16.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(name)
                    .when_some(column.mark, |header, mark| {
                        header.child(
                            div()
                                .font_family(MONO)
                                .text_size(px(9.5))
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(theme.radius_sm())
                                .bg(if mark == "ACTIVE" {
                                    theme.success.opacity(0.18)
                                } else {
                                    theme.warning.opacity(0.18)
                                })
                                .text_color(if mark == "ACTIVE" {
                                    theme.success
                                } else {
                                    theme.warning
                                })
                                .child(mark),
                        )
                    })
                    .when_some(column.preset, |header, index| {
                        header.child(
                            Button::new(SharedString::from(format!("preset-rename-{index}")))
                                .icon_only()
                                .icon(DockIcon::Pen)
                                .tooltip("Rename this preset")
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    this.start_preset_rename(index, window, cx);
                                })),
                        )
                    })
                    .when(working_copy, |header| {
                        header.child(
                            div()
                                .font_family(MONO)
                                .text_size(px(9.5))
                                .text_color(theme.text_subtle)
                                .child("WORKING COPY"),
                        )
                    }),
            )
            .child(evidence)
    }

    fn render_row(
        &self,
        cx: &mut Context<Shell>,
        columns: &[Column],
        section: SectionKind,
    ) -> Div {
        let theme = *cx.theme();
        let differs = self.row_differs(section);
        let reference = self.working_copy_choice(section);

        let mut row = div().flex().w_full().gap(px(1.0)).child(
            div()
                .w(px(LABEL_WIDTH))
                .flex_none()
                .bg(theme.surface)
                .px(px(16.0))
                .py(px(15.0))
                .font_family(SANS)
                .text_size(px(13.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(self.doc.section_title(section)),
        );

        for column in columns {
            let choice = self.choice(column, section);
            // Amber marks a cell that departs from the working copy, not one
            // that differs from its neighbour. With more than two columns
            // "differs" needs a reference, and the working copy is the one the
            // person is standing on.
            let marked = differs && choice != reference && column.preset.is_some();
            row = row.child(self.render_cell(cx, column, section, choice, marked));
        }
        row
    }

    fn render_cell(
        &self,
        cx: &mut Context<Shell>,
        column: &Column,
        section: SectionKind,
        choice: Choice,
        marked: bool,
    ) -> AnyElement {
        let theme = *cx.theme();
        let text = self.cell_text(section, choice);
        let detail = self.cell_detail(section, choice);
        let deleted = matches!(choice, Choice::Pin(id) if self.variant_label(section, id).is_none());

        let body = div()
            .flex()
            .items_baseline()
            .gap(px(6.0))
            .child(
                div()
                    .text_color(if deleted {
                        theme.danger
                    } else if marked {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .child(text),
            )
            .when_some(detail, |cell, d| {
                cell.child(
                    div()
                        .text_size(px(11.5))
                        .text_color(if marked {
                            theme.warning
                        } else {
                            theme.text_subtle
                        })
                        .child(d),
                )
            });

        let cell = div()
            .flex_1()
            .min_w(px(COLUMN_MIN_WIDTH))
            .px(px(16.0))
            .py(px(15.0))
            .text_style(TextStyle::body())
            .when(marked, |c| {
                c.bg(theme.hover)
                    .border_l_2()
                    .border_color(theme.warning)
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!marked, |c| c.bg(theme.elevated));

        let Some(index) = column.preset else {
            // The working copy is read-only here. It is what the document says,
            // and the document is edited in the editor — two places to change
            // one thing is how they come to disagree.
            return cell.child(body).into_any_element();
        };

        cell.child(self.render_cell_menu(cx, index, section, choice, body))
            .into_any_element()
    }

    /// A cell is chosen from a named menu, not cycled.
    ///
    /// Cycling was fine for two variants and a guessing game at five, and it
    /// put `Hidden` at the end of a loop where nothing announced it. The menu
    /// is the control the editor already uses for section variants, so a
    /// person meets one gesture rather than two.
    fn render_cell_menu(
        &self,
        cx: &mut Context<Shell>,
        preset: usize,
        section: SectionKind,
        choice: Choice,
        body: Div,
    ) -> AnyElement {
        let names = self.doc.variant_names(section);
        let ids = self.doc.variant_ids(section);
        // Profile is never hideable — a CV without a name is not a CV (O-13).
        let hideable = section != SectionKind::Profile;
        let shell = cx.weak_entity();

        Button::new(SharedString::from(format!("cell-{preset}-{section:?}")))
            .quiet()
            .w_full()
            .justify_start()
            .tooltip("Choose what this preset reads here")
            .child(body)
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu;
                for (i, name) in names.iter().enumerate() {
                    let Some(id) = ids.get(i).copied() else {
                        continue;
                    };
                    let shell = shell.clone();
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(choice == Choice::Pin(id))
                            .on_click(move |_ev, _window, cx| {
                                let _ = shell.update(cx, |this, cx| {
                                    this.set_matrix_cell(preset, section, Choice::Pin(id), cx);
                                });
                            }),
                    );
                }
                if hideable {
                    let shell = shell.clone();
                    menu = menu.separator().item(
                        PopupMenuItem::new("Hidden")
                            .checked(choice == Choice::Hidden)
                            .on_click(move |_ev, _window, cx| {
                                let _ = shell.update(cx, |this, cx| {
                                    this.set_matrix_cell(preset, section, Choice::Hidden, cx);
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }
}

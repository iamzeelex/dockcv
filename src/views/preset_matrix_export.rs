//! Exporting every preset at once, with the list of files on screen first.
//!
//! One gesture writing N files is only a good gesture if the user can see what
//! N is and what the files are called. A folder picker on its own hides both,
//! and the moment it hides them is the moment a name collides — which is how a
//! folder of `CV-final-2.pdf` gets made, by us, in the feature meant to abolish
//! it.
//!
//! So the folder is chosen first (a collision is a fact about a folder), the
//! sheet then names every file and marks the ones already taken, and nothing is
//! written until the user confirms. Cancelling writes nothing at all.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement, SharedString};

use dockcv_ui_components::{Button, ButtonExt, Disableable, IconName, ScrollableElement};

use crate::resume::export_names::{plan_batch, OnCollision, PlannedExport};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::shell::{Screen, Shell};

/// The batch export sheet, open over the Preset Matrix.
pub struct BatchExportSheet {
    /// Where the files go. Chosen before the sheet opens, because which names
    /// collide is a question about a folder.
    pub folder: std::path::PathBuf,
    /// What the user picked for names that are already taken.
    pub on_collision: OnCollision,
    /// The plan under that choice — recomputed when the choice changes, since
    /// the targets differ between the two.
    pub plan: Vec<PlannedExport>,
    /// True from the moment the user confirms until the writes finish, so the
    /// sheet cannot be confirmed twice.
    pub writing: bool,
}

impl BatchExportSheet {
    /// How many rows would land on a name that already exists.
    pub fn collisions(&self) -> usize {
        self.plan.iter().filter(|p| p.destination.collides).count()
    }
}

impl Shell {
    /// Close the sheet, writing nothing.
    pub(super) fn cancel_batch_export(&mut self, cx: &mut Context<Self>) {
        self.batch_export = None;
        cx.notify();
    }

    /// Answer collisions differently, and re-plan: `Keep both` and `Replace`
    /// aim at different files, so the list has to be recomputed rather than
    /// relabelled.
    pub(super) fn set_batch_collision_policy(
        &mut self,
        policy: OnCollision,
        cx: &mut Context<Self>,
    ) {
        let Screen::PresetMatrix(pm) = &self.screen else {
            return;
        };
        let Some(sheet) = &self.batch_export else {
            return;
        };
        let plan = plan_batch(
            &pm.doc,
            &sheet.folder,
            "pdf",
            &super::root_export_sheet::today(),
            policy,
        );
        if let Some(sheet) = &mut self.batch_export {
            sheet.on_collision = policy;
            sheet.plan = plan;
        }
        cx.notify();
    }

    /// The sheet: what is about to be written, before it is.
    pub(super) fn render_batch_export_sheet(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let Some(sheet) = &self.batch_export else {
            return div().into_any_element();
        };

        let collisions = sheet.collisions();
        let count = sheet.plan.len();

        let rows = div()
            .id("batch-export-rows")
            .flex()
            .flex_col()
            .gap(px(2.0))
            .max_h(px(280.0))
            .overflow_y_scrollbar()
            .children(sheet.plan.iter().map(|step| {
                let name = step
                    .destination
                    .target
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .px_2()
                    .py_1p5()
                    .rounded(theme.radius_sm())
                    .bg(if step.destination.collides {
                        theme.selected
                    } else {
                        theme.surface
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .min_w_0()
                            .child(
                                div()
                                    .text_style(TextStyle::code())
                                    .text_color(theme.text)
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_muted)
                                    .child(step.preset.clone()),
                            ),
                    )
                    .child(if step.destination.overwrites() {
                        div()
                            .text_style(TextStyle::chip())
                            .text_color(theme.danger)
                            .child("replaces")
                            .into_any_element()
                    } else if step.destination.collides {
                        div()
                            .text_style(TextStyle::chip())
                            .text_color(theme.warning)
                            .child("renamed")
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    })
            }));

        // Only asked when it means something. A folder with no clashes in it
        // gets no question, because there is nothing to answer.
        let collision_choice = (collisions > 0).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_style(TextStyle::body())
                        .text_color(theme.text_subtle)
                        .child(format!(
                            "{collisions} of these {} already exist in that folder.",
                            if count == 1 { "name" } else { "names" }
                        )),
                )
                .child(div().flex().gap(px(6.0)).children(
                    [OnCollision::KeepBoth, OnCollision::Replace].map(|policy| {
                        let (id, label) = match policy {
                            OnCollision::KeepBoth => ("batch-keep-both", "Keep both"),
                            OnCollision::Replace => ("batch-replace", "Replace"),
                        };
                        Button::new(SharedString::from(id))
                            .chip(sheet.on_collision == policy, &theme)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.set_batch_collision_policy(policy, cx);
                            }))
                            .child(label)
                    }),
                ))
        });

        let panel = div()
            .w(px(520.0))
            .flex()
            .flex_col()
            .rounded(theme.radius_lg())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_5()
                    .py_3p5()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_style(TextStyle::heading())
                            .text_color(theme.text)
                            .child("Export every preset"),
                    )
                    .child(
                        Button::new("batch-export-close")
                            .icon_only()
                            .icon(IconName::Close)
                            .tooltip("Close")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.cancel_batch_export(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .p_5()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_style(TextStyle::eyebrow())
                                    .text_color(theme.text_muted)
                                    .child("Into"),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::code())
                                    .text_color(theme.text)
                                    .child(sheet.folder.display().to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_style(TextStyle::eyebrow())
                                    .text_color(theme.text_muted)
                                    .child(format!(
                                        "{count} {}",
                                        if count == 1 { "file" } else { "files" }
                                    )),
                            )
                            .child(rows),
                    )
                    .children(collision_choice)
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .pt_1()
                            .child(
                                Button::new("batch-export-cancel")
                                    .toolbar()
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.cancel_batch_export(cx);
                                    })),
                            )
                            .child(
                                Button::new("batch-export-confirm")
                                    .toolbar_primary()
                                    .disabled(sheet.writing)
                                    .label(if sheet.writing {
                                        "Exporting…".to_string()
                                    } else {
                                        format!(
                                            "Export {count} {}",
                                            if count == 1 { "file" } else { "files" }
                                        )
                                    })
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.run_batch_export(cx);
                                    })),
                            ),
                    ),
            );

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.scrim)
            .child(panel)
            .into_any_element()
    }
}

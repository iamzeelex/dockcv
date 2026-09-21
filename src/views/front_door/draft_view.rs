//! Drawing the version constructor. Its state and rules are in
//! `front_door/draft.rs`; what a change *is* is in `front_door/changes.rs`.
//!
//! Two columns, and the split is the argument. On the left the page as it will
//! print, compiled from the working copy — the thing being made. On the right
//! the reasoning: why this starting point, and the short list of named changes
//! that turn it into the version for this job.
//!
//! The prototype drew a highlight over each changed block on the page. We
//! cannot: Typst gives us a rasterized page and `PageGeometry`, and no
//! per-section frame rectangles (O-12/E-15), so there is no honest y to draw a
//! band at. Inventing one would be inventing a measurement, which is the one
//! thing this product does not do. The page is therefore the page, and the
//! connection between a change and its effect is made the other way round: a
//! change names the section it touches, and toggling it redraws the page.

use gpui::prelude::*;
use gpui::{div, img, px, AnyElement, ClickEvent, Context, Div, FontWeight, SharedString};

use dockcv_ui_components::{
    Button, ButtonExt, DropdownMenu, PopupMenuItem, ScrollableElement, MONO, SANS,
};

use crate::theme::{ActiveTheme, StyledText, TextStyle};

use crate::views::shell::Shell;
use super::changes::{Change, ChangeKind};
use super::draft::{section_label, selected};

impl Shell {
    /// The whole screen.
    pub(crate) fn render_version_draft(&self, cx: &mut Context<Self>) -> Div {
        let Some(draft) = self.drafting.as_ref() else {
            return div();
        };
        let changes = draft.changes();
        let count = selected(&changes);

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(self.render_draft_heading(cx))
            .child(
                div()
                    .id("draft-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .px(px(34.0))
                    .pb(px(30.0))
                    .flex()
                    .flex_col()
                    .gap(px(18.0))
                    .child(self.render_draft_intent(cx, count))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_start()
                            .gap(px(18.0))
                            .child(self.render_draft_preview(cx))
                            .child(self.render_draft_side(cx, &changes, count)),
                    )
                    .child(self.render_draft_advanced(cx)),
            )
    }

    /// The job, not the document. This screen exists because of an application.
    fn render_draft_heading(&self, cx: &mut Context<Self>) -> Div {
        let theme = *cx.theme();
        let Some(draft) = self.drafting.as_ref() else {
            return div();
        };
        let person = self.front_door_title();
        let role = if draft.role.trim().is_empty() {
            draft.company.clone()
        } else {
            draft.role.clone()
        };

        div()
            .flex()
            .flex_wrap()
            .items_start()
            .justify_between()
            .gap_4()
            .px(px(34.0))
            .pt(px(30.0))
            .pb(px(20.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(280.0))
                    .flex()
                    .flex_col()
                    .gap(px(7.0))
                    .child(
                        div()
                            .text_style(TextStyle::eyebrow())
                            .text_color(theme.text_subtle)
                            .child(TextStyle::eyebrow().apply_case("New version for a job")),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::title())
                            .text_color(theme.text)
                            .child(role),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_muted)
                            .child(format!(
                                "{} · a focused version of {person}'s CV.",
                                draft.company
                            )),
                    ),
            )
            .child(
                Button::new("draft-back")
                    .toolbar()
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.discard_version_draft(cx);
                    }))
                    .child("‹ Back to CVs"),
            )
    }

    /// One strip answering four questions: what this is, what it came from,
    /// whether the source is at risk, and how to start from something else.
    fn render_draft_intent(&self, cx: &mut Context<Self>, count: usize) -> Div {
        let theme = *cx.theme();
        let Some(draft) = self.drafting.as_ref() else {
            return div();
        };
        let source = draft.source_name();

        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(16.0))
            .py(px(14.0))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(theme.accent.opacity(0.45))
            .bg(theme.accent.opacity(0.07))
            .child(
                div()
                    .flex_1()
                    .min_w(px(260.0))
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.0))
                                    .text_color(theme.warning)
                                    .child(TextStyle::eyebrow().apply_case(&format!(
                                        "Draft · {count} change{}",
                                        if count == 1 { "" } else { "s" }
                                    ))),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_muted)
                                    .child("Based on"),
                            )
                            .child(
                                div()
                                    .font_family(SANS)
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child(source.clone()),
                            )
                            .child(
                                // The sentence that makes the screen safe to
                                // use. Everything here is a copy; the version
                                // it came from is untouched on disk.
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_muted)
                                    .child("· source stays unchanged"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(
                                "Everything below is a copy. Save it and it becomes a version \
                                 of this CV you can send.",
                            ),
                    ),
            )
            .child(self.render_draft_source_picker(cx, &source))
    }

    fn render_draft_source_picker(&self, cx: &mut Context<Self>, source: &str) -> Div {
        let theme = *cx.theme();
        let Some(draft) = self.drafting.as_ref() else {
            return div();
        };
        let names: Vec<String> = draft.doc.presets.iter().map(|p| p.name.clone()).collect();
        let shell = cx.weak_entity();

        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_style(TextStyle::eyebrow())
                    .text_color(theme.text_subtle)
                    .child(TextStyle::eyebrow().apply_case("Start from")),
            )
            .child(
                Button::new("draft-source")
                    .selector()
                    .label(SharedString::from(source.to_string()))
                    .dropdown_menu(move |menu, _window, _cx| {
                        let mut menu = menu;
                        for (index, name) in names.iter().enumerate() {
                            let shell = shell.clone();
                            menu = menu.item(PopupMenuItem::new(name.clone()).on_click(
                                move |_ev, _window, cx| {
                                    let _ = shell
                                        .update(cx, |this, cx| this.set_draft_source(index, cx));
                                },
                            ));
                        }
                        menu
                    }),
            )
    }

    /// The page, as it will print.
    fn render_draft_preview(&self, cx: &mut Context<Self>) -> Div {
        let theme = *cx.theme();
        let Some(draft) = self.drafting.as_ref() else {
            return div();
        };
        let pages = draft.geometry.as_ref().map(|g| g.page_count.max(1));
        let fits = draft
            .geometry
            .as_ref()
            .is_some_and(|g| g.page_count <= 1 && g.overflow_pt <= 0.0);

        div()
            .flex_1()
            .min_w(px(330.0))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .px(px(14.0))
                    .py(px(12.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .font_family(MONO)
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child("Preview"),
                    )
                    .children(pages.map(|n| div().child(format!("Page 1 of {n}"))))
                    .child(div().flex_1())
                    .children(pages.map(|n| {
                        div()
                            .text_color(if fits { theme.success } else { theme.text_subtle })
                            .child(if fits {
                                "Fits on one page".to_string()
                            } else {
                                format!("{n} pages")
                            })
                    })),
            )
            .child(
                div()
                    .p(px(16.0))
                    .flex()
                    .justify_center()
                    .bg(theme.canvas)
                    .min_h(px(420.0))
                    .child(match draft.page.as_ref() {
                        Some(page) => img(page.image.clone())
                            .w(px(page.width))
                            .h(px(page.height))
                            .into_any_element(),
                        // Not a blank sheet: an empty page is a claim about the
                        // document, and the claim would be false.
                        None => div()
                            .flex()
                            .items_center()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_subtle)
                            .child("Laying the page out…")
                            .into_any_element(),
                    }),
            )
    }

    /// Why this start, and what it becomes.
    fn render_draft_side(
        &self,
        cx: &mut Context<Self>,
        changes: &[Change],
        count: usize,
    ) -> Div {
        let theme = *cx.theme();
        let Some(draft) = self.drafting.as_ref() else {
            return div();
        };

        div()
            .flex_1()
            .min_w(px(340.0))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .p(px(14.0))
            .flex()
            .flex_col()
            .children(self.draft_source_evidence().map(|(title, body)| {
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .justify_between()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .font_family(SANS)
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child("Why this start"),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.0))
                                    .text_color(theme.text_subtle)
                                    // Not "job match": nothing here has read
                                    // the job. This is the board's own record.
                                    .child("your record"),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(8.0))
                            .p(px(10.0))
                            .rounded(theme.radius_sm())
                            .bg(theme.accent.opacity(0.1))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .font_family(SANS)
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.accent)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.text_muted)
                                    .child(body),
                            ),
                    )
            }))
            .child(
                div()
                    .mt(px(14.0))
                    .pt(px(14.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .flex()
                    .items_baseline()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("Changes in this version"),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.0))
                            .text_color(theme.text_subtle)
                            .child(format!("{count} selected")),
                    ),
            )
            .child(if changes.is_empty() {
                div()
                    .mt(px(10.0))
                    .text_style(TextStyle::body())
                    .text_color(theme.text_muted)
                    .child(
                        "This CV has one cut of every section, so there is nothing to choose \
                         between yet. Write a second version of a section in the editor and it \
                         will appear here.",
                    )
                    .into_any_element()
            } else {
                div()
                    .mt(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .children(
                        changes
                            .iter()
                            .enumerate()
                            .map(|(index, change)| self.render_draft_change(cx, index, change)),
                    )
                    .into_any_element()
            })
            .child(self.render_draft_actions(cx, count, &draft.source_name()))
    }

    /// One change: what it is called, what it does, and whether it is in.
    fn render_draft_change(
        &self,
        cx: &mut Context<Self>,
        index: usize,
        change: &Change,
    ) -> AnyElement {
        let theme = *cx.theme();
        let on = change.applied;
        let owned = change.clone();
        // The section under a cut's name — "Lead with outcomes / Work
        // Experience" — but not under `Leave out Work Experience`, which has
        // already said it.
        let section = match change.kind {
            ChangeKind::Hide => String::new(),
            ChangeKind::Variant(_) => self
                .drafting
                .as_ref()
                .map(|d| section_label(&d.doc, change.section))
                .unwrap_or_default(),
        };

        div()
            .id(SharedString::from(format!("draft-change-{index}")))
            .flex()
            .items_start()
            .gap(px(10.0))
            .px(px(10.0))
            .py(px(9.0))
            .rounded(theme.radius_sm())
            .border_1()
            .cursor_pointer()
            .map(|row| {
                if on {
                    row.border_color(theme.accent.opacity(0.6))
                        .bg(theme.accent.opacity(0.08))
                } else {
                    row.border_color(theme.border).bg(theme.elevated)
                }
            })
            .hover(|s| s.border_color(theme.border_strong))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.toggle_draft_change(&owned, cx);
            }))
            .child(
                div()
                    .flex_none()
                    .mt(px(1.0))
                    .w(px(19.0))
                    .h(px(19.0))
                    .rounded_full()
                    .border_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.0))
                    .map(|check| {
                        if on {
                            check
                                .border_color(theme.accent)
                                .bg(theme.accent)
                                .text_color(theme.on_accent)
                                .child("✓")
                        } else {
                            check.border_color(theme.border_strong)
                        }
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(change.name.clone()),
                    )
                    .children(
                        // The author's own sentence when there is one, and the
                        // section's name when there is not. Never a generated
                        // description of what a variant "does".
                        Some(change.description.clone().unwrap_or(section))
                            .filter(|line| !line.is_empty())
                            .map(|line| {
                                div()
                                    .text_size(px(10.5))
                                    .line_height(px(15.0))
                                    .text_color(theme.text_muted)
                                    .child(line)
                            }),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(MONO)
                    .text_size(px(10.0))
                    .text_color(if on { theme.accent } else { theme.text_subtle })
                    .child(if on { "Included" } else { "Add" }),
            )
            .into_any_element()
    }

    fn render_draft_actions(&self, cx: &mut Context<Self>, count: usize, source: &str) -> Div {
        let theme = *cx.theme();
        div()
            .mt(px(14.0))
            .pt(px(12.0))
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(140.0))
                    .text_size(px(11.0))
                    .text_color(theme.text_subtle)
                    .child(if count == 0 {
                        format!("Nothing has changed in {source}.")
                    } else {
                        format!(
                            "{count} change{} selected. The source stays unchanged.",
                            if count == 1 { "" } else { "s" }
                        )
                    }),
            )
            .child(
                Button::new("draft-discard")
                    .quiet()
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.discard_version_draft(cx);
                    }))
                    .child("Discard draft"),
            )
            .child(
                Button::new("draft-save")
                    .action_primary()
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.save_version_draft(cx);
                    }))
                    .child("Save version"),
            )
    }

    /// The matrix, for when three named changes are not enough.
    fn render_draft_advanced(&self, cx: &mut Context<Self>) -> Div {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(240.0))
                    .text_size(px(11.5))
                    .text_color(theme.text_subtle)
                    .child(
                        "Change a specific section only when these named changes are not \
                         enough. This never makes a second copy of your CV.",
                    ),
            )
            .child(
                Button::new("draft-advanced")
                    .quiet()
                    .text_color(theme.text_muted)
                    // Named for both halves: the matrix reads a document from
                    // disk, so adjusting sections means saving this first.
                    .tooltip("Saves this version, then opens the section × version grid")
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.save_draft_and_open_matrix(cx);
                    }))
                    .child("Save and adjust sections →"),
            )
    }
}

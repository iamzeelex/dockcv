//! Gallery screen rendering for `Shell` (the gallery spec §3).
//!
//! The nav rail is shared chrome and lives in `sidebar.rs`; this file only
//! owns the main pane — header, search, doc grid, and the "new CV" flow.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement};

use super::import_flow::{self, ImportStep};
use crate::resume::model::{Resume, ResumeDoc};
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use dockcv_ui_components::{
    Button, ButtonExt, Card, EmptyState, Icon, IconName, ScrollableElement, Sizable, TextField,
};


use super::shell::Shell;

impl Shell {
    /// The gallery's main pane. The rail around it is mounted by
    /// `Shell::with_rail`, shared with every other vault screen.
    pub(super) fn render_gallery_main(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Unfiltered — the header's aggregate always describes the whole
        // vault, independent of what the search box narrows the grid to.
        let all_metas = self.cache.metadata();
        let vault_is_empty = all_metas.is_empty();

        let theme = *cx.theme();
        let top = div()
            .flex()
            .flex_wrap()
            .items_end()
            .justify_between()
            // GPUI has no media queries, so responsiveness here is flex doing
            // what flex does: the controls drop to their own line when the
            // title and them no longer fit side by side, instead of the title
            // being squeezed to nothing or the buttons leaving the window.
            .gap_4()
            .px(px(34.0))
            .pt(px(30.0))
            .pb(px(24.0))
            // No counts under the title. The version total is the number of rows
            // directly beneath it — a number that repeats what is already on
            // screen changes nothing about what the user does next. See the
            // number rule in the component audit.
            .child(
                div()
                    .flex_1()
                    .min_w(px(280.0))
                    .flex()
                    .flex_col()
                    .gap(px(9.0))
                    .child(
                        div()
                            .text_style(TextStyle::title())
                            .text_color(theme.text)
                            // The person, not the furniture. "Your CVs" is a
                            // label for a filing cabinet; the screen is about
                            // which version of *them* goes out next.
                            .child(self.front_door_title()),
                    )
                    .child(
                        div()
                            .max_w(px(540.0))
                            .text_style(TextStyle::body())
                            .text_color(theme.text_muted)
                            .child(
                                "Use a saved version as a safe starting point, then create a \
                                 focused version for the next role.",
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap_3()
                    .child(self.search_box(cx))
                    .child(self.gallery_sort_control(cx))
                    .child(self.render_add_cv(cx)),
            );

        // The import flow is a surface with its own footer and its own scroll,
        // so it does **not** go inside the gallery's scroll area. Nested there,
        // its height had nothing to resolve against: the panel grew to its
        // content, the page scrolled, and the action bar it pins to its own
        // bottom edge went below the fold — which is why the review step
        // appeared to have no way forward at all.
        if self.tailoring.is_some() {
            return div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(top)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .px(px(34.0))
                        .pb(px(30.0))
                        .child(self.render_tailor(cx)),
                );
        }

        if self.gallery_creating {
            return div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(top)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .px(px(34.0))
                        .pb(px(30.0))
                        .child(self.render_template_chooser(cx)),
                );
        }

        let body: AnyElement = if vault_is_empty {
            // US-01 / P-13: a vault with zero documents is often the first
            // screen in a user's life with the product, so it gets a real
            // empty state rather than a barren grid.
            self.render_gallery_empty(cx).into_any_element()
        } else {
            self.render_readings(cx).into_any_element()
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(top)
            .child(
                div()
                    .id("gallery-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .px(px(34.0))
                    .pb(px(30.0))
                    .children((!vault_is_empty).then(|| self.render_tailor_callout(cx)))
                    .child(body),
            )
    }

    pub(super) fn render_gallery_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        EmptyState::new("No CVs yet")
            .icon(IconName::File)
            .body("Start a new CV, or bring in one you already have.")
            .action(
                Button::new("empty-new-cv")
                    .action_primary()
                    .icon(IconName::Plus)
                    .label("Bring in a CV")
                    // US-01: an empty vault is usually somebody who already has
                    // a CV and does not want to start at a blank screen, so
                    // this goes to the import screen — which still offers the
                    // blank document as its second button for the rarer case.
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.open_import(cx);
                    })),
            )
    }

    pub(super) fn search_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        div()
            .w(px(230.0))
            .h(px(38.0))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .rounded(theme.radius_md())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .child(
                Icon::new(IconName::Search)
                    .with_size(cx.theme().icon_md())
                    .text_color(theme.text_subtle),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .children(self.search.as_ref().map(|state| {
                        TextField::new(state)
                            .seamless()
                            .placeholder("Search names, presets, variants…")
                    })),
            )
    }

    /// Bring in a file: the import screen, which is where that has always
    /// lived.
    pub(super) fn open_import(&mut self, cx: &mut Context<Self>) {
        self.gallery_creating = true;
        self.import_step = ImportStep::Step1Drop;
        cx.notify();
    }

    /// Start with nothing: a blank document, straight into the editor.
    ///
    /// No stop on the way. The screen in between is the *import* screen, and
    /// showing somebody a drop zone after they said "start from scratch" is
    /// asking the question they just answered. When there are real templates to
    /// choose between, that choice goes here — there are none today, and a
    /// chooser with one option is a dialog box.
    pub(super) fn start_blank_cv(&mut self, cx: &mut Context<Self>) {
        let doc = ResumeDoc::from_resume(Resume::default(), "Base");
        self.create_doc(doc, "cv", cx);
        self.gallery_creating = false;
        self.import_step = ImportStep::Step1Drop;
        cx.notify();
    }

    pub(super) fn render_template_chooser(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.import_step {
            ImportStep::Step1Drop => div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .w_full()
                .py(px(20.0))
                .child(import_flow::render_step1_bring_document(
                    cx,
                    |this, cx| {
                        this.import_existing_resume(cx);
                    },
                    |this, cx| this.start_blank_cv(cx),
                ))
                .child(
                    Button::new("tpl-cancel")
                        .quiet()
                        .mt(px(16.0))
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.gallery_creating = false;
                            this.import_step = ImportStep::Step1Drop;
                            cx.notify();
                        }))
                        .child("← Back to Gallery"),
                ),
            ImportStep::Parsing { filename } => div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .w_full()
                .py(px(20.0))
                .child(import_flow::render_parsing_step(cx, filename)),
            ImportStep::CouldNotRead { filename, error } => div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .w_full()
                .py(px(20.0))
                .child(import_flow::render_could_not_read(
                    cx,
                    filename,
                    error,
                    |this, cx| {
                        this.import_step = ImportStep::Step1Drop;
                        cx.notify();
                    },
                    |this, cx| this.start_blank_cv(cx),
                )),
            // `flex_1` and `min_h_0`, not `justify_center`: the review panel
            // pins an action bar to its own bottom edge, and a centred box with
            // no bound to resolve against grows past the window and takes that
            // bar with it.
            ImportStep::Step2Review { imported } => div()
                .flex()
                .flex_col()
                .items_center()
                .flex_1()
                .min_h_0()
                .w_full()
                .py(px(20.0))
                .child(import_flow::render_step2_review_split(
                    cx,
                    imported,
                    self.import_section_name.as_ref(),
                    |this, cx| {
                        this.import_step = ImportStep::Step1Drop;
                        cx.notify();
                    },
                    |this, cx| {
                        if let ImportStep::Step2Review { imported } = &this.import_step.clone() {
                            let doc = imported.doc.clone();
                            this.create_doc(doc, "imported", cx);
                            this.gallery_creating = false;
                            this.import_step = ImportStep::Step1Drop;
                        }
                    },
                    // Adopting mutates the *pending* import, not a document on
                    // disk: nothing has been created yet, and Undo import still
                    // throws the whole thing away. The section is simply there
                    // when Continue writes the file.
                    |this, heading, items, cx| {
                        if let ImportStep::Step2Review { imported } = &mut this.import_step {
                            let created = crate::import::model::adopt_as_section(
                                &mut imported.doc,
                                &heading,
                                &items,
                            );
                            if created > 0 {
                                imported.unplaced.retain(|left| !items.contains(left));
                            }
                            cx.notify();
                        }
                    },
                )),
        }
    }

    #[allow(dead_code, clippy::type_complexity)]
    pub(super) fn template_card(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        title: &'static str,
        description: &'static str,
        action: Box<dyn Fn(&mut Self, &mut Context<Self>) + 'static>,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        Card::new()
            .surface()
            .small()
            .interactive(id)
            .flex()
            .flex_col()
            .gap_1()
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                action(this, cx);
            }))
            .child(div().text_color(theme.text).text_sm().child(title))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(description),
            )
    }
}

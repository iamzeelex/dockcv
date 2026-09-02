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

use crate::vault;

use super::gallery_sort::sort_documents;
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
            .items_end()
            .justify_between()
            .gap_4()
            .px(px(34.0))
            .pt(px(30.0))
            .pb(px(24.0))
            // No counts under the title. The document total is the number of
            // cards directly beneath it and the preset total is the sum of the
            // named chips on those cards — neither changes what the user does
            // next. See the number rule in the component audit.
            .child(
                div()
                    .text_style(TextStyle::title())
                    .text_color(theme.text)
                    .child("Your CVs"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.search_box(cx))
                    .child(self.gallery_sort_control(cx))
                    .child(
                        Button::new("new-cv")
                            .action_primary()
                            .icon(IconName::Plus)
                            .label("New CV")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.gallery_creating = true;
                                cx.notify();
                            })),
                    ),
            );

        // The import flow is a surface with its own footer and its own scroll,
        // so it does **not** go inside the gallery's scroll area. Nested there,
        // its height had nothing to resolve against: the panel grew to its
        // content, the page scrolled, and the action bar it pins to its own
        // bottom edge went below the fold — which is why the review step
        // appeared to have no way forward at all.
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
            self.render_doc_grid(cx).into_any_element()
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
                    .label("New CV")
                    // TODO(US-01): once a dedicated first-run import screen
                    // exists, point this at it. Today it opens the same
                    // template chooser every other "New CV" entry point
                    // uses, which already offers "Import existing resume".
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.gallery_creating = true;
                        cx.notify();
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

    pub(super) fn render_doc_grid(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search_query(cx);
        // Cloned, not re-read: the cards take a `DocMeta` by value, and a
        // handful of small string clones per visible card is nothing beside the
        // full-vault TOML parse this used to be — twice a frame, once here and
        // once for the header's aggregate.
        let mut metas: Vec<vault::DocMeta> = self
            .cache
            .metadata()
            .iter()
            .filter(|m| query.is_empty() || m.best_match(&query).is_some())
            .cloned()
            .collect();
        sort_documents(&mut metas, self.gallery_sort, self.cache.applications());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Whether the card should print the person's name at all. In a vault
        // of one person's documents it is the same string on every card, and a
        // headline that never varies is a headline that tells you nothing.
        let mixed_names = metas
            .iter()
            .filter(|m| !m.unreadable)
            .map(|m| m.name.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1;

        let theme = *cx.theme();
        // A block is not a document, so it is not a card in this grid — but a
        // query that finds nothing here and six things in the Library should
        // say so rather than reading as "no results".
        let library_hits = self.library_hits(&query);
        let library_row = (library_hits > 0).then(|| {
            let query = query.clone();
            div()
                .mt(px(18.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .text_style(TextStyle::body())
                        .text_color(theme.text_subtle)
                        .child(format!(
                            "{library_hits} {} in your Library also match",
                            if library_hits == 1 { "block" } else { "blocks" }
                        )),
                )
                .child(
                    Button::new("gallery-library-hits")
                        .quiet()
                        .text_color(theme.accent)
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.open_library_with(query.clone(), window, cx);
                        }))
                        .child("Open Library"),
                )
        });

        div()
            .flex()
            .flex_col()
            .child(
                div().flex().flex_wrap().gap(px(18.0)).children(
                    metas
                        .into_iter()
                        .map(|meta| self.doc_card(cx, meta, now, mixed_names)),
                ),
            )
            .children(library_row)
    }

    /// One document card.
    ///
    /// ### The click model, changed deliberately from the mockup
    ///
    /// **The whole card opens the editor.** It used to be three regions with
    /// three behaviours — the thumbnail opened the document, the badge row
    /// opened the Preset Matrix, and the bottom third did nothing at all —
    /// which is the shape a user reads as "some of this is clickable and I
    /// have to find out which". One card, one primary action, and every
    /// competing destination moved into the `···` menu where it is named
    /// rather than guessed at. The Preset Matrix is still one click away, it
    /// just says so now.
    ///
    /// ### What the card says
    ///
    /// Two CVs for the same person with the same job title are *identical* on
    /// the mockup's card: same name, same role, same "N variants". The facts
    /// that actually tell them apart are the file name, the presets by name,
    /// and where the tailoring is — so those are what it shows. `2 presets`
    /// became `FAANG · concise` and `Infra-heavy`, which is P-01's whole
    /// complaint answered in the place the user looks first.
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
                    |this, cx| {
                        let doc = ResumeDoc::from_resume(Resume::default(), "Base");
                        this.create_doc(doc, "cv", cx);
                        this.gallery_creating = false;
                        this.import_step = ImportStep::Step1Drop;
                    },
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
                    |this, cx| {
                        let doc = ResumeDoc::from_resume(Resume::default(), "Base");
                        this.create_doc(doc, "cv", cx);
                        this.gallery_creating = false;
                        this.import_step = ImportStep::Step1Drop;
                    },
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

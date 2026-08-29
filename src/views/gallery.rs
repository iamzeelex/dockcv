//! Gallery screen rendering for `Shell` (`docs/design/gallery.md` §3).
//!
//! The nav rail is shared chrome and lives in `sidebar.rs`; this file only
//! owns the main pane — header, search, doc grid, and the "new CV" flow.

use gpui::prelude::*;
use gpui::{div, img, px, AnyElement, ClickEvent, Context, FontWeight, IntoElement, SharedString};

use crate::resume::model::{Resume, ResumeDoc};
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use super::import_flow::{self, ImportStep};
use dockcv_ui_components::{ScrollableElement, 
    Button, ButtonExt, Card, DropdownMenu, EmptyState, Icon, IconName, PopupMenuItem, Sizable, Tag,
    TextField, SANS,
};

use crate::vault;

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
            // next. See the number rule in `docs/design/component-audit.md`.
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
                div().flex_1().min_w_0().children(
                    self.search
                        .as_ref()
                        .map(|state| TextField::new(state).seamless().placeholder("Search CVs…")),
                ),
            )
    }

    pub(super) fn render_doc_grid(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search_query(cx);
        // Cloned, not re-read: the cards take a `DocMeta` by value, and a
        // handful of small string clones per visible card is nothing beside the
        // full-vault TOML parse this used to be — twice a frame, once here and
        // once for the header's aggregate.
        let metas: Vec<vault::DocMeta> = self
            .cache
            .metadata()
            .iter()
            .filter(|m| query.is_empty() || m.name.to_lowercase().contains(&query))
            .cloned()
            .collect();
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

        div()
            .flex()
            .flex_wrap()
            .gap(px(18.0))
            .children(
                metas
                    .into_iter()
                    .map(|meta| self.doc_card(cx, meta, now, mixed_names)),
            )
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
    pub(super) fn doc_card(
        &self,
        cx: &mut Context<Self>,
        meta: vault::DocMeta,
        now: u64,
        mixed_names: bool,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        let path = meta.path.clone();
        let open_path = path.clone();
        let renaming = self.renaming_doc.as_deref() == Some(path.as_path());

        let updated = meta
            .modified_secs
            .map(|secs| vault::relative_time(secs, now))
            .unwrap_or_else(|| "—".to_string());

        let thumb: AnyElement = match self.thumbnails.get(&meta.path) {
            Some(rendered) if rendered.width > 0.0 => {
                let ratio = (rendered.height / rendered.width).clamp(0.1, 4.0);
                img(rendered.image.clone())
                    .w(px(210.0))
                    .h(px(210.0 * ratio))
                    .into_any_element()
            }
            _ => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_style(TextStyle::meta())
                .text_color(theme.text_subtle)
                .child("rendering…")
                .into_any_element(),
        };

        let card_id = SharedString::from(format!("card-{}", meta.path.to_string_lossy()));
        Card::new()
            .elevated()
            // Full-bleed: the thumbnail runs to the card's own edge, so the
            // padding lives on the body rows rather than on the shell.
            .p_0()
            .w(px(248.0))
            .flex()
            .flex_col()
            // Deliberately *not* `overflow_hidden`: GPUI clips to the bounds
            // rectangle rather than the rounded path, so it never rounded the
            // thumbnail anyway — and it silently clipped the `···` popup.
            //
            // Interactive only while it is not being renamed: during a rename
            // the card holds a live text field, and a card-wide click target
            // over it would open the document on every caret placement.
            .map(|card| {
                if renaming {
                    card.id(card_id)
                } else {
                    card.interactive(card_id).on_click(cx.listener(
                        move |this: &mut Self, _: &ClickEvent, _window, cx| {
                            this.open_doc(open_path.clone(), cx);
                        },
                    ))
                }
            })
            .child(
                div()
                    .h(px(152.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(17.0))
                    .py(px(15.0))
                    .bg(theme.paper)
                    .border_b_1()
                    .border_color(theme.paper_border)
                    // Derived, not chosen: the thumbnail sits *inside* the
                    // card's 1px border, so its corner is the card's radius
                    // less that border. Written as the token minus the border
                    // so it follows the ladder if the ladder moves.
                    .rounded_t(theme.radius_lg() - px(1.0))
                    .overflow_hidden()
                    .child(thumb),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .px(px(16.0))
                    .pt(px(14.0))
                    .pb(px(14.0))
                    .child(self.card_menu(cx, &meta))
                    .child(self.card_title(cx, &meta, renaming))
                    // The person, and only when the gallery is looking at more
                    // than one of them. Otherwise this line is the same string
                    // on every card in the grid.
                    .children(
                        (mixed_names && !meta.unreadable && !meta.name.trim().is_empty()).then(
                            || {
                                div()
                                    .mt(px(2.0))
                                    .text_style(TextStyle::body())
                                    .text_color(theme.text_muted)
                                    .truncate()
                                    .child(meta.name.clone())
                            },
                        ),
                    )
                    .child(div().mt(px(11.0)).child(self.card_presets(cx, &meta)))
                    // File name and age are different kinds of fact and were
                    // being joined with middots into the least readable line on
                    // the card. Two columns under a hairline: the name scans
                    // down the left edge of the grid, the age down the right.
                    .child(
                        div()
                            .mt(px(12.0))
                            .pt(px(10.0))
                            .border_t_1()
                            .border_color(theme.border)
                            .flex()
                            .items_baseline()
                            .justify_between()
                            .gap(px(10.0))
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(div().flex_1().min_w_0().truncate().child(meta.stem.clone()))
                            .child(div().flex_none().child(updated.clone())),
                    ),
            )
    }

    /// The title row: the person's name, or the rename box in its place.
    fn card_title(
        &self,
        cx: &mut Context<Self>,
        meta: &vault::DocMeta,
        renaming: bool,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        if renaming {
            // Renaming edits the **file name**, which is what a document is
            // called under File-over-App — so the box is seeded with the stem,
            // not the person's name.
            return div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .pr(px(26.0))
                // Same reason as the `···` wrapper: typing in this box must
                // not read as a click on the card behind it.
                .occlude()
                .child(div().flex_1().min_w_0().children(
                    self.rename_field.as_ref().map(|state| {
                        TextField::new(state).placeholder("Document name")
                    }),
                ))
                // A way out that is not "press Escape and hope": the rename
                // box replaces the card's title, so without this the only
                // exits are committing or restarting the app.
                .child(
                    Button::new("rename-cancel")
                        .icon_only()
                        .icon(IconName::Close)
                        .tooltip("Cancel")
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.cancel_rename(cx);
                        })),
                )
                .into_any_element();
        }

        let is_copy = meta.stem.contains("-copy");
        // The **role**, not the person. In a vault of one person's documents
        // the name is the same string on every card, so heading each card with
        // it means six cards that read alike until you get to the second line.
        // The role is what tells them apart, so it takes the headline.
        let headline = if meta.unreadable {
            "unreadable file".to_string()
        } else if meta.label.trim().is_empty() {
            // Nothing to lead with: fall back to the document's own name on
            // disk rather than printing an empty headline.
            meta.stem.clone()
        } else {
            meta.label.clone()
        };

        div()
            .flex()
            .items_center()
            .gap_2()
            .pr(px(26.0)) // clears the "···" trigger
            .child(
                // `flex_1` alongside the `min_w_0`: without it the title box
                // shrinks to its own minimum and ellipsises a title that had
                // the whole card to sit in — "Leo Vaicer" came out
                // "Leo Vai…" with 200px to spare.
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(SANS)
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if meta.unreadable {
                        theme.text_muted
                    } else {
                        theme.text
                    })
                    .child(headline),
            )
            .when(is_copy, |row| {
                row.child(
                    // The chip keeps its size; the title is what gives.
                    div()
                        .flex_none()
                        .px_2()
                        .py(px(1.0))
                        .rounded_full()
                        .bg(theme.hover)
                        .text_style(TextStyle::chip())
                        .text_color(theme.text_muted)
                        .child("copy"),
                )
            })
            .into_any_element()
    }

    /// Presets, by name. `draft` when there are none — a document nobody has
    /// organised into a preset yet is not ready to send, and that is worth
    /// saying in a word rather than as "0 presets".
    fn card_presets(&self, cx: &mut Context<Self>, meta: &vault::DocMeta) -> impl IntoElement {
        let theme = *cx.theme();
        // Two names fit the card's width; the rest become a count, so a
        // document with six presets still reads at a glance.
        const SHOWN: usize = 2;
        let shown: Vec<String> = meta.preset_names.iter().take(SHOWN).cloned().collect();
        let hidden = meta.preset_names.len().saturating_sub(shown.len());

        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.0))
            .when(meta.is_draft(), |row| {
                row.child(
                    Tag::custom(theme.chip_bg_neutral, theme.text_muted, theme.chip_bg_neutral)
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(theme.radius_sm())
                        .text_style(TextStyle::chip())
                        .child("draft"),
                )
            })
            .children(shown.into_iter().map(|name| {
                Tag::custom(theme.chip_bg, theme.chip_fg, theme.chip_bg)
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(theme.radius_sm())
                    .text_style(TextStyle::chip())
                    .child(name)
            }))
            .when(hidden > 0, |row| {
                row.child(
                    div()
                        .text_style(TextStyle::chip())
                        .text_color(theme.text_subtle)
                        .child(format!("+{hidden}")),
                )
            })
    }

    /// The `···` menu. Sits higher and further right than the mockup draws it
    /// — flush with the body's own padding rather than inset from it, which is
    /// what makes it read as chrome on the card instead of an item in the
    /// content.
    fn card_menu(&self, cx: &mut Context<Self>, meta: &vault::DocMeta) -> impl IntoElement {
        let shell = cx.weak_entity();
        let path = meta.path.clone();
        let vault_dir = self.vault.clone();

        // Wrapped in an occluding div, not placed bare: the whole card is a
        // click target now, and without this the card's own `on_click` sees
        // the press first and opens the editor instead of the menu.
        // `occlude` blocks the mouse from reaching anything painted behind
        // this hitbox, which is exactly the card underneath it.
        div()
            .absolute()
            // Between the mockup's inset (14/13) and the corner: nudged up and
            // out as asked, but not pinned to the edge — at 8/6 it read as
            // falling off the card rather than sitting on it.
            .top(px(10.0))
            .right(px(10.0))
            .occlude()
            .child(
        Button::new(SharedString::from(format!(
            "card-menu-{}",
            meta.path.to_string_lossy()
        )))
        .icon_only()
        .icon(IconName::Ellipsis)
        .tooltip("More")
        .dropdown_menu(move |menu, _window, _cx| {
            let (rename, matrix, dup, del, reveal) = (
                shell.clone(),
                shell.clone(),
                shell.clone(),
                shell.clone(),
                shell.clone(),
            );
            let (p_rename, p_matrix, p_dup, p_del) =
                (path.clone(), path.clone(), path.clone(), path.clone());
            let vault_dir = vault_dir.clone();

            menu.item(PopupMenuItem::new("Rename…").on_click(
                move |_ev, window, cx| {
                    let _ = rename.update(cx, |this, cx| {
                        this.start_rename(p_rename.clone(), window, cx);
                    });
                },
            ))
            .item(PopupMenuItem::new("Presets…").on_click(move |_ev, _window, cx| {
                let _ = matrix.update(cx, |this, cx| {
                    this.open_preset_matrix(p_matrix.clone(), cx);
                });
            }))
            .item(PopupMenuItem::new("Duplicate").on_click(move |_ev, _window, cx| {
                let _ = dup.update(cx, |this, cx| this.duplicate_doc(p_dup.clone(), cx));
            }))
            .item(PopupMenuItem::new("Show in Finder").on_click(move |_ev, _window, cx| {
                let _ = reveal.update(cx, |_this, cx| {
                    if let Some(dir) = vault_dir.clone() {
                        cx.open_with_system(&dir);
                    }
                });
            }))
            .separator()
            .item(PopupMenuItem::new("Delete").on_click(move |_ev, _window, cx| {
                let _ = del.update(cx, |this, cx| this.delete_doc(p_del.clone(), cx));
            }))
        }),
            )
    }

    pub(super) fn render_template_chooser(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.import_step {
            ImportStep::Step1Drop => {
                div()
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
                    )
            }
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



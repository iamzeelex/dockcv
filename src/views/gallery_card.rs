//! One document's card in the gallery.
//!
//! Split from `gallery.rs`, which had grown past the line limit carrying the
//! screen, the search, the order and the card. The card is the largest of those
//! and the most self-contained: it renders a `DocMeta` and opens a document.

use gpui::prelude::*;
use gpui::{div, img, px, AnyElement, ClickEvent, Context, FontWeight, IntoElement, SharedString};

use dockcv_ui_components::{
    Button, ButtonExt, Card, DropdownMenu, IconName, PopupMenuItem, Tag, TextField, SANS,
};

use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;

use super::shell::Shell;

impl Shell {
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

        // Only while searching, and only when the match is not the headline or
        // the file name the card already prints.
        let query = self.search_query(cx);
        let matched = meta
            .best_match(&query)
            .and_then(|entry| entry.kind.label().map(|k| (k, entry.text.clone())));

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
                    // Why this card is in the results, when the reason is not
                    // already on it. A search that matched a variant name shows
                    // an otherwise unremarkable card, and a result that cannot
                    // explain itself is the flat list this task exists to end.
                    .children(matched.map(|(kind, text)| {
                        div()
                            .mt(px(7.0))
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .flex_none()
                                    .text_style(TextStyle::eyebrow())
                                    .text_color(theme.accent)
                                    .child(kind),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_subtle)
                                    .child(text),
                            )
                    }))
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
                .child(
                    div().flex_1().min_w_0().children(
                        self.rename_field
                            .as_ref()
                            .map(|state| TextField::new(state).placeholder("Document name")),
                    ),
                )
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
                // the whole card to sit in — "Marie Curie" came out
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
                    Tag::custom(
                        theme.chip_bg_neutral,
                        theme.text_muted,
                        theme.chip_bg_neutral,
                    )
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

                    menu.item(
                        PopupMenuItem::new("Rename…").on_click(move |_ev, window, cx| {
                            let _ = rename.update(cx, |this, cx| {
                                this.start_rename(p_rename.clone(), window, cx);
                            });
                        }),
                    )
                    .item(
                        PopupMenuItem::new("Presets…").on_click(move |_ev, _window, cx| {
                            let _ = matrix.update(cx, |this, cx| {
                                this.open_preset_matrix(p_matrix.clone(), cx);
                            });
                        }),
                    )
                    .item(
                        PopupMenuItem::new("Duplicate").on_click(move |_ev, _window, cx| {
                            let _ =
                                dup.update(cx, |this, cx| this.duplicate_doc(p_dup.clone(), cx));
                        }),
                    )
                    .item(
                        PopupMenuItem::new("Show in Finder").on_click(move |_ev, _window, cx| {
                            let _ = reveal.update(cx, |_this, cx| {
                                if let Some(dir) = vault_dir.clone() {
                                    cx.open_with_system(&dir);
                                }
                            });
                        }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("Delete").on_click(move |_ev, _window, cx| {
                            let _ = del.update(cx, |this, cx| this.delete_doc(p_del.clone(), cx));
                        }),
                    )
                }),
            )
    }
}

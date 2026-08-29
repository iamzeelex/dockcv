//! Library screen — the reusable block pool, "your building set".
//!
//! Design row: `.design/rows/row_the_library.txt`. This renders the **main
//! pane only**: the nav rail around it is shared chrome mounted by
//! `Shell::with_rail`, because the design draws Library as one tab of the
//! vault window rather than a page you navigate away to.
//!
//! What the mockup's block card carries and this one deliberately does not:
//! `2 variants`, `used in 3 CVs`, and a `Linked`/`Detached` badge. None of the
//! three exists in the model — a library block is a flat copy today, and
//! link status is D2 (review US-03). Rendering any of them would mean
//! inventing a number about the user's own corpus, which is the one thing
//! this product must never do.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement, SharedString};

use crate::config;
use crate::resume::model::{Library, SectionKind};
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;

use super::save_status;
use dockcv_ui_components::{ScrollableElement, Button, ButtonExt, DropdownMenu, Icon, IconName, PopupMenuItem, Sizable, Tag, TextField};

use super::confirm;
use super::applications_data::plural;
use super::library_usage::UsageIndex;
use super::shell::{remove_at, Screen, Shell};

/// The five section kinds that have a library pool, in the order the screen
/// lists them. Profile has no pool (there is only one of you), and custom
/// sections do not have one either — see `root.rs::save_block_to_library`.
const POOLS: [(SectionKind, &str); 5] = [
    (SectionKind::Work, "Work Experience"),
    (SectionKind::Education, "Education"),
    (SectionKind::Skills, "Skills"),
    (SectionKind::Certificates, "Certifications"),
    (SectionKind::Organizations, "Organizations"),
];

/// One block flattened for rendering: the lines a card draws, plus the
/// keywords a skill group shows as chips instead of a body line.
struct BlockCard {
    /// Position in the *pool*, not in the filtered list on screen — this is
    /// what delete addresses, so it must survive search and filtering.
    index: usize,
    title: String,
    subtitle: String,
    body: String,
    keywords: Vec<String>,
    /// Every bullet behind this block, whether or not the card draws it.
    ///
    /// The card shows one line; the block may hold six. Searching a library by
    /// the *content* of its blocks is the feature the library exists for
    /// (review P-14, US-20) and it was matching only what was on screen — so
    /// the phrase you half-remember from a bullet three lines down found
    /// nothing.
    hidden_text: Vec<String>,
}

impl BlockCard {
    /// Everything the card puts on screen, lowercased — what the search box
    /// matches against. Searching the text of blocks rather than their titles
    /// is the point of having a library at all (review P-14 / US-20); this
    /// covers what the card shows, not yet every bullet behind it.
    fn haystack(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.title,
            self.subtitle,
            self.body,
            self.keywords.join(" "),
            self.hidden_text.join(" ")
        )
        .to_lowercase()
    }
}

/// `start – end`, with an open end reading "Present" — the exact rule
/// `template.rs::daterange` applies, so a card and the rendered CV never
/// disagree about the same two fields.
fn date_range(start: &str, end: &str) -> String {
    match (start.trim(), end.trim()) {
        ("", "") => String::new(),
        (start, "") => format!("{start} – Present"),
        (start, end) => format!("{start} – {end}"),
    }
}

/// Join two parts with the design's middle dot, skipping empty ones.
fn join_em(a: &str, b: &str) -> String {
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() {
        b.to_string()
    } else if b.is_empty() {
        a.to_string()
    } else {
        format!("{a} · {b}")
    }
}

/// How the blocks inside each section group are ordered.
///
/// US-20 asks for "сортировка библиотеки по «давно не использовалось»" — and
/// that is exactly what the usage index makes answerable. A pool you cannot
/// sort by use is a pool you cannot prune: the formulations that have earned
/// their place and the ones that never did look identical.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum LibrarySort {
    /// The order the pool is stored in — the order blocks were added.
    #[default]
    Added,
    /// Busiest first: what you actually reach for.
    MostUsed,
    /// Never-used first, then least. The pruning view.
    LeastUsed,
}

impl LibrarySort {
    pub(super) const ALL: [LibrarySort; 3] = [
        LibrarySort::Added,
        LibrarySort::MostUsed,
        LibrarySort::LeastUsed,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            LibrarySort::Added => "Order added",
            LibrarySort::MostUsed => "Most used first",
            LibrarySort::LeastUsed => "Never used first",
        }
    }

    pub(super) fn short_label(self) -> &'static str {
        match self {
            LibrarySort::Added => "Added",
            LibrarySort::MostUsed => "Most used",
            LibrarySort::LeastUsed => "Unused",
        }
    }
}

/// Order `cards` in place. `uses` gives each card's usage count; ties fall
/// back to pool order so the arrangement is total and cannot reshuffle between
/// renders of identical data.
fn sort_cards(cards: &mut [BlockCard], uses: &dyn Fn(usize) -> usize, sort: LibrarySort) {
    match sort {
        LibrarySort::Added => cards.sort_by_key(|c| c.index),
        LibrarySort::MostUsed => {
            cards.sort_by(|a, b| uses(b.index).cmp(&uses(a.index)).then(a.index.cmp(&b.index)))
        }
        LibrarySort::LeastUsed => {
            cards.sort_by(|a, b| uses(a.index).cmp(&uses(b.index)).then(a.index.cmp(&b.index)))
        }
    }
}

/// Append a clone of `block` to `into`, if there is one at that index.
fn push_clone<T: Clone>(block: Option<&T>, into: &mut Vec<T>) {
    if let Some(block) = block {
        into.push(block.clone());
    }
}

/// Flatten one section's pool into cards, in pool order.
fn cards_for(library: &Library, section: SectionKind) -> Vec<BlockCard> {
    match section {
        SectionKind::Work => library
            .work
            .iter()
            .enumerate()
            .map(|(index, w)| BlockCard {
                index,
                title: w.position.clone(),
                subtitle: join_em(&w.name, &date_range(&w.start_date.text, &w.end_date.text)),
                body: if w.summary.trim().is_empty() {
                    w.highlights.first().cloned().unwrap_or_default()
                } else {
                    w.summary.clone()
                },
                keywords: Vec::new(),
                hidden_text: {
                    let mut text = w.highlights.clone();
                    text.push(w.summary.clone());
                    text
                },
            })
            .collect(),
        SectionKind::Education => library
            .education
            .iter()
            .enumerate()
            .map(|(index, e)| BlockCard {
                index,
                title: e.study_type.clone(),
                subtitle: join_em(&e.institution, &date_range(&e.start_date.text, &e.end_date.text)),
                body: String::new(),
                keywords: Vec::new(),
                hidden_text: e.highlights.clone(),
            })
            .collect(),
        SectionKind::Skills => library
            .skills
            .iter()
            .enumerate()
            .map(|(index, s)| BlockCard {
                index,
                title: s.name.clone(),
                subtitle: String::new(),
                body: String::new(),
                keywords: s.keywords.clone(),
                hidden_text: Vec::new(),
            })
            .collect(),
        SectionKind::Certificates => library
            .certificates
            .iter()
            .enumerate()
            .map(|(index, c)| BlockCard {
                index,
                title: c.name.clone(),
                subtitle: join_em(&c.issuer, &c.date.text),
                body: String::new(),
                keywords: Vec::new(),
                hidden_text: Vec::new(),
            })
            .collect(),
        SectionKind::Organizations => library
            .volunteer
            .iter()
            .enumerate()
            .map(|(index, v)| BlockCard {
                index,
                title: v.position.clone(),
                subtitle: join_em(&v.organization, &date_range(&v.start_date.text, &v.end_date.text)),
                body: v.highlights.first().cloned().unwrap_or_default(),
                keywords: Vec::new(),
                hidden_text: v.highlights.clone(),
            })
            .collect(),
        SectionKind::Profile | SectionKind::Custom(_) => Vec::new(),
    }
}

impl Shell {
    pub(super) fn render_library_screen(&self, cx: &mut Context<Self>) -> gpui::Div {
        let library = self.cache.library();
        let query = self.library_query(cx);
        // Once per render, not once per card: asking each card to walk every
        // document would be the per-frame work this codebase has already been
        // bitten by once.
        let usage = UsageIndex::build(self.cache.library(), self.cache.readable_documents());

        // Counts describe the whole library, never the filtered view — same
        // rule the gallery header follows.
        let counts: Vec<(SectionKind, &str, usize)> = POOLS
            .iter()
            .map(|&(section, title)| (section, title, cards_for(library, section).len()))
            .collect();
        let total: usize = counts.iter().map(|(_, _, n)| n).sum();

        let groups: Vec<AnyElement> = counts
            .iter()
            .filter(|(section, _, count)| {
                *count > 0 && self.library_filter.is_none_or(|f| f == *section)
            })
            .filter_map(|&(section, title, _)| {
                let mut cards: Vec<BlockCard> = cards_for(library, section)
                    .into_iter()
                    .filter(|card| query.is_empty() || card.haystack().contains(&query))
                    .collect();
                if cards.is_empty() {
                    return None;
                }
                sort_cards(
                    &mut cards,
                    &|index| usage.count_for(library, section, index),
                    self.library_sort,
                );
                Some(
                    self.library_group(cx, title, section, cards, &usage)
                        .into_any_element(),
                )
            })
            .collect();

        let body: AnyElement = if total == 0 {
            self.render_library_onboarding(cx).into_any_element()
        } else if groups.is_empty() {
            // Everything is filtered out — say so rather than showing a blank
            // pane the user has to guess about.
            div()
                .text_style(TextStyle::body())
                .text_color(cx.theme().text_muted)
                .child("No blocks match this filter.")
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(28.0))
                .children(groups)
                .into_any_element()
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .child(self.library_header(cx, total, usage.total_reuses(library)))
            .child(
                div()
                    .id("library-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .px(px(34.0))
                    .pb(px(30.0))
                    .flex()
                    .flex_col()
                    .gap(px(18.0))
                    .children(self.library_helper(cx, total))
                    .children((total > 0).then(|| self.library_filter_chips(cx, &counts, total)))
                    .child(body),
            )
            .children(self.render_library_edit_sheet(cx))
            .children(self.render_library_push_sheet(cx))
    }

    /// Title, block count, search and the "New block" menu.
    fn library_header(
        &self,
        cx: &mut Context<Self>,
        total: usize,
        reuses: usize,
    ) -> impl IntoElement {
        let shell = cx.weak_entity();
        div()
            .flex()
            .items_end()
            .justify_between()
            .gap_4()
            .px(px(34.0))
            .pt(px(30.0))
            .pb(px(24.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_style(TextStyle::title())
                            .text_color(cx.theme().text)
                            .child("Your library"),
                    )
                    // The mockup's `8 blocks · reused 14 times`. The second
                    // figure is real now: it is derived by matching each block
                    // against every document in the vault
                    // (`library_usage`), never stored, so it cannot drift and
                    // cannot be a number about the user's corpus that nothing
                    // stands behind (US-14).
                    .child(
                        div()
                            .mt(px(7.0))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Tag::secondary().small().child(format!(
                                "{total} block{}",
                                if total == 1 { "" } else { "s" }
                            )))
                            .children((reuses > 0).then(|| {
                                Tag::secondary()
                                    .small()
                                    .child(format!("reused {reuses} time{}", plural(reuses)))
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.library_sort_control(cx))
                    .child(self.library_search_box(cx))
                    .child(
                        Button::new("new-block")
                            .action_primary()
                            .icon(IconName::Plus)
                            .label("New block")
                            .dropdown_menu(move |mut menu, _window, _cx| {
                                for (section, title) in POOLS {
                                    let shell = shell.clone();
                                    menu = menu.item(PopupMenuItem::new(title).on_click(
                                        move |_ev, window, cx| {
                                            let _ = shell.update(cx, |this, cx| {
                                                this.open_library_edit(section, None, window, cx);
                                            });
                                        },
                                    ));
                                }
                                menu
                            }),
                    ),
            )
    }

    /// The block order, as a menu — the same control shape the Applications
    /// toolbar uses, because it is the same job.
    fn library_sort_control(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.library_sort;
        let shell = cx.weak_entity();
        Button::new("library-sort")
            .selector()
            .icon(IconName::SortAscending)
            .label(active.short_label())
            .tooltip("Order blocks")
            .dropdown_menu(move |mut menu, _window, _cx| {
                for sort in LibrarySort::ALL {
                    let shell = shell.clone();
                    menu = menu.item(
                        PopupMenuItem::new(sort.label())
                            .checked(sort == active)
                            .on_click(move |_ev, _window, cx| {
                                let _ = shell.update(cx, |this, cx| {
                                    this.library_sort = sort;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
    }

    fn library_search_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                    self.library_search
                        .as_ref()
                        .map(|state| TextField::new(state).seamless().placeholder("Search blocks…")),
                ),
            )
    }

    /// The one-line helper the design's caption describes: the wall-sized
    /// tutorial "shrinks to one quiet line once you've started". Shown only
    /// once there is something in the library — before that the full
    /// onboarding card is the empty state — and dismissible for good.
    fn library_helper(&self, cx: &mut Context<Self>, total: usize) -> Option<impl IntoElement> {
        if total == 0 || self.library_helper_dismissed {
            return None;
        }
        let theme = *cx.theme();
        Some(
            div()
                .flex()
                .items_center()
                .gap_3()
                .px_4()
                .py_3()
                .rounded(theme.radius_md())
                .bg(theme.surface)
                .border_1()
                .border_color(theme.border)
                .child(
                    Icon::new(IconName::Star)
                        .with_size(cx.theme().icon_md())
                        .text_color(theme.accent),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_style(TextStyle::body())
                        .text_color(theme.text_muted)
                        // Names the two controls that are actually on this
                        // screen. It used to point at “★ From library”, which
                        // is not how a block gets reused any more.
                        .child(
                            "Reuse drops a block straight into another CV. New block writes \
                             one from scratch, and ★ in the editor lifts one out of a CV.",
                        ),
                )
                .child(
                    Button::new("library-helper-dismiss")
                        .icon_only()
                        .icon(IconName::Close)
                        .tooltip("Dismiss")
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.library_helper_dismissed = true;
                            config::dismiss_library_helper();
                            cx.notify();
                        })),
                ),
        )
    }

    /// `All 8 · Work 3 · Skills 2 …` — one chip per pool that has anything in
    /// it, exactly as the mockup draws them (it lists no empty kind).
    fn library_filter_chips(
        &self,
        cx: &mut Context<Self>,
        counts: &[(SectionKind, &str, usize)],
        total: usize,
    ) -> impl IntoElement {
        let mut row = div().flex().flex_wrap().items_center().gap_2().child(
            self.filter_chip(cx, "libfilter-all", "All", total, self.library_filter.is_none(), None),
        );
        for &(section, title, count) in counts {
            if count == 0 {
                continue;
            }
            row = row.child(self.filter_chip(
                cx,
                SharedString::from(format!("libfilter-{section:?}")),
                title,
                count,
                self.library_filter == Some(section),
                Some(section),
            ));
        }
        row
    }

    fn filter_chip(
        &self,
        cx: &mut Context<Self>,
        id: impl Into<SharedString>,
        label: &str,
        count: usize,
        active: bool,
        section: Option<SectionKind>,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        Button::new(id.into())
            .chip(active, &theme)
            .gap(px(6.0))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.library_filter = section;
                cx.notify();
            }))
            .child(label.to_string())
            // The count is data — mono, and dimmer than the label it follows.
            .child(
                div()
                    .text_style(TextStyle::chip())
                    .text_color(if active {
                        theme.chip_fg
                    } else {
                        theme.text_subtle
                    })
                    .child(format!("{count}")),
            )
    }

    fn library_group(
        &self,
        cx: &mut Context<Self>,
        title: &'static str,
        section: SectionKind,
        cards: Vec<BlockCard>,
        usage: &UsageIndex,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        let count = cards.len();

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .mb(px(12.0))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .text_style(TextStyle::heading())
                            .text_color(theme.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(format!("{count}")),
                    ),
            )
            .child(
                Button::new(SharedString::from(format!("libadd-{section:?}")))
                    .quiet()
                    .icon(IconName::Plus)
                    .label("Add")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_library_edit(section, None, window, cx);
                    })),
            );

        div()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .children(
                        cards
                            .into_iter()
                            .map(|card| self.block_card(cx, section, card, usage)),
                    ),
            )
    }

    fn block_card(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
        card: BlockCard,
        usage: &UsageIndex,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        let index = card.index;
        let shell = cx.weak_entity();
        let used_in = usage.documents_for(self.cache.library(), section, index).to_vec();
        // Every document in the vault, as somewhere this block could go.
        let destinations: Vec<(String, std::path::PathBuf)> = self
            .cache
            .metadata()
            .iter()
            .filter(|m| !m.unreadable)
            .map(|m| {
                let label = if m.name.trim().is_empty() {
                    m.stem.clone()
                } else {
                    format!("{} · {}", m.name, m.stem)
                };
                (label, m.path.clone())
            })
            .collect();

        div()
            .w(px(320.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .relative()
            .px_4()
            .py_3()
            .rounded(theme.radius_lg())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .hover(|s| s.border_color(theme.accent))
            // Destructive action behind the "···" menu, never as a visible ✕
            // (review L-07 — the same fix the gallery card already carries).
            // The positioning lives on this wrapper, never on the trigger
            // itself. `dropdown_menu` hands the trigger's style to the popover
            // and `Popover::render` then ignores it — so an `absolute()`
            // Button paints in the corner while its click region, which is the
            // popover's own unstyled wrapper, collapses to a zero-height
            // strip. The button looks right and is dead. Applications has had
            // the wrapper since the drag work; Library and Diary had not.
            .child(
                div().absolute().top(px(10.0)).right(px(8.0)).child(
                    Button::new(SharedString::from(format!("libmenu-{section:?}-{index}")))
                        .icon_only()
                        .icon(IconName::Ellipsis)
                        .tooltip("More")
                        .dropdown_menu(move |menu, _window, _cx| {
                            let shell = shell.clone();
                            let for_edit = shell.clone();
                            menu.item(PopupMenuItem::new("Edit").on_click(
                                move |_ev, window, cx| {
                                    let _ = for_edit.update(cx, |this, cx| {
                                        this.open_library_edit(section, Some(index), window, cx);
                                    });
                                },
                            ))
                            .separator()
                            .item(PopupMenuItem::new("Delete").on_click(
                                move |_ev, window, cx| {
                                    let _ = shell.update(cx, |_this, cx| {
                                        confirm::destructive(
                                            "Delete this block from your library?".into(),
                                            format!(
                                                "{} Copies already placed in a CV are not \
                                                 affected — a library block is a source to \
                                                 copy from, not a link.",
                                                confirm::CANNOT_UNDO
                                            ),
                                            "Delete",
                                            window,
                                            cx,
                                            move |this, _window, cx| {
                                                this.remove_library_block(section, index, cx)
                                            },
                                        );
                                    });
                                },
                            ))
                        }),
                ),
            )
            .child(
                div()
                    .pr(px(24.0)) // clears the "···" trigger
                    .text_style(TextStyle::body())
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(card.title.clone()),
            )
            .children((!card.subtitle.is_empty()).then(|| {
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child(card.subtitle.clone())
            }))
            .children((!card.body.is_empty()).then(|| {
                div()
                    .text_style(TextStyle::body())
                    .text_color(theme.text_muted)
                    .child(card.body.clone())
            }))
            .children((!card.keywords.is_empty()).then(|| {
                div().flex().flex_wrap().gap(px(5.0)).children(
                    card.keywords
                        .iter()
                        .map(|k| Tag::secondary().small().child(k.clone())),
                )
            }))
            // The footer is what turns a shelf into a workshop: where this
            // block already is, and one move to put it somewhere else. Before
            // this the Library was a screen you could only look at — every
            // path out of it went through opening a CV first.
            .child(
                div()
                    .mt(px(4.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(self.used_in_line(cx, used_in))
                    .children((!destinations.is_empty()).then(|| {
                        let shell = cx.weak_entity();
                        Button::new(SharedString::from(format!("libuse-{section:?}-{index}")))
                            .selector_inline()
                            .label("Reuse")
                            .tooltip("Add a copy of this block to a CV")
                            .dropdown_menu(move |mut menu, _window, _cx| {
                                for (label, path) in &destinations {
                                    let shell = shell.clone();
                                    let path = path.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(label.clone()).on_click(
                                            move |_ev, _window, cx| {
                                                let path = path.clone();
                                                let _ = shell.update(cx, |this, cx| {
                                                    this.copy_block_into_document(
                                                        section, index, &path, cx,
                                                    );
                                                });
                                            },
                                        ),
                                    );
                                }
                                menu
                            })
                    })),
            )
    }

    /// `used in 3 CVs`, and clicking it says **which** — the reverse
    /// navigation the review asks for by name (P-02: «на карточке есть
    /// `Reuse ▾`, но нет обратного действия — показать, где используется»).
    ///
    /// A block nobody has placed says so out loud rather than drawing `0`: the
    /// useful reading of this line is "is this formulation earning its keep",
    /// and "not used yet" answers that where a zero just looks broken.
    fn used_in_line(
        &self,
        cx: &mut Context<Self>,
        used_in: Vec<super::library_usage::DocumentRef>,
    ) -> AnyElement {
        let theme = *cx.theme();
        if used_in.is_empty() {
            return div()
                .text_style(TextStyle::meta())
                .text_color(theme.text_subtle)
                .child("not used yet")
                .into_any_element();
        }

        let count = used_in.len();
        // The second number is the one that answers "what happens if I edit
        // this": a tailored copy is one the CV reworded, and the one a push
        // would overwrite. Without it the card reports reach and stays silent
        // about consequence — which is the P-02 silence this feature exists to
        // break. A block nobody has touched says nothing extra.
        let tailored = used_in.iter().filter(|d| d.diverged).count();
        let label = if tailored == 0 {
            format!("used in {count} CV{}", plural(count))
        } else {
            format!("used in {count} CV{} \u{b7} {tailored} tailored", plural(count))
        };
        let shell = cx.weak_entity();
        Button::new(SharedString::from(format!(
            "libused-{}",
            used_in
                .iter()
                .map(|d| d.stem.as_str())
                .collect::<Vec<_>>()
                .join("-")
        )))
        .selector_inline()
        .label(label)
        .tooltip("Show which CVs")
        .dropdown_menu(move |mut menu, _window, _cx| {
            for reference in &used_in {
                let shell = shell.clone();
                let stem = reference.stem.clone();
                // Named on the row rather than counted only in the summary:
                // the useful question is *which* CV said it differently.
                let entry = if reference.diverged {
                    format!("Open {} \u{2014} tailored there", reference.label)
                } else {
                    format!("Open {}", reference.label)
                };
                menu = menu.item(PopupMenuItem::new(entry).on_click(
                    move |_ev, _window, cx| {
                        let stem = stem.clone();
                        let _ = shell.update(cx, |this, cx| this.open_doc_by_stem(&stem, cx));
                    },
                ));
            }
            menu
        })
        .into_any_element()
    }

    /// First-run explanation of what the library is and how to fill it.
    pub(super) fn render_library_onboarding(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let shell = cx.weak_entity();
        let step = |n: &'static str, title: &'static str, body: &'static str| {
            div()
                .flex()
                .gap_3()
                .items_start()
                .child(
                    // `flex_none`, or the longest step squashes its own circle
                    // into an ellipse — a fixed `size` is still a flex basis.
                    div()
                        .flex_none()
                        .size(px(26.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(theme.elevated)
                        .border_1()
                        .border_color(theme.border)
                        .text_style(TextStyle::chip())
                        .text_color(theme.accent)
                        .child(n),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_style(TextStyle::control())
                                .text_color(theme.text)
                                .child(title),
                        )
                        .child(
                            // Sans, not `meta()`: this is a sentence to read,
                            // and mono is for data only (L-05).
                            div()
                                .text_style(TextStyle::prose())
                                .text_color(theme.text_muted)
                                .child(body),
                        ),
                )
        };

        div()
            .max_w(px(560.0))
            .flex()
            .flex_col()
            .gap_5()
            .p_8()
            .rounded(theme.radius_lg())
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_style(TextStyle::heading())
                            .text_color(theme.text)
                            .child("Build your reusable “me”"),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::prose())
                            .text_color(theme.text_muted)
                            .child(
                                "Your library is a personal pool of work, skills, education and \
                                 certificates. Fill it once, then assemble any tailored CV from \
                                 these blocks like Lego — no retyping.",
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(step(
                        "1",
                        "Write one here",
                        "New block asks for the fields that section needs. No CV required.",
                    ))
                    .child(step(
                        "2",
                        "Or lift one out of a CV",
                        "Press ★ on any work entry, skill or certificate to copy it here.",
                    ))
                    .child(step(
                        "3",
                        "Drop it into any CV",
                        "Reuse on a block copies it into another CV without leaving this screen.",
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("library-onboarding-new")
                            .action_primary()
                            .icon(IconName::Plus)
                            .label("New block")
                            .dropdown_menu(move |mut menu, _window, _cx| {
                                for (section, title) in POOLS {
                                    let shell = shell.clone();
                                    menu = menu.item(PopupMenuItem::new(title).on_click(
                                        move |_ev, window, cx| {
                                            let _ = shell.update(cx, |this, cx| {
                                                this.open_library_edit(section, None, window, cx);
                                            });
                                        },
                                    ));
                                }
                                menu
                            }),
                    )
                    .child(
                        Button::new("library-onboarding-cta")
                            .action_secondary()
                            .label("Browse CVs")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.screen = Screen::Gallery;
                                cx.notify();
                            })),
                    ),
            )
    }

    /// Append a copy of a library block to a document's **active** variant of
    /// that section, and write the file.
    ///
    /// A copy, deliberately: a library block is a copy pool
    /// (`CLAUDE.md`, data model invariants), and the `Linked`/`Detached`
    /// status the design row draws is a stored field with a migration and a
    /// push-to-all flow behind it (US-03, roadmap D2). Reuse is the half of
    /// that story the copy pool already supports honestly.
    ///
    /// The document is read, changed and written here rather than opened,
    /// because the point is to fill several CVs from one screen without
    /// leaving it. Nothing can be editing it at the same time: the rail is
    /// mounted on vault screens only, so no editor entity is alive while the
    /// Library is showing.
    pub(super) fn copy_block_into_document(
        &mut self,
        section: SectionKind,
        index: usize,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        let library = self.cache.library().clone();
        let mut doc = match vault::load(path) {
            Ok(doc) => doc,
            Err(message) => {
                save_status::report_unreadable(cx, path, message);
                cx.notify();
                return;
            }
        };

        use SectionKind::*;
        match section {
            Work => push_clone(library.work.get(index), doc.work.active_mut()),
            Education => push_clone(library.education.get(index), doc.education.active_mut()),
            Skills => push_clone(library.skills.get(index), doc.skills.active_mut()),
            Certificates => push_clone(
                library.certificates.get(index),
                doc.certificates.active_mut(),
            ),
            Organizations => push_clone(library.volunteer.get(index), doc.volunteer.active_mut()),
            // No pool for these — see `root.rs::save_block_to_library`.
            Profile | Custom(_) => return,
        }

        save_status::record(cx, "document", vault::save(&doc, path));
        cx.notify();
    }

    /// Open a document by its file stem — how the library's "used in N CVs"
    /// list and `Application::source_doc` both refer to one.
    pub(super) fn open_doc_by_stem(&mut self, stem: &str, cx: &mut Context<Self>) {
        let Some(path) = self
            .cache
            .metadata()
            .iter()
            .find(|m| m.stem == stem)
            .map(|m| m.path.clone())
        else {
            return;
        };
        self.open_doc(path, cx);
    }

    pub(super) fn remove_library_block(
        &mut self,
        section: SectionKind,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let mut library = vault::load_library(&vault);
        use SectionKind::*;
        match section {
            Work => remove_at(&mut library.work, index),
            Education => remove_at(&mut library.education, index),
            Skills => remove_at(&mut library.skills, index),
            Certificates => remove_at(&mut library.certificates, index),
            Organizations => remove_at(&mut library.volunteer, index),
            // No library pool for Profile or for custom sections (D-9).
            Profile | Custom(_) => {}
        }
        save_status::record(cx, "library", vault::save_library(&vault, &library));
        cx.notify();
    }
}


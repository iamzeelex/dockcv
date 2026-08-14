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
use dockcv_ui_components::{
    Button, ButtonExt, ButtonVariants, DropdownMenu, Icon, IconName, PopupMenuItem, Sizable, Tag, TextField,
};

use super::confirm;
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
}

impl BlockCard {
    /// Everything the card puts on screen, lowercased — what the search box
    /// matches against. Searching the text of blocks rather than their titles
    /// is the point of having a library at all (review P-14 / US-20); this
    /// covers what the card shows, not yet every bullet behind it.
    fn haystack(&self) -> String {
        format!(
            "{} {} {} {}",
            self.title,
            self.subtitle,
            self.body,
            self.keywords.join(" ")
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
            })
            .collect(),
        SectionKind::Profile | SectionKind::Custom(_) => Vec::new(),
    }
}

impl Shell {
    pub(super) fn render_library_screen(&self, cx: &mut Context<Self>) -> gpui::Div {
        let library = self.cache.library();
        let query = self.library_query(cx);

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
                let cards: Vec<BlockCard> = cards_for(library, section)
                    .into_iter()
                    .filter(|card| query.is_empty() || card.haystack().contains(&query))
                    .collect();
                if cards.is_empty() {
                    return None;
                }
                Some(
                    self.library_group(cx, title, section, cards)
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
            .flex()
            .flex_col()
            .child(self.library_header(cx, total))
            .child(
                div()
                    .id("library-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(34.0))
                    .pb(px(30.0))
                    .flex()
                    .flex_col()
                    .gap(px(18.0))
                    .children(self.library_helper(cx, total))
                    .children((total > 0).then(|| self.library_filter_chips(cx, &counts, total)))
                    .child(body),
            )
    }

    /// Title, block count, search and the "New block" menu.
    fn library_header(&self, cx: &mut Context<Self>, total: usize) -> impl IntoElement {
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
                    // The mockup's second figure here is "reused 14 times".
                    // Nothing records reuse, so only the count that is real
                    // gets drawn (see this module's header comment).
                    .child(div().mt(px(7.0)).flex().items_center().gap_2().child(
                        Tag::secondary().small().child(format!(
                            "{total} block{}",
                            if total == 1 { "" } else { "s" }
                        )),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.library_search_box(cx))
                    .child(
                        Button::new("new-block")
                            .cursor_pointer()
                            .header_primary()
                            .icon(IconName::Plus)
                            .label("New block")
                            .dropdown_menu(move |mut menu, _window, _cx| {
                                for (section, title) in POOLS {
                                    let shell = shell.clone();
                                    menu = menu.item(PopupMenuItem::new(title).on_click(
                                        move |_ev, _window, cx| {
                                            let _ = shell.update(cx, |this, cx| {
                                                this.add_library_block(section, cx);
                                            });
                                        },
                                    ));
                                }
                                menu
                            }),
                    ),
            )
    }

    fn library_search_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .w(px(230.0))
            .h(px(38.0))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .rounded(px(9.0))
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .child(
                Icon::new(IconName::Search)
                    .with_size(px(13.0))
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
        let theme = cx.theme().clone();
        Some(
            div()
                .flex()
                .items_center()
                .gap_3()
                .px_4()
                .py_3()
                .rounded_lg()
                .bg(theme.surface)
                .border_1()
                .border_color(theme.border)
                .child(
                    Icon::new(IconName::Star)
                        .with_size(px(14.0))
                        .text_color(theme.accent),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_style(TextStyle::body())
                        .text_color(theme.text_muted)
                        .child(
                            "Star any entry while editing a CV to drop it here — then reuse it \
                             in any other CV from the section's “★ From library” menu.",
                        ),
                )
                .child(
                    Button::new("library-helper-dismiss")
                        .icon(IconName::Close)
                        .ghost()
                        .xsmall()
                        .cursor_pointer()
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
        let theme = cx.theme().clone();
        div()
            .id(id.into())
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(10.0))
            .py(px(5.0))
            .rounded(px(7.0))
            .border_1()
            .cursor_pointer()
            .when(active, |el| {
                el.bg(theme.chip_bg)
                    .border_color(theme.chip_bg)
                    .text_color(theme.chip_fg)
            })
            .when(!active, |el| {
                el.bg(theme.chip_bg_neutral)
                    .border_color(theme.chip_bg_neutral)
                    .text_color(theme.text_muted)
                    .hover(|s| s.text_color(theme.text))
            })
            .text_style(TextStyle::control())
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
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
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
                    .cursor_pointer()
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .label("Add")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.add_library_block(section, cx);
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
                            .map(|card| self.block_card(cx, section, card)),
                    ),
            )
    }

    fn block_card(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
        card: BlockCard,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let index = card.index;
        let shell = cx.weak_entity();

        div()
            .w(px(320.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .relative()
            .px_4()
            .py_3()
            .rounded(px(11.0))
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .hover(|s| s.border_color(theme.accent))
            // Destructive action behind the "···" menu, never as a visible ✕
            // (review L-07 — the same fix the gallery card already carries).
            .child(
                Button::new(SharedString::from(format!("libmenu-{section:?}-{index}")))
                    .icon(IconName::Ellipsis)
                    .ghost()
                    .xsmall()
                    .cursor_pointer()
                    .tooltip("More")
                    .absolute()
                    .top(px(10.0))
                    .right(px(8.0))
                    .dropdown_menu(move |menu, _window, _cx| {
                        let shell = shell.clone();
                        menu.item(PopupMenuItem::new("Delete").on_click(
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
    }

    /// First-run explanation of what the library is and how to fill it.
    pub(super) fn render_library_onboarding(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let step = |n: &'static str, title: &'static str, body: &'static str| {
            div()
                .flex()
                .gap_3()
                .items_start()
                .child(
                    div()
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
                        .gap_1()
                        .child(
                            div()
                                .text_style(TextStyle::body())
                                .text_color(theme.text)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_style(TextStyle::meta())
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
            .rounded_xl()
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
                    .child(step("1", "Open a CV", "Pick any CV from the gallery."))
                    .child(step(
                        "2",
                        "Save a block with ★",
                        "Next to any work entry, skill or certificate, press ★ to add it here.",
                    ))
                    .child(step(
                        "3",
                        "Reuse it anywhere",
                        "In another CV, use “★ From library” to drop the block in.",
                    )),
            )
            .child(
                Button::new("library-onboarding-cta")
                    .cursor_pointer()
                    .primary()
                    .label("Browse CVs")
                    .self_start()
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.screen = Screen::Gallery;
                        cx.notify();
                    })),
            )
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


//! Diary screen — the running journal of wins, "catch them before you forget".
//!
//! Design row: `.design/rows/row_the_diary.txt`. Renders the **main pane
//! only**; the rail around it is mounted by `Shell::with_rail`, and the rail's
//! `Roles` facet — which belongs to this screen alone — lives in `sidebar.rs`.
//!
//! Two things the row draws are deliberately absent. The header's
//! **`6-week streak`** is gone because the review kills it by name (P-20: a
//! streak in a career tool is cheap gamification that punishes the week you
//! miss); US-12 replaces it with role coverage, which the rail's `Roles` list
//! now shows. The entry's **`↓ 50% p99` metric chip** is not drawn because
//! nothing may extract a number from prose and present it as fact (US-14) —
//! that is the AI layer's job, behind review.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement, SharedString};

use dockcv_ui_components::{ScrollableElement, 
    Button, ButtonExt, DropdownMenu, Icon, IconName, PopupMenuItem, Sizable, Tag, TextField,
};

use crate::resume::model::{Diary, DiaryEntry};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::confirm;
use super::shell::Shell;

/// `2026-06-18` → `(2026, 6, 18)`. The vault writes this format itself
/// (`vault::today_iso`), so anything else is a hand-edited file, and the
/// screen shows what it can rather than hiding the entry.
fn parse_iso(date: &str) -> Option<(i32, u32, u32)> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    (1..=12).contains(&month).then_some((year, month, day))
}

const MONTHS: [(&str, &str); 12] = [
    ("JAN", "January"),
    ("FEB", "February"),
    ("MAR", "March"),
    ("APR", "April"),
    ("MAY", "May"),
    ("JUN", "June"),
    ("JUL", "July"),
    ("AUG", "August"),
    ("SEP", "September"),
    ("OCT", "October"),
    ("NOV", "November"),
    ("DEC", "December"),
];

impl Shell {
    pub(super) fn render_diary_screen(&self, cx: &mut Context<Self>) -> gpui::Div {
        let diary = self.cache.diary();

        // Newest first, whatever order the file happens to be in — ISO dates
        // sort correctly as strings, which is half of why the vault writes them.
        let mut entries: Vec<(usize, DiaryEntry)> =
            diary.entries.iter().cloned().enumerate().collect();
        entries.sort_by(|(_, a), (_, b)| b.date.cmp(&a.date));

        let total = entries.len();
        let query = self.diary_query(cx);
        let shown: Vec<(usize, DiaryEntry)> = entries
            .into_iter()
            .filter(|(_, e)| {
                self.diary_role_filter
                    .as_ref()
                    .is_none_or(|role| &e.role == role)
            })
            .filter(|(_, e)| matches_diary_query(e, &query))
            .collect();

        let body: AnyElement = if total == 0 {
            self.render_diary_empty(cx).into_any_element()
        } else if shown.is_empty() {
            div()
                .text_style(TextStyle::body())
                .text_color(cx.theme().text_muted)
                .child(if query.is_empty() {
                    "No wins logged for this role yet."
                } else {
                    // The search found nothing — say which of the two filters
                    // is responsible rather than leaving the user to guess.
                    "No wins match that search."
                })
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(26.0))
                .children(self.render_timeline(cx, shown))
                .into_any_element()
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .child(self.diary_header(cx, total))
            .child(
                div()
                    .id("diary-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .px(px(34.0))
                    .pb(px(30.0))
                    .flex()
                    .flex_col()
                    .gap(px(24.0))
                    .child(self.quick_capture(cx, diary))
                    .child(body),
            )
            .children(self.render_diary_use_sheet(cx))
            .children(self.render_diary_paste_sheet(cx))
    }

    fn diary_header(&self, cx: &mut Context<Self>, total: usize) -> impl IntoElement {
        div()
            .flex()
            .items_end()
            .justify_between()
            .gap_4()
            .px(px(34.0))
            .pt(px(30.0))
            .pb(px(20.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_style(TextStyle::title())
                            .text_color(cx.theme().text)
                            .child("Diary"),
                    )
                    .child(
                        div().mt(px(7.0)).flex().items_center().gap_2().child(
                            Tag::secondary()
                                .small()
                                .child(format!("{total} win{} logged", plural(total))),
                        ),
                    ),
            )
            .child(self.diary_search_box(cx))
    }

    /// Search across an entry's text, role and tags.
    ///
    /// US-20 asks for it and the screen needed it more than any other: the
    /// diary's whole promise is that in March you can find what you fixed in
    /// October, and a year of entries behind a month-by-month scroll is not
    /// findable. The role facet in the rail narrows by *who you were*; this
    /// narrows by *what happened*.
    fn diary_search_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        div()
            .w(px(230.0))
            .h(px(34.0))
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
                    self.diary_search
                        .as_ref()
                        .map(|state| TextField::new(state).seamless().placeholder("Search wins")),
                ),
            )
    }

    /// The diary search query, lowercased and trimmed.
    pub(super) fn diary_query(&self, cx: &gpui::App) -> String {
        self.diary_search
            .as_ref()
            .map(|f| f.read(cx).value(cx).trim().to_lowercase())
            .unwrap_or_default()
    }

    /// The design's capture block: one line about the week, the role it belongs
    /// to, its tags, and a button. `⏎` in either text box commits, so the
    /// keyboard path the persona actually uses never needs the mouse.
    fn quick_capture(&self, cx: &mut Context<Self>, diary: &Diary) -> impl IntoElement {
        let theme = *cx.theme();
        let roles = self.known_roles(diary);
        let shell = cx.weak_entity();
        let role_label = if self.diary_role.is_empty() {
            "No role".to_string()
        } else {
            self.diary_role.clone()
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded(theme.radius_lg())
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .child(div().w_full().children(self.diary_draft.as_ref().map(|state| {
                TextField::new(state).placeholder("What did you ship, fix, or learn this week?")
            })))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("diary-role")
                            .selector_inline()
                            .label(role_label)
                            .tooltip("Which role this win belongs to")
                            .dropdown_menu(move |mut menu, _window, _cx| {
                                let clear = shell.clone();
                                menu = menu.item(PopupMenuItem::new("No role").on_click(
                                    move |_ev, _window, cx| {
                                        let _ = clear.update(cx, |this, cx| {
                                            this.diary_role.clear();
                                            cx.notify();
                                        });
                                    },
                                ));
                                for role in roles.clone() {
                                    let shell = shell.clone();
                                    let picked = role.clone();
                                    menu = menu.item(PopupMenuItem::new(role).on_click(
                                        move |_ev, _window, cx| {
                                            let picked = picked.clone();
                                            let _ = shell.update(cx, |this, cx| {
                                                this.diary_role = picked;
                                                cx.notify();
                                            });
                                        },
                                    ));
                                }
                                menu
                            }),
                    )
                    .child(
                        div()
                            .w(px(190.0))
                            .h(px(30.0))
                            .flex()
                            .items_center()
                            .px_2()
                            .rounded(theme.radius_sm())
                            .bg(theme.elevated)
                            .border_1()
                            .border_color(theme.border)
                            .children(
                                self.diary_tags
                                    .as_ref()
                                    .map(|s| TextField::new(s).seamless().placeholder("# tag")),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        // The way in for the person the review says will not
                        // keep a diary: they already wrote this week's status
                        // to their manager. Beside "Log win", not hidden in a
                        // menu, because for that person it is the *more*
                        // likely of the two.
                        Button::new("diary-paste")
                            .toolbar()
                            .label("Paste a status…")
                            .tooltip("Find wins in something you already wrote")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.open_diary_paste(window, cx);
                            })),
                    )
                    .child(
                        Button::new("diary-log-win")
                            .toolbar_primary()
                            .label("Log win")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.commit_diary_entry(window, cx);
                            })),
                    ),
            )
    }

    /// Entries under `June 2026` headers, newest month first.
    fn render_timeline(
        &self,
        cx: &mut Context<Self>,
        entries: Vec<(usize, DiaryEntry)>,
    ) -> Vec<AnyElement> {
        let theme = *cx.theme();
        let mut months: Vec<AnyElement> = Vec::new();
        let mut current: Option<(i32, u32)> = None;
        let mut bucket: Vec<AnyElement> = Vec::new();

        // Flushing on change rather than grouping into a map keeps the months
        // in the order the sorted entries already put them in.
        let flush = |label: Option<(i32, u32)>, bucket: &mut Vec<AnyElement>| {
            if bucket.is_empty() {
                return None;
            }
            let heading = label
                .map(|(year, month)| format!("{} {year}", MONTHS[(month - 1) as usize].1))
                .unwrap_or_else(|| "Undated".to_string());
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_style(TextStyle::eyebrow())
                            .text_color(theme.text_subtle)
                            .child(TextStyle::eyebrow().apply_case(&heading)),
                    )
                    .children(std::mem::take(bucket))
                    .into_any_element(),
            )
        };

        for (index, entry) in entries {
            let label = parse_iso(&entry.date).map(|(y, m, _)| (y, m));
            if label != current {
                if let Some(month) = flush(current, &mut bucket) {
                    months.push(month);
                }
                current = label;
            }
            bucket.push(self.entry_card(cx, index, entry).into_any_element());
        }
        if let Some(month) = flush(current, &mut bucket) {
            months.push(month);
        }
        months
    }

    fn entry_card(
        &self,
        cx: &mut Context<Self>,
        index: usize,
        entry: DiaryEntry,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        let shell = cx.weak_entity();
        let is_confidential = entry.confidential;
        let (day, month) = match parse_iso(&entry.date) {
            Some((_, m, d)) => (format!("{d:02}"), MONTHS[(m - 1) as usize].0.to_string()),
            None => ("--".to_string(), "···".to_string()),
        };

        div()
            .flex()
            .items_start()
            .gap_4()
            .relative()
            .px_4()
            .py_3()
            .rounded(theme.radius_lg())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            // Date block: a day and a month, both data, both mono.
            .child(
                div()
                    .w(px(38.0))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(day),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::chip())
                            .text_color(theme.text_subtle)
                            .child(month),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .pr(px(24.0)) // clears the "···" trigger
                    .children((!entry.role.is_empty()).then(|| {
                        div()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(entry.role.clone())
                    }))
                    .child(
                        div()
                            .text_style(TextStyle::prose())
                            .text_color(theme.text)
                            .child(entry.text.clone()),
                    )
                    .children((!entry.tags.is_empty()).then(|| {
                        div().flex().flex_wrap().gap(px(5.0)).children(
                            entry
                                .tags
                                .iter()
                                .map(|tag| Tag::secondary().small().child(format!("#{tag}"))),
                        )
                    }))
                    // P-05: an entry captured from a CV knows which one. The
                    // mockup never draws this, but the data is the whole point
                    // of `source_doc` and hiding it wastes it.
                    .children(entry.source_doc.as_ref().map(|doc| {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(
                                Icon::new(IconName::File)
                                    .with_size(cx.theme().icon_sm())
                                    .text_color(theme.text_subtle),
                            )
                            .child(format!("captured from {doc}"))
                    }))
                    .children(entry.confidential.then(|| {
                        // Visible on the entry, not only in the menu: the
                        // author has to be able to see at a glance which of a
                        // year's wins carry something that must not leave the
                        // vault as written (US-36).
                        // `flex` so the chip hugs its text: the parent is a
                        // column, and a bare child stretches to its width — a
                        // one-word mark drawn as a full-width bar reads as an
                        // error banner, not a label.
                        div().mt(px(6.0)).flex().child(
                            Tag::custom(theme.warning.opacity(0.16), theme.warning, theme.warning)
                                .px(px(7.0))
                                .py(px(2.0))
                                .rounded(theme.radius_sm())
                                .text_style(TextStyle::chip())
                                .child("confidential"),
                        )
                    }))
                    // The mockup draws `Use in a CV →` on every entry and it
                    // was the one link with nothing behind it (P-05). A diary
                    // you can only write into is a diary you stop keeping.
                    .child(
                        div()
                            .mt(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                Button::new(SharedString::from(format!("diary-use-{index}")))
                                    .quiet()
                                    .label("Use in a CV →")
                                    .tooltip("Add this win as a bullet of a CV")
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, window, cx| {
                                            this.open_diary_use(index, window, cx);
                                        },
                                    )),
                            )
                            .children((!entry.used_in.is_empty()).then(|| {
                                // Recorded rather than derived — a bullet is a
                                // bare string, so nothing in a document can be
                                // matched back to the entry it came from.
                                div()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_subtle)
                                    .child(format!("used in {}", entry.used_in.join(", ")))
                            })),
                    ),
            )
            // Positioned on the wrapper, not on the trigger — see the note on
            // the library card. A `dropdown_menu` trigger that positions
            // itself paints correctly and never opens.
            .child(
                div().absolute().top(px(10.0)).right(px(8.0)).child(
                    Button::new(SharedString::from(format!("diary-menu-{index}")))
                        .icon_only()
                        .icon(IconName::Ellipsis)
                        .tooltip("More")
                        .dropdown_menu(move |menu, _window, _cx| {
                            let shell = shell.clone();
                            let shell_mark = shell.clone();
                            menu.item(
                                PopupMenuItem::new(if is_confidential {
                                    "Remove confidential mark"
                                } else {
                                    "Mark confidential"
                                })
                                .on_click(move |_ev, _window, cx| {
                                    let _ = shell_mark.update(cx, |this, cx| {
                                        this.toggle_diary_confidential(index, cx);
                                    });
                                }),
                            )
                            .separator()
                            .item(PopupMenuItem::new("Delete").on_click(
                                move |_ev, window, cx| {
                                    let _ = shell.update(cx, |_this, cx| {
                                        confirm::destructive(
                                            "Delete this diary entry?".into(),
                                            format!(
                                                "{} A diary entry is the record a CV bullet \
                                                 is drawn from, and nothing else holds it.",
                                                confirm::CANNOT_UNDO
                                            ),
                                            "Delete",
                                            window,
                                            cx,
                                            move |this, _window, cx| {
                                                this.delete_diary_entry(index, cx)
                                            },
                                        );
                                    });
                                },
                            ))
                        }),
                ),
            )
    }

    fn render_diary_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .max_w(px(560.0))
            .child(
                div()
                    .text_style(TextStyle::heading())
                    .text_color(theme.text)
                    .child("No wins logged yet"),
            )
            .child(
                div()
                    .text_style(TextStyle::prose())
                    .text_color(theme.text_muted)
                    .child(
                        "Log one the moment it happens — what you shipped, fixed or learned, \
                         and the numbers while you still remember them. In six months this is \
                         the only record of what you actually did.",
                    ),
            )
    }

    /// Roles the user could tag a win with: the ones already used in the diary,
    /// plus every role in their library's work pool. Both come from data they
    /// entered themselves — this list never invents an employer.
    pub(super) fn known_roles(&self, diary: &Diary) -> Vec<String> {
        let mut roles: Vec<String> = Vec::new();
        let mut push = |role: String| {
            if !role.is_empty() && !roles.contains(&role) {
                roles.push(role);
            }
        };

        for entry in &diary.entries {
            push(entry.role.clone());
        }
        for work in &self.cache.library().work {
            let (employer, position) = (work.name.trim(), work.position.trim());
            match (employer.is_empty(), position.is_empty()) {
                (false, false) => push(format!("{employer} · {position}")),
                (false, true) => push(employer.to_string()),
                (true, false) => push(position.to_string()),
                (true, true) => {}
            }
        }
        roles
    }

    /// `role -> wins logged`, newest-first by first appearance, for the rail's
    /// `Roles` facet. This is US-12's replacement for the streak: which roles
    /// are covered, not how many weeks in a row you showed up.
    pub(super) fn role_counts(&self, _cx: &mut Context<Self>) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for entry in &self.cache.diary().entries {
            if entry.role.is_empty() {
                continue;
            }
            match counts.iter_mut().find(|(role, _)| role == &entry.role) {
                Some((_, count)) => *count += 1,
                None => counts.push((entry.role.clone(), 1)),
            }
        }
        counts
    }
}

/// Whether an entry matches the search box.
///
/// Text, role and tags together: the phrase you remember could be in any of
/// them, and a search that only read the prose would miss `#reliability` —
/// which is exactly the kind of thing people tag *so they can find it later*.
pub(super) fn matches_diary_query(entry: &DiaryEntry, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    entry.text.to_lowercase().contains(query)
        || entry.role.to_lowercase().contains(query)
        || entry.tags.iter().any(|t| t.to_lowercase().contains(query))
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::{matches_diary_query, parse_iso, MONTHS};
    use crate::resume::model::DiaryEntry;
    use crate::views::shell::parse_tags;

    /// The month index reaches straight into `MONTHS`, so a bad month must
    /// never come back as `Some` — that would be an out-of-bounds panic on a
    /// hand-edited `diary.toml`, which is a file the user is invited to edit.
    #[test]
    fn search_reads_the_text_the_role_and_the_tags() {
        let entry = DiaryEntry {
            date: "2026-06-18".into(),
            text: "Cut p99 latency in half".into(),
            role: "Acme Corp · Senior SWE".into(),
            tags: vec!["performance".into(), "architecture".into()],
            ..Default::default()
        };

        assert!(matches_diary_query(&entry, ""), "an empty query matches all");
        assert!(matches_diary_query(&entry, "p99"));
        assert!(matches_diary_query(&entry, "acme"), "role is searchable");
        // The reason tags exist is to be findable later.
        assert!(matches_diary_query(&entry, "architecture"));
        assert!(!matches_diary_query(&entry, "kafka"));
    }

    #[test]
    fn a_date_the_month_table_cannot_index_is_rejected() {
        assert_eq!(parse_iso("2026-06-18"), Some((2026, 6, 18)));
        assert_eq!(parse_iso("2026-13-01"), None);
        assert_eq!(parse_iso("2026-00-01"), None);
        assert_eq!(parse_iso("not a date"), None);
        assert_eq!(parse_iso(""), None);

        for month in 1..=12u32 {
            let date = format!("2026-{month:02}-01");
            let (_, parsed, _) = parse_iso(&date).expect("every real month parses");
            // The indexing the card does, proven not to panic.
            let _ = MONTHS[(parsed - 1) as usize];
        }
    }

    #[test]
    fn tags_lose_their_hash_and_their_duplicates() {
        assert_eq!(
            parse_tags("#performance, #architecture"),
            vec!["performance", "architecture"]
        );
        // Separator the user reached for, whichever it was.
        assert_eq!(parse_tags("perf architecture"), vec!["perf", "architecture"]);
        // Same tag twice is a typo, not two facts — and case is not a
        // distinction between tags.
        assert_eq!(parse_tags("#perf #PERF perf"), vec!["perf"]);
        assert!(parse_tags("   ").is_empty());
        assert!(parse_tags("#").is_empty());
    }
}

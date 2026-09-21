//! Drawing the front door. The model and its rules are in `front_door.rs`,
//! which stays free of `gpui` so they can be tested without a window.
//!
//! The unit on screen is a **document**, drawn as one card holding its
//! versions. That is the fix for a list where a draft and a version were the
//! same rectangle: the model is that a CV owns versions, and until the card
//! said so, the only way to know which object a `···` would act on was to open
//! it and read. A card with more than one version gets a strip naming the CV
//! and a row per version under it; a card with one gets a single row carrying
//! the whole address, because a strip over one row printed the same thing
//! twice.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, Div, FontWeight, Hsla, SharedString, Stateful};

use dockcv_ui_components::{
    Button, ButtonExt, DropdownMenu, Icon, IconName, PopupMenuItem, Sizable, TextField, MONO, SANS,
};

use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;

use super::{groups, history_line, is_generated_name, page_count_label, Group, Reading};
use crate::views::gallery_sort::sort_documents;
use crate::views::shell::Shell;

/// How far a version's name sits in from the card's edge: past the CV glyph, so
/// the versions read as being *under* the document rather than beside it.
const VERSION_INDENT: f32 = 40.0;

impl Shell {
    /// The list of what this vault can send.
    pub(crate) fn render_readings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search_query(cx);
        let mut metas: Vec<vault::DocMeta> = self
            .cache
            .metadata()
            .iter()
            .filter(|m| query.is_empty() || m.best_match(&query).is_some())
            .cloned()
            .collect();
        sort_documents(&mut metas, self.gallery_sort, self.cache.applications());

        let all = groups(&metas);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let theme = *cx.theme();
        let mut children: Vec<AnyElement> = Vec::new();
        for group in all.iter() {
            let meta = metas.iter().find(|m| m.path == group.path);
            children.push(
                self.render_group(cx, group, meta, &query, now)
                    .into_any_element(),
            );
        }

        if all.is_empty() {
            children.push(
                div()
                    .py(px(28.0))
                    .text_style(TextStyle::body())
                    .text_color(theme.text_muted)
                    .child("Nothing here matches.")
                    .into_any_element(),
            );
        }

        // A block is not a reading, so it is not a row — but a query that finds
        // nothing here and six things in the Library should say so rather than
        // reading as "no results".
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
            .gap(px(10.0))
            .children(children)
            .children(library_row)
    }

    /// One CV, with everything it can send inside it.
    fn render_group(
        &self,
        cx: &mut Context<Self>,
        group: &Group,
        meta: Option<&vault::DocMeta>,
        query: &str,
        now: u64,
    ) -> Div {
        let theme = *cx.theme();
        // Where you were. Seven identical cards gave the eye nothing to land
        // on; this is the CV the app would reopen on its own. It marks the
        // *document*, because that is what `last_opened` knows.
        let here = self.last_opened.as_deref() == Some(group.path.as_path());

        let card = div()
            .flex()
            .flex_col()
            .rounded(theme.radius_md())
            .border_1()
            .border_color(if here { theme.accent } else { theme.border })
            .bg(theme.surface);

        if !group.needs_header() {
            return card.child(
                self.render_reading(cx, &group.readings[0], meta, query, now, true)
                    .rounded(theme.radius_md()),
            );
        }

        let last = group.readings.len() - 1;
        card.child(self.render_document_strip(cx, group))
            .child(div().h(px(1.0)).bg(theme.border))
            .children(group.readings.iter().enumerate().map(|(i, row)| {
                self.render_reading(cx, row, meta, query, now, false)
                    .when(i == last, |r| r.rounded_b(theme.radius_md()))
            }))
    }

    /// The strip over a CV's versions: whose versions these are.
    ///
    /// Its right edge carries the count and nothing else. The CV's actions hang
    /// off the CV's *name* — see `front_door/menus.rs` — so there is no second
    /// `···` here to line up under the rows' own and mean something different.
    fn render_document_strip(&self, cx: &mut Context<Self>, group: &Group) -> Div {
        let theme = *cx.theme();
        let count = group.readings.len();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .pl(px(15.0))
            .pr(px(15.0))
            .py(px(10.0))
            .child(document_glyph(theme))
            .child(self.render_document_name(cx, &group.path, &group.stem, true))
            .child(div().flex_1().min_w(px(20.0)))
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.0))
                    .text_color(theme.text_subtle)
                    .child(format!("{count} versions")),
            )
    }

    /// One reading.
    ///
    /// The whole row opens it, at that version. The gallery's card learned this
    /// the hard way — three regions with three behaviours reads as "some of
    /// this is clickable and I have to find out which" — so there is one target
    /// and everything that competes for the click is a labelled control inside
    /// it.
    ///
    /// `standalone` is a row that is its whole card, which happens for a draft
    /// and for a CV with exactly one version. It carries the CV's name itself,
    /// since there is no strip above it to do so.
    fn render_reading(
        &self,
        cx: &mut Context<Self>,
        row: &Reading,
        meta: Option<&vault::DocMeta>,
        query: &str,
        now: u64,
        standalone: bool,
    ) -> Stateful<Div> {
        let theme = *cx.theme();
        let path = row.path.clone();
        let index = row.preset.as_ref().map(|(i, _)| *i);
        // Why this row is in the results, when the reason is not already on it
        // (E1). A query that matched a variant name or a job title shows an
        // otherwise unremarkable row, and a result that cannot explain itself
        // is the flat list E1 exists to end. Suppressed when the thing that
        // matched is the row's own name, which would be explaining the obvious.
        let matched = meta
            .filter(|_| !query.is_empty())
            .and_then(|meta| meta.best_match(query))
            .filter(|entry| entry.text != row.label())
            .and_then(|entry| entry.kind.label().map(|k| (k, entry.text.clone())));

        div()
            .id(SharedString::from(format!(
                "reading-{}-{}",
                row.stem,
                row.label()
            )))
            .flex()
            .items_center()
            .gap(px(12.0))
            .min_h(px(if standalone { 66.0 } else { 58.0 }))
            .pl(px(if standalone { 15.0 } else { VERSION_INDENT }))
            .pr(px(12.0))
            .py(px(11.0))
            // A bar rather than a shift. The card used to step two pixels
            // toward the pointer, which cannot work for a row inside a card —
            // the row would slide out of its own container.
            .border_l_2()
            .border_color(Hsla::transparent_black())
            .cursor_pointer()
            .hover(|s| s.bg(theme.elevated).border_color(theme.accent))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_doc_at(path.clone(), index, window, cx);
            }))
            .children((standalone && row.is_draft()).then(|| document_glyph(theme)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(self.render_title_line(cx, row, standalone))
                    .when_some(row.subtitle(), |title, subtitle| {
                        title.child(
                            div()
                                .text_size(px(11.5))
                                .text_color(theme.text_muted)
                                .child(subtitle),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_subtle)
                            .child(history_line(
                                self.cache
                                    .applications()
                                    .record_for(row.sent_as().0, row.sent_as().1),
                                row.modified_secs,
                                now,
                            )),
                    ),
            )
            .when_some(matched, |r, (kind, text)| {
                r.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .child(
                            div()
                                .text_style(TextStyle::eyebrow())
                                .text_color(theme.accent)
                                .child(kind),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(theme.text_subtle)
                                .child(text),
                        ),
                )
            })
            .child(div().flex_1().min_w_0())
            .child(self.render_length(cx, row))
            // A draft has no version, so there is nothing here that a version
            // menu could act on — and its absence is itself the signal that
            // this row is the CV rather than one reading of it.
            .children(
                row.preset
                    .as_ref()
                    .map(|(index, name)| self.render_version_menu(cx, &row.path, *index, name)),
            )
    }

    /// The row's first line: what it is, and — when no strip says so — whose.
    fn render_title_line(
        &self,
        cx: &mut Context<Self>,
        row: &Reading,
        standalone: bool,
    ) -> AnyElement {
        let theme = *cx.theme();
        // A draft's title *is* the CV's name, so the name carries its own menu
        // and there is no version half to follow it.
        if row.is_draft() {
            return div()
                .flex()
                .items_center()
                .child(self.render_document_name(cx, &row.path, &row.stem, true))
                .into_any_element();
        }

        let mut line = div().flex().flex_wrap().items_center().gap(px(2.0));
        if standalone {
            // `imported-5 › Northwind`, which is the address the strip would
            // have given from above. The CV half is quiet and is the CV's menu;
            // the version half is the title and is the version's.
            line = line
                .child(self.render_document_name(cx, &row.path, &row.stem, false))
                .child(
                    div()
                        .px(px(4.0))
                        .text_size(px(11.5))
                        .text_color(theme.text_subtle)
                        .child("›"),
                );
        }
        line.child(self.render_version_name(cx, row)).into_any_element()
    }

    /// The version's own name, or the box replacing it.
    fn render_version_name(&self, cx: &mut Context<Self>, row: &Reading) -> AnyElement {
        let theme = *cx.theme();
        let Some((index, name)) = row.preset.clone() else {
            return div().into_any_element();
        };

        let renaming = self
            .renaming_version
            .as_ref()
            .filter(|r| r.path == row.path && r.index == index);
        if let Some(rename) = renaming {
            return div()
                .w(px(240.0))
                .child(TextField::new(&rename.field))
                .into_any_element();
        }

        if is_generated_name(&name) {
            // The placeholder is the control that replaces it. An `unnamed`
            // chip used to sit beside it saying the same thing in a second
            // word; one mark that leads somewhere beats two that agree.
            let path = row.path.clone();
            return Button::new(SharedString::from(format!(
                "name-version-{}-{index}",
                row.stem
            )))
            .quiet()
            // Cancels the rung's padding, so a placeholder title starts on the
            // same pixel as a real one instead of six to its right.
            .ml(px(-6.0))
            .tooltip("Give this version a name")
            .on_click(
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.start_version_rename(&path, index, window, cx);
                }),
            )
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(14.0))
                    .text_color(theme.text_muted)
                    .child(name),
            )
            .into_any_element();
        }

        div()
            .font_family(SANS)
            .text_size(px(14.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(if row.unreadable {
                theme.danger
            } else {
                theme.text
            })
            .child(name)
            .into_any_element()
    }

    /// `+ Add a CV`, and the two ways one arrives.
    ///
    /// Import leads, because that is how most people get here: the review's
    /// US-01 is a person who already has a CV and does not want to start at an
    /// empty screen. A blank document is the rarer intent, so it is second
    /// rather than the default the old `+ New CV` assumed.
    pub(crate) fn render_add_cv(&self, cx: &mut Context<Self>) -> AnyElement {
        let shell = cx.weak_entity();
        Button::new("add-cv")
            .action_primary()
            .icon(IconName::Plus)
            .label("Add a CV")
            .dropdown_menu(move |menu, _window, _cx| {
                let importing = shell.clone();
                let blank = shell.clone();
                menu.item(
                    PopupMenuItem::new("Import a file…").on_click(
                        move |_ev, _window, cx| {
                            let _ = importing.update(cx, |this, cx| this.open_import(cx));
                        },
                    ),
                )
                .item(
                    PopupMenuItem::new("Start from scratch").on_click(
                        move |_ev, _window, cx| {
                            let _ = blank.update(cx, |this, cx| this.start_blank_cv(cx));
                        },
                    ),
                )
            })
            .into_any_element()
    }

    /// The banner: where a version gets made.
    ///
    /// It used to compete with a `Tailor for a job` button in the header, which
    /// is why one of them had to go. The header kept the action that is about
    /// the *vault* — bringing a CV in — and this kept the one that is about the
    /// next application. They are different jobs and now they look it.
    ///
    /// The sentence under the heading is not decoration: "your saved source
    /// stays unchanged" is the promise that makes the flow safe to try, and a
    /// bare button cannot make it. This is also where the local-agent flow will
    /// hang, which is the other reason it is a panel and not a button.
    pub(crate) fn render_tailor_callout(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        div()
            .mb(px(24.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(14.0))
            .px(px(17.0))
            .py(px(15.0))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(theme.accent.opacity(0.45))
            .bg(theme.accent.opacity(0.07))
            .child(
                div()
                    .text_size(px(18.0))
                    .text_color(theme.accent)
                    .child("✦"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(220.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("Make the next application specific."),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(
                                "Create a new version from a safe starting point. Your saved \
                                 source stays unchanged.",
                            ),
                    ),
            )
            .child(
                Button::new("callout-tailor")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_tailor(window, cx);
                    }))
                    .child("Create a version  →"),
            )
    }

    /// Whose CVs these are.
    ///
    /// The person's name when the vault agrees on one, which is the ordinary
    /// case — a vault is one person's documents. When it does not agree (a
    /// shared machine, a vault of samples), naming one of them would be wrong,
    /// so the screen falls back to saying what it is.
    pub(crate) fn front_door_title(&self) -> String {
        let mut names = self
            .cache
            .metadata()
            .iter()
            .filter(|meta| !meta.unreadable)
            .map(|meta| meta.name.trim())
            .filter(|name| !name.is_empty());
        match names.next() {
            Some(first) if names.all(|other| other == first) => first.to_string(),
            _ => "Your CVs".to_string(),
        }
    }

    /// How many pages, in the smallest form that still says it.
    fn render_length(&self, cx: &mut Context<Self>, row: &Reading) -> Div {
        let theme = *cx.theme();
        let geometry = self
            .reading_pages
            .get(&(row.path.clone(), row.preset.clone().map(|(_, n)| n)));

        div()
            .flex_none()
            .font_family(MONO)
            .text_size(px(11.0))
            .text_color(theme.text_subtle)
            .child(page_count_label(geometry))
    }
}

/// The mark that means "a CV" wherever one is named.
///
/// One glyph, used in exactly one place per card, so the eye can find the
/// document without reading: on the strip over a set of versions, and on a
/// draft's own row. A version never gets it.
fn document_glyph(theme: crate::theme::Theme) -> impl IntoElement {
    Icon::new(IconName::File)
        .with_size(theme.icon_sm())
        .text_color(theme.text_subtle)
}

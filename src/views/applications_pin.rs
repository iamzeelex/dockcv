//! Choosing which CV an application was sent with.
//!
//! This used to be part of the card's `···` menu: one `Pin CV: <label>` item
//! per document × preset, flat, at the top level. A vault with six CVs and
//! three presets each put eighteen items in a menu that also holds four column
//! moves and a delete — and the list has no ceiling, because presets are
//! exactly the thing this app encourages you to make more of.
//!
//! So the menu keeps one item and the choosing happens here. A sheet is the
//! right shape for three reasons the menu could not manage:
//!
//! * It **groups**. Documents are drawn once, presets under them, so the list
//!   grows with the number of CVs rather than with CVs × presets.
//! * It **filters**. Past a handful of documents, typing two letters beats
//!   reading, and a menu has nowhere to type.
//! * It **says what is pinned now**, and offers to unpin. A menu of pin
//!   actions can only add; there was no way to take a wrong pin back.
//!
//! Pinning is one click — the row *is* the action, so there is no Save button
//! to hunt for after picking.

use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, ClickEvent, Context, Entity, IntoElement, SharedString, Subscription,
    Window,
};

use dockcv_ui_components::{Button, ButtonExt, TextField, TextFieldEvent, TextFieldState};

use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::applications_snapshot::pin_groups;
use super::shell::Shell;

/// The open CV picker.
pub(super) struct PinPick {
    /// Position in `Applications::entries` — the same identity the menu
    /// addresses.
    pub index: usize,
    /// Who the application is to, for the sheet's subtitle.
    pub company: String,
    /// What is pinned right now: file stem and preset name.
    pub current: Option<(String, String)>,
    pub filter: Entity<TextFieldState>,
    _subscription: Subscription,
}

impl Shell {
    pub(super) fn open_pin_pick(
        &mut self,
        index: usize,
        company: String,
        current: Option<(String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filter = cx.new(|cx| TextFieldState::single_line(window, cx));
        let subscription = cx.subscribe(&filter, |_this, _field, _event: &TextFieldEvent, cx| {
            cx.notify()
        });
        self.pin_pick = Some(PinPick {
            index,
            company,
            current,
            filter,
            _subscription: subscription,
        });
        cx.notify();
    }

    pub(super) fn close_pin_pick(&mut self, cx: &mut Context<Self>) {
        self.pin_pick = None;
        cx.notify();
    }

    fn commit_pin_pick(&mut self, stem: String, preset: String, cx: &mut Context<Self>) {
        let Some(pick) = self.pin_pick.as_ref() else {
            return;
        };
        let index = pick.index;
        self.pin_pick = None;
        self.pin_application_cv(index, stem, preset, cx);
    }

    pub(super) fn render_pin_pick_sheet(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pick = self.pin_pick.as_ref()?;
        let theme = cx.theme().clone();
        let query = pick.filter.read(cx).value(cx).trim().to_lowercase();

        // A document matches on its own name, or on any preset's — typing
        // "concise" should find the CV that has a concise cut, not nothing.
        let groups: Vec<_> = pin_groups(self.cache.metadata())
            .into_iter()
            .filter_map(|group| {
                if query.is_empty() || group.label.to_lowercase().contains(&query) {
                    return Some(group);
                }
                let presets: Vec<String> = group
                    .presets
                    .iter()
                    .filter(|p| p.to_lowercase().contains(&query))
                    .cloned()
                    .collect();
                (!presets.is_empty()).then_some(super::applications_snapshot::PinGroup {
                    presets,
                    ..group
                })
            })
            .collect();

        let current = pick.current.clone();
        let row = |stem: String,
                   preset: String,
                   label: SharedString,
                   indented: bool,
                   cx: &mut Context<Self>|
         -> AnyElement {
            let pinned = current.as_ref().is_some_and(|(s, p)| *s == stem && *p == preset);
            let click_stem = stem.clone();
            let click_preset = preset.clone();
            div()
                .id(SharedString::from(format!("pin-row-{stem}-{preset}")))
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_2()
                .when(indented, |el| el.ml(px(10.0)))
                .py(px(6.0))
                .rounded_md()
                .cursor_pointer()
                .when(pinned, |el| el.bg(theme.selected))
                .hover(|s| s.bg(theme.hover))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.commit_pin_pick(click_stem.clone(), click_preset.clone(), cx);
                }))
                .child(
                    // `flex_1` with the `min_w_0`: without it the box shrinks
                    // to its own minimum and truncates a name that had room.
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_style(TextStyle::body())
                        .text_color(if pinned { theme.text } else { theme.text_muted })
                        .child(label),
                )
                .children(pinned.then(|| {
                    div()
                        .flex_none()
                        .text_style(TextStyle::chip())
                        .text_color(theme.accent)
                        .child("pinned")
                }))
                .into_any_element()
        };

        let body: AnyElement = if groups.is_empty() {
            div()
                .py(px(10.0))
                .text_style(TextStyle::body())
                .text_color(theme.text_subtle)
                .child(if query.is_empty() {
                    "No readable CVs in this vault yet."
                } else {
                    "No CV or preset matches that."
                })
                .into_any_element()
        } else {
            div()
                .id("pin-pick-list")
                .max_h(px(300.0))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .children(groups.into_iter().map(|group| {
                    let stem = group.stem.clone();
                    // A document with no presets is one row that reads as
                    // itself; a document with presets is a heading and a row
                    // per cut, because the cut is what was actually sent.
                    let rows: Vec<AnyElement> = if group.presets.is_empty() {
                        vec![row(
                            stem.clone(),
                            String::new(),
                            SharedString::from(format!("{}  ·  {}", group.label, group.stem)),
                            false,
                            cx,
                        )]
                    } else {
                        group
                            .presets
                            .iter()
                            .map(|preset| {
                                row(
                                    stem.clone(),
                                    preset.clone(),
                                    SharedString::from(preset.clone()),
                                    true,
                                    cx,
                                )
                            })
                            .collect()
                    };

                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .children((!group.presets.is_empty()).then(|| {
                            div()
                                .px_2()
                                .flex()
                                .items_baseline()
                                .gap_2()
                                .child(
                                    div()
                                        .text_style(TextStyle::label())
                                        .text_color(theme.text)
                                        .child(SharedString::from(group.label.clone())),
                                )
                                .child(
                                    div()
                                        .text_style(TextStyle::meta())
                                        .text_color(theme.text_subtle)
                                        .child(SharedString::from(group.stem.clone())),
                                )
                        }))
                        .children(rows)
                }))
                .into_any_element()
        };

        let company = pick.company.trim();
        let subtitle = if company.is_empty() {
            "Recorded on the card, and used to capture the PDF that was sent.".to_string()
        } else {
            format!(
                "For {company}. Recorded on the card, and used to capture the \
                 PDF that was sent."
            )
        };
        let index = pick.index;
        let has_pin = pick.current.is_some();

        let panel = div()
            .w(px(460.0))
            .flex()
            .flex_col()
            .rounded_lg()
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_style(TextStyle::heading())
                            .text_color(theme.text)
                            .child("Which CV did you send?"),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_muted)
                            .child(SharedString::from(subtitle)),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(TextField::new(&pick.filter).placeholder("Filter CVs and presets…"))
                    .child(body),
            )
            .child(
                div()
                    .px_4()
                    .pb_4()
                    .flex()
                    .justify_between()
                    .gap(px(8.0))
                    .child(div().flex().children(has_pin.then(|| {
                        Button::new("pin-pick-unpin")
                            .cursor_pointer()
                            .toolbar_secondary()
                            .label("Unpin")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.pin_pick = None;
                                this.unpin_application_cv(index, cx);
                            }))
                    })))
                    .child(
                        Button::new("pin-pick-cancel")
                            .cursor_pointer()
                            .toolbar_secondary()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.close_pin_pick(cx);
                            })),
                    ),
            );

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.scrim)
                .child(panel)
                .into_any_element(),
        )
    }
}

//! The import screen's offer for what it could not place.
//!
//! Split from `import_flow.rs`, which was already past the line limit carrying
//! the drop zone, the parsing animation, the failure step and the review. This
//! is one panel with one job, and the grouping in it is the whole of E8.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, Entity, IntoElement, SharedString};

use dockcv_ui_components::{Button, ButtonExt, Disableable, Sizable, TextField, TextFieldState};

use crate::import::model::{ImportedDoc, Unplaced, UnplacedOffer};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

/// What the panel calls when a group is adopted.
///
/// `Rc` because there is one button per group and a closure is not `Clone`.
pub type AdoptHandler<V> = Rc<dyn Fn(&mut V, String, Vec<Unplaced>, &mut Context<V>)>;

/// What the importer read and had nowhere to put — and what can be done about it.
///
/// This panel used to offer two ways forward and both of them lost. *Copy them
/// by hand* puts the work back on the person at the exact moment the machine is
/// holding the data already parsed. *Undo the import* throws a good import away
/// over two fields. It is the one screen that knows exactly what arrived, and
/// it did nothing with it.
///
/// Now the leftovers are grouped by what can honestly be offered for them, and
/// the two kinds are kept apart on purpose. A structured source knew the name
/// of the field, so a heading can be proposed and the entries are typed. An
/// unstructured line has no label at all, so the person names the section and
/// each line becomes a highlight. Merging the two would make the second lie
/// about what it knows. What is left in the warning is what genuinely has
/// nowhere to go, and a warning that is rarely wrong gets read.
pub fn render_unplaced<V: 'static>(
    cx: &mut Context<V>,
    imported: &ImportedDoc,
    name_field: Option<&Entity<TextFieldState>>,
    on_adopt: AdoptHandler<V>,
) -> Option<impl IntoElement> {
    if imported.unplaced.is_empty() {
        return None;
    }
    let theme = *cx.theme();

    // Grouped by the heading each would take, in the order they arrived, so a
    // file with `interests` before `references` reads that way.
    let mut proposed: Vec<(String, Vec<Unplaced>)> = Vec::new();
    let mut unnamed: Vec<Unplaced> = Vec::new();
    let mut nothing: Vec<Unplaced> = Vec::new();
    for item in &imported.unplaced {
        match item.offer() {
            UnplacedOffer::Section { heading } => {
                match proposed.iter_mut().find(|(h, _)| *h == heading) {
                    Some((_, group)) => group.push(item.clone()),
                    None => proposed.push((heading, vec![item.clone()])),
                }
            }
            UnplacedOffer::NamedByPerson => unnamed.push(item.clone()),
            UnplacedOffer::Nothing => nothing.push(item.clone()),
        }
    }

    // Nothing is *shown* without being listed: an offer to create something a
    // person cannot see first is the copy-it-by-hand problem wearing a button.
    const SHOWN: usize = 6;
    let preview = |items: &[Unplaced]| {
        let hidden = items.len().saturating_sub(SHOWN);
        let lines: Vec<String> = items.iter().take(SHOWN).map(Unplaced::line).collect();
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(lines.into_iter().map(|line| {
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child(line)
            }))
            .children((hidden > 0).then(|| {
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child(format!("…and {hidden} more"))
            }))
    };

    let group_box = |children: gpui::Div| {
        div()
            .p(px(12.0))
            .rounded(theme.radius_md())
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(children)
    };

    Some(
        div()
            .mt(px(6.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            // A heading we can propose: one click, and the entries keep their
            // shape because the source told us what the fields were.
            .children(proposed.into_iter().map(|(heading, items)| {
                let count = items.len();
                let on_adopt = on_adopt.clone();
                let for_click = items.clone();
                let heading_for_click = heading.clone();
                group_box(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .text_style(TextStyle::label())
                                        .text_color(theme.text)
                                        .child(format!(
                                            "{count} thing{} for a “{heading}” section",
                                            if count == 1 { "" } else { "s" }
                                        )),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("adopt-{heading}")))
                                        .toolbar_primary()
                                        .label(format!("Make “{heading}”"))
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, _w, cx| {
                                                on_adopt(
                                                    this,
                                                    heading_for_click.clone(),
                                                    for_click.clone(),
                                                    cx,
                                                );
                                            },
                                        )),
                                ),
                        )
                        .child(preview(&items)),
                )
            }))
            // No label to propose, so the person supplies one.
            .children((!unnamed.is_empty()).then(|| {
                let count = unnamed.len();
                let on_adopt = on_adopt.clone();
                let for_click = unnamed.clone();
                let field = name_field.cloned();
                let typed = field
                    .as_ref()
                    .map(|f| f.read(cx).value(cx).trim().to_string())
                    .unwrap_or_default();
                let ready = !typed.is_empty();
                group_box(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_style(TextStyle::label())
                                .text_color(theme.text)
                                .child(format!(
                                    "{count} line{} DockCV could not place. Name a section for \
                                     them and each becomes one bullet.",
                                    if count == 1 { "" } else { "s" }
                                )),
                        )
                        .child(preview(&unnamed))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .children(field.as_ref().map(|f| {
                                    div().flex_1().min_w_0().child(TextField::new(f).small())
                                }))
                                .child(
                                    Button::new("adopt-unnamed")
                                        .toolbar_primary()
                                        .disabled(!ready)
                                        .label("Make section")
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, _w, cx| {
                                                if typed.is_empty() {
                                                    return;
                                                }
                                                on_adopt(
                                                    this,
                                                    typed.clone(),
                                                    for_click.clone(),
                                                    cx,
                                                );
                                            },
                                        )),
                                ),
                        ),
                )
            }))
            // What is left is what genuinely has nowhere to go.
            .children((!nothing.is_empty()).then(|| {
                div()
                    .p(px(12.0))
                    .rounded(theme.radius_md())
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.warning)
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_style(TextStyle::label())
                            .text_color(theme.warning)
                            .child(format!(
                                "{} thing{} DockCV has nowhere for",
                                nothing.len(),
                                if nothing.len() == 1 { "" } else { "s" }
                            )),
                    )
                    .child(preview(&nothing))
            })),
    )
}

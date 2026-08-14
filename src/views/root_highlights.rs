//! Removable list rows for the sidebar's sections — split out of
//! `root_sidebar.rs` to keep that file under the ~800-line house limit, same
//! reasoning as `root_section_variants.rs`/`root_section_rename.rs`.
//!
//! One anatomy, everywhere: [`Root::highlight_list`] draws a labelled list of
//! short items — a bullet, the input, and the remove control on one line.
//!
//! It replaced a per-row-labelled version (`Highlight 1`, `Skill 2`, …) whose
//! numbers named nothing the eye could not already count. C-5
//! (`docs/design/editor-comfort.md`) scoped that change to the built-in
//! sections' highlights, which left Skills' keywords and a custom section's
//! highlights on the old shape — two visual languages in one panel, which is
//! worse than either. They use this too.

use gpui::prelude::*;
use gpui::{div, px, Context, SharedString};

use dockcv_ui_components::Field;

use crate::resume::edit::{FieldId, ListId};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::root_sidebar::FIELD_LABEL_LINE_HEIGHT;
use super::Root;

impl Root {

    /// C-5 (`docs/design/editor-comfort.md`): a "Highlights" list — one label
    /// above a stack of bullet rows, each a small glyph, the input and the
    /// remove control together on the input's own line. Replaces giving
    /// every highlight its own `Highlight 1`, `Highlight 2`… labelled row:
    /// the number carried no information the list position didn't already
    /// show, and six of them cost six full label+input rows.
    ///
    /// `None` when `fields` is empty — the "Add highlight" button the caller
    /// pushes after this still offers the first one, and an empty list under
    /// a lone "Highlights" label would just be dead chrome.
    pub(super) fn highlight_list(
        &self,
        cx: &mut Context<Self>,
        label: impl Into<SharedString>,
        list: ListId,
        fields: &[FieldId],
    ) -> Option<Field> {
        if fields.is_empty() {
            return None;
        }
        let theme = cx.theme().clone();
        let label: SharedString = label.into();

        let mut rows = div().flex().flex_col().gap(px(6.0));
        for (index, &field) in fields.iter().enumerate() {
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_subtle)
                            .child("•"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_style(TextStyle::prose())
                            .child(self.field_box(field)),
                    )
                    .child(self.remove_button(cx, list, index)),
            );
        }

        Some(
            Field::new()
                .col_span(2)
                .gap(px(0.0))
                .label_fn(move |_window, _cx| {
                    div()
                        .text_style(TextStyle::label())
                        .line_height(FIELD_LABEL_LINE_HEIGHT)
                        .text_color(theme.text_muted)
                        .child(label.clone())
                })
                .child(rows),
        )
    }
}

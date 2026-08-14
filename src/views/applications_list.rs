//! The List view — the same applications as the board, in one sortable table.
//!
//! The mockup draws a `Board`/`List` pill and never draws List's layout
//! (design doc §10), which is why List shipped as an inert pill. What a list
//! is *for* is not ambiguous, though: a board is arranged by stage, so the
//! one thing it cannot do is put every application in one order and let you
//! compare a column across all of them. Everything here follows from that —
//! one row per application, one sortable column per fact worth comparing.
//!
//! Built from upstream's compositional `Table`, not `DataTable`. `DataTable`
//! virtualizes and owns its rows through a `TableDelegate` entity; a vault
//! holds tens of applications, so virtualization buys nothing, and every cell
//! here draws a `Tag`, a chip or a monogram rather than a string — which is
//! what the composable form exists for. The sort is
//! [`super::applications_data::sort_rows`]: pure, tested, and shared with the
//! board so switching views never silently re-orders the same rows.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, IntoElement, SharedString};

use dockcv_ui_components::{
    ContextMenuExt, EmptyState, Icon, IconName, Sizable, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow, Tag,
};

use crate::resume::model::{Application, ApplicationStatus, Applications};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::applications_card::column_tint;
use super::applications_menu::{application_menu, MenuContext};
use super::applications_snapshot::pin_options;
use super::applications_data::{
    matches_query, next_step_caption, short_date, sort_rows, ApplicationSort,
};
use super::shell::Shell;

/// The columns, and the sort each header click selects.
///
/// A header is only clickable when there is a sort that means it — "Next step"
/// and "CV" have no ordering anyone would ask for, so they are labels. A
/// header that looks sortable and is not is worse than one that never offered.
const COLUMNS: [(&str, Option<ApplicationSort>); 6] = [
    ("Company", Some(ApplicationSort::Company)),
    ("Stage", Some(ApplicationSort::Stage)),
    ("CV sent", None),
    ("Applied", Some(ApplicationSort::Applied)),
    ("Next step", None),
    ("Last touched", Some(ApplicationSort::Stale)),
];

impl Shell {
    pub(super) fn render_applications_list(
        &self,
        cx: &mut Context<Self>,
        applications: &Applications,
        query: &str,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();

        let mut rows: Vec<(usize, Application)> = applications
            .entries
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, a)| matches_query(a, query))
            .collect();
        sort_rows(&mut rows, self.applications_sort);

        if rows.is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .child(if applications.entries.is_empty() {
                    EmptyState::new("No applications yet").body(
                        "Add one from the board, or with ⌘N — this list is the same \
                         applications in one order.",
                    )
                } else {
                    EmptyState::new("Nothing matches that search")
                        .body("Clear the search box to see every application again.")
                })
                .into_any_element();
        }

        let active_sort = self.applications_sort;
        let header = TableRow::new().children(COLUMNS.into_iter().map(|(title, sort)| {
            let head = TableHead::new();
            let is_active = sort == Some(active_sort);

            let mut label = div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_style(TextStyle::label())
                .text_color(if is_active { theme.text } else { theme.text_muted })
                .child(title);
            if is_active {
                // Only the column actually in force draws the caret. A caret
                // on every sortable column would be an invitation, not a state.
                label = label.child(
                    Icon::new(IconName::ChevronDown)
                        .with_size(px(11.0))
                        .text_color(theme.accent),
                );
            }

            match sort {
                Some(sort) => head.child(
                    div()
                        .id(SharedString::from(format!("apps-sort-{title}")))
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme.text))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            this.set_applications_sort(sort, cx);
                        }))
                        .child(label),
                ),
                None => head.child(label),
            }
        }));

        let body = TableBody::new().children(
            rows.into_iter()
                .map(|(index, app)| self.list_row(cx, index, app)),
        );

        div()
            .id("applications-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(34.0))
            .pb(px(28.0))
            .child(
                Table::new()
                    .small()
                    .child(TableHeader::new().child(header))
                    .child(body),
            )
            .into_any_element()
    }

    fn list_row(&self, cx: &mut Context<Self>, index: usize, app: Application) -> TableRow {
        let theme = cx.theme().clone();
        let tint = column_tint(&theme, app.status);

        let company = if app.company.trim().is_empty() {
            "Untitled".to_string()
        } else {
            app.company.clone()
        };

        // Company over role, one cell: they are one identity, and two columns
        // for them would push everything else off the right edge.
        let identity = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .text_style(TextStyle::control())
                    .text_color(theme.text)
                    .child(company),
            )
            .when(!app.role.trim().is_empty(), |el| {
                el.child(
                    div()
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_muted)
                        .child(app.role.clone()),
                )
            });

        // Same chip the board's cards wear, so a stage reads identically in
        // both views. `Tag::custom` takes fill/foreground/border explicitly —
        // the named variants carry upstream's palette, not ours.
        let stage = Tag::custom(tint.chip_bg, tint.fg, tint.chip_bg)
            .px(px(7.0))
            .py(px(2.0))
            .rounded(px(5.0))
            .text_style(TextStyle::chip())
            .child(status_label(app.status));

        let cv: SharedString = match (app.preset.trim(), app.source_doc.as_deref()) {
            ("", None) => "—".into(),
            ("", Some(doc)) => doc.to_string().into(),
            (preset, None) => preset.to_string().into(),
            (preset, Some(doc)) => format!("{doc} · {preset}").into(),
        };

        let applied: SharedString = app
            .applied
            .as_deref()
            .filter(|d| !d.is_empty())
            .map(|d| short_date(d).into())
            .unwrap_or_else(|| "—".into());

        let next: SharedString = app
            .next_step
            .as_ref()
            .map(|s| next_step_caption(s).into())
            .unwrap_or_else(|| "—".into());

        let touched: SharedString = {
            let snapshots = app.snapshots.len();
            if snapshots > 0 {
                format!("{snapshots} snapshot{}", if snapshots == 1 { "" } else { "s" }).into()
            } else {
                short_date(&app.created).into()
            }
        };

        let muted = |text: SharedString| {
            div()
                .text_style(TextStyle::meta())
                .text_color(theme.text_muted)
                .child(text)
        };

        TableRow::new()
            // The row opens the same `···` actions the board card does, so a
            // user who lives in the list is not sent back to the board to move
            // a card. Right-click rather than a per-row button: the list is
            // dense, and six columns plus a control is five columns of data.
            .child(TableCell::new().child(
                // The list gets the board card's whole menu, on right-click:
                // a user who lives here should never be sent back to the board
                // to move a card. A per-row `···` button would be a seventh
                // column of chrome on a table that is already six of data.
                div()
                    .id(SharedString::from(format!("apps-row-{index}")))
                    .context_menu(application_menu(MenuContext::of(
                        cx.weak_entity(),
                        index,
                        &app,
                        pin_options(self.cache.metadata()),
                    )))
                    .child(identity),
            ))
            .child(TableCell::new().child(stage))
            .child(TableCell::new().child(muted(cv)))
            .child(TableCell::new().child(muted(applied)))
            .child(TableCell::new().child(muted(next)))
            .child(TableCell::new().child(muted(touched)))
    }
}

fn status_label(status: ApplicationStatus) -> &'static str {
    match status {
        ApplicationStatus::Wishlist => "Wishlist",
        ApplicationStatus::Applied => "Applied",
        ApplicationStatus::Interviewing => "Interviewing",
        ApplicationStatus::Offer => "Offer",
        ApplicationStatus::Rejected => "Rejected",
    }
}

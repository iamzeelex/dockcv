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
use gpui::{div, px, relative, ClickEvent, Context, IntoElement, SharedString};

use dockcv_ui_components::{
    ContextMenuExt, EmptyState, Icon, IconName, Sizable, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow, Tag,
};

use crate::resume::model::{Application, Applications};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::applications_card::column_tint;
use super::applications_menu::{application_menu, MenuContext};
use super::applications_data::{
    last_touched, matches_query, next_step_caption, short_date, sort_rows, status_title,
    ApplicationSort,
};
use super::shell::Shell;

/// A column: what it is called, the sort a header click selects, and how much
/// of the row it gets.
struct Column {
    title: &'static str,
    sort: Option<ApplicationSort>,
    /// Share of the row's width, against the other columns' shares.
    grow: f32,
    /// Where it stops shrinking. Past this the table scrolls sideways rather
    /// than squeezing every column into uselessness together.
    min_px: f32,
}

/// The columns, and the sort each header click selects.
///
/// A header is only clickable when there is a sort that means it — "Next step"
/// and "CV" have no ordering anyone would ask for, so they are labels. A
/// header that looks sortable and is not is worse than one that never offered.
///
/// **Weights, not equal shares.** Upstream hands every cell the same
/// `flex_basis`, so "Applied" — six characters of date — was given exactly as
/// much of the row as a company and a role stacked together, and the columns
/// with something to say were the first to run out of room. The numbers below
/// are read off what each column actually holds.
const COLUMNS: [Column; 6] = [
    Column { title: "Company",      sort: Some(ApplicationSort::Company), grow: 2.6, min_px: 150.0 },
    Column { title: "Stage",        sort: Some(ApplicationSort::Stage),   grow: 1.2, min_px: 96.0 },
    Column { title: "CV sent",      sort: None,                           grow: 2.4, min_px: 120.0 },
    Column { title: "Applied",      sort: Some(ApplicationSort::Applied), grow: 1.0, min_px: 76.0 },
    Column { title: "Next step",    sort: None,                           grow: 2.2, min_px: 120.0 },
    Column { title: "Last touched", sort: Some(ApplicationSort::Stale),   grow: 1.2, min_px: 92.0 },
];

/// Give a header or a cell its column's width.
///
/// Both `TableHead` and `TableCell` apply their own `flex_basis` first and
/// `refine_style` last, so what is set here wins — and setting it in one
/// function is what keeps the header from drifting out of step with the body
/// it labels.
fn sized<T: Styled>(el: T, col: &Column) -> T {
    el.flex_basis(relative(col.grow))
        .min_w(px(col.min_px))
        // The defect this whole column table exists to fix: a cell shrinks,
        // but its *contents* had no width to shrink to, so a long "CV sent"
        // painted straight across "Applied" and the row became unreadable.
        .overflow_hidden()
}

impl Shell {
    pub(super) fn render_applications_list(
        &self,
        cx: &mut Context<Self>,
        applications: &Applications,
        query: &str,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();

        // Borrowed: a table row reads its application and never keeps it.
        let mut rows: Vec<(usize, &Application)> = applications
            .entries
            .iter()
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
        let header = TableRow::new().children(COLUMNS.iter().map(|col| {
            let (title, sort) = (col.title, col.sort);
            let head = sized(TableHead::new(), col);
            let is_active = sort == Some(active_sort);

            let mut label = div()
                .flex()
                .items_center()
                .min_w_0()
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
            // Once every column is at its floor the table is as narrow as it
            // can honestly be. A window narrower than that scrolls to the rest
            // rather than clipping it out of existence.
            .overflow_x_scroll()
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

    fn list_row(&self, cx: &mut Context<Self>, index: usize, app: &Application) -> TableRow {
        let theme = cx.theme().clone();
        let tint = column_tint(&theme, app.status());

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
            .w_full()
            .min_w_0()
            .child(
                div()
                    .w_full()
                    .truncate()
                    .text_style(TextStyle::control())
                    .text_color(theme.text)
                    .child(company),
            )
            .when(!app.role.trim().is_empty(), |el| {
                el.child(
                    div()
                        .w_full()
                        .truncate()
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
            .child(status_title(app.status()));

        // Two cases now, not four: a CV was attributed or it was not.
        let cv: SharedString = match &app.sent_as {
            None => "—".into(),
            Some(sent) => sent.label().into(),
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

        // The column says "Last touched", so it prints when something last
        // happened. It used to print a snapshot *count* — not a date, not a
        // touch, and out of step with the sort the header offers, which has
        // always ordered by `last_touched`.
        let touched: SharedString = short_date(last_touched(app)).into();

        // `w_full` so `truncate` has a width to cut against, and one ellipsis
        // rather than a second column's worth of overpainted text.
        let muted = |text: SharedString| {
            div()
                .w_full()
                .truncate()
                .text_style(TextStyle::meta())
                .text_color(theme.text_muted)
                .child(text)
        };

        TableRow::new()
            // The row opens the same `···` actions the board card does, so a
            // user who lives in the list is not sent back to the board to move
            // a card. Right-click rather than a per-row button: the list is
            // dense, and six columns plus a control is five columns of data.
            .child(sized(TableCell::new(), &COLUMNS[0]).child(
                // The list gets the board card's whole menu, on right-click:
                // a user who lives here should never be sent back to the board
                // to move a card. A per-row `···` button would be a seventh
                // column of chrome on a table that is already six of data.
                div()
                    .id(SharedString::from(format!("apps-row-{index}")))
                    .w_full()
                    .min_w_0()
                    .context_menu(application_menu(MenuContext::of(
                        cx.weak_entity(),
                        index,
                        app,
                    )))
                    .child(identity),
            ))
            .child(sized(TableCell::new(), &COLUMNS[1]).child(stage))
            .child(sized(TableCell::new(), &COLUMNS[2]).child(muted(cv)))
            .child(sized(TableCell::new(), &COLUMNS[3]).child(muted(applied)))
            .child(sized(TableCell::new(), &COLUMNS[4]).child(muted(next)))
            .child(sized(TableCell::new(), &COLUMNS[5]).child(muted(touched)))
    }
}


#[cfg(test)]
mod tests {
    use super::COLUMNS;

    /// Every column has to be reachable in a window someone actually uses.
    ///
    /// The floors exist so a narrow window scrolls instead of squeezing six
    /// columns into illegibility — but a table whose floor is wider than the
    /// pane is one that *always* scrolls sideways, which is the same defect
    /// wearing a different hat. This is the budget: adding a seventh column
    /// means taking the room from somewhere, deliberately.
    #[test]
    fn the_table_fits_a_window_someone_would_open() {
        let floor: f32 = COLUMNS.iter().map(|c| c.min_px).sum();
        assert!(
            floor <= 700.0,
            "the list cannot be drawn under {floor}px without scrolling sideways"
        );
    }

    /// Weights are shares of what is left over, so one that is zero or
    /// negative would collapse its column the moment there is room to spare.
    #[test]
    fn every_column_asks_for_a_share_of_the_row() {
        for col in COLUMNS.iter() {
            assert!(col.grow > 0.0, "{} asks for no width", col.title);
            assert!(col.min_px > 0.0, "{} has no floor", col.title);
        }
    }
}

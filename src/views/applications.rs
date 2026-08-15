//! Applications screen — the board that tracks a role from wishlist to offer,
//! each card pinned to the exact CV preset that was sent for it.
//!
//! Design row: `.design/rows/row_applications_new_surface.txt`,
//! `docs/design/applications.md`. Renders the **main pane only** — the rail
//! is shared chrome mounted by `Shell::with_rail`, same as every other vault
//! screen.
//!
//! Two things the design row draws that this build deliberately does not:
//! the PDF snapshot **capture** (needs the Typst engine wired into the shell —
//! a separate task; the snapshot *line* still renders once
//! `Application::snapshots` holds one) and **`★ N wins attached`** (needs a
//! stable `DiaryEntry` id that does not exist yet). Both are called out in
//! `docs/design/applications.md` §9/§10 as gaps in the underlying data, not
//! rendering choices.
//!
//! The Board/List toggle is drawn per the mockup, but only `Board` is real:
//! `List`'s layout is never drawn in either temperature of the mockup (§10),
//! so it is rendered as a visibly inert pill — no id, no click handler, no
//! hover — rather than an invented layout.

use gpui::prelude::*;
use gpui::{
    div, px, App, ClickEvent, Context, FontWeight, IntoElement, SharedString, Window,
};

use dockcv_ui_components::{
    Button, ButtonExt, ButtonVariants, DropdownMenu, Icon, IconName, PopupMenuItem, Sizable,
    StatusTint, Tag, TextField,
};

use crate::resume::model::{Application, ApplicationStatus, Applications, PresetConversion};
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;

use super::save_status;

use super::applications_data::{
    card_chip_text, conversion_line, interviews_this_week, matches_query, plural, sort_rows,
    ApplicationSort, ApplicationsView,
};
use super::applications_card::{card_meta, column_tint};
use super::applications_menu::{application_menu, MenuContext};
use super::shell::{remove_at, Shell};

/// The five board columns, in display order.
const COLUMNS: [(ApplicationStatus, &str); 5] = [
    (ApplicationStatus::Wishlist, "Wishlist"),
    (ApplicationStatus::Applied, "Applied"),
    (ApplicationStatus::Interviewing, "Interviewing"),
    (ApplicationStatus::Offer, "Offer"),
    (ApplicationStatus::Rejected, "Rejected"),
];

impl Shell {
    pub(super) fn render_applications_screen(&self, cx: &mut Context<Self>) -> gpui::Div {
        let applications = self.cache.applications();
        let query = self.applications_query(cx);
        let today = vault::today_iso();
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let conversions = applications.conversion();

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .child(self.applications_header(cx, applications, &today))
            // The conversion strip is the board's and the list's running
            // total. Insights draws the same facts as a chart, so a duplicate
            // strip above it would be the same numbers twice.
            .children(
                (self.applications_view != ApplicationsView::Insights)
                    .then(|| self.conversion_strip(cx, &conversions))
                    .flatten(),
            )
            .child(match self.applications_view {
                ApplicationsView::Board => {
                    self.board(cx, applications, &query, now_secs).into_any_element()
                }
                ApplicationsView::List => self
                    .render_applications_list(cx, applications, &query)
                    .into_any_element(),
                ApplicationsView::Insights => self
                    .render_applications_insights(cx, applications)
                    .into_any_element(),
            })
            // Last child, so it paints over the board rather than under it.
            .children(self.render_pin_pick_sheet(cx))
    }

    /// Two rows, not one.
    ///
    /// Five controls beside a title do not fit a default-width window: adding
    /// Insights and the sort control first clipped "New application" off the
    /// right edge, and squeezing the search to make room left it too narrow to
    /// type in. Splitting them is not a workaround — it is the shape a tracker
    /// toolbar takes anyway. The top row is *what you are looking at* (the
    /// title, the view, and the one action that creates something); the second
    /// is *how it is filtered and ordered*, which is where a sort and a search
    /// belong and where the conversion strip already was.
    fn applications_header(
        &self,
        cx: &mut Context<Self>,
        applications: &Applications,
        today: &str,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let active = applications.active();
        let interviews = interviews_this_week(applications, today);

        let title_row = div()
            .flex()
            .items_end()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_style(TextStyle::title())
                            .text_color(theme.text)
                            .child("Applications"),
                    )
                    .child(
                        div()
                            .mt(px(3.0))
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(format!(
                                "{active} active · {interviews} interview{} this week",
                                plural(interviews)
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(self.view_toggle(cx))
                    .child(
                        Button::new("new-application")
                            .cursor_pointer()
                            .header_primary()
                            .icon(IconName::Plus)
                            .label("New application")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.open_applications_compose(
                                    ApplicationStatus::Wishlist,
                                    window,
                                    cx,
                                );
                            })),
                    ),
            );

        // Insights is neither ordered nor searched, so its second row would be
        // empty — and an empty row is a gap the eye reads as a mistake.
        let filters = (self.applications_view != ApplicationsView::Insights).then(|| {
            div()
                .mt(px(14.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(self.sort_control(cx))
                .child(self.applications_search_box(cx))
        });

        div()
            .flex()
            .flex_col()
            .px(px(34.0))
            .pt(px(30.0))
            .pb(px(18.0))
            .child(title_row)
            .children(filters)
    }

    /// `Board` / `List` / `Insights`. All three are real now; the mockup's
    /// two-way pill grew a third segment rather than a second control,
    /// because these are three views of one set of applications and not three
    /// destinations.
    fn view_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let active = self.applications_view;

        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .h(px(34.0))
            .p(px(3.0))
            .rounded(px(8.0))
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .children(ApplicationsView::ALL.map(|view| {
                let is_active = view == active;
                div()
                    .id(SharedString::from(format!("apps-view-{view:?}")))
                    .px(px(11.0))
                    .py(px(5.0))
                    .rounded(px(6.0))
                    .when(is_active, |el| el.bg(theme.text))
                    .text_style(TextStyle::control())
                    .when(!is_active, |el| el.font_weight(FontWeight::NORMAL))
                    .text_color(if is_active {
                        theme.on_accent
                    } else {
                        theme.text_subtle
                    })
                    .cursor_pointer()
                    .when(!is_active, |el| {
                        el.hover(|s| s.text_color(theme.text))
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.set_applications_view(view, cx);
                    }))
                    .child(view.label())
            }))
    }

    /// The order both the board and the list use, as a menu rather than a row
    /// of chips: five options is more than a toolbar should spend width on,
    /// and the current one is the only part worth showing at rest.
    fn sort_control(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.applications_sort;
        let shell = cx.weak_entity();

        Button::new("apps-sort")
            .cursor_pointer()
            .toolbar_secondary()
            .icon(IconName::SortAscending)
            .label(active.short_label())
            .tooltip("Sort applications")
            .dropdown_menu(move |mut menu, _window, _cx| {
                for sort in ApplicationSort::ALL {
                    let shell = shell.clone();
                    menu = menu.item(
                        PopupMenuItem::new(sort.label())
                            .checked(sort == active)
                            .on_click(move |_ev, _window, cx| {
                                let _ = shell.update(cx, |this, cx| {
                                    this.set_applications_sort(sort, cx);
                                });
                            }),
                    );
                }
                menu
            })
    }

    pub(super) fn set_applications_view(&mut self, view: ApplicationsView, cx: &mut Context<Self>) {
        if self.applications_view == view {
            return;
        }
        self.applications_view = view;
        // A compose box open in a column the user just navigated away from
        // would reappear, half-typed, the next time they came back.
        self.applications_compose_target = None;
        cx.notify();
    }

    pub(super) fn set_applications_sort(&mut self, sort: ApplicationSort, cx: &mut Context<Self>) {
        self.applications_sort = sort;
        cx.notify();
    }

    fn applications_search_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .w(px(230.0))
            .h(px(34.0))
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
                    self.applications_search
                        .as_ref()
                        .map(|state| TextField::new(state).seamless().placeholder("Search")),
                ),
            )
    }

    /// `Conversion by preset` — `FAANG · concise — 4 sent → 1 interview → 1
    /// offer`. Absent (not drawn empty) until at least one application is
    /// attributed to a preset, since a kicker with nothing after it explains
    /// nothing.
    fn conversion_strip(
        &self,
        cx: &mut Context<Self>,
        conversions: &[PresetConversion],
    ) -> Option<impl IntoElement> {
        if conversions.is_empty() {
            return None;
        }
        let theme = cx.theme().clone();
        let mut row = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(px(14.0))
            .px(px(34.0))
            .mb(px(14.0))
            .child(
                div()
                    .text_style(TextStyle::eyebrow())
                    .text_color(theme.text_subtle)
                    .child(TextStyle::eyebrow().apply_case("Conversion by preset")),
            );
        for (index, conv) in conversions.iter().enumerate() {
            if index > 0 {
                row = row.child(
                    div()
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_subtle)
                        .child("·"),
                );
            }
            row = row.child(
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_muted)
                    .child(conversion_line(conv)),
            );
        }
        Some(row)
    }

    /// The five columns.
    fn board(
        &self,
        cx: &mut Context<Self>,
        applications: &Applications,
        query: &str,
        now_secs: u64,
    ) -> impl IntoElement {
        let all: Vec<(usize, Application)> = applications.entries.iter().cloned().enumerate().collect();

        div()
            .id("applications-board")
            .flex_1()
            .min_h_0()
            .overflow_x_scroll()
            .flex()
            .gap(px(10.0))
            .px(px(34.0))
            .pb(px(24.0))
            .children(COLUMNS.into_iter().map(|(status, title)| {
                let mut cards: Vec<(usize, Application)> = all
                    .iter()
                    .filter(|(_, a)| a.status() == status && matches_query(a, query))
                    .cloned()
                    .collect();
                // Same order the list is in. A column sorted differently from
                // the table showing the same cards would be two answers to one
                // question.
                sort_rows(&mut cards, self.applications_sort);
                let total = applications.count(status);
                self.column(cx, status, title, total, cards, now_secs)
                    .into_any_element()
            }))
    }

    fn column(
        &self,
        cx: &mut Context<Self>,
        status: ApplicationStatus,
        title: &'static str,
        total: usize,
        cards: Vec<(usize, Application)>,
        now_secs: u64,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tint = column_tint(&theme, status);
        // Rejected draws no "+": the design's own ruling (§3) is that you
        // don't add directly into it, you move a card there from elsewhere.
        let show_add = status != ApplicationStatus::Rejected;

        let mut header = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .mb(px(12.0))
            .child(div().size(px(7.0)).rounded(px(2.0)).bg(tint.dot))
            .child(
                div()
                    .text_style(TextStyle::control())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(title),
            )
            .child(
                div()
                    .text_style(TextStyle::chip())
                    .text_color(theme.text_subtle)
                    .child(format!("{total}")),
            );
        if show_add {
            header = header.child(div().flex_1()).child(
                Button::new(SharedString::from(format!("apps-add-{status:?}")))
                    .icon(IconName::Plus)
                    .ghost()
                    .xsmall()
                    .cursor_pointer()
                    .tooltip("Add")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_applications_compose(status, window, cx);
                    })),
            );
        }

        let compose = self
            .applications_compose_target
            .filter(|t| *t == status)
            .map(|_| self.compose_box(cx));

        // The whole column is the drop target, empty space included — a card
        // has to be droppable into a column that holds nothing yet.
        let column = div()
            .id(SharedString::from(format!("apps-col-{status:?}")))
            // Adaptive, not 168px fixed. Five fixed columns left two thirds of
            // a wide window empty and huddled at the left edge; below a narrow
            // window they were unreadably thin either way. `flex_1` with a
            // floor gives each column an equal share of whatever there is, and
            // the board's own `overflow_x_scroll` takes over once five columns
            // can no longer fit above the floor.
            .flex_1()
            // Five columns plus their gaps and the pane's own padding come to
            // `5 * min + 108`, so the floor is what decides whether a default
            // 1100px window shows the whole board or hides Rejected behind a
            // scroll. 150 fits; 184 did not. Above the floor they share the
            // width evenly, and the ceiling keeps a two-card vault from
            // drawing columns wider than a card ever needs.
            .min_w(px(150.0))
            .max_w(px(300.0))
            .h_full()
            .flex()
            .flex_col()
            .rounded(px(10.0))
            .border_1()
            .border_color(gpui::transparent_black())
            .child(header)
            .child(
                div()
                    .id(SharedString::from(format!("apps-col-scroll-{status:?}")))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .children(compose)
                    .children(
                        cards
                            .into_iter()
                            .map(|(index, app)| self.card(cx, index, app, status, &tint, now_secs)),
                    ),
            );

        self.column_drop_target(cx, status, column)
    }

    /// The inline "company + role" compose box — the only two fields a new
    /// card starts with; everything else is edited later (design doc §8/§10:
    /// no compose surface is drawn in the mockup, so this is a minimal,
    /// self-consistent one rather than an invented full form).
    fn compose_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(10.0))
            .rounded(px(10.0))
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border_strong)
            .child(div().children(
                self.applications_compose_company
                    .as_ref()
                    .map(|s| TextField::new(s).placeholder("Company")),
            ))
            .child(div().children(
                self.applications_compose_role
                    .as_ref()
                    .map(|s| TextField::new(s).placeholder("Role")),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        Button::new("apps-compose-cancel")
                            .cursor_pointer()
                            .toolbar_secondary()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.cancel_applications_compose(cx);
                            })),
                    )
                    .child(
                        Button::new("apps-compose-add")
                            .cursor_pointer()
                            .toolbar_primary()
                            .label("Add")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.commit_applications_compose(window, cx);
                            })),
                    ),
            )
    }

    fn card(
        &self,
        cx: &mut Context<Self>,
        index: usize,
        app: Application,
        status: ApplicationStatus,
        tint: &StatusTint,
        now_secs: u64,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let shell = cx.weak_entity();
        // Built out here, from the cache, and moved into the menu closure.
        // It used to be built *inside* the closure with a full vault parse, on
        // the grounds that a menu opens rarely — but the closure borrows
        // `self`, and `DocMeta` already carries the preset names it needed.
        let menu_context = MenuContext::of(shell, index, &app);

        // Only Interviewing and Offer draw the tinted border — Applied keeps
        // the neutral hairline per the design doc's own table (§4).
        let border = if matches!(
            status,
            ApplicationStatus::Interviewing | ApplicationStatus::Offer
        ) {
            tint.border
        } else {
            theme.border
        };
        let bg = if status == ApplicationStatus::Rejected {
            theme.elevated_muted
        } else {
            theme.elevated
        };
        let letter = app
            .company
            .trim()
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_default();

        let chip = card_chip_text(&app, status).map(|text| {
            Tag::custom(tint.chip_bg, tint.fg, tint.chip_bg)
                .px(px(7.0))
                .py(px(2.0))
                .rounded(px(5.0))
                .text_style(TextStyle::chip())
                .child(text)
        });

        let meta = card_meta(&theme, &app, status, now_secs);

        let card = div()
            .id(SharedString::from(format!("apps-card-{index}")))
            .relative()
            .flex()
            .flex_col()
            .px(px(13.0))
            .py(px(12.0))
            .rounded(px(10.0))
            .bg(bg)
            .border_1()
            .border_color(border)
            .when(status == ApplicationStatus::Rejected, |el| el.opacity(0.72));

        // O-16: the whole card drags. The `···` menu stays as the path that
        // works without a mouse.
        self.card_drag_source(index, status, app.company.clone().into(), card)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .mb(px(9.0))
                    .pr(px(20.0)) // clears the "···" trigger
                    .child(
                        div()
                            .flex_none()
                            .size(px(24.0))
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(tint.wash)
                            .text_style(TextStyle::chip())
                            .font_weight(FontWeight::BOLD)
                            .text_color(tint.fg)
                            .child(letter),
                    )
                    .child(
                        div()
                            // `flex_1`, not just `min_w_0`. Without it the
                            // block's basis is its content and flex-shrink
                            // clipped names that had room to spare — a column
                            // at its 300px ceiling still showed "Tesse…".
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_style(TextStyle::body())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child(app.company.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_subtle)
                                    .child(app.role.clone()),
                            ),
                    ),
            )
            .children(chip.map(|c| div().mb(px(9.0)).child(c)))
            .children(meta)
            .child(
                // Occluded, and for the same reason the gallery card's menu
                // is: the whole card is a drag source, so without this the
                // press lands on the card and starts a drag instead of
                // opening the menu. Confirmed on the gallery, where the
                // equivalent button was simply not clickable.
                div()
                    .absolute()
                    .top(px(10.0))
                    .right(px(8.0))
                    .occlude()
                    .child(
                Button::new(SharedString::from(format!("apps-menu-{index}")))
                    .icon(IconName::Ellipsis)
                    .ghost()
                    .xsmall()
                    .cursor_pointer()
                    .tooltip("More")
                    .dropdown_menu(application_menu(menu_context)),
                    ),
            )
    }

    /// The applications search query, lowercased and trimmed.
    pub(super) fn applications_query(&self, cx: &App) -> String {
        self.applications_search
            .as_ref()
            .map(|f| f.read(cx).value(cx).trim().to_lowercase())
            .unwrap_or_default()
    }

    /// Open the compose box, scoped to `target`'s column, clearing any
    /// previous draft.
    pub(super) fn open_applications_compose(
        &mut self,
        target: ApplicationStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.applications_compose_target = Some(target);
        if let Some(field) = self.applications_compose_company.clone() {
            field.update(cx, |state, cx| state.seed("", window, cx));
        }
        if let Some(field) = self.applications_compose_role.clone() {
            field.update(cx, |state, cx| state.seed("", window, cx));
        }
        cx.notify();
    }

    pub(super) fn cancel_applications_compose(&mut self, cx: &mut Context<Self>) {
        self.applications_compose_target = None;
        cx.notify();
    }

    /// Commit the compose box: company and role are the only two fields a
    /// new card starts with (design doc's "Build these" instruction) — both
    /// required, since neither has a sane default.
    pub(super) fn commit_applications_compose(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.applications_compose_target else {
            return;
        };
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let company = self
            .applications_compose_company
            .as_ref()
            .map(|f| f.read(cx).value(cx).trim().to_string())
            .unwrap_or_default();
        let role = self
            .applications_compose_role
            .as_ref()
            .map(|f| f.read(cx).value(cx).trim().to_string())
            .unwrap_or_default();
        if company.is_empty() || role.is_empty() {
            return;
        }

        let mut applications = vault::load_applications(&vault);
        let mut application = Application {
            company,
            role,
            created: vault::today_iso(),
            ..Default::default()
        };
        // Creation goes through `advance_to` too, not a direct field
        // assignment, so `furthest` starts consistent even for a card
        // created straight into a later column.
        application.advance_to(target);
        applications.entries.push(application);
        save_status::record(cx, "applications board", vault::save_applications(&vault, &applications));

        self.applications_compose_target = None;
        if let Some(field) = self.applications_compose_company.clone() {
            field.update(cx, |state, cx| state.seed("", window, cx));
        }
        if let Some(field) = self.applications_compose_role.clone() {
            field.update(cx, |state, cx| state.seed("", window, cx));
        }
        cx.notify();
    }

    /// Move a card to a new column — always through `Application::advance_to`,
    /// never a direct `status =` assignment, so `furthest` (and therefore the
    /// conversion funnel) stays honest.
    pub(super) fn advance_application(
        &mut self,
        index: usize,
        status: ApplicationStatus,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let mut applications = vault::load_applications(&vault);
        let mut first_send = false;
        if let Some(application) = applications.entries.get_mut(index) {
            application.advance_to(status);
            // The actual send date, recorded once — never overwritten, so
            // moving a card back and forth doesn't rewrite when it was
            // really sent. This is what makes "applied N ago" real data.
            if status == ApplicationStatus::Applied && application.applied.is_none() {
                application.applied = Some(vault::today_iso());
                first_send = true;
            }
        }
        save_status::record(cx, "applications board", vault::save_applications(&vault, &applications));
        cx.notify();

        // D4a: the moment "this is what they got" becomes a true statement is
        // the moment to freeze it. Only on the *first* send — a later move
        // through the board doesn't change what the company is holding.
        if first_send {
            self.capture_snapshot(index, cx);
        }
    }

    pub(super) fn delete_application(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let mut applications = vault::load_applications(&vault);
        remove_at(&mut applications.entries, index);
        save_status::record(cx, "applications board", vault::save_applications(&vault, &applications));
        cx.notify();
    }
}

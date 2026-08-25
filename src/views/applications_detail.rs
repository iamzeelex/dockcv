//! The detail panel: the one place an application can actually be written.
//!
//! The board and the list both *show* an application's notes, its
//! compensation, why it was rejected and what the next step is. Until this
//! existed nothing could write any of them — the fields round-tripped through
//! TOML, rendered on the card, and were reachable only by opening the vault in
//! a text editor. An app that displays data it cannot author is not a smaller
//! app; it is a wrong one.
//!
//! A right-edge drawer rather than a modal, deliberately: the board stays
//! visible beside it, because half of what you want while writing a note is
//! the column the card is sitting in.
//!
//! **When it saves.** On blur, on close, and when another card is opened —
//! not on every keystroke. The applications file is written whole, and a
//! write per character would be a file rewrite per character. Blur is the
//! boundary a user already understands as "done with that field".

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, Entity, SharedString, Subscription, Window};

use dockcv_ui_components::{ScrollableElement, 
    Button, ButtonExt, Disableable, DropdownMenu, Icon, IconName, PopupMenuItem, Sizable, TextField,
    TextFieldEvent, TextFieldState,
};

use crate::resume::model::{Application, ApplicationStatus, Closure, InterviewRound, NextStep};
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;
use super::save_status;

use super::applications_card::column_tint;
use super::applications_data::{short_date, status_title};
use super::shell::Shell;

/// Width of the drawer. Wide enough for a sentence of notes to not wrap every
/// four words, narrow enough to leave the board readable behind it.
const PANEL_WIDTH: f32 = 380.0;

/// The open panel: which card, and a field per writable value.
///
/// The subscriptions live here rather than on `Shell` so they are dropped with
/// the panel. Pushed onto the shell's list instead they would outlive every
/// card ever opened, one dead subscription per visit.
pub(super) struct ApplicationDetail {
    pub(super) index: usize,
    company: Entity<TextFieldState>,
    role: Entity<TextFieldState>,
    url: Entity<TextFieldState>,
    compensation: Entity<TextFieldState>,
    notes: Entity<TextFieldState>,
    rejection: Entity<TextFieldState>,
    next_label: Entity<TextFieldState>,
    next_date: Entity<TextFieldState>,
    next_time: Entity<TextFieldState>,
    _subs: Vec<Subscription>,
}

impl ApplicationDetail {
    /// What is in the two naming fields *right now*, before any blur has
    /// written it back. The footer's primary action turns on the first letter
    /// typed, so it cannot read the saved card.
    fn typed_name(&self, cx: &gpui::App) -> String {
        format!(
            "{}{}",
            self.company.read(cx).value(cx).trim(),
            self.role.read(cx).value(cx).trim()
        )
    }
}

impl Shell {
    /// Open the panel on `index`, saving whatever the last one was holding.
    pub(super) fn open_application_detail(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_application_detail(cx);

        let Some(vault) = self.vault.clone() else {
            return;
        };
        let applications = vault::load_applications(&vault);
        let Some(app) = applications.entries.get(index) else {
            return;
        };

        // Spelled out rather than built by a closure: each `new` needs
        // `window` mutably, and two closures holding it is one borrow too many.
        let company = cx.new(|cx| TextFieldState::single_line(window, cx));
        let role = cx.new(|cx| TextFieldState::single_line(window, cx));
        let url = cx.new(|cx| TextFieldState::single_line(window, cx));
        let compensation = cx.new(|cx| TextFieldState::single_line(window, cx));
        let notes = cx.new(|cx| TextFieldState::auto_grow(2, 8, window, cx));
        let rejection = cx.new(|cx| TextFieldState::auto_grow(2, 5, window, cx));
        let next_label = cx.new(|cx| TextFieldState::single_line(window, cx));
        let next_date = cx.new(|cx| TextFieldState::single_line(window, cx));
        let next_time = cx.new(|cx| TextFieldState::single_line(window, cx));

        // `seed`, not `set_value`: seeding must not echo back as a change and
        // mark the card dirty the moment the panel opens.
        let step = app.next_step.clone().unwrap_or_default();
        for (field, value) in [
            (&company, app.company.clone()),
            (&role, app.role.clone()),
            (&url, app.url.clone()),
            (&compensation, app.compensation.clone()),
            (&notes, app.notes.clone()),
            (&rejection, app.closure_note.clone().unwrap_or_default()),
            (&next_label, step.label),
            (&next_date, step.date),
            (&next_time, step.time),
        ] {
            field.update(cx, |state, cx| state.seed(value, window, cx));
        }

        // Every field saves the same way, so they subscribe the same way.
        let fields = [
            &company,
            &role,
            &url,
            &compensation,
            &notes,
            &rejection,
            &next_label,
            &next_date,
            &next_time,
        ];
        let subs = fields
            .iter()
            .map(|field| {
                cx.subscribe(*field, |this, _field, event: &TextFieldEvent, cx| {
                    match event {
                        // Enter in a one-line field means the same as leaving
                        // it: the value is finished.
                        TextFieldEvent::Blurred | TextFieldEvent::Submitted => {
                            this.commit_application_detail(cx)
                        }
                        _ => {}
                    }
                })
            })
            .collect();

        self.applications_detail = Some(ApplicationDetail {
            index,
            company,
            role,
            url,
            compensation,
            notes,
            rejection,
            next_label,
            next_date,
            next_time,
            _subs: subs,
        });
        cx.notify();
    }

    /// Put the cursor in Company, so a card started from the board opens
    /// ready to be named rather than ready to be looked at.
    pub(super) fn focus_application_company(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(detail) = self.applications_detail.as_ref() {
            detail
                .company
                .update(cx, |state, cx| state.focus(window, cx));
        }
    }

    /// Throw away a card that was opened and never named.
    ///
    /// The same thing closing a blank panel already did, but reachable on
    /// purpose rather than only as a side effect of pressing ✕.
    pub(super) fn discard_application_detail(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.applications_detail.take() else {
            return;
        };
        if let Some(vault) = self.vault.clone() {
            let mut applications = vault::load_applications(&vault);
            if applications.entries.get(detail.index).is_some() {
                super::shell::remove_at(&mut applications.entries, detail.index);
                save_status::record(
                    cx,
                    "applications board",
                    vault::save_applications(&vault, &applications),
                );
            }
        }
        cx.notify();
    }

    pub(super) fn close_application_detail(&mut self, cx: &mut Context<Self>) {
        self.commit_application_detail(cx);
        let Some(detail) = self.applications_detail.take() else {
            return;
        };
        // A card is created the moment "New application" is pressed, so
        // closing without naming it has to throw it away — otherwise every
        // change of mind leaves a blank card on the board. Company *and* role
        // both empty: one of them filled is a card someone meant to keep.
        if let Some(vault) = self.vault.clone() {
            let mut applications = vault::load_applications(&vault);
            let blank = applications
                .entries
                .get(detail.index)
                .is_some_and(is_unnamed);
            if blank {
                super::shell::remove_at(&mut applications.entries, detail.index);
                save_status::record(
                    cx,
                    "applications board",
                    vault::save_applications(&vault, &applications),
                );
            }
        }
        cx.notify();
    }

    /// Write the panel's fields back to the card, if anything actually
    /// changed. The equality check is what keeps a blur from rewriting the
    /// file — and from stamping a "last touched" on a card you only looked at.
    pub(super) fn commit_application_detail(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.applications_detail.as_ref() else {
            return;
        };
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let mut applications = vault::load_applications(&vault);
        let Some(app) = applications.entries.get_mut(detail.index) else {
            return;
        };

        let read = |field: &Entity<TextFieldState>| field.read(cx).value(cx).trim().to_string();
        let updated = Application {
            company: read(&detail.company),
            role: read(&detail.role),
            url: read(&detail.url),
            compensation: read(&detail.compensation),
            // Notes are prose: leading indentation is the author's, and
            // trimming the ends is all that is safe to do to them.
            notes: detail.notes.read(cx).value(cx).trim_end().to_string(),
            closure_note: none_if_empty(read(&detail.rejection)),
            next_step: next_step_from(
                read(&detail.next_label),
                read(&detail.next_date),
                read(&detail.next_time),
            ),
            ..app.clone()
        };

        if updated == *app {
            return;
        }
        *app = updated;
        save_status::record(
            cx,
            "applications board",
            vault::save_applications(&vault, &applications),
        );
        cx.notify();
    }

    /// Load, change, save. The panel's own edits go through
    /// `commit_application_detail`; this is for the buttons, which change the
    /// card rather than a field of it.
    fn update_application(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Application),
    ) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let mut applications = vault::load_applications(&vault);
        let Some(app) = applications.entries.get_mut(index) else {
            return;
        };
        change(app);
        save_status::record(
            cx,
            "applications board",
            vault::save_applications(&vault, &applications),
        );
        cx.notify();
    }

    /// Record that an interview happened.
    ///
    /// Takes its name from the scheduled next step when there is one, and
    /// clears it: a step you have had is not a step you are waiting for, and
    /// leaving it behind is how a board ends up advertising last Tuesday.
    pub(super) fn log_interview_round(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let today = vault::today_iso();
        self.update_application(index, cx, |app| {
            let label = app
                .next_step
                .take()
                .map(|step| step.label)
                .unwrap_or_default();
            app.rounds.push(InterviewRound { at: today, label });
        });
        // The panel's own fields still hold the step that has just been
        // consumed, and the next blur would write it straight back.
        self.reseed_application_detail(window, cx);
    }

    /// Say how an application ended — or that it has not after all.
    ///
    /// Through `close_as`/`reopen` rather than by assigning the field, so the
    /// column follows the ending and the move is recorded in the history. A
    /// closure written straight into the struct would put the card's own board
    /// column at odds with the diagram that reads it.
    pub(super) fn set_application_closure(
        &mut self,
        index: usize,
        closure: Option<Closure>,
        cx: &mut Context<Self>,
    ) {
        let today = vault::today_iso();
        self.update_application(index, cx, |app| match closure {
            Some(closure) => app.close_as(closure, &today),
            // Reopening lands where the work actually stopped: back in the
            // interviews if there were any, otherwise back to sent.
            None => {
                let to = if app.rounds.is_empty() {
                    ApplicationStatus::Applied
                } else {
                    ApplicationStatus::Interviewing
                };
                app.reopen(to, &today);
            }
        });
    }

    /// Re-read the open panel's fields from the card, after something other
    /// than typing has changed it.
    fn reseed_application_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(detail) = self.applications_detail.as_ref() else {
            return;
        };
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let applications = vault::load_applications(&vault);
        let Some(app) = applications.entries.get(detail.index) else {
            return;
        };
        let step = app.next_step.clone().unwrap_or_default();
        for (field, value) in [
            (&detail.next_label, step.label),
            (&detail.next_date, step.date),
            (&detail.next_time, step.time),
        ] {
            field.update(cx, |state, cx| state.seed(value, window, cx));
        }
    }

    pub(super) fn render_application_detail(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let detail = self.applications_detail.as_ref()?;
        let applications = self.cache.applications();
        let app = applications.entries.get(detail.index)?;
        let theme = *cx.theme();
        let tint = column_tint(&theme, app.status());

        let label = |text: &'static str| {
            div()
                .text_style(TextStyle::label())
                .text_color(theme.text_muted)
                .child(text)
        };
        let field = |title: &'static str, state: &Entity<TextFieldState>, hint: &'static str| {
            div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(label(title))
                .child(TextField::new(state).placeholder(hint))
        };

        let header = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(18.0))
            .pt(px(18.0))
            .pb(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_style(TextStyle::eyebrow())
                    .text_color(theme.text_subtle)
                    .child(TextStyle::eyebrow().apply_case("Application")),
            )
            .child(
                Button::new("apps-detail-close")
                    .icon_only()
                    .icon(IconName::Close)
                    .tooltip("Close")
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.close_application_detail(cx);
                    })),
            );

        // Read-only, because they are records of things that happened rather
        // than fields: the stage is the board's job, the dates are stamped
        // when the move was made, and a snapshot is what a company was
        // actually sent (US-04).
        let facts = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .px(px(7.0))
                            .py(px(2.0))
                            .rounded(theme.radius_sm())
                            .bg(tint.chip_bg)
                            .text_style(TextStyle::chip())
                            .text_color(tint.fg)
                            .child(status_title(app.status())),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(match app.applied.as_deref() {
                                Some(d) if !d.is_empty() => format!("sent {}", short_date(d)),
                                _ => "not sent yet".to_string(),
                            }),
                    ),
            )
            .when(!app.snapshots.is_empty(), |el| {
                el.child(
                    div()
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_subtle)
                        .child(format!(
                            "{} snapshot{}",
                            app.snapshots.len(),
                            if app.snapshots.len() == 1 { "" } else { "s" }
                        )),
                )
            });

        // The stage history, which until now was recorded and never shown.
        let history = (!app.history.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(label("History"))
                .children(app.history.iter().enumerate().map(|(i, change)| {
                    let to = ApplicationStatus::from_word(&change.to);
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_subtle)
                        .child(SharedString::from(short_date(&change.at)))
                        .child(Icon::new(IconName::ArrowRight).with_size(cx.theme().icon_sm()))
                        .child(SharedString::from(match to {
                            Some(stage) => status_title(stage).to_string(),
                            // A word this build does not know: shown as the
                            // file spells it rather than guessed at.
                            None => change.to.clone(),
                        }))
                        .child(
                            div()
                                .text_color(theme.text_muted)
                                .child(SharedString::from(format!(
                                    "from {}",
                                    status_title(app.stage_before(i))
                                ))),
                        )
                }))
        });

        Some(
            div()
                .absolute()
                .top_0()
                .right_0()
                .h_full()
                .w(px(PANEL_WIDTH))
                .flex()
                .flex_col()
                .bg(theme.surface)
                .border_l_1()
                .border_color(theme.border)
                .shadow_lg()
                // The board behind it scrolls, and a wheel over a panel must
                // move the panel rather than slide the thing you are reaching
                // into out from under the cursor (E-16).
                .occlude()
                .child(header)
                .child(
                    div()
                        .id("apps-detail-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .px(px(18.0))
                        .pb(px(24.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(facts)
                        .child(field("Company", &detail.company, "Acme"))
                        .child(field("Role", &detail.role, "Staff Engineer"))
                        .child(field("Posting", &detail.url, "https://…"))
                        .child(field("Compensation", &detail.compensation, "€128k + RSU"))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(5.0))
                                .child(label("Next step"))
                                .child(TextField::new(&detail.next_label).placeholder("Final panel"))
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(6.0))
                                        .child(
                                            div().flex_1().min_w_0().child(
                                                TextField::new(&detail.next_date)
                                                    .placeholder("2026-08-25"),
                                            ),
                                        )
                                        .child(
                                            div().w(px(96.0)).child(
                                                TextField::new(&detail.next_time)
                                                    .placeholder("14:00"),
                                            ),
                                        ),
                                ),
                        )
                        .child(self.rounds_block(cx, detail.index, app))
                        .child(self.closure_row(cx, detail.index, app))
                        .child(field("Notes", &detail.notes, "What you want to remember"))
                        // Only once there is an ending to explain. Asking why
                        // on a live application is a question about something
                        // that has not happened.
                        .when(app.closed_as.is_some(), |el| {
                            el.child(field(
                                "What happened",
                                &detail.rejection,
                                "No reply in six weeks",
                            ))
                        })
                        .children(history),
                )
                .child(self.detail_footer(cx, app))
                .into_any_element(),
        )
    }

    /// The panel's way out, said out loud.
    ///
    /// Every field here already writes on blur, so a button labelled *Save*
    /// would be describing something that has happened four keystrokes ago.
    /// What was missing is different: the only exit was a ✕ in the corner,
    /// which reads as *discard* — and on a card that has been opened but not
    /// named, discard is exactly what it does. So the footer names both
    /// outcomes instead of leaving the user to guess which one the ✕ is.
    fn detail_footer(&self, cx: &mut Context<Self>, app: &Application) -> impl IntoElement {
        let theme = *cx.theme();
        // Live, not from the saved card: the button has to react as the user
        // types the first letter of a company, before any blur has committed.
        let named = self
            .applications_detail
            .as_ref()
            .is_some_and(|d| !d.typed_name(cx).is_empty());
        // A card the user has never named is one this panel created a moment
        // ago; anything else is an application that already exists.
        let is_new = is_unnamed(app);

        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .px(px(18.0))
            .py(px(14.0))
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.chrome)
            .children(is_new.then(|| {
                Button::new("apps-detail-discard")
                    .quiet()
                    .label("Discard")
                    .tooltip("Close without keeping this application")
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.discard_application_detail(cx);
                    }))
            }))
            .child(
                Button::new("apps-detail-done")
                    .action_primary()
                    .label(if is_new { "Add application" } else { "Done" })
                    // Nothing to add until it has a name — and closing a blank
                    // card throws it away, so the primary action must not look
                    // like it would keep one.
                    .disabled(is_new && !named)
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.close_application_detail(cx);
                    })),
            )
    }
}

/// Where an open panel points after the card at `removed` is deleted.
///
/// `None` means the panel was on that card and has to close. Without this the
/// panel keeps an index into a list that has shifted under it, and the next
/// blur writes the fields of one application into another — the panel is a
/// writer, so a stale index is not a display bug.
pub(super) fn index_after_removal(open: usize, removed: usize) -> Option<usize> {
    use std::cmp::Ordering;
    match open.cmp(&removed) {
        Ordering::Equal => None,
        Ordering::Less => Some(open),
        Ordering::Greater => Some(open - 1),
    }
}

impl Shell {
    /// The interviews that have happened, and the button that records one.
    ///
    /// A list rather than a stage per round: the board keeps one
    /// `Interviewing` column, and how many times you have been through it is
    /// a count. See `InterviewRound`.
    fn rounds_block(
        &self,
        cx: &mut Context<Self>,
        index: usize,
        app: &Application,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        let scheduled = app.next_step.as_ref().map(|s| s.label.clone());

        div()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_style(TextStyle::label())
                            .text_color(theme.text_muted)
                            .child(match app.rounds.len() {
                                1 => "1 interview".to_string(),
                                n => format!("{n} interviews"),
                            }),
                    )
                    .child(
                        Button::new("apps-detail-log-round")
                            .quiet()
                            // Named after what it consumes when there is
                            // something to consume, so the button says what
                            // will happen rather than what it is called.
                            .label(match scheduled.as_deref() {
                                Some(label) if !label.is_empty() => format!("{label} happened"),
                                _ => "Log an interview".to_string(),
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.log_interview_round(index, window, cx);
                            })),
                    ),
            )
            .children(app.rounds.iter().enumerate().map(|(i, round)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child(SharedString::from(ordinal(i + 1)))
                    .child(SharedString::from(short_date(&round.at)))
                    .when(!round.label.is_empty(), |el| {
                        el.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(theme.text_muted)
                                .child(round.label.clone()),
                        )
                    })
            }))
    }

    /// How it ended — or that it has not.
    fn closure_row(
        &self,
        cx: &mut Context<Self>,
        index: usize,
        app: &Application,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        let current = app.closed_as;
        let root = cx.weak_entity();

        div()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(
                div()
                    .text_style(TextStyle::label())
                    .text_color(theme.text_muted)
                    .child("Outcome"),
            )
            .child(
                Button::new("apps-detail-closure")
                    .selector()
                    .w_full()
                    .label(match current {
                        None => "Still going".to_string(),
                        Some(c) => c.label().to_string(),
                    })
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        // "Still going" is the absence of an ending, and it
                        // has to be reachable: an application marked ghosted
                        // that then replies is the best possible reason to
                        // need this control to go backwards.
                        let clear = root.clone();
                        menu = menu
                            .item(
                                PopupMenuItem::new("Still going")
                                    .checked(current.is_none())
                                    .on_click(move |_ev, _window, cx| {
                                        let _ = clear.update(cx, |this, cx| {
                                            this.set_application_closure(index, None, cx);
                                        });
                                    }),
                            )
                            .separator();
                        for closure in Closure::ALL {
                            let root = root.clone();
                            menu = menu.item(
                                PopupMenuItem::new(closure.label())
                                    .checked(current == Some(closure))
                                    .on_click(move |_ev, _window, cx| {
                                        let _ = root.update(cx, |this, cx| {
                                            this.set_application_closure(
                                                index,
                                                Some(closure),
                                                cx,
                                            );
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
            )
    }
}

/// `1st`, `2nd`, `3rd`, `4th` — the teens are the exception every naive
/// implementation gets wrong.
fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// A card nobody has named. What "started and abandoned" looks like on disk.
fn is_unnamed(app: &Application) -> bool {
    app.company.trim().is_empty() && app.role.trim().is_empty()
}

fn none_if_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

/// A next step is only a step if it says *what* or *when*. All three empty is
/// no step at all, and storing an empty one would put a blank chip on the card.
fn next_step_from(label: String, date: String, time: String) -> Option<NextStep> {
    (!label.is_empty() || !date.is_empty()).then_some(NextStep { label, date, time })
}

#[cfg(test)]
mod tests {
    use super::{index_after_removal, next_step_from, none_if_empty};

    /// Deleting a card must never leave the panel writing into its neighbour.
    #[test]
    fn a_deletion_moves_or_closes_the_open_panel() {
        // The open card itself: nothing left to edit.
        assert_eq!(index_after_removal(3, 3), None);
        // Deleted above it: everything below shifts up by one.
        assert_eq!(index_after_removal(3, 1), Some(2));
        // Deleted below it: unaffected.
        assert_eq!(index_after_removal(3, 7), Some(3));
        assert_eq!(index_after_removal(0, 1), Some(0));
    }

    #[test]
    fn an_empty_next_step_is_no_step() {
        assert!(next_step_from(String::new(), String::new(), String::new()).is_none());
        // A time with nothing to be on time for is not a step either.
        assert!(next_step_from(String::new(), String::new(), "14:00".into()).is_none());
    }

    #[test]
    fn either_half_of_a_next_step_is_enough_to_keep_it() {
        let what_only = next_step_from("Onsite".into(), String::new(), String::new());
        assert_eq!(what_only.unwrap().label, "Onsite");
        let when_only = next_step_from(String::new(), "2026-08-25".into(), String::new());
        assert_eq!(when_only.unwrap().date, "2026-08-25");
    }

    #[test]
    fn an_empty_closure_note_is_absent_rather_than_blank() {
        assert_eq!(none_if_empty(String::new()), None);
        assert_eq!(none_if_empty("role filled".into()), Some("role filled".into()));
    }
}

#[cfg(test)]
mod ordinal_tests {
    use super::ordinal;

    #[test]
    fn the_teens_are_not_first_second_third() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(4), "4th");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(12), "12th");
        assert_eq!(ordinal(13), "13th");
        assert_eq!(ordinal(21), "21st");
        assert_eq!(ordinal(112), "112th");
    }
}

#[cfg(test)]
mod draft_tests {
    use super::is_unnamed;
    use crate::resume::model::Application;

    /// A card is created the moment "New application" is pressed, so the rule
    /// that throws an abandoned one away has to be exact: one filled field is
    /// a card someone meant to keep, and only a card with neither is a change
    /// of mind.
    #[test]
    fn only_a_card_with_neither_name_is_a_change_of_mind() {
        assert!(is_unnamed(&Application::default()));
        assert!(is_unnamed(&Application {
            company: "   ".into(),
            role: "\t".into(),
            ..Default::default()
        }));
        assert!(!is_unnamed(&Application {
            company: "Acme".into(),
            ..Default::default()
        }));
        assert!(!is_unnamed(&Application {
            role: "Staff Engineer".into(),
            ..Default::default()
        }));
    }
}

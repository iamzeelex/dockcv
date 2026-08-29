//! The one line that says a newer DockCV exists.
//!
//! Everything about the shape of this is a consequence of not wanting it:
//! updates are the app talking about itself, which is never why anyone opened
//! it. So it is **one row in the rail's own chrome**, next to the vault it
//! belongs beside — never a modal, never a toast that steals the pointer,
//! never on the editor screen, and never during an import. It waits.
//!
//! It appears in three states and no others: an offer to turn the weekly check
//! on (once, ever), a newer version with somewhere to go, and the outcome of a
//! check the user asked for. A check that finds nothing new says so for the
//! rest of the session and then stops; a check that fails says the failure and
//! points at the page, because "couldn't check" with no way forward is worse
//! than not checking.
//!
//! The work happens on the background executor. A check that blocked the frame
//! would make the whole app feel like it needs the network, which is the exact
//! impression this product cannot afford — and the timeout is five seconds,
//! which is five seconds of a spinner nobody is watching.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement, SharedString};

use dockcv_ui_components::{Button, ButtonExt, DockIcon, Icon, Sizable};

use crate::app::APP_VERSION;
use crate::config;
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::update::{self, Channel, CheckFailure, Release};
use crate::vault;

use super::shell::Shell;

/// What the rail currently has to say about versions. All of it is per
/// session; nothing here is state the vault or the config has to carry.
#[derive(Default)]
pub(super) struct UpdateState {
    /// A check is in flight.
    pub checking: bool,
    /// A release newer than this build, not one the user skipped.
    pub found: Option<Release>,
    /// Why the last check produced nothing.
    pub failure: Option<CheckFailure>,
    /// The last check found nothing newer. Shown only after a check the user
    /// asked for — an automatic one that finds nothing should say nothing.
    pub current: bool,
    /// The one-time offer has not been answered yet. Read from the config
    /// once per launch, never per frame — the rail redraws constantly and the
    /// config is a file.
    pub offer_pending: bool,
    /// Everything above has been initialised from the config, and the weekly
    /// check has had its one chance this launch.
    pub started: bool,
}

impl Shell {
    /// Read the update settings, once, and run the weekly check if it is due.
    ///
    /// Called from `Shell::render` on the vault screens. Everything after the
    /// first call is one bool: the config is a file, and a rail that redraws
    /// on every keystroke must not read it.
    pub(super) fn start_update_checks(&mut self, cx: &mut Context<Self>) {
        if self.update.started {
            return;
        }
        self.update.started = true;

        let config = config::load();
        self.update.offer_pending = !config.update_asked;
        if update::due(
            config.update_channel(),
            Some(config.update_last_checked.as_str()).filter(|d| !d.is_empty()),
            &vault::iso_days_ago(7),
        ) {
            self.check_for_update(false, cx);
        }
    }

    /// Ask the feed. `announced` is true when the user pressed the button, and
    /// is the only difference between the two paths: a check somebody asked
    /// for owes them an answer even when the answer is "nothing new".
    pub(super) fn check_for_update(&mut self, announced: bool, cx: &mut Context<Self>) {
        if self.update.checking {
            return;
        }
        self.update.checking = true;
        self.update.failure = None;
        self.update.current = false;
        cx.notify();

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let outcome = executor
                .spawn(async move { update::check(update::FEED) })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.update.checking = false;
                // The date is recorded on any completed round trip, including
                // a failed one: otherwise a week offline turns into a check on
                // every launch, which is the opposite of what weekly means.
                config::record_update_check(&vault::today_iso());

                match outcome {
                    Ok(release) => {
                        let skipped = config::load().update_skipped;
                        let newer = update::is_newer(&release.version, APP_VERSION);
                        let wanted = newer && release.version != skipped;
                        log::info!(
                            "update check: latest {}, running {APP_VERSION}",
                            release.version
                        );
                        this.update.current = announced && !newer;
                        this.update.found = wanted.then_some(release);
                    }
                    Err(failure) => {
                        // Only ever surfaced for a check the user asked for.
                        // An automatic one failing is the network's business,
                        // not theirs, and the log already has it.
                        this.update.failure = announced.then_some(failure);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Never mention this version again. The next one is announced normally.
    fn skip_this_update(&mut self, cx: &mut Context<Self>) {
        if let Some(release) = self.update.found.take() {
            config::skip_update(&release.version);
        }
        cx.notify();
    }

    /// Hand the download to the browser — see `update.rs` for why the app
    /// never fetches a binary itself.
    fn open_release_page(&mut self, cx: &mut Context<Self>) {
        if let Some(release) = self.update.found.as_ref() {
            cx.open_url(&release.page);
        }
    }

    /// Answer the one-time offer, either way, and never ask again.
    fn answer_update_offer(&mut self, weekly: bool, cx: &mut Context<Self>) {
        self.update.offer_pending = false;
        if weekly {
            config::set_update_channel(Channel::Weekly);
            self.check_for_update(false, cx);
        } else {
            config::mark_update_asked();
        }
        cx.notify();
    }

    /// The row, when there is one. `None` is the ordinary case and the one the
    /// rail is designed around.
    pub(super) fn render_update_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = *cx.theme();

        // A row of the rail: same inset as its neighbours, quiet until it has
        // something to say.
        let row = || {
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(theme.radius_md())
                .bg(theme.elevated)
                .border_1()
                .border_color(theme.border)
        };
        let line = |text: SharedString, colour| {
            div()
                .text_style(TextStyle::meta())
                .text_color(colour)
                .child(text)
        };

        if self.update.checking {
            return Some(
                row()
                    .child(line("Checking for updates…".into(), theme.text_subtle))
                    .into_any_element(),
            );
        }

        if let Some(release) = self.update.found.as_ref() {
            let version = release.version.clone();
            return Some(
                row()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                Icon::new(DockIcon::Download)
                                    .with_size(theme.icon_sm())
                                    .text_color(theme.accent),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::label())
                                    .text_color(theme.text)
                                    .child(SharedString::from(format!("DockCV {version}"))),
                            ),
                    )
                    // What it is, in the user's terms: not installed, not
                    // downloaded, nothing happening until they say so.
                    .child(line(
                        SharedString::from(if release.published.is_empty() {
                            "is available to download.".to_string()
                        } else {
                            format!("published {}.", release.published)
                        }),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                Button::new("update-open")
                                    .chip(false, &theme)
                                    .label("What's new")
                                    .tooltip("Opens the release page in your browser")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.open_release_page(cx);
                                    })),
                            )
                            .child(
                                Button::new("update-skip")
                                    .quiet()
                                    .label("Skip")
                                    .tooltip("Don't mention this version again")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.skip_this_update(cx);
                                    })),
                            ),
                    )
                    .into_any_element(),
            );
        }

        if let Some(failure) = self.update.failure {
            return Some(
                row()
                    .child(line(failure.message().into(), theme.text_muted))
                    .into_any_element(),
            );
        }

        if self.update.current {
            return Some(
                row()
                    .child(line(
                        SharedString::from(format!("DockCV {APP_VERSION} is the newest.")),
                        theme.text_subtle,
                    ))
                    .into_any_element(),
            );
        }

        // The one-time offer. A line, not a dialog, and it does not come back
        // whichever way it is answered.
        if self.update.offer_pending {
            return Some(
                row()
                    .child(line(
                        "Check for updates weekly? It asks github.com for a version \
                         number and sends nothing about you."
                            .into(),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                Button::new("update-offer-yes")
                                    .chip(false, &theme)
                                    .label("Yes")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.answer_update_offer(true, cx);
                                    })),
                            )
                            .child(
                                Button::new("update-offer-no")
                                    .quiet()
                                    .label("No thanks")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.answer_update_offer(false, cx);
                                    })),
                            ),
                    )
                    .into_any_element(),
            );
        }

        None
    }
}

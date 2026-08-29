//! Settings, in a window of its own (O-21).
//!
//! It used to be a pane inside the vault rail, which is why upstream's
//! `Settings` component could not be used: the component brings its own
//! navigation column, and a second one beside the rail is the "two navigations
//! for one move" the design rules forbid. In a window that objection
//! disappears — the component's column *is* the navigation — so the hand-built
//! `GroupBox` version this replaces is gone with it.
//!
//! The window owns no data. Every control reaches back into the running
//! [`Shell`] through a weak handle, so the vault path, the trash count and the
//! theme are read from the one place that has them and written the same way the
//! pane wrote them. Nothing here is a second copy of application state.
//!
//! `SettingField`'s value and setter closures take `&App` / `&mut App` and
//! never a `Window`, which is what made this shape look impossible from a view.
//! It is not: a control built inside the component's *render* closure gets its
//! own `Window` and `App` from the click handler at click time, and a weak
//! `Shell` handle turns that into a `Context<Shell>`.

use gpui::prelude::*;
use gpui::{div, px, App, ClickEvent, Context, IntoElement, SharedString, WeakEntity, Window};

use dockcv_ui_components::{
    Button, ButtonExt, ButtonVariants, SettingField, SettingGroup, SettingItem, SettingPage,
    Settings,
};

use crate::config;
use crate::logging;
use crate::theme::{ActiveTheme, StyledText, TextStyle, Theme, ThemeMode};
use crate::update::Channel;
use crate::vault;

use super::shell::Shell;

pub struct SettingsWindow {
    shell: WeakEntity<Shell>,
}

impl SettingsWindow {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        Self { shell }
    }
}

/// A palette choice. A chip previewing its own palette, not a name in a
/// dropdown — the user compares rather than reads, which is the whole reason
/// this is not a `SettingField::dropdown`.
fn theme_chip(shell: &WeakEntity<Shell>, mode: ThemeMode, cx: &mut App) -> impl IntoElement {
    let theme = *cx.theme();
    let preview = Theme::of(mode);
    let active = theme.mode == mode;
    let shell = shell.clone();

    Button::new(SharedString::from(format!("theme-{mode:?}")))
        .chip(active, &theme)
        .h(theme.control_md())
        .gap_2()
        // The chosen palette gets the accent edge as well as the wash: this is
        // a setting in force, not a highlight in a list.
        .when(active, |el| el.border_1().border_color(theme.accent))
        .on_click(move |_: &ClickEvent, _window, cx| {
            let _ = shell.update(cx, |shell, cx| shell.set_theme(mode, cx));
        })
        .child(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .rounded_full()
                .bg(preview.background)
                .border_1()
                .border_color(preview.accent),
        )
        .child(mode.label())
}

/// How often DockCV may ask whether a newer version exists.
///
/// Three chips rather than a switch, because "off" and "only when I ask" are
/// genuinely different answers and a switch can only hold two. The wording is
/// the user's side of it — `When I ask`, not `manual`.
fn update_chip(shell: &WeakEntity<Shell>, channel: Channel, cx: &mut App) -> impl IntoElement {
    let theme = *cx.theme();
    let active = config::load().update_channel() == channel;
    let shell = shell.clone();

    Button::new(SharedString::from(format!("update-{}", channel.word())))
        .chip(active, &theme)
        .h(theme.control_md())
        .when(active, |el| el.border_1().border_color(theme.accent))
        .on_click(move |_: &ClickEvent, _window, cx| {
            config::set_update_channel(channel);
            let _ = shell.update(cx, |shell, cx| {
                shell.update.offer_pending = false;
                if channel == Channel::Weekly {
                    shell.check_for_update(true, cx);
                }
                cx.notify();
            });
        })
        .child(channel.label())
}

fn action_button(
    shell: &WeakEntity<Shell>,
    id: &'static str,
    label: String,
    danger: bool,
    // Takes the `Window` too, because a destructive action has to be able to
    // put an alert in front of itself, and an alert belongs to a window.
    action: fn(&mut Shell, &mut Window, &mut Context<Shell>),
) -> impl IntoElement {
    let shell = shell.clone();
    Button::new(id)
        .toolbar()
        // Destructive actions wear the danger variant rather than only a red
        // label: the border and the pressed state move with it.
        .when(danger, |el| el.danger())
        .on_click(move |_: &ClickEvent, window, cx| {
            let _ = shell.update(cx, |shell, cx| action(shell, window, cx));
        })
        .child(label)
}

/// A read-only value. Mono where the value is data — a path is data.
fn readout(text: String, mono: bool, cx: &App) -> impl IntoElement {
    div()
        .text_style(if mono {
            TextStyle::code()
        } else {
            TextStyle::body()
        })
        .text_color(cx.theme().text)
        .child(text)
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .size_full()
            .bg(theme.background)
            .text_color(theme.text)
            .child(
                Settings::new("settings")
                    .sidebar_width(px(190.0))
                    .page(general_page(&self.shell))
                    .page(storage_page(&self.shell)),
            )
    }
}

fn general_page(shell: &WeakEntity<Shell>) -> SettingPage {
    let for_vault = shell.clone();
    let for_theme = shell.clone();
    let for_updates = shell.clone();
    let for_check = shell.clone();

    SettingPage::new("General")
        .description("Where your work lives, and how the app looks.")
        .group(
            SettingGroup::new()
                .title("Vault")
                .description(
                    "Your CVs, library and diary are plain text files in it \u{2014} open them \
                     in any editor, copy them anywhere, keep them in a backup you control.",
                )
                .item(SettingItem::new(
                    "Folder",
                    SettingField::render(move |_o, _window, cx: &mut App| {
                        let path = for_vault
                            .read_with(cx, |shell, _| {
                                shell
                                    .vault
                                    .as_ref()
                                    .map(|v| v.to_string_lossy().to_string())
                            })
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| "—".to_string());
                        readout(path, true, cx).into_any_element()
                    }),
                )),
        )
        .group(
            SettingGroup::new()
                .title("Updates")
                .description(
                    "DockCV works offline and never sends your CVs anywhere. A check looks \
                     up the latest version number and nothing else, and nothing is ever \
                     downloaded or installed without you.",
                )
                .item(SettingItem::new(
                    "Check",
                    SettingField::render(move |_o, _window, cx: &mut App| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .children(
                                Channel::ALL.map(|channel| update_chip(&for_updates, channel, cx)),
                            )
                            .into_any_element()
                    }),
                ))
                .item(SettingItem::new(
                    "Now",
                    SettingField::render(move |_o, _window, cx: &mut App| {
                        let shell = for_check.clone();
                        let (checking, last) = shell
                            .read_with(cx, |shell, _| shell.update.checking)
                            .map(|checking| (checking, config::load().update_last_checked))
                            .unwrap_or((false, String::new()));
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                Button::new("settings-check-updates")
                                    .toolbar()
                                    .on_click(move |_: &ClickEvent, _window, cx| {
                                        let _ = shell.update(cx, |shell, cx| {
                                            shell.check_for_update(true, cx)
                                        });
                                    })
                                    .child(if checking {
                                        "Checking\u{2026}".to_string()
                                    } else {
                                        "Check now".to_string()
                                    }),
                            )
                            // The answer appears in the rail, where it can be
                            // acted on; this row only says when it last ran,
                            // which is the question a settings screen is for.
                            .child(readout(
                                if last.is_empty() {
                                    "not checked yet".to_string()
                                } else {
                                    format!("last checked {last}")
                                },
                                true,
                                cx,
                            ))
                            .into_any_element()
                    }),
                )),
        )
        .group(
            SettingGroup::new()
                .title("Appearance")
                .description("Applies immediately, everywhere.")
                .item(SettingItem::new(
                    "Theme",
                    SettingField::render(move |_o, _window, cx: &mut App| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .children(ThemeMode::ALL.map(|mode| theme_chip(&for_theme, mode, cx)))
                            .into_any_element()
                    }),
                )),
        )
}

fn storage_page(shell: &WeakEntity<Shell>) -> SettingPage {
    let for_buttons = shell.clone();

    SettingPage::new("Storage")
        .description("Housekeeping that touches files on disk.")
        .group(
            SettingGroup::new()
                .title("Trash")
                .description(
                    "Deleted CVs move to the vault's .trash folder; emptying it is permanent.",
                )
                .item(SettingItem::new(
                    "Maintenance",
                    SettingField::render(move |_o, _window, cx: &mut App| {
                        let count = for_buttons
                            .read_with(cx, |shell, _| {
                                shell
                                    .vault
                                    .as_ref()
                                    .map(|v| vault::trash_count(v))
                                    .unwrap_or(0)
                            })
                            .unwrap_or(0);
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(action_button(
                                &for_buttons,
                                "settings-empty-trash",
                                format!("Empty trash ({count})"),
                                true,
                                Shell::empty_trash,
                            ))
                            .child(action_button(
                                &for_buttons,
                                "settings-rebuild-thumbs",
                                "Rebuild thumbnails".to_string(),
                                false,
                                Shell::rebuild_thumbnails,
                            ))
                            .into_any_element()
                    }),
                )),
        )
        // Between Trash and About, because that is the order the questions
        // arrive in: what is on disk, what to do when something went wrong,
        // what version this is.
        .group(
            SettingGroup::new()
                .title("If something goes wrong")
                .description(
                    "DockCV keeps a log of what it did \u{2014} vaults opened, imports, \
                     exports, and every failed read or write. It records what happened and \
                     never what you wrote: no CV text, no diary entries, and your home \
                     folder is written as ~ rather than by your account name. It stays on \
                     this computer; nothing sends it anywhere.",
                )
                .item(SettingItem::new(
                    "Log",
                    SettingField::render(|_o, _window, _cx: &mut App| {
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                Button::new("settings-open-log")
                                    .toolbar()
                                    .on_click(|_: &ClickEvent, _window, cx: &mut App| {
                                        cx.open_with_system(&logging::log_path());
                                    })
                                    .child("Open log"),
                            )
                            .child(
                                // The folder, not the file: a log is something
                                // a person drags into a bug report, and the
                                // previous session's is in there too.
                                Button::new("settings-reveal-log")
                                    .toolbar()
                                    .on_click(|_: &ClickEvent, _window, cx: &mut App| {
                                        if let Some(dir) = logging::log_path().parent() {
                                            cx.open_with_system(dir);
                                        }
                                    })
                                    .child("Show in Finder"),
                            )
                            .into_any_element()
                    }),
                ))
                .item(SettingItem::new(
                    "File",
                    SettingField::render(|_o, _window, cx: &mut App| {
                        readout(logging::log_path().to_string_lossy().to_string(), true, cx)
                            .into_any_element()
                    }),
                )),
        )
        .group(
            SettingGroup::new()
                .title("About")
                .description("Worth quoting if you ever report a problem.")
                .item(SettingItem::new(
                    "Version",
                    SettingField::render(|_o, _window, cx: &mut App| {
                        // Mono, because a version is data — and present at all,
                        // because this row used to read the literal word
                        // "DockCV", which made a bug report impossible to place
                        // against a build.
                        readout(
                            format!("{} {}", crate::app::APP_NAME, crate::app::APP_VERSION),
                            true,
                            cx,
                        )
                        .into_any_element()
                    }),
                )),
        )
}

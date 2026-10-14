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
use gpui::{
    div, px, App, ClickEvent, Context, Entity, FontWeight, IntoElement, SharedString, WeakEntity,
    Window,
};

use dockcv_ui_components::{
    Button, ButtonExt, ButtonVariants, SettingField, SettingGroup, SettingItem, SettingPage,
    Settings, TextField, TextFieldState, SANS,
};

use crate::config;
use crate::logging;
use crate::theme::{ActiveTheme, StyledText, TextStyle, Theme, ThemeMode};
use crate::update::Channel;
use crate::vault;
use crate::views::assistant;

use super::shell::Shell;

pub struct SettingsWindow {
    shell: WeakEntity<Shell>,
    custom_cmd_field: Option<Entity<TextFieldState>>,
}

impl SettingsWindow {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        Self {
            shell,
            custom_cmd_field: None,
        }
    }

    fn ensure_custom_cmd_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.custom_cmd_field.is_none() {
            let initial = config::load().assistant_custom_cmd.unwrap_or_default();
            let state = cx.new(|cx| {
                let s = TextFieldState::single_line(window, cx);
                if !initial.is_empty() {
                    s.set_value(initial, window, cx);
                }
                s
            });
            self.custom_cmd_field = Some(state);
        }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_custom_cmd_field(window, cx);
        let theme = *cx.theme();

        div()
            .size_full()
            .bg(theme.background)
            .text_color(theme.text)
            .child(
                Settings::new("settings")
                    .sidebar_width(px(190.0))
                    .page(general_page(&self.shell))
                    .page(assistants_page(&self.shell, &self.custom_cmd_field))
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

fn assistants_page(
    shell: &WeakEntity<Shell>,
    custom_cmd_field: &Option<Entity<TextFieldState>>,
) -> SettingPage {
    let custom_cmd_for_field = custom_cmd_field.clone();
    let shell_for_pref = shell.clone();
    let shell_for_custom = shell.clone();
    let shell_for_presets = shell.clone();

    SettingPage::new("Assistants")
        .description("Local AI tools, Claude Desktop, and privacy settings.")
        .group(
            SettingGroup::new()
                .title("Preferred Assistant")
                .description(
                    "Which assistant to use by default when requesting advice or polishing \
                     highlights in the editor or tailor screen.",
                )
                .item(SettingItem::new(
                    "Default",
                    SettingField::render(move |_o, _window, cx: &mut App| {
                        let current = config::load()
                            .assistant_preferred
                            .unwrap_or_else(|| "auto".to_string());
                        let options = [
                            ("auto", "Auto (First available)"),
                            ("claude-desktop", "Claude Desktop"),
                            ("ollama", "Ollama"),
                            ("codex", "Codex"),
                            ("gemini", "Gemini"),
                            ("claude-cli", "Claude CLI"),
                            ("custom", "Custom Command"),
                            ("browser", "Browser (Clipboard)"),
                        ];
                        let for_pref = shell_for_pref.clone();
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .children(options.into_iter().map(|(id, label)| {
                                let active = current == id;
                                let for_pref = for_pref.clone();
                                Button::new(SharedString::from(format!("pref-asst-{id}")))
                                    .chip(active, cx.theme())
                                    .h(cx.theme().control_md())
                                    .when(active, |el| el.border_1().border_color(cx.theme().accent))
                                    .on_click(move |_: &ClickEvent, _window, cx| {
                                        config::set_preferred_assistant(if id == "auto" {
                                            None
                                        } else {
                                            Some(id.to_string())
                                        });
                                        let _ = for_pref.update(cx, |_, cx| cx.notify());
                                    })
                                    .child(label)
                            }))
                            .into_any_element()
                    }),
                ))
                .item(SettingItem::new(
                    "Active route",
                    SettingField::render(move |_o, _window, cx: &mut App| {
                        let pref = config::load()
                            .assistant_preferred
                            .unwrap_or_else(|| "auto".to_string());
                        let resolved = match pref.as_str() {
                            "auto" => {
                                let all = assistant::detect_all_assistants();
                                if let Some(first) = all.iter().find(|a| a.available) {
                                    format!("{} ({})", first.name, first.detail)
                                } else if config::load().assistant_custom_cmd.is_some() {
                                    "Custom Command".to_string()
                                } else {
                                    "Browser / Clipboard (no local tools detected)".to_string()
                                }
                            }
                            "claude-desktop" => {
                                if assistant::claude_desktop() {
                                    "Claude Desktop (Available via claude://)".to_string()
                                } else {
                                    "Claude Desktop (Not detected in /Applications)".to_string()
                                }
                            }
                            "ollama" => {
                                if let Some(p) = assistant::find_executable("ollama") {
                                    format!("Ollama CLI ({})", p.display())
                                } else {
                                    "Ollama CLI (Not found in PATH)".to_string()
                                }
                            }
                            "codex" => {
                                if let Some(p) = assistant::find_executable("codex") {
                                    format!("Codex CLI ({})", p.display())
                                } else {
                                    "Codex CLI (Not found in PATH)".to_string()
                                }
                            }
                            "gemini" => {
                                if let Some(p) = assistant::find_executable("gemini") {
                                    format!("Gemini CLI ({})", p.display())
                                } else {
                                    "Gemini CLI (Not found in PATH)".to_string()
                                }
                            }
                            "claude-cli" => {
                                if let Some(p) = assistant::find_executable("claude") {
                                    format!("Claude CLI ({})", p.display())
                                } else {
                                    "Claude CLI (Not found in PATH)".to_string()
                                }
                            }
                            "custom" => config::load()
                                .assistant_custom_cmd
                                .unwrap_or_else(|| "Not configured yet".to_string()),
                            "browser" => "Web Browser / System Clipboard".to_string(),
                            other => other.to_string(),
                        };
                        readout(resolved, true, cx).into_any_element()
                    }),
                )),
        )
        .group(
            SettingGroup::new()
                .title("Auto-Detected Local Tools")
                .description(
                    "DockCV searches your system PATH and standard locations (/opt/homebrew/bin, \
                     /usr/local/bin, ~/.cargo/bin) without network lookups or telemetry.",
                )
                .item(SettingItem::new(
                    "Detected",
                    SettingField::render(|_o, _window, cx: &mut App| {
                        let theme = *cx.theme();
                        let assistants = assistant::detect_all_assistants();
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .w_full()
                            .children(assistants.into_iter().map(|item| {
                                let is_avail = item.available;
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .p_2()
                                    .rounded(theme.radius_sm())
                                    .bg(theme.canvas)
                                    .border_1()
                                    .border_color(theme.border)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .font_family(SANS)
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_size(px(13.0))
                                                            .text_color(theme.text)
                                                            .child(item.name),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_style(TextStyle::meta())
                                                            .text_color(theme.text_subtle)
                                                            .child(format!("({})", item.kind)),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_style(TextStyle::code())
                                                    .text_color(if is_avail {
                                                        theme.text_muted
                                                    } else {
                                                        theme.text_subtle
                                                    })
                                                    .child(item.detail),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py(px(2.0))
                                            .rounded_full()
                                            .text_style(TextStyle::meta())
                                            .border_1()
                                            .border_color(if is_avail {
                                                theme.success
                                            } else {
                                                theme.border
                                            })
                                            .text_color(if is_avail {
                                                theme.success
                                            } else {
                                                theme.text_subtle
                                            })
                                            .child(if is_avail {
                                                "Installed"
                                            } else {
                                                "Not found"
                                            }),
                                    )
                            }))
                            .into_any_element()
                    }),
                )),
        )
        .group(
            SettingGroup::new()
                .title("Custom Local Command")
                .description(
                    "Execute your own local LLM runner (e.g. Ollama, llama.cpp, LocalAI, vLLM). \
                     DockCV appends the prompt as the final argument and reads stdout.",
                )
                .item(SettingItem::new(
                    "Command",
                    SettingField::render(move |_o, _window, cx: &mut App| {
                        let theme = *cx.theme();
                        let current_cmd = config::load().assistant_custom_cmd.unwrap_or_default();
                        let field_el = if let Some(field) = custom_cmd_for_field.as_ref() {
                            div()
                                .w_full()
                                .max_w(px(380.0))
                                .child(TextField::new(field).placeholder("e.g. ollama run llama3.2"))
                        } else {
                            div().child(readout(current_cmd.clone(), true, cx))
                        };

                        let save_field = custom_cmd_for_field.clone();
                        let shell_for_custom = shell_for_custom.clone();
                        let shell_for_presets = shell_for_presets.clone();
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .w_full()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(field_el)
                                    .child(
                                        Button::new("save-custom-cmd")
                                            .toolbar()
                                            .on_click(move |_: &ClickEvent, _window, cx: &mut App| {
                                                if let Some(f) = save_field.as_ref() {
                                                    let val = f.read(cx).value(cx).trim().to_string();
                                                    config::set_custom_assistant_cmd(if val.is_empty() {
                                                        None
                                                    } else {
                                                        Some(val)
                                                    });
                                                    let _ = shell_for_custom.update(cx, |_, cx| cx.notify());
                                                }
                                            })
                                            .child("Save"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_style(TextStyle::meta())
                                            .text_color(theme.text_subtle)
                                            .child("Quick presets:"),
                                    )
                                    .children([
                                        "ollama run llama3.2",
                                        "ollama run mistral",
                                        "ollama run qwen2.5:7b",
                                    ].map(|preset| {
                                        let f = custom_cmd_for_field.clone();
                                        let shell_for_presets = shell_for_presets.clone();
                                        Button::new(SharedString::from(format!("preset-{preset}")))
                                            .quiet()
                                            .text_size(px(11.0))
                                            .on_click(move |_: &ClickEvent, window, cx: &mut App| {
                                                if let Some(f) = f.as_ref() {
                                                    f.update(cx, |field, cx| {
                                                        field.set_value(preset.to_string(), window, cx);
                                                    });
                                                }
                                                config::set_custom_assistant_cmd(Some(preset.to_string()));
                                                let _ = shell_for_presets.update(cx, |_, cx| cx.notify());
                                            })
                                            .child(preset)
                                    })),
                            )
                            .into_any_element()
                    }),
                )),
        )
        .group(
            SettingGroup::new()
                .title("Privacy & Guarantees")
                .description(
                    "DockCV includes zero cloud API keys, zero network telemetry, and makes no background requests. \
                     All assistants run strictly locally on your machine or copy prompts to your clipboard for your review. \
                     No changes are ever written to your CV without your explicit review and confirmation.",
                )
                .item(SettingItem::new(
                    "Network access",
                    SettingField::render(|_o, _window, cx: &mut App| {
                        readout("100% Offline / Local Execution Only".to_string(), false, cx).into_any_element()
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

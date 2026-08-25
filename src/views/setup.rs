//! Setup screen rendering for `Shell`.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, IntoElement, Window};

use dockcv_ui_components::{Button, ButtonExt, Card, Sizable};

use crate::theme::ActiveTheme;

use super::shell::{Screen, Shell};

impl Shell {
    pub(super) fn render_setup(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        let content = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .w(px(520.0))
            .child(
                div()
                    .text_color(theme.text)
                    .text_2xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Set up your vault"),
            )
            .child(
                div()
                    .text_center()
                    .text_color(theme.text_muted)
                    .child("A cvault is a plain folder of open TOML files you own and control."),
            )
            .child(self.setup_card(
                cx,
                "setup-create",
                "Create a new vault",
                "Pick a folder — we'll create a cvault inside it.",
                Box::new(|this, _window, cx| this.create_new_vault(cx)),
            ))
            .child(self.setup_card(
                cx,
                "setup-open",
                "Open an existing vault",
                "Choose a cvault folder you already have.",
                Box::new(|this, window, cx| this.open_existing_vault(window, cx)),
            ))
            // The one place DockCV reaches the network, and it says so.
            // "No network calls" (US-10) is a promise about the app: it holds
            // for the editor, the preview, the fonts and every importer. Here
            // the user asks for a repository to be fetched, and `git` — their
            // git, with their credentials — does the fetching. Stating it on
            // the card is cheaper than a footnote nobody reads.
            .child(self.setup_card(
                cx,
                "setup-clone",
                "Clone from Git",
                "Copy a repo URL to the clipboard, then pick where to clone it. \
                 Runs your own git, and is the only thing in DockCV that uses the network.",
                Box::new(|this, _window, cx| this.clone_from_git(cx)),
            ))
            .child(
                div()
                    .h(px(20.0))
                    .when_some(self.setup_error.clone(), |row, error| {
                        row.text_sm().text_color(theme.danger).child(error)
                    }),
            )
            .child(
                Button::new("setup-back")
                    .quiet()
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.screen = Screen::Welcome;
                        cx.notify();
                    }))
                    .child("← Back"),
            );

        self.backdrop(cx)
            .child(self.fade_in("setup-enter", content))
    }

    #[allow(clippy::type_complexity)]
    fn setup_card(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        title: &'static str,
        description: &'static str,
        // Carries the `Window` because opening a vault may have to put a
        // confirmation in front of itself.
        action: Box<dyn Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static>,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        Card::new()
            .surface()
            .small()
            .interactive(id)
            .flex()
            .flex_col()
            .gap_1()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                action(this, window, cx);
            }))
            .child(div().text_color(theme.text).text_sm().child(title))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(description),
            )
    }
}

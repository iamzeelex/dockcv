//! Setup screen rendering for `Shell`.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, IntoElement};

use crate::theme::ActiveTheme;

use super::shell::{Screen, Shell};

impl Shell {
    pub(super) fn render_setup(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

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
                Box::new(|this, cx| this.create_new_vault(cx)),
            ))
            .child(self.setup_card(
                cx,
                "setup-open",
                "Open an existing vault",
                "Choose a cvault folder you already have.",
                Box::new(|this, cx| this.open_existing_vault(cx)),
            ))
            .child(self.setup_card(
                cx,
                "setup-clone",
                "Clone from Git",
                "Copy a repo URL to the clipboard, then pick where to clone it.",
                Box::new(|this, cx| this.clone_from_git(cx)),
            ))
            .child(
                div()
                    .h(px(20.0))
                    .when_some(self.setup_error.clone(), |row, error| {
                        row.text_sm().text_color(theme.danger).child(error)
                    }),
            )
            .child(
                div()
                    .id("setup-back")
                    .text_xs()
                    .text_color(theme.text_muted)
                    .cursor_pointer()
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
        action: Box<dyn Fn(&mut Self, &mut Context<Self>) + 'static>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .id(id)
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .px_4()
            .py_3()
            .rounded_lg()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .cursor_pointer()
            .hover(|s| s.border_color(theme.accent))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                action(this, cx);
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

//! Welcome screen rendering for `Shell`.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, IntoElement};

use dockcv_ui_components::{Button, ButtonExt};

use crate::theme::ActiveTheme;

use super::shell::{Screen, Shell};

impl Shell {
    pub(super) fn render_welcome(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        let content = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_5()
            .child(
                div()
                    .text_color(theme.accent)
                    .text_xs()
                    .child("LOCAL-FIRST · TYPST-POWERED"),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .child(
                        div()
                            .text_color(theme.text)
                            .child("Dock")
                            .text_3xl()
                            .font_weight(gpui::FontWeight::BOLD),
                    )
                    .child(
                        div()
                            .text_color(theme.accent)
                            .child("CV")
                            .text_3xl()
                            .font_weight(gpui::FontWeight::BOLD),
                    ),
            )
            .child(
                div()
                    .max_w(px(440.0))
                    .text_center()
                    .text_color(theme.text_muted)
                    .child(
                        "Craft CVs tailored to each role — version every section, \
                         compose with presets, and keep everything in open files you own.",
                    ),
            )
            .child(
                Button::new("get-started")
                    .action_primary()
                    .mt_4()
                    // The one control on the first screen of the product: it
                    // keeps the pill shape the welcome art is built around.
                    .rounded_full()
                    .px_6()
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.screen = Screen::Setup;
                        cx.notify();
                    }))
                    .child("Get started  →"),
            );

        self.backdrop(cx)
            .child(self.fade_in("welcome-enter", content))
    }
}

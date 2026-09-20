//! Drawing the assistant hand-off. Which routes exist and what they do is in
//! `import_assistant.rs`, which stays free of layout so the prompts and the
//! trust marks can be read in one place.
//!
//! The shape is one row per assistant, and that is the fix rather than the
//! decoration. Eleven buttons in three unlabelled bands asked "which of these
//! is mine" first, which is the one question the reader already knows the
//! answer to. They have an assistant; what they do not know is where they can
//! use it. So the name is the heading and the routes sit under it, each with a
//! glyph for the *kind* of thing it is — a terminal, an app window, a browser
//! — because that is what decides whether they will be pasting anything back.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, Div, FontWeight, SharedString};

use dockcv_ui_components::{lucide, Button, ButtonExt, Icon, Sizable, Spinner, MONO, SANS};

use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::import_assistant::{route_url, LocalRun, Route, Trust, Via, ASSISTANTS};
use super::shell::Shell;

impl Shell {
    /// The panel under a PDF that turned out to be a picture.
    pub(super) fn render_assistant_handoff(
        &self,
        cx: &mut Context<Self>,
        path: Option<&std::path::Path>,
    ) -> Div {
        let theme = *cx.theme();
        let Some(path) = path else {
            return div();
        };
        let running = self.import_run.is_some();

        div()
            .mt(px(18.0))
            .pt(px(16.0))
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("Or have an assistant read it"),
                    )
                    .child(
                        div()
                            .max_w(px(620.0))
                            .text_size(px(11.5))
                            .line_height(px(17.0))
                            .text_color(theme.text_muted)
                            .child(
                                "It transcribes the pages and answers with a CV DockCV can \
                                 read. You see every section before anything is saved.",
                            ),
                    ),
            )
            .children(
                self.import_run
                    .as_ref()
                    .map(|run| self.render_local_run(cx, run)),
            )
            .children((!running).then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .children(
                        ASSISTANTS
                            .iter()
                            .filter(|a| a.routes.iter().any(|r| r.via.available()))
                            .map(|assistant| {
                                self.render_assistant_row(cx, assistant.name, assistant.routes, path)
                            }),
                    )
            }))
            .children((!running).then(|| self.render_anything_else(cx, path)))
            .children((!running).then(|| self.render_bring_it_back(cx)))
    }

    /// One assistant, and everywhere it can be reached from here.
    fn render_assistant_row(
        &self,
        cx: &mut Context<Self>,
        name: &'static str,
        routes: &'static [Route],
        path: &std::path::Path,
    ) -> Div {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .py(px(7.0))
            .border_b_1()
            .border_color(theme.border.opacity(0.5))
            .child(
                div()
                    .flex_none()
                    .w(px(96.0))
                    .font_family(SANS)
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(name),
            )
            .children(
                routes
                    .iter()
                    .filter(|route| route.via.available())
                    .map(|route| self.render_route(cx, route, path)),
            )
    }

    /// One way in, with a glyph for the kind of thing it is.
    fn render_route(
        &self,
        cx: &mut Context<Self>,
        route: &'static Route,
        path: &std::path::Path,
    ) -> Div {
        let theme = *cx.theme();
        let file = path.to_path_buf();
        let via = route.via;

        div()
            .flex()
            .items_center()
            .gap(px(3.0))
            .child(
                Button::new(SharedString::from(route.id))
                    .quiet()
                    .icon(route.via.icon())
                    .text_color(theme.text)
                    .tooltip(match route.via {
                        Via::Cli { .. } => "Runs here and reads the answer itself",
                        Via::Cowork => "Opens Cowork with the PDF attached",
                        Via::Code => "Opens on the folder the PDF is in",
                        Via::Web { .. } => "Opens your browser — you attach the file",
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        match via {
                            Via::Cli { program, argv } => {
                                this.run_local_assistant(program, argv, file.clone(), cx)
                            }
                            other => {
                                // The clipboard every time, not only when the
                                // link cannot carry the prompt. Cowork
                                // attached the file and left the composer
                                // empty, and a person looking at an empty
                                // composer should already have the answer in
                                // their hand rather than a reason to come back.
                                this.stage_transcription(Some(file.clone()), cx);
                                if let Some(url) = route_url(other, &file) {
                                    cx.open_url(&url);
                                }
                            }
                        }
                    }))
                    .child(route.label),
            )
            .children(self.trust_mark(cx, route.trust))
    }

    /// For the assistant that is not on the list, which is most of them.
    fn render_anything_else(&self, cx: &mut Context<Self>, path: &std::path::Path) -> Div {
        let theme = *cx.theme();
        let file = path.to_path_buf();
        div()
            .mt(px(6.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(240.0))
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(theme.text_muted)
                    // Not an apology and not a dead end. The list above is
                    // short because opening somebody else's app is the part we
                    // cannot do, and none of the work depends on it.
                    .child(
                        "Using something else — Gemini in a browser, a model on your own \
                         machine, something we have never heard of? Nothing here depends on \
                         DockCV knowing it. Take the instructions and the file, and bring the \
                         answer back.",
                    ),
            )
            .child(
                Button::new("assistant-copy")
                    .action_secondary()
                    .icon(lucide("copy"))
                    .tooltip("Copies the instructions and shows you the PDF")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.stage_transcription(Some(file.clone()), cx);
                    }))
                    .child("Copy the instructions"),
            )
    }

    /// The second half of every route but the terminal.
    fn render_bring_it_back(&self, cx: &mut Context<Self>) -> Div {
        let theme = *cx.theme();
        div()
            .mt(px(4.0))
            .px(px(12.0))
            .py(px(11.0))
            .rounded(theme.radius_sm())
            .border_1()
            .border_color(theme.border)
            .bg(theme.elevated)
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(240.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_style(TextStyle::eyebrow())
                            .text_color(theme.text_subtle)
                            .child(TextStyle::eyebrow().apply_case("When it answers")),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .line_height(px(17.0))
                            .text_color(theme.text_muted)
                            .child(
                                "Copy its whole reply — DockCV finds the CV inside it and shows \
                                 you what it read. Nothing is saved until you say so.",
                            ),
                    ),
            )
            .child(
                // Named for what it reads, not for what it is about. `Paste
                // the answer` was a button whose whole content was the word
                // "the answer", which the person is being asked to supply.
                Button::new("assistant-paste")
                    .action_primary()
                    .icon(lucide("copy"))
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.import_from_clipboard(cx);
                    }))
                    .child("Paste from clipboard"),
            )
    }

    /// While a local tool is working.
    fn render_local_run(&self, cx: &mut Context<Self>, run: &LocalRun) -> Div {
        let theme = *cx.theme();
        let _ = cx;
        div()
            .px(px(12.0))
            .py(px(11.0))
            .rounded(theme.radius_sm())
            .border_1()
            .border_color(theme.accent.opacity(0.4))
            .bg(theme.accent.opacity(0.06))
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .child(Spinner::new().color(theme.accent))
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(format!("{} is reading the pages…", run.tool)),
                    ),
            )
            .child(
                // A command that reads somebody's CV and sends it to a model is
                // not a thing to run behind their back, and "trust me" is not a
                // design.
                div()
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(theme.text_subtle)
                    .child(run.command.clone()),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(
                        "Reading a page of images is slow. Leaving this screen stops DockCV \
                         waiting for it.",
                    ),
            )
    }

    /// The mark that invites a bug report.
    fn trust_mark(&self, cx: &mut Context<Self>, trust: Trust) -> Option<AnyElement> {
        let theme = *cx.theme();
        (trust == Trust::Unverified).then(|| {
            div()
                .flex()
                .items_center()
                .text_color(theme.warning.opacity(0.8))
                // Not a badge. A badge beside every second button is a row of
                // warnings; this is a mark you notice when you look at one.
                .child(Icon::new(lucide("circle-help")).xsmall())
                .into_any_element()
        })
    }
}

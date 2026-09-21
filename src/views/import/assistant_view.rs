//! Drawing the assistant hand-off. Which routes exist and what they do is in
//! `import/assistant.rs`, which stays free of layout so the prompts and the
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
use gpui::{div, px, ClickEvent, Context, Div, FontWeight, SharedString};

use dockcv_ui_components::{lucide, Button, ButtonExt, Icon, Sizable, Spinner, MONO, SANS};

use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::assistant::{
    route_url, verified_note, Assistant, LocalRun, Route, Via, ASSISTANTS,
};
use crate::views::shell::Shell;

impl Shell {
    /// The panel under a PDF that turned out to be a picture.
    pub(crate) fn render_assistant_handoff(
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
                        // A heading, at heading weight. `Or have an assistant
                        // read it` in body semibold read as one more
                        // paragraph in a column of paragraphs, on the section
                        // that is the actual answer to the failure above it.
                        div()
                            .font_family(SANS)
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("Let your assistant help you out"),
                    )
                    .child(
                        div()
                            .max_w(px(620.0))
                            .text_size(px(11.5))
                            .line_height(px(17.0))
                            .text_color(theme.text_muted)
                            .child(
                                "It can read the pages for you and hand back a CV we \
                                 understand. Whichever you pick, we put the instructions on \
                                 your clipboard too — paste them if the box opens empty.",
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
                            .filter(|a| a.reachable() && !a.browser_only())
                            .map(|assistant| self.render_assistant_row(cx, assistant, path)),
                    )
                    .children(self.render_browser_only(cx, path))
                    .child(div().mt(px(9.0)).child(self.render_trust_note(cx)))
            }))
            .children((!running).then(|| self.render_anything_else(cx, path)))
            .children((!running).then(|| self.render_bring_it_back(cx)))
    }

    /// One assistant, and everywhere it can be reached from here.
    fn render_assistant_row(
        &self,
        cx: &mut Context<Self>,
        assistant: &'static Assistant,
        path: &std::path::Path,
    ) -> Div {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(theme.border.opacity(0.5))
            .child(
                div()
                    .flex_none()
                    .w(px(112.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        // On a tile, at full text colour. A 14px monochrome
                        // glyph in `text_muted` beside a 12.5px label is
                        // punctuation — the eye reads one grey smudge per row
                        // and scans the words instead, which is the whole of
                        // what a mark is meant to save it from.
                        div()
                            .flex_none()
                            .w(px(26.0))
                            .h(px(26.0))
                            .rounded(theme.radius_sm())
                            .bg(theme.hover)
                            .text_color(theme.text)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(match assistant.mark {
                                Some(mark) => Icon::new(mark).small().into_any_element(),
                                None => Icon::new(lucide("bot")).small().into_any_element(),
                            }),
                    )
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(assistant.name),
                    ),
            )
            .children(
                assistant
                    .ordered()
                    .into_iter()
                    .enumerate()
                    .map(|(index, route)| {
                        self.render_route(cx, route, path, route.label, index == 0)
                    })
                    .collect::<Vec<_>>(),
            )
    }

    /// The ones whose only way in is a browser, on one line.
    ///
    /// Each was a row of its own saying its own name and the word `Browser`,
    /// which is a list four items long carrying one fact. The fact is the
    /// names; it fits on a line.
    fn render_browser_only(
        &self,
        cx: &mut Context<Self>,
        path: &std::path::Path,
    ) -> Option<Div> {
        let theme = *cx.theme();
        let rest: Vec<&'static Assistant> = ASSISTANTS
            .iter()
            .filter(|a| a.reachable() && a.browser_only())
            .collect();
        if rest.is_empty() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .py(px(8.0))
                .child(
                    div()
                        .flex_none()
                        .w(px(112.0))
                        .flex()
                        .items_center()
                        .gap(px(7.0))
                        .child(
                            div()
                                .flex_none()
                                .text_color(theme.text_subtle)
                                .child(Icon::new(lucide("globe")).small()),
                        )
                        .child(
                            div()
                                .font_family(SANS)
                                .text_size(px(12.5))
                                .text_color(theme.text_muted)
                                .child("In a browser"),
                        ),
                )
                .children(rest.into_iter().map(|assistant| {
                    let route = assistant
                        .routes
                        .iter()
                        .find(|r| r.via.available())
                        .expect("filtered to reachable");
                    // Named for the assistant, not for the route: four
                    // buttons all saying `Browser` under a heading that
                    // already says it would be the same repetition in one
                    // line instead of four.
                    self.render_route(cx, route, path, assistant.name, false)
                })),
        )
    }

    /// One way in, with a glyph for the kind of thing it is.
    fn render_route(
        &self,
        cx: &mut Context<Self>,
        route: &'static Route,
        path: &std::path::Path,
        label: &'static str,
        lead: bool,
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
                    // The one that asks least of the person is outlined and
                    // carries the glyph; the rest are plain text. Four buttons
                    // of equal weight said the four ways in were equally good,
                    // and a glyph on each turned a row into a row of boxes.
                    .map(|button| {
                        if lead {
                            button.action_secondary().icon(route.via.icon())
                        } else {
                            button.quiet().text_color(theme.text_muted)
                        }
                    })
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
                    .child(label),
            )

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
                        "Not seeing yours? It still works. Any assistant can do this — take \
                         the instructions and your file to whichever one you like, and bring \
                         its answer back here.",
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
                            .child(TextStyle::eyebrow().apply_case("When it answers you")),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .line_height(px(17.0))
                            .text_color(theme.text_muted)
                            .child(
                                "Copy the whole reply and bring it back. We will find the CV \
                                 inside it and show you what it says — nothing is saved until \
                                 you are happy with it.",
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
                            .child(format!("{} is reading your CV…", run.tool)),
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
                        "Reading a page of pictures takes a while. Leave this screen and we \
                         stop waiting for it.",
                    ),
            )
    }

    /// Which of these we have actually seen work.
    fn render_trust_note(&self, cx: &mut Context<Self>) -> Div {
        let theme = *cx.theme();
        div()
            .text_size(px(11.0))
            .line_height(px(16.0))
            .text_color(theme.text_subtle)
            .child(verified_note())
    }
}

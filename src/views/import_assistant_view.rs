//! Drawing the assistant hand-off. Which routes exist and what they do is in
//! `import_assistant.rs`, which stays free of layout so the prompts and the
//! trust marks can be read in one place.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, Div, FontWeight, SharedString};

use dockcv_ui_components::{Button, ButtonExt, Spinner, MONO, SANS};

use crate::theme::ActiveTheme;

use super::import_assistant::{
    claude_desktop, code_url, cowork_url, encode, installed, transcription_prompt, Assistant,
    LocalRun, Route, Trust, LOCAL, WEB,
};
use super::shell::Shell;

impl Shell {
    /// The panel under a PDF that turned out to be a picture.
    pub(super) fn render_assistant_handoff(
        &self,
        cx: &mut Context<Self>,
        path: Option<&std::path::Path>,
    ) -> Div {
        let theme = *cx.theme();
        let local: Vec<&Assistant> = LOCAL
            .iter()
            .filter(|a| match a.route {
                Route::Local { program, .. } => installed().contains(&program),
                Route::Web { .. } => false,
            })
            .collect();

        div()
            .mt(px(18.0))
            .pt(px(16.0))
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Or have an assistant read it"),
            )
            .children(
                self.import_run
                    .as_ref()
                    .map(|run| self.render_local_run(cx, run)),
            )
            .children((self.import_run.is_none() && !local.is_empty() && path.is_some()).then(
                || self.render_local_choices(cx, &local, path.expect("checked")),
            ))
            .children(
                (self.import_run.is_none() && claude_desktop() && path.is_some())
                    .then(|| self.render_desktop_choices(cx, path.expect("checked"))),
            )
            .children(
                self.import_run
                    .is_none()
                    .then(|| self.render_web_choices(cx, path)),
            )
    }

    /// The tools on this machine, which do the whole round trip.
    fn render_local_choices(
        &self,
        cx: &mut Context<Self>,
        local: &[&'static Assistant],
        path: &std::path::Path,
    ) -> Div {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(
                div()
                    .max_w(px(600.0))
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(theme.text_muted)
                    .child(
                        "These are installed here and can open the file themselves — one click, \
                         nothing to drag or paste. DockCV starts the command and reads what it \
                         prints; the tool sends your CV wherever it normally sends things.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .children(local.iter().map(|assistant| {
                        let Route::Local { program, argv } = assistant.route else {
                            unreachable!("filtered to local routes");
                        };
                        let file = path.to_path_buf();
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .child(
                                Button::new(SharedString::from(assistant.id))
                                    .action_secondary()
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _window, cx| {
                                            this.run_local_assistant(
                                                program,
                                                argv,
                                                file.clone(),
                                                cx,
                                            );
                                        },
                                    ))
                                    .child(format!("Run {}", assistant.name)),
                            )
                            .children(self.trust_mark(cx, assistant.trust))
                    })),
            )
    }

    /// While a local tool is working.
    fn render_local_run(&self, cx: &mut Context<Self>, run: &LocalRun) -> Div {
        let theme = *cx.theme();
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

    /// The browser routes, where the person carries the file.
    fn render_web_choices(&self, cx: &mut Context<Self>, path: Option<&std::path::Path>) -> Div {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(
                div()
                    .max_w(px(600.0))
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(theme.text_muted)
                    .child(
                        "Or open one in your browser with the instructions already written. A \
                         link can carry text but not a file, so you attach it and paste the \
                         answer back — DockCV itself sends nothing.",
                    ),
            )
            .child(
                // The consequence, in the place where the decision is made. A
                // CV is the most personal document most people own, and this
                // is the one action in DockCV that takes it off the machine.
                div()
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(theme.radius_sm())
                    .bg(theme.warning.opacity(0.08))
                    .border_1()
                    .border_color(theme.warning.opacity(0.3))
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(theme.text_muted)
                    .child(
                        "Either way your CV goes to that company under your own account, with \
                         whatever retention their terms set. Nothing else in DockCV leaves this \
                         machine.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .children(WEB.iter().map(|assistant| {
                        let Route::Web { new_chat } = assistant.route else {
                            unreachable!("WEB holds web routes");
                        };
                        let url = format!("{new_chat}{}", encode(&transcription_prompt()));
                        let file = path.map(|p| p.to_path_buf());
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .child(
                                Button::new(SharedString::from(assistant.id))
                                    .quiet()
                                    .text_color(theme.text)
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _window, cx| {
                                            this.stage_transcription(file.clone(), cx);
                                            cx.open_url(&url);
                                        },
                                    ))
                                    .child(assistant.name),
                            )
                            .children(self.trust_mark(cx, assistant.trust))
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        Button::new("assistant-copy")
                            .quiet()
                            .text_color(theme.text_muted)
                            .tooltip("Copies the instructions and reveals the PDF")
                            .on_click({
                                let file = path.map(|p| p.to_path_buf());
                                cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                    this.stage_transcription(file.clone(), cx);
                                })
                            })
                            .child("Copy the prompt instead"),
                    )
                    .child(div().flex_1().min_w(px(20.0)))
                    .child(
                        Button::new("assistant-paste")
                            .action_primary()
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.import_from_clipboard(cx);
                            }))
                            .child("Paste the answer"),
                    ),
            )
    }

    /// The mark that invites a bug report.
    fn trust_mark(&self, cx: &mut Context<Self>, trust: Trust) -> Option<Div> {
        let theme = *cx.theme();
        (trust == Trust::Unverified).then(|| {
            div()
                .font_family(MONO)
                .text_size(px(9.0))
                .px(px(4.0))
                .py(px(1.0))
                .rounded(theme.radius_sm())
                .bg(theme.warning.opacity(0.12))
                .text_color(theme.warning)
                .child("?")
        })
    }

    /// Claude Desktop, through the `claude://` scheme it registers.
    ///
    /// This is the route that broke my own rule. I wrote that a link carries
    /// text and cannot carry a file — true of every `https://` chat, and false
    /// here: Cowork's documented `file` parameter takes an absolute path and
    /// attaches it. So for anyone with the app installed this is strictly
    /// better than the browser, because the one manual step left in that route
    /// disappears.
    ///
    /// Code gets `folder` instead. Its `file` parameter is documented as
    /// accepted and not yet supported, so passing it would look like it worked
    /// and attach nothing; the folder goes across and the prompt names the
    /// file inside it, which is the same shape the command-line route uses.
    fn render_desktop_choices(&self, cx: &mut Context<Self>, path: &std::path::Path) -> Div {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(
                div()
                    .max_w(px(600.0))
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(theme.text_muted)
                    .child(
                        "Claude Desktop is installed, and its links can carry the file. Cowork \
                         opens with your CV already attached; Code opens on the folder it is in. \
                         Claude asks you to confirm either before it uses them.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Button::new("desktop-cowork")
                            .action_secondary()
                            .tooltip("Opens Cowork with the PDF attached")
                            .on_click({
                                let url = cowork_url(path);
                                cx.listener(move |_this, _: &ClickEvent, _window, cx| {
                                    cx.open_url(&url);
                                })
                            })
                            .child("Open in Cowork"),
                    )
                    .child(
                        Button::new("desktop-code")
                            .quiet()
                            .text_color(theme.text)
                            .tooltip("Opens Claude Code on the folder the PDF is in")
                            .on_click({
                                let url = code_url(path);
                                cx.listener(move |_this, _: &ClickEvent, _window, cx| {
                                    cx.open_url(&url);
                                })
                            })
                            .child("Open in Claude Code"),
                    ),
            )
    }
}

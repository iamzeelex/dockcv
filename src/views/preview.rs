//! The Typst live-preview view — the demo that renders a "PDF" inside GPUI.
//!
//! It owns the full native pipeline and wires up the bidirectional loop:
//!
//! ```text
//!   key event ─► edit source ─► TypstEngine.compile_to_pixels ─► pixels_to_render_image ─► img()
//! ```
//!
//! The source buffer is editable directly: the focused panel captures key
//! events and recompiles on every change, so typing updates the rendered page
//! live. It is a deliberately minimal editor (no selection/cursor movement) —
//! enough to demonstrate the round-trip without pulling in a full text engine.

use gpui::{div, img, prelude::*, px, App, Context, FocusHandle, Focusable, KeyDownEvent, Window};

use crate::render::{self, Rendered};
use crate::theme::ActiveTheme;
use crate::typst_engine::TypstEngine;

/// The document shown on first launch.
const INITIAL_SOURCE: &str = r#"#set page(width: 12cm, height: 16cm, margin: 1.5cm)
#set text(size: 12pt)

= DockCV
A native Typst preview, rendered on the GPU.

This page is compiled *in-process* by the `typst`
crate, exported to SVG, rasterized with `resvg`,
and drawn by GPUI as a BGRA image.

#line(length: 100%)

Type in the left panel to edit — the page
re-renders on every keystroke.

$ sum_(k=1)^n k = (n (n + 1)) / 2 $
"#;

pub struct TypstPreview {
    engine: TypstEngine,
    source: String,
    rendered: Option<Rendered>,
    error: Option<String>,
    focus_handle: FocusHandle,
    /// Rasters replaced by a newer one, released a frame later. See
    /// `Root::retired_images` for why immediate release aborts the process.
    retired_images: Vec<std::sync::Arc<gpui::RenderImage>>,
    /// First-frame setup is done lazily in `render` because it needs a `Window`
    /// (for the scale factor and initial focus), which `new` does not have.
    initialized: bool,
}

impl TypstPreview {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            engine: TypstEngine::new(INITIAL_SOURCE),
            source: INITIAL_SOURCE.to_string(),
            rendered: None,
            error: None,
            focus_handle: cx.focus_handle(),
            retired_images: Vec::new(),
            initialized: false,
        }
    }

    /// Push the current source through the pipeline and store the resulting
    /// image (or error). Frees the previous GPU texture first.
    fn recompile(&mut self, window: &mut Window) {
        self.engine.set_source(self.source.clone());
        let scale = window.scale_factor();

        match self
            .engine
            .compile_to_pixels(scale)
            .and_then(|(px, _geometry)| render::pixels_to_render_image(px, scale))
        {
            Ok(rendered) => {
                if let Some(previous) = self.rendered.take() {
                    // Same hazard as the editor's preview: an atlas tile pulled
                    // out from under a frame that still references it aborts the
                    // process. Released at the top of the next `render`.
                    self.retired_images.push(previous.image);
                }
                self.rendered = Some(rendered);
                self.error = None;
            }
            Err(message) => self.error = Some(message),
        }
    }

    /// Apply a single key event to the source buffer. Returns whether the
    /// buffer changed (and therefore needs a recompile).
    fn apply_key(&mut self, event: &KeyDownEvent) -> bool {
        let keystroke = &event.keystroke;

        // Leave shortcuts (cmd/ctrl/alt combinations) to the rest of the app.
        if keystroke.modifiers.secondary() || keystroke.modifiers.control || keystroke.modifiers.alt
        {
            return false;
        }

        match keystroke.key.as_str() {
            "backspace" => self.source.pop().is_some(),
            "enter" => {
                self.source.push('\n');
                true
            }
            "tab" => {
                self.source.push_str("  ");
                true
            }
            _ => match &keystroke.key_char {
                Some(text) if !text.is_empty() && !text.chars().any(char::is_control) => {
                    self.source.push_str(text);
                    true
                }
                _ => false,
            },
        }
    }

    fn render_source_panel(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme().clone();

        // Render the buffer line by line; blank lines keep their height.
        let lines = self.source.lines().map(|line| {
            div().min_h(px(18.0)).child(if line.is_empty() {
                " ".to_string()
            } else {
                line.to_string()
            })
        });

        div()
            .flex()
            .flex_col()
            .w(px(360.0))
            .h_full()
            .bg(theme.surface)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .px_4()
                    .py_2()
                    .text_sm()
                    .text_color(theme.accent)
                    .child("main.typ — type to edit (live)"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .px_4()
                    .pb_4()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .children(lines),
            )
    }

    fn render_preview_panel(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme().clone();

        let body = if let Some(error) = &self.error {
            div()
                .p_6()
                .text_sm()
                .text_color(theme.danger)
                .child(format!("compile error\n\n{error}"))
                .into_any_element()
        } else if let Some(rendered) = &self.rendered {
            img(rendered.image.clone())
                .w(px(rendered.width))
                .h(px(rendered.height))
                .shadow_lg()
                .into_any_element()
        } else {
            div()
                .text_color(theme.text_muted)
                .child("compiling…")
                .into_any_element()
        };

        div()
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .justify_center()
            .items_center()
            .overflow_hidden()
            .p_6()
            .child(body)
    }
}

impl Focusable for TypstPreview {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TypstPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        for image in self.retired_images.drain(..) {
            let _ = window.drop_image(image);
        }

        if !self.initialized {
            self.initialized = true;
            self.recompile(window);
            self.focus_handle.focus(window, cx);
        }

        div()
            .track_focus(&self.focus_handle)
            .key_context("TypstPreview")
            .flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().text)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.apply_key(event) {
                    this.recompile(window);
                    cx.notify();
                }
            }))
            .child(self.render_source_panel(cx))
            .child(self.render_preview_panel(cx))
    }
}

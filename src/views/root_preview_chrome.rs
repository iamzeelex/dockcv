//! The floating bar under the paper: zoom, page count, compile state, overflow.
//!
//! Not the layout rail. The rail changes the *document*; this changes how the
//! document is being looked at, and the two were in one file only because they
//! both float over the preview.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, IntoElement};

use dockcv_ui_components::{Button, ButtonExt, IconName};
use crate::theme::StyledText;

use crate::theme::{ActiveTheme, TextStyle};

use super::root::{CompileState, Root};

/// Zoom steps the `−`/`+` buttons walk, in percent. A fixed ladder rather
/// than a free multiplier: the design draws a percentage readout, and round
/// numbers are what a user can say out loud and get back to.
const ZOOM_STEPS: [u16; 9] = [50, 67, 80, 90, 100, 110, 125, 150, 200];

/// The range a continuous pinch may reach. The same ends as the stepped
/// control — a gesture must not be able to leave the zoom somewhere `+`/`-`
/// cannot bring it back from.
pub(super) const MIN_ZOOM_PCT: f32 = ZOOM_STEPS[0] as f32;
pub(super) const MAX_ZOOM_PCT: f32 = ZOOM_STEPS[ZOOM_STEPS.len() - 1] as f32;

impl Root {
    /// The floating toolbar under the paper: zoom, page count, compile state,
    /// and — when it overflows — how much is over (US-07, US-08).
    ///
    /// Drawn as one persistent bar rather than the two the mockup shows in
    /// different rows, per `typst-controls.md`'s own synthesis: zoom and the
    /// page counter are always there, the overflow affordance appears only
    /// when there is overflow.
    pub(super) fn render_preview_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let pages = self.geometry.as_ref().map(|g| g.page_count).unwrap_or(1);

        div()
            .absolute()
            .bottom(px(16.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()

            .child(
                div()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .px(px(6.0))
                    .rounded(theme.radius_md())
                    .bg(theme.chrome.opacity(0.92))
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .child(self.zoom_button(cx, "zoom-out", IconName::Minus, -1))
                    .child(
                        div()
                            .min_w(px(46.0))
                            .flex()
                            .justify_center()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(format!("{}%", self.zoom_pct.round() as i32)),
                    )
                    .child(self.zoom_button(cx, "zoom-in", IconName::Plus, 1))
                    .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                    .child(
                        div()
                            .px(px(8.0))
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(format!("1 / {pages}")),
                    )
                    .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                    .child(self.compile_status(cx)),
            )
    }

    fn zoom_button(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        icon: IconName,
        direction: i32,
    ) -> impl IntoElement {
        Button::new(id)
            .icon_only()
            .icon(icon)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                // From wherever a pinch left the zoom, `+`/`-` moves to the
                // next step in that direction rather than to the step nearest
                // the current value — pressing `+` must always zoom in.
                let next = if direction > 0 {
                    ZOOM_STEPS
                        .iter()
                        .find(|z| f32::from(**z) > this.zoom_pct + 0.5)
                        .copied()
                        .unwrap_or(ZOOM_STEPS[ZOOM_STEPS.len() - 1])
                } else {
                    ZOOM_STEPS
                        .iter()
                        .rev()
                        .find(|z| f32::from(**z) < this.zoom_pct - 0.5)
                        .copied()
                        .unwrap_or(ZOOM_STEPS[0])
                };
                this.zoom_pct = f32::from(next);
                cx.notify();
                // Zooming changes how many pixels the sheet occupies, so it
                // changes the resolution the page must be rasterized at. The
                // bitmap stretches immediately (so the control feels instant)
                // and a sharp pass lands behind it.
                this.schedule_recompile(window, cx);
            }))
    }

    /// Compile state, always visible — US-07's acceptance text asks for
    /// "compiling / ready / error" at all times, and neither mockup draws the
    /// in-flight case at all.
    fn compile_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        // A word only when there is something to say. "ready" is the state
        // the preview is in almost always, and naming it every second told
        // the user nothing they could not see by looking at the page; the dot
        // alone carries it. `compiling` and the two failure states are the
        // ones worth reading, so those keep their label.
        let (color, label) = match &self.compile_state {
            CompileState::Compiling => (theme.text_subtle, Some("compiling")),
            CompileState::Ready { warnings } if warnings.is_empty() => (theme.success, None),
            CompileState::Ready { .. } => (theme.warning, Some("warnings")),
            CompileState::Error { .. } => (theme.danger, Some("error")),
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .child(div().size(px(7.0)).rounded_full().bg(color))
            .children(label.map(|label| {
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child(label)
            }))
    }
}

impl Root {
    /// Width the paper is drawn at, in display pixels.
    pub(super) fn preview_width(&self) -> f32 {
        480.0 * self.zoom_pct / 100.0
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::{MAX_ZOOM_PCT, MIN_ZOOM_PCT, ZOOM_STEPS};

    /// A pinch leaves the zoom between steps. `+` must still zoom *in* from
    /// there — stepping to the nearest step instead would sometimes move the
    /// wrong way, or not at all.
    #[test]
    fn plus_and_minus_step_past_a_value_a_pinch_left_behind() {
        let step = |from: f32, direction: i32| -> f32 {
            if direction > 0 {
                ZOOM_STEPS
                    .iter()
                    .find(|z| f32::from(**z) > from + 0.5)
                    .copied()
                    .unwrap_or(ZOOM_STEPS[ZOOM_STEPS.len() - 1])
            } else {
                ZOOM_STEPS
                    .iter()
                    .rev()
                    .find(|z| f32::from(**z) < from - 0.5)
                    .copied()
                    .unwrap_or(ZOOM_STEPS[0])
            }
            .into()
        };

        assert_eq!(step(103.7, 1), 110.0, "+ must leave a mid-step value upward");
        assert_eq!(step(103.7, -1), 100.0, "- must leave it downward");
        // Exactly on a step, it moves to the next one rather than standing still.
        assert_eq!(step(100.0, 1), 110.0);
        assert_eq!(step(100.0, -1), 90.0);
        // The ends hold.
        assert_eq!(step(MAX_ZOOM_PCT, 1), MAX_ZOOM_PCT);
        assert_eq!(step(MIN_ZOOM_PCT, -1), MIN_ZOOM_PCT);
    }

    /// A gesture must not be able to leave the zoom somewhere the buttons
    /// cannot bring it back from.
    #[test]
    fn a_pinch_cannot_leave_the_stepped_range() {
        let pinch = |from: f32, delta: f32| (from * (1.0 + delta)).clamp(MIN_ZOOM_PCT, MAX_ZOOM_PCT);
        assert_eq!(pinch(200.0, 0.5), MAX_ZOOM_PCT);
        assert_eq!(pinch(50.0, -0.5), MIN_ZOOM_PCT);
        assert!((pinch(100.0, 0.1) - 110.0).abs() < 0.01);
    }
}

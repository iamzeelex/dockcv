//! The floating bar under the paper: zoom, page count, compile state, overflow.
//!
//! Not the layout rail. The rail changes the *document*; this changes how the
//! document is being looked at, and the two were in one file only because they
//! both float over the preview.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, IntoElement};

use crate::theme::StyledText;
use dockcv_ui_components::{Button, ButtonExt, IconName, Selectable};

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
                            .child(format!("{}%", self.effective_zoom_pct().round() as i32)),
                    )
                    .child(self.zoom_button(cx, "zoom-in", IconName::Plus, 1))
                    .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                    .child(self.fit_button(cx, "fit-width", "Fit width", PreviewFit::Width))
                    .child(self.fit_button(cx, "fit-page", "Fit page", PreviewFit::Page))
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

    /// One of the two fit modes, as a toggle that shows which is on.
    fn fit_button(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        label: &'static str,
        fit: PreviewFit,
    ) -> impl IntoElement {
        Button::new(id)
            .quiet()
            .selected(self.preview_fit == fit)
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.preview_fit = fit;
                cx.notify();
                // The sheet changes size, so the raster it was drawn at is the
                // wrong one — `crisp_scale` reads the new width.
                this.schedule_recompile(window, cx);
            }))
    }

    fn zoom_button(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        icon: IconName,
        direction: i32,
    ) -> impl IntoElement {
        Button::new(id).icon_only().icon(icon).on_click(cx.listener(
            move |this, _: &ClickEvent, window, cx| {
                // Asking for a step is asking for a number, so the fit modes
                // let go here — and hand over the size already on screen.
                this.take_manual_zoom();
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
            },
        ))
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

/// How the paper is sized against the pane it sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PreviewFit {
    /// The sheet spans the pane, less the gutter. The default: a CV is read
    /// column-width, and every other size wastes the pane it is shown in.
    Width,
    /// One whole sheet, top to bottom.
    Page,
    /// Whatever `zoom_pct` says, because the user set it.
    Manual,
}

/// Space kept between the sheet and the pane's edges, in display pixels.
///
/// Horizontal and vertical are the same number so a "fit page" sheet is inset
/// by as much as a "fit width" one — a page that fits sideways but bleeds off
/// the top does not read as fitted.
pub(super) const PREVIEW_GUTTER: f32 = 34.0;

impl Root {
    /// The sheet's width at 100%, in display pixels.
    ///
    /// **Actual size.** This was a flat 480px, which for A4 is 60% — so the
    /// zoom readout said `100%` over a page shown at three fifths, and every
    /// document opened looking tiny for a reason nothing on screen explained.
    /// A point is 1/72", a CSS pixel 1/96", so the sheet's own width in points
    /// is the only honest basis for the word.
    pub(super) fn natural_page_width(&self) -> f32 {
        self.effective_layout().page_size.width_pt() / 72.0 * 96.0
    }

    /// Width the paper is drawn at, in display pixels.
    pub(super) fn preview_width(&self) -> f32 {
        self.natural_page_width() * self.effective_zoom_pct() / 100.0
    }

    /// The percentage actually in force — the one the readout shows.
    ///
    /// A fit mode has no stored percentage: it has a pane, and the number falls
    /// out of it. Before the pane has been measured once there is nothing to
    /// compute from, so the manual value stands in for a frame.
    pub(super) fn effective_zoom_pct(&self) -> f32 {
        let Some(pane) = self.preview_pane else {
            return self.zoom_pct;
        };
        let natural = self.natural_page_width();
        if natural <= 0.0 {
            return self.zoom_pct;
        }
        if self.preview_fit == PreviewFit::Manual {
            return self.zoom_pct;
        }
        let layout = self.effective_layout();
        fitted_pct(
            self.preview_fit,
            pane,
            natural,
            layout.page_size.height_pt() / layout.page_size.width_pt(),
        )
    }

    /// Leave a fit mode, keeping the size the eye is already on.
    ///
    /// Seeding `zoom_pct` from the effective value is what stops `+` from
    /// snapping the page to some stale percentage before it grows.
    pub(super) fn take_manual_zoom(&mut self) {
        if self.preview_fit != PreviewFit::Manual {
            self.zoom_pct = self.effective_zoom_pct();
            self.preview_fit = PreviewFit::Manual;
        }
    }
}

/// What percentage a fit mode resolves to. Pure, so it can be tested without
/// a window.
///
/// `sheet_ratio` is **one page's** height over its width. The rendered bitmap
/// is as tall as the CV is long, and fitting that would shrink a three-page CV
/// to a thumbnail the moment it turned three pages.
fn fitted_pct(fit: PreviewFit, pane: (f32, f32), natural: f32, sheet_ratio: f32) -> f32 {
    let by_width = (pane.0 - PREVIEW_GUTTER * 2.0) / natural;
    let pct = match fit {
        PreviewFit::Manual => return 100.0,
        PreviewFit::Width => by_width,
        PreviewFit::Page => {
            by_width.min((pane.1 - PREVIEW_GUTTER * 2.0) / (natural * sheet_ratio))
        }
    } * 100.0;
    // A fit must not land somewhere `+`/`−` cannot bring it back from.
    pct.clamp(MIN_ZOOM_PCT, MAX_ZOOM_PCT)
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

        assert_eq!(
            step(103.7, 1),
            110.0,
            "+ must leave a mid-step value upward"
        );
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
        let pinch =
            |from: f32, delta: f32| (from * (1.0 + delta)).clamp(MIN_ZOOM_PCT, MAX_ZOOM_PCT);
        assert_eq!(pinch(200.0, 0.5), MAX_ZOOM_PCT);
        assert_eq!(pinch(50.0, -0.5), MIN_ZOOM_PCT);
        assert!((pinch(100.0, 0.1) - 110.0).abs() < 0.01);
    }
}

#[cfg(test)]
mod fit_tests {
    use super::{fitted_pct, PreviewFit, MAX_ZOOM_PCT, MIN_ZOOM_PCT, PREVIEW_GUTTER};

    /// A4 at 96 dpi.
    const A4_PX: f32 = 595.276 / 72.0 * 96.0;
    const A4_RATIO: f32 = 841.89 / 595.276;

    #[test]
    fn fit_width_fills_the_pane_less_its_gutters() {
        let pane = (A4_PX + PREVIEW_GUTTER * 2.0, 2000.0);
        let pct = fitted_pct(PreviewFit::Width, pane, A4_PX, A4_RATIO);
        assert!((pct - 100.0).abs() < 0.5, "exactly one page wide: {pct}");
    }

    /// The bug this mode replaces: the sheet was a flat 480px whatever the pane
    /// was, so a wide window showed a small page in a field of grey.
    #[test]
    fn a_wider_pane_shows_a_wider_page() {
        let narrow = fitted_pct(PreviewFit::Width, (700.0, 2000.0), A4_PX, A4_RATIO);
        let wide = fitted_pct(PreviewFit::Width, (1100.0, 2000.0), A4_PX, A4_RATIO);
        assert!(wide > narrow, "{wide} should beat {narrow}");
    }

    /// Fit page is bounded by whichever side runs out first, and in a pane that
    /// is wide and short that is the height.
    #[test]
    fn fit_page_takes_the_smaller_of_the_two() {
        let pane = (2000.0, 400.0);
        let by_page = fitted_pct(PreviewFit::Page, pane, A4_PX, A4_RATIO);
        let by_width = fitted_pct(PreviewFit::Width, pane, A4_PX, A4_RATIO);
        assert!(by_page < by_width, "height should bind: {by_page} vs {by_width}");
    }

    /// Neither mode may park the page where `+`/`−` cannot reach it.
    #[test]
    fn a_fit_stays_inside_the_stepped_range() {
        for pane in [(60.0, 60.0), (9000.0, 9000.0)] {
            for fit in [PreviewFit::Width, PreviewFit::Page] {
                let pct = fitted_pct(fit, pane, A4_PX, A4_RATIO);
                assert!(
                    (MIN_ZOOM_PCT..=MAX_ZOOM_PCT).contains(&pct),
                    "{fit:?} at {pane:?} gave {pct}"
                );
            }
        }
    }
}

//! The welcome screen — the first thing anyone sees.
//!
//! ### Why it is a sequence and not a fade
//!
//! Everything used to arrive at once, on the shared `fade_in`. That is fine for
//! a screen you are returning to and wrong for the one that introduces the
//! product: a single fade says "here is a page", while an order says what
//! matters and in what relation. So the parts land in the order a person would
//! read them, each settling before the next begins.
//!
//! The mark it opens on is three sheets of paper stacking. That is the product's
//! whole model in one image — **one document, several versions of it** — and it
//! is the same paper the gallery card draws a thumbnail on, so the first thing
//! seen is the thing the app is made of rather than an abstract shape.
//!
//! ### How the stagger is built
//!
//! GPUI hands an animator a delta over the *whole* run and has no notion of a
//! delay, so a stagger is a **remap**: every part gets the same
//! [`INTRO`]-long animation and reads only the window that belongs to it (see
//! [`step`]). One clock for all of them, so nothing can drift apart, and the
//! sequence is legible in one place — the table in [`Shell::render_welcome`] —
//! rather than as a dozen scattered durations.
//!
//! Motion is opacity and offset only. GPUI's `div` has no transform, so there is
//! no rotation and no scale to reach for; the sheets fan by inset alone, which
//! is quieter than a rotation would have been anyway.
//!
//! `with_animation` honours `App::reduce_motion` on its own: with it set, every
//! part renders in its finished state and no frames are scheduled. Nothing here
//! needs to check for that.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    div, linear, px, Animation, AnimationExt, ClickEvent, Context, Div, IntoElement,
};

use dockcv_ui_components::{Button, ButtonExt, StyledText, TextStyle, SANS};

use crate::theme::{ActiveTheme, Theme};

use super::shell::{Screen, Shell};

/// How long the whole entrance takes, start to finish.
///
/// Long enough to read as deliberate, short enough that the second launch does
/// not feel like waiting — and every part is on screen and legible well before
/// the end, because the last window closes on the button rather than on the
/// content.
const INTRO: Duration = Duration::from_millis(1_150);

/// One part's slice of the run, eased.
///
/// `delta` is the position in the *whole* animation; `start` and `end` are the
/// fractions of it this part owns. Outside its window a part is fully out or
/// fully in, so a stagger costs one remap rather than a timer per element.
///
/// The curve is ease-out quintic — the same one `Shell::fade_in` and
/// `slide_in` settle on, so the welcome screen moves like the rest of the app
/// rather than inventing its own feel.
fn step(delta: f32, start: f32, end: f32) -> f32 {
    let t = ((delta - start) / (end - start)).clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(5)
}

/// A part that fades in while rising into place.
///
/// `relative` + `top`, never a margin: a margin would reflow the column on every
/// frame and the parts already settled would drift while the next one arrived.
fn rise(id: &'static str, window: (f32, f32), lift: f32, content: Div) -> impl IntoElement {
    content.with_animation(
        id,
        Animation::new(INTRO).with_easing(linear),
        move |el, delta| {
            let t = step(delta, window.0, window.1);
            el.relative().top(px((1.0 - t) * lift)).opacity(t)
        },
    )
}

/// The mark's box, and where one sheet sits inside it before the fan is applied.
const MARK_W: f32 = 128.0;
const MARK_H: f32 = 130.0;
const SHEET_W: f32 = 88.0;
const SHEET_H: f32 = 112.0;
const MARK_INSET_X: f32 = (MARK_W - SHEET_W) / 2.0;
const MARK_INSET_Y: f32 = (MARK_H - SHEET_H) / 2.0;

/// One sheet of the opening mark.
///
/// `depth` is how far back it sits: 0 is the page you are looking at, 2 the one
/// furthest behind. The sheets start spread and settle into a stack, which is
/// the motion a pile of paper actually makes when it is squared up.
fn sheet(theme: &Theme, depth: usize, window: (f32, f32), lines: usize) -> impl IntoElement {
    let index = depth as u64;
    let depth = depth as f32;
    // Centred on the *stack*, not on the front sheet: the fan spreads right and
    // up, so without pulling the whole thing back by half its spread the mark
    // sits off-centre in a column that is otherwise exactly centred.
    const FAN_X: f32 = 13.0;
    const FAN_Y: f32 = 9.0;
    let settled_x = MARK_INSET_X + depth * FAN_X - FAN_X;
    let settled_y = MARK_INSET_Y - depth * FAN_Y + FAN_Y;
    let opacity = 1.0 - depth * 0.34;

    let page = div()
        .absolute()
        .w(px(SHEET_W))
        .h(px(SHEET_H))
        .rounded(px(7.0))
        .bg(theme.paper)
        .border_1()
        .border_color(theme.paper_border)
        .shadow_lg()
        .p(px(11.0))
        .flex()
        .flex_col()
        .gap(px(5.0))
        // The same grey bars the gallery card draws on its thumbnail: what the
        // welcome screen shows is what a CV looks like in this app.
        .child(div().h(px(5.0)).w(px(38.0)).rounded(px(2.0)).bg(theme.text_subtle))
        .children((0..lines).map(|i| {
            let width = [56.0, 48.0, 62.0, 40.0][i % 4];
            div()
                .h(px(3.0))
                .w(px(width))
                .rounded(px(2.0))
                .bg(theme.paper_border)
        }));

    page.with_animation(
        gpui::ElementId::NamedInteger("welcome-sheet".into(), index),
        Animation::new(INTRO).with_easing(linear),
        move |el, delta| {
            let t = step(delta, window.0, window.1);
            // From spread and low, to squared up. The furthest sheet travels
            // furthest, which is what makes the stack look like it is settling
            // rather than sliding as one block.
            let spread = (1.0 - t) * (18.0 + depth * 16.0);
            el.left(px(settled_x + spread))
                .top(px(settled_y + (1.0 - t) * 20.0))
                .opacity(t * opacity)
        },
    )
}

impl Shell {
    pub(super) fn render_welcome(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        // The order, in one table. Each pair is the slice of `INTRO` that part
        // owns; they overlap on purpose, so the screen is always composing
        // rather than stepping.
        const SHEET_BACK: (f32, f32) = (0.00, 0.42);
        const SHEET_MID: (f32, f32) = (0.06, 0.48);
        const SHEET_FRONT: (f32, f32) = (0.12, 0.54);
        const EYEBROW: (f32, f32) = (0.30, 0.60);
        const WORDMARK: (f32, f32) = (0.38, 0.70);
        const RULE: (f32, f32) = (0.52, 0.80);
        const TAGLINE: (f32, f32) = (0.58, 0.86);
        const ACTION: (f32, f32) = (0.70, 1.00);

        let mark = div()
            .relative()
            .w(px(MARK_W))
            .h(px(MARK_H))
            // Back to front, so the page you are meant to read ends up on top.
            .child(sheet(&theme, 2, SHEET_BACK, 2))
            .child(sheet(&theme, 1, SHEET_MID, 3))
            .child(sheet(&theme, 0, SHEET_FRONT, 4));

        let eyebrow = div()
            .text_style(TextStyle::eyebrow())
            .text_color(theme.accent)
            .child(TextStyle::eyebrow().apply_case("Local-first · Typst-powered"));

        let wordmark = div()
            .flex()
            .items_baseline()
            .font_family(SANS)
            .text_size(px(44.0))
            .font_weight(gpui::FontWeight::BOLD)
            .line_height(px(46.0))
            .child(div().text_color(theme.text).child("Dock"))
            .child(div().text_color(theme.accent).child("CV"));

        // A hairline that draws outward under the wordmark. The one flourish
        // on the screen, and it earns its place by being what a typesetter
        // does: it rules a line.
        let rule = div().h(px(1.0)).bg(theme.border).with_animation(
            "welcome-rule",
            Animation::new(INTRO).with_easing(linear),
            |el, delta| {
                let t = step(delta, RULE.0, RULE.1);
                el.w(px(t * 132.0)).opacity(t)
            },
        );

        let tagline = div()
            .max_w(px(420.0))
            .text_center()
            .text_style(TextStyle::prose())
            .text_color(theme.text_muted)
            .child(
                "Craft CVs tailored to each role — version every section, compose with \
                 presets, and keep everything in open files you own.",
            );

        let action = div().child(
            Button::new("get-started")
                .action_primary()
                // The one control on the first screen of the product: it keeps
                // the pill shape the welcome art is built around.
                .rounded_full()
                .px_6()
                .icon(dockcv_ui_components::IconName::ArrowRight)
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.screen = Screen::Setup;
                    cx.notify();
                }))
                .child("Get started"),
        );

        let content = div()
            .flex()
            .flex_col()
            .items_center()
            .child(mark)
            .child(div().h(px(26.0)))
            .child(rise("welcome-eyebrow", EYEBROW, 10.0, eyebrow))
            .child(div().h(px(14.0)))
            .child(rise("welcome-wordmark", WORDMARK, 16.0, wordmark))
            .child(div().h(px(18.0)))
            .child(rule)
            .child(div().h(px(20.0)))
            .child(rise("welcome-tagline", TAGLINE, 12.0, tagline))
            .child(div().h(px(30.0)))
            .child(rise("welcome-action", ACTION, 14.0, action));

        self.backdrop(cx).child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::step;

    /// A part is fully out before its window and fully in after it. Without the
    /// clamp, a part would still be moving while the ones after it arrived —
    /// which is the difference between a sequence and a smear.
    #[test]
    fn a_part_is_still_outside_its_own_window() {
        assert_eq!(step(0.0, 0.3, 0.6), 0.0);
        assert_eq!(step(0.29, 0.3, 0.6), 0.0);
        assert_eq!(step(0.6, 0.3, 0.6), 1.0);
        assert_eq!(step(1.0, 0.3, 0.6), 1.0);
    }

    /// And inside it, it only ever moves forward — an entrance that backtracks
    /// reads as a glitch rather than as motion.
    #[test]
    fn inside_the_window_it_only_advances() {
        let mut previous = 0.0;
        for i in 0..=100 {
            let value = step(i as f32 / 100.0, 0.2, 0.8);
            assert!(value >= previous, "went backwards at {i}: {value} < {previous}");
            assert!((0.0..=1.0).contains(&value), "left the range at {i}: {value}");
            previous = value;
        }
    }

    /// Ease-out: most of the distance is covered early, so a part is legible
    /// well before its window closes and the screen never feels held up.
    #[test]
    fn most_of_the_move_happens_in_the_first_half() {
        let halfway = step(0.5, 0.0, 1.0);
        assert!(halfway > 0.9, "ease-out quintic is front-loaded: {halfway}");
    }
}

//! Status tints — one base colour, the four derived shades a status-coloured
//! card needs.
//!
//! The Applications board draws each column in its own temperature: a dot, a
//! tinted card border, a translucent avatar wash, and a chip. That is four
//! shades per status, and the board has five statuses — twenty values, all of
//! them derivations of two or three base colours the palette already carries.
//!
//! Neither obvious approach works. Twenty stored tokens would triple the
//! palette to describe one screen, and every future status would add four more.
//! Scattering `theme.accent.opacity(0.35)` across the view puts the *alpha* —
//! a real design decision, and the thing that makes a border read as a tint
//! rather than a box — in whichever call site happened to need it first, where
//! nothing keeps the five columns consistent with each other.
//!
//! So: one named concept, derived once. A view asks for the tint of a status
//! and gets all four shades already in proportion.
//!
//! ```ignore
//! let tint = StatusTint::of(cx.theme().accent);
//! card.border_color(tint.border).child(avatar.bg(tint.wash));
//! ```

use gpui::Hsla;

/// The four shades a status-coloured surface needs, derived from one base.
///
/// Alphas are the mockup's own, read from the Applications row: the border is
/// the strongest at 0.35–0.4 (it has to survive being one pixel wide), the
/// avatar wash sits at ~0.22, and the chip fill just under it at 0.2.
#[derive(Clone, Copy, Debug)]
pub struct StatusTint {
    /// The 7×7 column indicator. The base colour at full strength.
    pub dot: Hsla,
    /// A 1px card border that reads as tinted rather than as a box.
    pub border: Hsla,
    /// Translucent fill behind a monogram or icon.
    pub wash: Hsla,
    /// Fill behind a chip carrying this status's own word.
    pub chip_bg: Hsla,
    /// Text on top of [`StatusTint::wash`] or [`StatusTint::chip_bg`].
    ///
    /// Lightened rather than the base colour itself: the base is chosen to
    /// carry a 1px line on a dark surface, which is too dim to read as text on
    /// a wash of itself. The mockup lightens by hand (`#6f8fd6` → `#b8c8ef`,
    /// `#7fa28f` → `#a8c9b8`); this reproduces that as a lightness step, so it
    /// keeps working when the palette changes underneath it.
    pub fg: Hsla,
}

impl StatusTint {
    /// Derive a status tint from a palette colour — `theme.accent`,
    /// `theme.success`, `theme.status_closed`.
    pub fn of(base: Hsla) -> Self {
        Self {
            dot: base,
            border: base.opacity(0.35),
            wash: base.opacity(0.22),
            chip_bg: base.opacity(0.2),
            fg: lighten(base, 0.22),
        }
    }

    /// The neutral tint, for a status with no colour of its own (Wishlist).
    /// Takes the palette's `border`, so the card keeps the ordinary hairline
    /// and the wash stays a grey the surface can absorb.
    pub fn neutral(border: Hsla, text_subtle: Hsla) -> Self {
        Self {
            dot: text_subtle,
            border,
            wash: border,
            chip_bg: border,
            fg: text_subtle,
        }
    }
}

/// Raise a colour's lightness towards white by `amount`, keeping hue and
/// saturation. Clamped, so a base that is already light stays valid.
fn lighten(color: Hsla, amount: f32) -> Hsla {
    Hsla {
        l: (color.l + amount).clamp(0.0, 1.0),
        ..color
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    /// The whole point of `fg` is that it reads on this tint's own wash. A
    /// derivation that comes out darker than the base it lightens would fail
    /// silently — the text would still render, just illegibly.
    #[test]
    fn the_foreground_is_lighter_than_the_base_it_derives_from() {
        for theme in [Theme::slate_dark(), Theme::slate_light()] {
            for base in [theme.accent, theme.success] {
                let tint = StatusTint::of(base);
                assert!(
                    tint.fg.l > base.l || base.l >= 1.0,
                    "fg must lighten the base, got {} from {}",
                    tint.fg.l,
                    base.l
                );
            }
        }
    }

    /// The border has to be the strongest derivation: at one pixel wide it has
    /// the least area to make its case. If a palette edit ever inverts this
    /// order the cards stop reading as status-coloured at all.
    #[test]
    fn the_border_is_the_most_opaque_derivation() {
        let tint = StatusTint::of(Theme::slate_dark().accent);
        assert!(tint.border.a > tint.wash.a);
        assert!(tint.wash.a > tint.chip_bg.a);
        assert!(tint.dot.a >= tint.border.a);
    }

    /// A neutral status must not invent a colour: its dot and text are the
    /// palette's own subtle grey, not a tinted derivation of anything.
    #[test]
    fn the_neutral_tint_stays_neutral() {
        let theme = Theme::slate_dark();
        let tint = StatusTint::neutral(theme.border, theme.text_subtle);
        assert_eq!(tint.dot, theme.text_subtle);
        assert_eq!(tint.fg, theme.text_subtle);
        assert_eq!(tint.border, theme.border);
    }
}

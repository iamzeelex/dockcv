//! The three type roles, and the scale that goes with them.
//!
//! The redesign's whole typographic argument is that a résumé workbench has
//! three kinds of text and they must not be confused:
//!
//! | Role | Family | Used for |
//! |---|---|---|
//! | [`Display`](TextRole::Display) | [`SERIF`] | screen titles, empty states, section headings |
//! | [`Ui`](TextRole::Ui) | [`SANS`] | everything else — labels, field values, buttons, prose |
//! | [`Data`](TextRole::Data) | [`MONO`] | dates, counts, paths, keyboard hints, metric chips |
//!
//! **Mono is for data only.** Résumé prose — Summary, bullets — is `Ui`. Setting
//! it in mono is the single thing the review calls out as making the old
//! interface read like a config file rather than a document (L-05).
//!
//! ```ignore
//! div().text_style(TextStyle::title(), cx).child("Your CVs")
//! div().text_style(TextStyle::meta(), cx).child("updated 2d ago")
//! ```

use gpui::{px, FontWeight, Pixels, Styled};

/// Interface sans. Bundled; see `register_fonts` in the app.
pub const SANS: &str = "Geist";

/// Editorial serif for display type.
pub const SERIF: &str = "Newsreader";

/// Monospace, for data only.
pub const MONO: &str = "JetBrains Mono";

/// Which family a piece of text belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    Display,
    Ui,
    Data,
}

impl TextRole {
    pub fn family(self) -> &'static str {
        match self {
            Self::Display => SERIF,
            Self::Ui => SANS,
            Self::Data => MONO,
        }
    }
}

/// One step of the type scale: a role, a size, a weight, and the tracking and
/// line height that belong with them.
///
/// Constructed through the named steps below rather than by hand — the point of
/// the scale is that a screen picks from a fixed set.
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    pub role: TextRole,
    pub size: Pixels,
    pub weight: FontWeight,
    /// Line height, as a multiple of the font size.
    pub leading: f32,
    /// Whether callers should upper-case the string. GPUI has no text-transform,
    /// so [`TextStyle::apply_case`] does it to the text itself.
    pub uppercase: bool,
}

// Note: the mockup tracks its small-caps eyebrows at +0.12em. GPUI's `Styled`
// exposes no letter-spacing, so that is deliberately not modelled here rather
// than carried as a field that silently does nothing.

impl TextStyle {
    const fn new(role: TextRole, size: f32, weight: FontWeight) -> Self {
        Self {
            role,
            size: px(size),
            weight,
            leading: 1.45,
            uppercase: false,
        }
    }

    const fn leading(mut self, leading: f32) -> Self {
        self.leading = leading;
        self
    }

    const fn uppercase(mut self) -> Self {
        self.uppercase = true;
        self
    }

    // --- display: serif, editorial, sparing ---

    /// The one large title on a screen ("The front door — your CVs").
    pub const fn hero() -> Self {
        Self::new(TextRole::Display, 34.0, FontWeight::MEDIUM).leading(1.1)
    }

    /// A screen heading ("Your CVs", "Settings").
    pub const fn title() -> Self {
        Self::new(TextRole::Display, 21.0, FontWeight::MEDIUM).leading(1.2)
    }

    /// A card or section heading.
    pub const fn heading() -> Self {
        Self::new(TextRole::Display, 16.0, FontWeight::MEDIUM).leading(1.3)
    }

    // --- interface: sans, the default ---

    /// Body copy and field values.
    pub const fn body() -> Self {
        Self::new(TextRole::Ui, 13.5, FontWeight::NORMAL)
    }

    /// Résumé prose — Summary, bullets. Sans, never mono (L-05).
    pub const fn prose() -> Self {
        Self::new(TextRole::Ui, 13.5, FontWeight::NORMAL).leading(1.55)
    }

    /// Buttons, tabs, rows — anything the user clicks.
    ///
    /// 14, not 13, and the reason is a constraint rather than a taste: a
    /// `Button` renders its label at whatever `button_text_size` maps its `Size`
    /// to, which is 12, 14 or 16 and nothing in between (see
    /// `crate::styles`). A scale that named 13 here would have been describing a
    /// size no button in the application could actually be.
    pub const fn control() -> Self {
        Self::new(TextRole::Ui, 14.0, FontWeight::MEDIUM)
    }

    /// A field label above its input.
    pub const fn label() -> Self {
        Self::new(TextRole::Ui, 12.0, FontWeight::MEDIUM)
    }

    /// A small-caps section marker ("STORAGE", "APPEARANCE").
    pub const fn eyebrow() -> Self {
        Self::new(TextRole::Data, 11.0, FontWeight::MEDIUM).uppercase()
    }

    // --- data: mono, and only data ---

    /// Dates, counts, "updated 3w ago".
    pub const fn meta() -> Self {
        Self::new(TextRole::Data, 11.5, FontWeight::NORMAL)
    }

    /// A metric or status chip (`↓ 50% p99`, `2 variants`).
    pub const fn chip() -> Self {
        Self::new(TextRole::Data, 11.0, FontWeight::MEDIUM)
    }

    /// File paths and keyboard hints.
    pub const fn code() -> Self {
        Self::new(TextRole::Data, 12.0, FontWeight::NORMAL)
    }

    /// Apply this style's casing to a string, since GPUI cannot.
    pub fn apply_case(&self, text: &str) -> String {
        if self.uppercase {
            text.to_uppercase()
        } else {
            text.to_string()
        }
    }
}

/// Applies a [`TextStyle`] to any styled element.
pub trait StyledText: Styled + Sized {
    fn text_style(self, style: TextStyle) -> Self {
        let TextStyle {
            role,
            size,
            weight,
            leading,
            ..
        } = style;

        self.font_family(role.family())
            .text_size(size)
            .font_weight(weight)
            .line_height(px(f32::from(size) * leading))
    }
}

impl<T: Styled + Sized> StyledText for T {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mono carries data. Résumé prose set in mono is what the redesign is
    /// moving away from, so the scale must not offer it by accident.
    #[test]
    fn prose_and_body_are_never_mono() {
        for style in [TextStyle::body(), TextStyle::prose(), TextStyle::control()] {
            assert_eq!(style.role, TextRole::Ui, "interface text must be sans");
        }
        for style in [TextStyle::hero(), TextStyle::title(), TextStyle::heading()] {
            assert_eq!(style.role, TextRole::Display, "display text must be serif");
        }
        for style in [TextStyle::meta(), TextStyle::chip(), TextStyle::code()] {
            assert_eq!(style.role, TextRole::Data, "data must be mono");
        }
    }

    #[test]
    fn only_the_eyebrow_shouts() {
        assert_eq!(TextStyle::eyebrow().apply_case("Storage"), "STORAGE");
        assert_eq!(TextStyle::body().apply_case("Storage"), "Storage");
    }

    #[test]
    fn the_scale_descends() {
        let steps = [
            TextStyle::hero(),
            TextStyle::title(),
            TextStyle::heading(),
            TextStyle::body(),
            TextStyle::label(),
            TextStyle::meta(),
        ];
        for pair in steps.windows(2) {
            assert!(
                pair[0].size > pair[1].size,
                "{:?} should be larger than {:?}",
                pair[0].role,
                pair[1].role
            );
        }
    }
}

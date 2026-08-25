//! Theme tokens, palettes, and global accessors for `dockcv-ui`.
//!
//! A theme is **data**: a flat set of semantic [`Hsla`] tokens held as a GPUI
//! `Global` and read through [`ActiveTheme`]. Views never name a color — they
//! name a role. That is what makes a second temperature (the warm *Clay*
//! palette from the design mockup) a new constructor here rather than a fork of
//! every screen.
//!
//! ## Elevation
//!
//! Five surfaces, ordered from furthest-from-content to closest. The ordering is
//! the same in both palettes even though the direction of "lighter" flips:
//!
//! | Token        | What sits on it                                  |
//! |--------------|--------------------------------------------------|
//! | [`chrome`]   | title bar, nav rail — window furniture           |
//! | [`background`] | the window behind content                      |
//! | [`surface`]  | sidebars, toolbars, panels                       |
//! | [`elevated`] | cards and raised panels on a surface             |
//! | [`hover`]    | interactive fill on top of any of the above      |
//!
//! [`chrome`]: Theme::chrome
//! [`background`]: Theme::background
//! [`surface`]: Theme::surface
//! [`elevated`]: Theme::elevated
//! [`hover`]: Theme::hover
//!
//! ## Contrast
//!
//! Every foreground token clears **4.5:1** against every one of those five
//! surfaces — enforced by [`tests::foreground_tokens_meet_wcag_aa`], not by
//! taste. The design mockup's own muted greys do not clear it (`#5d6573`
//! measures 2.55:1 on `hover`); dates, counts and `updated 3w ago` live in
//! exactly that token, so the mockup values are corrected here rather than
//! reproduced. See P-16 in `docs/user-review.md`.

pub mod bridge;
pub mod tint;

/// The geometry ladder, as constants.
///
/// [`Theme`] exposes each of these as a method so a view reads it the way it
/// reads a colour (`cx.theme().radius_lg()`). They are also public here because
/// [`crate::styles`] builds the button vocabulary in `const` position, where a
/// `&Theme` is not available.
pub mod metrics {
    use gpui::{px, Pixels};

    /// Chips, tags, inline badges, the smallest hit targets.
    pub const RADIUS_SM: Pixels = px(6.0);
    /// Buttons, inputs, selectors, menu items — the default control radius.
    pub const RADIUS_MD: Pixels = px(8.0);
    /// Cards, panels and popovers.
    pub const RADIUS_LG: Pixels = px(11.0);
    /// Sheets, modals, anything that covers the window.
    pub const RADIUS_XL: Pixels = px(14.0);

    /// A control tucked inside a dense row: the `···` menu, a variant tab.
    pub const CONTROL_XS: Pixels = px(24.0);
    /// A compact control in a sidebar or a card footer.
    pub const CONTROL_SM: Pixels = px(28.0);
    /// The default: toolbar buttons, selectors, single-line inputs.
    pub const CONTROL_MD: Pixels = px(32.0);
    /// A screen's one primary action, and the field it sits beside.
    pub const CONTROL_LG: Pixels = px(38.0);

    /// A glyph beside `TextStyle::meta()` or `chip()` text.
    pub const ICON_SM: Pixels = px(12.0);
    /// A glyph beside `TextStyle::control()` or `body()` text — the default.
    pub const ICON_MD: Pixels = px(14.0);
    /// A glyph that carries a row on its own, with no label beside it.
    pub const ICON_LG: Pixels = px(18.0);
}

use gpui::{rgb, rgba, App, Global, Hsla};

/// Which palette to show. Persisted in app config; switched from Settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    SlateDark,
    SlateLight,
}

impl ThemeMode {
    /// Human-readable name for the Settings switcher.
    pub fn label(self) -> &'static str {
        match self {
            Self::SlateDark => "Slate Dark",
            Self::SlateLight => "Slate Light",
        }
    }

    pub const ALL: [ThemeMode; 2] = [ThemeMode::SlateDark, ThemeMode::SlateLight];
}

/// `Copy`, deliberately: every field is an `Hsla` or a `ThemeMode`, so a theme
/// is ~450 bytes of plain data. Views read it in render loops and used to
/// `.clone()` it to escape the borrow on `cx`; copying makes that free and lets
/// a view take the tokens it needs without the destructure-into-a-tuple dance.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    /// Which palette this is, so the Settings switcher can show the active one.
    pub mode: ThemeMode,

    // --- surfaces, in elevation order ---
    /// Window furniture: title bar and nav rail. Furthest from content.
    pub chrome: Hsla,
    /// Window background, behind every other surface.
    pub background: Hsla,
    /// Sidebars, toolbars, and other panels (one step up).
    pub surface: Hsla,
    /// Cards / raised panels sitting on top of a surface.
    pub elevated: Hsla,
    /// Hover/active fill for interactive surfaces.
    pub hover: Hsla,
    /// Fill for the currently selected item (nav entry, list row, board card).
    /// Accent-tinted and translucent so it reads on any surface.
    pub selected: Hsla,

    // --- lines ---
    /// Hairline separators and outlines (subtle).
    pub border: Hsla,
    /// Borders and decorative glyphs that must actually read — drag handles,
    /// path separators, the outline of a focused control.
    pub border_strong: Hsla,

    // --- text ---
    /// Primary foreground text.
    pub text: Hsla,
    /// Secondary / de-emphasized text.
    pub text_muted: Hsla,
    /// Tertiary text: dates, counts, placeholders, `updated 3w ago`.
    pub text_subtle: Hsla,

    // --- accent ---
    /// Brand / interactive accent.
    pub accent: Hsla,
    /// Accent on hover.
    pub accent_hover: Hsla,
    /// Foreground for content sitting on the accent.
    pub on_accent: Hsla,
    /// Translucent accent wash behind a chip or tag.
    pub chip_bg: Hsla,
    /// Text on top of [`Theme::chip_bg`].
    pub chip_fg: Hsla,
    /// Wash behind a chip that carries no accent — a count, a status word.
    /// Translucent so it reads on any surface. Text on it is [`Theme::text_muted`].
    pub chip_bg_neutral: Hsla,
    /// Focus ring drawn around the keyboard-focused control.
    pub focus_ring: Hsla,

    // --- status ---
    /// Destructive / error.
    pub danger: Hsla,
    /// Success, and the quiet "this has content" indicator.
    pub success: Hsla,
    /// Warning — also the amber edge on cells that differ between presets.
    pub warning: Hsla,
    /// A status that is over: the Applications board's Rejected column.
    ///
    /// Not [`Theme::danger`], deliberately. A rejection is not an error and not
    /// something the user did wrong — most applications end here, which is the
    /// whole reason the review insists the column exist (P-04). Painting it red
    /// would make the ordinary outcome look like a failure state.
    pub status_closed: Hsla,
    /// One step *below* [`Theme::elevated`]: a card that is still on the board
    /// but no longer live. The only surface in the palette that recedes rather
    /// than raises.
    pub elevated_muted: Hsla,

    // --- document ---
    /// The surface the paper sheet sits *on* — the desk under the document.
    ///
    /// Deliberately deeper than [`Theme::background`]: the review's L-08 asks the
    /// preview to read as a document on a desk rather than as another panel, and
    /// that only works if the backdrop is distinct from the window behind it.
    pub canvas: Hsla,
    /// The rendered résumé sheet in the preview. A document is paper in every
    /// palette; it does not invert with the UI.
    pub paper: Hsla,
    /// Hairline *on* [`Theme::paper`] — the only border drawn on a light document
    /// surface, so it cannot come from the UI-chrome border tokens.
    pub paper_border: Hsla,
    /// Backdrop behind a modal sheet or overlay.
    pub scrim: Hsla,
}

impl Theme {
    /// Slate Dark — the default. Values trace to the design mockup's Slate
    /// specimen, with the tertiary text token raised to clear 4.5:1.
    pub fn slate_dark() -> Self {
        Self {
            mode: ThemeMode::SlateDark,

            chrome: rgb(0x101216).into(),
            background: rgb(0x14161a).into(),
            surface: rgb(0x171a1f).into(),
            elevated: rgb(0x1b1e23).into(),
            hover: rgb(0x23272e).into(),
            selected: rgba(0x6f8fd626).into(),

            border: rgb(0x262a30).into(),
            border_strong: rgb(0x3e444e).into(),

            text: rgb(0xeef1f6).into(),
            text_muted: rgb(0x9aa3b0).into(),
            // Mockup uses #5d6573 here (2.55:1 on `hover`). Raised to 4.7:1.
            text_subtle: rgb(0x88919f).into(),

            accent: rgb(0x6f8fd6).into(),
            accent_hover: rgb(0x84a0e0).into(),
            on_accent: rgb(0x0f1320).into(),
            chip_bg: rgba(0x6f8fd61f).into(),
            chip_fg: rgb(0x9bb3e6).into(),
            chip_bg_neutral: rgba(0xdce4f00d).into(),
            focus_ring: rgba(0x6f8fd6cc).into(),

            danger: rgb(0xe0685f).into(),
            success: rgb(0x7fa28f).into(),
            warning: rgb(0xe5c07b).into(),
            // Both from the Applications row's Slate variant, verbatim.
            status_closed: rgb(0x6b7280).into(),
            elevated_muted: rgb(0x16181c).into(),

            canvas: rgb(0x0f1115).into(),
            paper: rgb(0xf6f7f9).into(),
            paper_border: rgb(0xe3e5e9).into(),
            scrim: rgba(0x0b0d1099).into(),
        }
    }

    /// Slate Light — the same hues, inverted elevation. Foregrounds are darkened
    /// until they clear 4.5:1 against `hover`, the darkest light surface.
    pub fn slate_light() -> Self {
        Self {
            mode: ThemeMode::SlateLight,

            chrome: rgb(0xeaedf2).into(),
            background: rgb(0xf2f4f7).into(),
            surface: rgb(0xf8f9fb).into(),
            elevated: rgb(0xffffff).into(),
            hover: rgb(0xe8ebf1).into(),
            selected: rgba(0x3863c422).into(),

            border: rgb(0xdde1e8).into(),
            border_strong: rgb(0xb9c0cc).into(),

            text: rgb(0x1b1f26).into(),
            text_muted: rgb(0x4c5563).into(),
            text_subtle: rgb(0x5d6573).into(),

            accent: rgb(0x3863c4).into(),
            accent_hover: rgb(0x2b53ad).into(),
            on_accent: rgb(0xffffff).into(),
            chip_bg: rgba(0x3863c41f).into(),
            chip_fg: rgb(0x2b53ad).into(),
            chip_bg_neutral: rgba(0x1b1f260d).into(),
            focus_ring: rgba(0x3863c4cc).into(),

            danger: rgb(0xc23026).into(),
            success: rgb(0x506e5e).into(),
            warning: rgb(0x86611a).into(),
            // Light has no drawn Applications row to copy: `status_closed` is
            // the palette's own subtle grey, and `elevated_muted` recedes from
            // white the way the dark palette's recedes from `elevated`.
            status_closed: rgb(0x8a919e).into(),
            elevated_muted: rgb(0xeef0f4).into(),

            canvas: rgb(0xdfe3ea).into(),
            paper: rgb(0xffffff).into(),
            paper_border: rgb(0xe3e5e9).into(),
            scrim: rgba(0x1b1f2666).into(),
        }
    }

    // --- geometry ---
    //
    // Methods, not fields, and on purpose: geometry is identical in every
    // palette — a second temperature (the warm *Clay* set) changes colour, never
    // the size of a control. As fields these would be twenty-two lines of
    // verbatim duplication across the two constructors. As methods the call site
    // still reads as a token (`cx.theme().radius_lg()`) and a palette that one
    // day *does* want its own metric can override one without touching a single
    // call site.
    //
    // Before this existed the codebase carried **ten** corner radii (2, 5, 6, 7,
    // 8, 9, 10, 11, 12, 9999) for what are conceptually four shapes, because
    // every screen picked its own number. See `docs/design/component-audit.md`.

    /// Chips, tags, inline badges, the smallest hit targets.
    pub const fn radius_sm(&self) -> gpui::Pixels {
        metrics::RADIUS_SM
    }

    /// Buttons, inputs, selectors, menu items — the default control radius, and
    /// what `radius` is set to on upstream's theme so its widgets agree.
    pub const fn radius_md(&self) -> gpui::Pixels {
        metrics::RADIUS_MD
    }

    /// Cards, panels and popovers. The gallery, library, diary and applications
    /// cards all converged on 11px independently; this is that number, named.
    pub const fn radius_lg(&self) -> gpui::Pixels {
        metrics::RADIUS_LG
    }

    /// Sheets, modals, the import wizard — anything that covers the window.
    pub const fn radius_xl(&self) -> gpui::Pixels {
        metrics::RADIUS_XL
    }

    /// A control tucked inside a dense row: the `···` menu, a variant tab.
    pub const fn control_xs(&self) -> gpui::Pixels {
        metrics::CONTROL_XS
    }

    /// A compact control in a sidebar or a card footer.
    pub const fn control_sm(&self) -> gpui::Pixels {
        metrics::CONTROL_SM
    }

    /// The default: toolbar buttons, selectors, single-line inputs.
    pub const fn control_md(&self) -> gpui::Pixels {
        metrics::CONTROL_MD
    }

    /// A screen's one primary action (`+ New CV`), and the field it sits beside.
    pub const fn control_lg(&self) -> gpui::Pixels {
        metrics::CONTROL_LG
    }

    /// A glyph beside `TextStyle::meta()` or `chip()` text.
    pub const fn icon_sm(&self) -> gpui::Pixels {
        metrics::ICON_SM
    }

    /// A glyph beside `TextStyle::control()` or `body()` text — the default.
    pub const fn icon_md(&self) -> gpui::Pixels {
        metrics::ICON_MD
    }

    /// A glyph that carries a row on its own, without a label beside it.
    pub const fn icon_lg(&self) -> gpui::Pixels {
        metrics::ICON_LG
    }

    pub fn of(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::SlateDark => Self::slate_dark(),
            ThemeMode::SlateLight => Self::slate_light(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::slate_dark()
    }
}

/// Key struct to register Theme as a GPUI `Global`.
struct GlobalTheme(Theme);

impl Global for GlobalTheme {}

/// Extension trait for convenient theme access from GPUI contexts.
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        if self.has_global::<GlobalTheme>() {
            &self.global::<GlobalTheme>().0
        } else {
            static DEFAULT_THEME: std::sync::OnceLock<Theme> = std::sync::OnceLock::new();
            DEFAULT_THEME.get_or_init(Theme::slate_dark)
        }
    }
}

/// Initialize the global theme state on the GPUI application context.
pub fn init_theme(cx: &mut App, theme: Theme) {
    cx.set_global(GlobalTheme(theme));
}

/// Swap the active palette. Safe to call before [`init_theme`].
pub fn set_theme_mode(cx: &mut App, mode: ThemeMode) {
    let theme = Theme::of(mode);
    bridge::apply(cx, &theme);
    cx.set_global(GlobalTheme(theme));
    cx.refresh_windows();
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Rgba;

    /// The geometry ladders must actually be ladders. A radius or a height that
    /// lands out of order is how a "small" control ends up taller than a
    /// "medium" one, which is the failure the ten ad-hoc radii were made of.
    #[test]
    fn the_geometry_ladders_ascend() {
        let t = Theme::slate_dark();
        assert!(t.radius_sm() < t.radius_md());
        assert!(t.radius_md() < t.radius_lg());
        assert!(t.radius_lg() < t.radius_xl());

        assert!(t.control_xs() < t.control_sm());
        assert!(t.control_sm() < t.control_md());
        assert!(t.control_md() < t.control_lg());

        assert!(t.icon_sm() < t.icon_md());
        assert!(t.icon_md() < t.icon_lg());
    }

    /// Geometry is palette-independent by contract — Clay may change colour, not
    /// the size of a button. If that ever stops being true these become fields.
    #[test]
    fn geometry_does_not_vary_by_palette() {
        let (dark, light) = (Theme::slate_dark(), Theme::slate_light());
        assert_eq!(dark.radius_md(), light.radius_md());
        assert_eq!(dark.control_md(), light.control_md());
        assert_eq!(dark.icon_md(), light.icon_md());
    }

    /// WCAG 2.1 relative luminance.
    fn luminance(color: Hsla) -> f32 {
        let rgba: Rgba = color.into();
        let lin = |c: f32| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(rgba.r) + 0.7152 * lin(rgba.g) + 0.0722 * lin(rgba.b)
    }

    /// WCAG 2.1 contrast ratio. Both inputs must be opaque.
    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// The floor the review's P-16 asks for. Metadata is the information the
    /// user came to the screen to read; it does not get to be decorative.
    const AA: f32 = 4.5;

    fn assert_palette_readable(theme: &Theme) {
        let surfaces = [
            ("chrome", theme.chrome),
            ("background", theme.background),
            ("surface", theme.surface),
            ("elevated", theme.elevated),
            ("hover", theme.hover),
        ];
        let foregrounds = [
            ("text", theme.text),
            ("text_muted", theme.text_muted),
            ("text_subtle", theme.text_subtle),
            ("accent", theme.accent),
            ("accent_hover", theme.accent_hover),
            ("danger", theme.danger),
            ("success", theme.success),
            ("warning", theme.warning),
        ];

        for (fg_name, fg) in foregrounds {
            for (bg_name, bg) in surfaces {
                let ratio = contrast(fg, bg);
                assert!(
                    ratio >= AA,
                    "{:?}: {fg_name} on {bg_name} is {ratio:.2}:1, below {AA}:1",
                    theme.mode,
                );
            }
        }

        // Chips carry their own background, so they are checked against it. Both
        // washes are translucent; compose them over each surface first.
        for (bg_name, bg) in surfaces {
            for (chip, fg, label) in [
                (theme.chip_bg, theme.chip_fg, "chip_fg on chip_bg"),
                (theme.chip_bg_neutral, theme.text_muted, "text_muted on chip_bg_neutral"),
            ] {
                let ratio = contrast(fg, over(chip, bg));
                assert!(
                    ratio >= AA,
                    "{:?}: {label} over {bg_name} is {ratio:.2}:1, below {AA}:1",
                    theme.mode,
                );
            }
        }

        let ratio = contrast(theme.on_accent, theme.accent);
        assert!(
            ratio >= AA,
            "{:?}: on_accent on accent is {ratio:.2}:1, below {AA}:1",
            theme.mode,
        );
    }

    /// Source-over composite of a translucent color onto an opaque one.
    fn over(fg: Hsla, bg: Hsla) -> Hsla {
        let (f, b): (Rgba, Rgba) = (fg.into(), bg.into());
        Rgba {
            r: f.r * f.a + b.r * (1.0 - f.a),
            g: f.g * f.a + b.g * (1.0 - f.a),
            b: f.b * f.a + b.b * (1.0 - f.a),
            a: 1.0,
        }
        .into()
    }

    #[test]
    fn foreground_tokens_meet_wcag_aa() {
        assert_palette_readable(&Theme::slate_dark());
        assert_palette_readable(&Theme::slate_light());
    }

    /// L-08: the sheet must read as sitting *on* something. If the desk ever
    /// matches the paper, the document stops being a document.
    #[test]
    fn paper_reads_as_raised_above_the_canvas() {
        for theme in [Theme::slate_dark(), Theme::slate_light()] {
            let ratio = contrast(theme.paper, theme.canvas);
            assert!(
                ratio >= 1.2,
                "{:?}: paper on canvas is only {ratio:.2}:1 — the sheet would vanish into the desk",
                theme.mode,
            );
        }
    }

    #[test]
    fn elevation_is_monotonic() {
        for theme in [Theme::slate_dark(), Theme::slate_light()] {
            let ladder = [
                ("chrome", theme.chrome),
                ("background", theme.background),
                ("surface", theme.surface),
                ("elevated", theme.elevated),
            ];
            // Dark palettes climb toward white, light palettes toward black;
            // either way each step must move away from the one before it.
            let ascending = luminance(theme.elevated) > luminance(theme.chrome);
            for pair in ladder.windows(2) {
                let [(lower, lo), (upper, hi)] = [pair[0], pair[1]];
                let moved = if ascending {
                    luminance(hi) > luminance(lo)
                } else {
                    luminance(hi) < luminance(lo)
                };
                assert!(
                    moved,
                    "{:?}: {upper} does not sit above {lower} in the elevation ladder",
                    theme.mode,
                );
            }
        }
    }

    #[test]
    fn every_mode_has_a_palette() {
        for mode in ThemeMode::ALL {
            assert_eq!(Theme::of(mode).mode, mode);
            assert!(!mode.label().is_empty());
        }
    }
}

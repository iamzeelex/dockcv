//! Shared geometry, and the button vocabulary every screen builds from.
//!
//! Before this file grew a vocabulary, `Button` was dressed at each of its 58
//! call sites: five different label sizes (12, 12, 13, 14 and 16px — the last
//! because upstream's `button_text_size` maps `Size::Medium` to `text_base`,
//! which is `rems(1.0)` against an un-overridden 16px rem), five radii, and six
//! ways of dressing the same dropdown trigger. `docs/design/component-audit.md`
//! has the census.
//!
//! The rule that replaces it: **a view never sizes a button.** It picks the role
//! the button plays, and the role decides height, padding, radius and type.
//!
//! ## Where a button's label size actually comes from
//!
//! Not from `.text_size()`, and not from `TextStyle`. `Button::render` puts its
//! label inside an inner `h_flex().size_full()` that sets `button_text_size(self.size)`
//! itself, and a child's own font size beats anything set on the element around
//! it. So the outer `refine_style` — which is everything a caller can reach —
//! never touches the label.
//!
//! That leaves exactly three label sizes, the ones upstream's ladder maps to:
//!
//! | `Size` | `button_text_size` | px (rem 16) |
//! |---|---|---|
//! | `XSmall` | `text_xs` | 12 |
//! | `Small` | `text_sm` | 14 |
//! | anything else | `text_base` | **16** |
//!
//! Every role below therefore states its `Size` explicitly. Leaving it unset is
//! not "inherit", it is *16px* — which is how this file's first version put every
//! label in the application a size or two above what it was before.
//!
//! ## And why nothing here is a left-aligned row
//!
//! That same inner flex is `justify_center()`, and no caller can reach it either.
//! A `Button` is centred, full stop. Anything that has to read as a row — a nav
//! entry, a list item — is a [`ListItem`](gpui_component::list::ListItem), not a
//! button wearing a row's padding.

use gpui::{px, FontWeight, Styled};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Sizable as _, Size};

use crate::theme::{metrics, Theme};
use crate::typography::TextStyle;

/// The height of a screen's top chrome bar.
///
/// One value, not one per screen. The editor's bar and the Preset Matrix's were
/// both 46px, chosen separately and never compared — and 46 is too little for
/// what the editor's carries: a two-line identity block beside four controls,
/// with macOS's traffic lights already occupying the left 80px. A bar that
/// tight reads as a strip the window is trying to hide rather than as the place
/// the document's controls live.
pub const CHROME_HEIGHT: gpui::Pixels = px(56.0);

/// The button roles. Ten of them, and between them they cover every control in
/// the application that is not a text field or a row.
///
/// The last three exist because `Button` turned out to be the answer to the 45
/// hand-rolled `div().cursor_pointer().on_click(..)` controls as well. Upstream's
/// button already carries a selected fill, a pressed state, a disabled state, a
/// focus ring, `Role::Button` and an `aria_label`; every one of those was absent
/// from all 45. A bespoke `Chip` would have had to reimplement the lot, which is
/// what `crates/ui-components/THIRD_PARTY.md` rule 2 exists to prevent.
///
/// Each sets height, horizontal padding, radius and type; none of them is a
/// starting point a caller then adjusts. If a button needs a shape that is not
/// here, the shape is missing from the vocabulary — add it here, with a note
/// saying what it is for, rather than tuning one call site.
pub trait ButtonExt {
    /// The one action a screen exists to offer: `+ New CV`, `+ New block`,
    /// `+ New application`. Accent-filled, tallest in the ladder.
    fn action_primary(self) -> Self;

    /// The alternative beside a primary action — `Start blank` next to
    /// `Import a PDF`, `Cancel` next to `Save`. Same height, outlined.
    fn action_secondary(self) -> Self;

    /// A filled action living in a toolbar rather than in a hero: `Export PDF`.
    fn toolbar_primary(self) -> Self;

    /// The ordinary toolbar control — sort, filter, `Capture`. Outlined rather
    /// than filled, so a row of them reads as one group instead of as a row of
    /// buttons competing with the screen's real action.
    fn toolbar(self) -> Self;

    /// Anything that opens a menu of choices and shows the current one.
    ///
    /// Pair with `dropdown_menu`, and **do not add a `ChevronDown` icon**: this
    /// turns on upstream's `dropdown_caret`, which draws the caret *after* the
    /// label and justifies the two apart. Passing the chevron as `.icon(..)`
    /// instead — which is what all 21 triggers used to do — puts it on the
    /// wrong side, because `Button::render` emits its icon before its label.
    fn selector(self) -> Self;

    /// A selector in a dense row, where a full-height control would not fit:
    /// the Insights period, `Reuse` on a library block, `used in 3 CVs`.
    fn selector_inline(self) -> Self;

    /// A square, label-less control: the `···` menu, a section's gear.
    fn icon_only(self) -> Self;

    /// An action that should read as text rather than as a control —
    /// breadcrumbs, `Save as preset`, inline links inside a card.
    fn quiet(self) -> Self;

    /// A selectable inline pill: a filter chip, a preset pill, a variant tab, a
    /// role facet.
    ///
    /// Takes its own `selected` flag rather than pairing with
    /// [`Selectable::selected`](gpui_component::Selectable::selected), and takes
    /// a [`Theme`] with it. Both follow from the same fact: a chip's rest fill is
    /// `chip_bg_neutral` and its selected fill is `chip_bg` — neither is a colour
    /// upstream's variant map can produce, so the role has to paint them, and a
    /// role that paints a fill through `Styled` paints it in *every* state.
    /// Branching here keeps both states in tokens and in one place.
    ///
    /// Hover still comes from upstream: `.hover()` writes a separate style slot,
    /// which a base-style refinement does not touch.
    fn chip(self, selected: bool, theme: &Theme) -> Self;

    /// The dashed "one more" affordance that ends a list: `＋ Add`,
    /// `＋ new variant`, `★ From library`. [`ButtonExt::chip`]'s geometry, drawn
    /// as an outline that is asking rather than a control that is asserting.
    fn chip_dashed(self, theme: &Theme) -> Self;
}

/// One rung of the ladder: everything a role decides that is not colour.
///
/// A `const` table rather than a `match` buried in each role, so
/// [`tests`] asserts against the rungs the roles actually use. The audit's
/// standing lesson is that a scale nothing checks drifts — this file shipped a
/// version where every label was 16px and the type scale said 13.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rung {
    /// Upstream's `Size`, which is the **only** thing that reaches the label.
    pub size: Size,
    pub height: gpui::Pixels,
    pub pad_x: gpui::Pixels,
    pub radius: gpui::Pixels,
    pub weight: FontWeight,
}

impl Rung {
    const fn new(size: Size, height: gpui::Pixels, pad_x: gpui::Pixels) -> Self {
        Self {
            size,
            height,
            pad_x,
            radius: metrics::RADIUS_MD,
            weight: FontWeight::NORMAL,
        }
    }

    /// A role that carries weight: the screen's own action, or a chip in force.
    const fn medium(mut self) -> Self {
        self.weight = FontWeight::MEDIUM;
        self
    }

    const fn tight(mut self) -> Self {
        self.radius = metrics::RADIUS_SM;
        self
    }

    /// The size the label *actually* renders at, straight off upstream's
    /// `button_text_size`. Not a preference — a readout.
    pub fn label_px(&self) -> gpui::Pixels {
        match self.size {
            Size::XSmall => px(12.0),
            Size::Small => px(14.0),
            _ => px(16.0),
        }
    }
}

/// The ladder. One rung per role, and the roles below are the only readers.
pub mod rung {
    use super::{metrics, px, Rung, Size};

    pub const ACTION_PRIMARY: Rung = Rung::new(Size::Small, metrics::CONTROL_LG, px(16.0)).medium();
    pub const ACTION_SECONDARY: Rung =
        Rung::new(Size::Small, metrics::CONTROL_LG, px(16.0)).medium();
    pub const TOOLBAR_PRIMARY: Rung =
        Rung::new(Size::Small, metrics::CONTROL_MD, px(14.0)).medium();
    pub const TOOLBAR: Rung = Rung::new(Size::Small, metrics::CONTROL_MD, px(12.0));
    pub const SELECTOR: Rung = Rung::new(Size::Small, metrics::CONTROL_MD, px(11.0));
    pub const SELECTOR_INLINE: Rung = Rung::new(Size::XSmall, metrics::CONTROL_XS, px(8.0)).tight();
    pub const ICON_ONLY: Rung = Rung::new(Size::XSmall, metrics::CONTROL_SM, px(0.0)).tight();
    pub const QUIET: Rung = Rung::new(Size::Small, metrics::CONTROL_SM, px(6.0)).tight();
    pub const CHIP: Rung = Rung::new(Size::XSmall, metrics::CONTROL_XS, px(10.0)).tight();

    /// Every rung, for the tests that keep the ladder a ladder.
    pub const ALL: [(&str, Rung); 9] = [
        ("action_primary", ACTION_PRIMARY),
        ("action_secondary", ACTION_SECONDARY),
        ("toolbar_primary", TOOLBAR_PRIMARY),
        ("toolbar", TOOLBAR),
        ("selector", SELECTOR),
        ("selector_inline", SELECTOR_INLINE),
        ("icon_only", ICON_ONLY),
        ("quiet", QUIET),
        ("chip", CHIP),
    ];
}

/// The shared body of every role: cursor, height, padding, radius, type. Kept in
/// one place so a role differs from its neighbour only where it means to.
///
/// The cursor is settled here rather than at the call site. Upstream renders a
/// button with `cursor_default` — the macOS convention — and DockCV overrode it
/// to a pointer on forty-odd buttons and forgot on the rest. That is a decision
/// worth making once; this is where it is made.
fn shell(button: Button, rung: Rung) -> Button {
    button
        .with_size(rung.size)
        .cursor_pointer()
        .h(rung.height)
        .px(rung.pad_x)
        .rounded(rung.radius)
        // Family only. The *size* comes from `rung.size` above (see the module
        // note), and the weight from the rung — `TextStyle::control()` would
        // make every label medium, which is what made a rail of four nav
        // entries read as four headings.
        .font_family(TextStyle::control().role.family())
        .font_weight(rung.weight)
}

impl ButtonExt for Button {
    fn action_primary(self) -> Self {
        shell(self.primary(), rung::ACTION_PRIMARY)
    }

    fn action_secondary(self) -> Self {
        shell(self.outline(), rung::ACTION_SECONDARY)
    }

    fn toolbar_primary(self) -> Self {
        shell(self.primary(), rung::TOOLBAR_PRIMARY)
    }

    fn toolbar(self) -> Self {
        shell(self.outline(), rung::TOOLBAR)
    }

    fn selector(self) -> Self {
        shell(self.outline(), rung::SELECTOR).dropdown_caret(true)
    }

    fn selector_inline(self) -> Self {
        shell(self.outline(), rung::SELECTOR_INLINE).dropdown_caret(true)
    }

    fn icon_only(self) -> Self {
        // Square, so the rung's `pad_x` is zero and the box is set outright.
        self.ghost()
            .with_size(rung::ICON_ONLY.size)
            .cursor_pointer()
            .size(rung::ICON_ONLY.height)
            .rounded(rung::ICON_ONLY.radius)
    }

    fn quiet(self) -> Self {
        shell(self.ghost(), rung::QUIET)
    }

    fn chip(self, selected: bool, theme: &Theme) -> Self {
        let chip = shell(self.ghost(), rung::CHIP);
        if selected {
            chip.bg(theme.chip_bg)
                .text_color(theme.chip_fg)
                .font_weight(FontWeight::MEDIUM)
        } else {
            chip.bg(theme.chip_bg_neutral)
        }
    }

    fn chip_dashed(self, theme: &Theme) -> Self {
        shell(self.ghost(), rung::CHIP)
            .bg(gpui::Hsla::transparent_black())
            .border_1()
            .border_dashed()
            .border_color(theme.border)
            .text_color(theme.text_subtle)
    }
}

/// The one row shape, on the element that can actually draw one.
///
/// [`ListItem`] rather than [`Button`] because a button centres its label and
/// cannot be told otherwise (see the module note). A nav entry whose text drifts
/// to the middle of the rail is the visible symptom of using the wrong element.
///
/// `ListItem` brings hover, `Selectable::selected`, `Disableable` and a click
/// handler; what it does not bring is a focus ring or `Role::Button`, because it
/// is a `Stateful<Div>` underneath. That gap is real and is recorded in
/// `docs/design/component-audit.md`.
pub trait ListItemExt {
    /// A selectable row inside a panel, a sheet or the nav rail.
    ///
    /// Sets no height: a row is as tall as its content, and several of these
    /// carry two lines.
    fn row(self) -> Self;
}

impl ListItemExt for gpui_component::list::ListItem {
    fn row(self) -> Self {
        self.px(px(8.0))
            .py(px(5.0))
            .rounded(metrics::RADIUS_MD)
            .cursor_pointer()
            .font_family(TextStyle::control().role.family())
            .text_size(TextStyle::control().size)
            .font_weight(FontWeight::NORMAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The regression this file exists to prevent.**
    ///
    /// `button_text_size` sends every `Size` that is not `XSmall` or `Small` to
    /// `text_base` — 16px. A role that forgets to state its `Size` therefore does
    /// not inherit anything; it lands on 16, silently, and takes every call site
    /// with it. That shipped once, across all 58 buttons.
    #[test]
    fn no_rung_falls_through_to_sixteen_pixels() {
        for (name, rung) in rung::ALL {
            assert!(
                matches!(rung.size, Size::XSmall | Size::Small),
                "`{name}` states no reachable Size, so its label renders at 16px"
            );
            assert_ne!(rung.label_px(), px(16.0), "`{name}` renders at 16px");
        }
    }

    /// The label sizes a Button can actually produce, and the only ones the type
    /// scale may name for a control.
    #[test]
    fn label_sizes_are_upstreams_and_not_ours() {
        assert_eq!(rung::TOOLBAR.label_px(), TextStyle::control().size);
        assert_eq!(rung::ACTION_PRIMARY.label_px(), px(14.0));
        assert_eq!(rung::CHIP.label_px(), px(12.0));
        assert_eq!(rung::SELECTOR_INLINE.label_px(), px(12.0));
    }

    /// Heights descend with how much of the screen a role is asking for.
    #[test]
    fn the_ladder_descends() {
        assert!(rung::ACTION_PRIMARY.height > rung::TOOLBAR.height);
        assert!(rung::TOOLBAR.height > rung::QUIET.height);
        assert!(rung::QUIET.height > rung::CHIP.height);
    }

    /// Roles that share a row share its height, or the row goes ragged. The
    /// toolbar's controls sit beside each other; so do a chip and an inline
    /// selector.
    #[test]
    fn roles_that_sit_together_line_up() {
        assert_eq!(rung::TOOLBAR.height, rung::SELECTOR.height);
        assert_eq!(rung::TOOLBAR.height, rung::TOOLBAR_PRIMARY.height);
        assert_eq!(rung::ACTION_PRIMARY.height, rung::ACTION_SECONDARY.height);
        assert_eq!(rung::CHIP.height, rung::SELECTOR_INLINE.height);
    }

    /// Weight is a role's decision, not the type scale's. Exactly the roles that
    /// carry an action carry the weight; a rail of medium-weight nav entries
    /// reads as a rail of headings.
    #[test]
    fn only_actions_and_a_chip_in_force_carry_weight() {
        let medium: Vec<&str> = rung::ALL
            .iter()
            .filter(|(_, r)| r.weight == FontWeight::MEDIUM)
            .map(|(n, _)| *n)
            .collect();
        assert_eq!(
            medium,
            vec!["action_primary", "action_secondary", "toolbar_primary"]
        );
    }

    /// Every rung rounds on the ladder, never on a number of its own.
    #[test]
    fn every_rung_rounds_on_the_ladder() {
        for (name, rung) in rung::ALL {
            assert!(
                rung.radius == metrics::RADIUS_SM || rung.radius == metrics::RADIUS_MD,
                "`{name}` rounds at {:?}, which is not a step on the ladder",
                rung.radius
            );
        }
    }
}

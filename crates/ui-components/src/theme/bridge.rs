//! Projects the DockCV palette onto `gpui-component`'s theme.
//!
//! We render *their* widgets, and those widgets colour themselves from *their*
//! `Theme` — 129 configurable fields against our 22 semantic tokens. This module
//! is the projection, and it is the reason a DockCV screen built from upstream
//! components still looks like Slate rather than like the upstream demo.
//!
//! Two properties make it tractable:
//!
//! 1. `Theme::apply_config` starts from upstream's own coherent `ThemeColor::dark()`
//!    / `light()` baseline and overlays only what the config sets. So we set the
//!    fields that carry Slate's identity and inherit sensible values for the rest
//!    (chart series, terminal base colours, accordion shading …).
//! 2. The config is built as JSON — upstream's own `themes/*.json` format, and
//!    the only way in, since `ThemeConfigColors` keeps some fields private. That
//!    also keeps the mapping a readable table rather than colour arithmetic.
//!
//! **Our [`Theme`] stays the source of truth.** It is where the palette is defined
//! and where the WCAG floor is enforced; this file only translates. Adding a token
//! means adding it there first, then routing it here.

use std::rc::Rc;

use gpui::{App, Hsla, Rgba};
use gpui_component::theme::{Theme as UpstreamTheme, ThemeConfig};

use super::{Theme, ThemeMode};
use crate::typography::{MONO, SANS};

/// Hand the palette to upstream. Call at startup and on every palette switch.
pub fn apply(cx: &mut App, theme: &Theme) {
    let config = Rc::new(config_for(theme));
    UpstreamTheme::global_mut(cx).apply_config(&config);
}

/// Upstream types its radii as whole pixels, so the ladder has to arrive as one.
fn px_int(value: gpui::Pixels) -> u64 {
    f32::from(value).round().max(0.0) as u64
}

/// `#rrggbb`, or `#rrggbbaa` when the token is translucent.
fn hex(color: Hsla) -> String {
    let rgba: Rgba = color.into();
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    if rgba.a >= 0.999 {
        format!("#{:02x}{:02x}{:02x}", byte(rgba.r), byte(rgba.g), byte(rgba.b))
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            byte(rgba.r),
            byte(rgba.g),
            byte(rgba.b),
            byte(rgba.a)
        )
    }
}

/// Upstream's vocabulary in our terms:
///
/// - `primary` — the accent; what a default button is made of
/// - `secondary` — a quiet raised control
/// - `muted` — de-emphasised fill and its text
/// - `popover` / `list` / `sidebar` / `title_bar` / `tab_bar` — our elevation ladder
fn slate_colors(theme: &Theme) -> Vec<(&'static str, Hsla)> {
    vec![
        // --- ground ---
        ("background", theme.background),
        ("foreground", theme.text),
        ("border", theme.border),
        ("input.border", theme.border),
        // The focus ring is its own token, not the accent. It shipped wired to
        // `accent` and so `Theme::focus_ring` — defined, documented and given a
        // value in both palettes — painted nowhere in the application.
        ("ring", theme.focus_ring),
        ("caret", theme.accent),
        ("selection.background", theme.selected),
        ("overlay", theme.scrim),
        ("window.border", theme.border),
        ("drag.border", theme.accent),
        ("drop_target.background", theme.selected),
        ("skeleton.background", theme.hover),

        // --- muted / secondary surfaces ---
        ("muted.background", theme.hover),
        ("muted.foreground", theme.text_subtle),
        ("secondary.background", theme.elevated),
        ("secondary.active.background", theme.selected),
        // `text`, not `text_muted`, would be wrong here: this is the foreground
        // of upstream's **Ghost** variant, which is what `quiet()` and
        // `icon_only()` are made of — the `···` menus, the ✕ closes, the inline
        // text actions. Every one of them was overriding it back to muted at the
        // call site. Routed once instead.
        ("secondary.foreground", theme.text_muted),
        ("secondary.hover.background", theme.hover),
        ("accent.background", theme.selected),
        ("accent.foreground", theme.text),
        ("accordion.background", theme.elevated),
        ("accordion.hover.background", theme.hover),
        ("group_box.background", theme.elevated),
        ("group_box.foreground", theme.text),
        ("group_box.title.foreground", theme.text_muted),

        // --- elevation ladder ---
        ("popover.background", theme.elevated),
        ("popover.foreground", theme.text),
        ("title_bar.background", theme.chrome),
        ("title_bar.border", theme.border),
        ("status_bar.background", theme.chrome),
        ("status_bar.border", theme.border),
        ("sidebar.background", theme.surface),
        ("sidebar.foreground", theme.text),
        ("sidebar.border", theme.border),
        ("sidebar.accent.background", theme.selected),
        ("sidebar.accent.foreground", theme.text),
        ("sidebar.primary.background", theme.accent),
        ("sidebar.primary.foreground", theme.on_accent),
        ("tiles.background", theme.background),

        // --- accent family ---
        ("primary.background", theme.accent),
        ("primary.hover.background", theme.accent_hover),
        ("primary.active.background", theme.accent_hover),
        ("primary.foreground", theme.on_accent),
        ("link", theme.accent),
        ("link.hover", theme.accent_hover),
        ("link.active", theme.accent_hover),
        ("progress.bar.background", theme.accent),
        ("slider.background", theme.accent),
        ("slider.thumb.background", theme.on_accent),

        // --- buttons ---
        ("button.background", theme.elevated),
        ("button.hover.background", theme.hover),
        ("button.active.background", theme.selected),
        ("button.foreground", theme.text),
        ("button.primary.background", theme.accent),
        ("button.primary.hover.background", theme.accent_hover),
        ("button.primary.active.background", theme.accent_hover),
        ("button.primary.foreground", theme.on_accent),
        ("button.secondary.background", theme.elevated),
        ("button.secondary.hover.background", theme.hover),
        ("button.secondary.active.background", theme.selected),
        ("button.secondary.foreground", theme.text),
        ("button.danger.background", theme.danger),
        ("button.danger.hover.background", theme.danger),
        ("button.danger.active.background", theme.danger),
        ("button.danger.foreground", theme.on_accent),
        ("button.success.background", theme.success),
        ("button.success.hover.background", theme.success),
        ("button.success.active.background", theme.success),
        ("button.success.foreground", theme.on_accent),
        ("button.warning.background", theme.warning),
        ("button.warning.hover.background", theme.warning),
        ("button.warning.active.background", theme.warning),
        ("button.warning.foreground", theme.on_accent),
        ("button.info.background", theme.accent),
        ("button.info.hover.background", theme.accent_hover),
        ("button.info.active.background", theme.accent_hover),
        ("button.info.foreground", theme.on_accent),

        // --- status ---
        ("danger.background", theme.danger),
        ("danger.hover.background", theme.danger),
        ("danger.active.background", theme.danger),
        ("danger.foreground", theme.on_accent),
        ("success.background", theme.success),
        ("success.hover.background", theme.success),
        ("success.active.background", theme.success),
        ("success.foreground", theme.on_accent),
        ("warning.background", theme.warning),
        ("warning.hover.background", theme.warning),
        ("warning.active.background", theme.warning),
        ("warning.foreground", theme.on_accent),
        ("info.background", theme.accent),
        ("info.hover.background", theme.accent_hover),
        ("info.active.background", theme.accent_hover),
        ("info.foreground", theme.on_accent),

        // --- lists and tables ---
        ("list.background", theme.surface),
        ("list.hover.background", theme.hover),
        ("list.active.background", theme.selected),
        ("list.active.border", theme.accent),
        ("list.even.background", theme.background),
        ("list.head.background", theme.surface),
        ("table.background", theme.surface),
        ("table.hover.background", theme.hover),
        ("table.active.background", theme.selected),
        ("table.active.border", theme.accent),
        ("table.even.background", theme.background),
        ("table.head.background", theme.surface),
        ("table.head.foreground", theme.text_muted),
        ("table.foot.background", theme.surface),
        ("table.foot.foreground", theme.text_muted),
        ("table.row.border", theme.border),
        ("description_list.label.background", theme.elevated),
        ("description_list.label.foreground", theme.text_muted),

        // --- tabs ---
        ("tab.background", theme.surface),
        ("tab.foreground", theme.text_muted),
        ("tab.active.background", theme.elevated),
        ("tab.active.foreground", theme.text),
        ("tab_bar.background", theme.background),
        ("tab_bar.segmented.background", theme.elevated),

        // --- upstream's base ramp ---
        //
        // Not decoration: `Avatar` derives its monogram colour from
        // `theme().blue`, and charts read the rest. Leaving them unset is what
        // made the vault marker render teal in a blue app — the exact "unrouted
        // field silently keeps the demo palette" failure this module exists to
        // prevent.
        ("base.blue", theme.accent),
        ("base.blue.light", theme.accent_hover),
        ("base.cyan", theme.accent_hover),
        ("base.cyan.light", theme.chip_fg),
        ("base.green", theme.success),
        ("base.green.light", theme.success),
        ("base.red", theme.danger),
        ("base.red.light", theme.danger),
        ("base.yellow", theme.warning),
        ("base.yellow.light", theme.warning),
        ("base.magenta", theme.accent),
        ("base.magenta.light", theme.accent_hover),

        // --- controls ---
        ("switch.background", theme.hover),
        ("switch.thumb.background", theme.text_muted),
        ("scrollbar.background", theme.background),
        ("scrollbar.thumb.background", theme.border_strong),
        ("scrollbar.thumb.hover.background", theme.text_subtle),

    ]
}

fn config_for(theme: &Theme) -> ThemeConfig {
    let mut json = serde_json::Map::new();
    json.insert("name".into(), theme.mode.label().into());
    json.insert(
        "mode".into(),
        if matches!(theme.mode, ThemeMode::SlateDark) {
            "dark"
        } else {
            "light"
        }
        .into(),
    );
    // Interface text is sans; only a code editor would want the mono family, and
    // DockCV never puts an input in that mode.
    json.insert("font.family".into(), SANS.into());
    json.insert("mono_font.family".into(), MONO.into());

    // Geometry. Without these upstream keeps its own defaults — `radius` 6 and
    // `radius.lg` 8 — so every upstream Button rounded at 6px while the pill
    // beside it rounded at 7 and the card behind it at 11. `ThemeConfig` types
    // both as whole pixels, which is why the ladder is integral.
    json.insert("radius".into(), px_int(theme.radius_md()).into());
    json.insert("radius.lg".into(), px_int(theme.radius_lg()).into());

    // The colours are a **nested** object on `ThemeConfig`, not top-level keys.
    // Flattening them here parses without error and silently drops every one.
    let mut colors = serde_json::Map::new();
    for (key, color) in slate_colors(theme) {
        colors.insert(key.into(), hex(color).into());
    }
    json.insert("colors".into(), serde_json::Value::Object(colors));

    serde_json::from_value(serde_json::Value::Object(json))
        .expect("the Slate projection must deserialize into upstream's ThemeConfig")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_opaque_and_translucent() {
        assert_eq!(hex(gpui::rgb(0x14161a).into()), "#14161a");
        // A translucent token must keep its alpha: upstream draws `selection`
        // and `overlay` over content, and a flattened value would hide it.
        assert_eq!(hex(gpui::rgba(0x6f8fd626).into()), "#6f8fd626");
    }

    /// The projection must carry Slate's identity, not upstream's defaults. If a
    /// palette change stops reaching these, every upstream widget silently
    /// reverts to the demo colours.
    #[test]
    fn identity_fields_come_from_our_palette() {
        for theme in [Theme::slate_dark(), Theme::slate_light()] {
            let map: std::collections::HashMap<_, _> = slate_colors(&theme).into_iter().collect();
            assert_eq!(map["background"], theme.background);
            assert_eq!(map["foreground"], theme.text);
            assert_eq!(map["primary.background"], theme.accent);
            assert_eq!(map["primary.foreground"], theme.on_accent);
            assert_eq!(map["overlay"], theme.scrim);
            assert_eq!(map["title_bar.background"], theme.chrome);
            assert_eq!(map["sidebar.background"], theme.surface);
            assert_eq!(map["muted.foreground"], theme.text_subtle);
            // The Ghost variant's foreground; see the note beside it.
            assert_eq!(map["secondary.foreground"], theme.text_muted);
            // Wired to `accent` once, which is how the focus ring went missing.
            assert_eq!(map["ring"], theme.focus_ring);
        }
    }

    /// Every colour must actually arrive.
    ///
    /// serde ignores unknown fields, so neither a mistyped key nor a key put in
    /// the wrong object fails to parse — it silently drops that colour and leaves
    /// the surface on upstream's demo palette. **This exact bug shipped once**: the
    /// colours were written at the top level of `ThemeConfig` instead of inside its
    /// nested `colors` object, so all 123 were discarded and every widget rendered
    /// grey.
    ///
    /// Checking key *names* is not enough — that is what the first version of this
    /// test did, and it passed while the bug was live. Round-trip the built config
    /// and assert the values are present on the other side.
    #[test]
    fn every_colour_survives_the_round_trip() {
        for theme in [Theme::slate_dark(), Theme::slate_light()] {
            let parsed = serde_json::to_value(config_for(&theme).colors)
                .expect("upstream's colour struct serializes");
            let parsed = parsed.as_object().expect("… as an object");

            for (key, color) in slate_colors(&theme) {
                let got = parsed.get(key).and_then(|v| v.as_str());
                assert_eq!(
                    got,
                    Some(hex(color).as_str()),
                    "`{key}` did not survive into ThemeConfig — that surface would \
                     silently keep upstream's demo colour"
                );
            }
        }
    }

    #[test]
    fn the_projection_deserializes_into_upstream() {
        for theme in [Theme::slate_dark(), Theme::slate_light()] {
            let config = config_for(&theme);
            assert_eq!(config.name.as_ref(), theme.mode.label());
        }
    }

    /// Geometry is `Option` on `ThemeConfig`: leaving it `None` is not an error,
    /// it silently keeps upstream's 6/8 and puts every Button on a different
    /// radius from everything drawn beside it. That is what shipped.
    #[test]
    fn the_radius_ladder_reaches_upstream() {
        for theme in [Theme::slate_dark(), Theme::slate_light()] {
            let config = config_for(&theme);
            assert_eq!(config.radius, Some(px_int(theme.radius_md()) as usize));
            assert_eq!(config.radius_lg, Some(px_int(theme.radius_lg()) as usize));
        }
    }

    /// Dark and light must not project to the same thing — the cheapest way to
    /// catch a mapping that accidentally ignores its argument.
    #[test]
    fn the_two_palettes_project_differently() {
        assert!(config_for(&Theme::slate_dark()).mode.is_dark());
        assert!(!config_for(&Theme::slate_light()).mode.is_dark());

        let dark: std::collections::HashMap<_, _> =
            slate_colors(&Theme::slate_dark()).into_iter().collect();
        let light: std::collections::HashMap<_, _> =
            slate_colors(&Theme::slate_light()).into_iter().collect();
        assert_ne!(dark["background"], light["background"]);
        assert_ne!(dark["foreground"], light["foreground"]);
    }
}

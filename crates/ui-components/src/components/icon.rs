//! Icons.
//!
//! The glyph set is upstream's — 99 [Lucide](https://lucide.dev) icons shipped by
//! `gpui-component-assets` and addressed through [`IconName`]. Use them:
//!
//! ```ignore
//! Icon::new(IconName::Star).small()
//! ```
//!
//! ## The three DockCV adds
//!
//! Lucide has no drag handle, no download glyph and no board icon, and the design
//! mockup uses all three (`⠿` on every section row, `↑` on the import drop zone,
//! the Applications nav entry). Those live in [`DockIcon`], which implements
//! upstream's [`IconNamed`] trait — the extension point they document for exactly
//! this — so it is a drop-in wherever `IconName` goes:
//!
//! ```ignore
//! Icon::new(DockIcon::Grip).small()
//! ```
//!
//! [`Assets`] serves both sets and is what the application registers.

use std::borrow::Cow;

use gpui::{Result, SharedString};
use gpui_component::IconNamed;

pub use gpui_component::{Icon, IconName};

/// Glyphs the Lucide set does not carry.
///
/// Deliberately tiny. Before adding one, check [`IconName`] — 99 icons cover most
/// needs, and a near-duplicate here is worse than an approximate match there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockIcon {
    /// `⠿` — the drag handle on a résumé section.
    Grip,
    /// The import drop zone's upward arrow into a tray.
    Download,
    /// The Applications board, in the nav rail.
    Kanban,
    /// Rename / edit-in-place. Lucide has it; upstream's curated `IconName`
    /// does not expose it, which is exactly what this set is for.
    Pen,
    /// **Reserved for the AI layer (M5), and deliberately unused until then.**
    ///
    /// Sparkles read as "a model produced this" across every tool this
    /// audience uses, so spending the glyph on an ordinary edit control would
    /// promise generation where there is none. When retrieval-backed
    /// proposals land — each one a word-level diff the user accepts (US-11) —
    /// this is the mark they get.
    PencilSparkles,
    // The contact rows in the editor's Profile card. Upstream's curated set
    // serves none of these — `every_lucide_glyph_used_is_actually_served`
    // proved it by failing — and a contact row without its glyph is four
    // identical text fields.
    Mail,
    Phone,
    MapPin,
    Link,
}

/// A Lucide glyph that ships in the upstream bundle but is missing from
/// upstream's curated [`IconName`].
///
/// Not a third icon set — the same assets, reached by name. It exists so app
/// code never carries a raw `"icons/….svg"` string: the path stays in this
/// crate, one place to fix if the bundle is ever replaced. If the glyph is
/// *not* in the bundle it silently renders nothing, so every call site here is
/// covered by `every_lucide_glyph_used_is_actually_served`.
pub fn lucide(stem: &'static str) -> Icon {
    Icon::default().path(format!("icons/{stem}.svg"))
}

/// The Lucide stems reached through [`lucide`]. Listed so the test can prove
/// each one resolves; a typo would otherwise show up as a blank control.
#[cfg_attr(not(test), allow(dead_code))]
const LUCIDE_BY_NAME: &[&str] = &["undo"];

macro_rules! dock_icons {
    ($($variant:ident => $stem:literal),* $(,)?) => {
        impl DockIcon {
            fn bytes(self) -> &'static [u8] {
                match self {
                    $(Self::$variant => {
                        include_bytes!(concat!("../../assets/icons/", $stem, ".svg"))
                    })*
                }
            }

            pub const ALL: &'static [DockIcon] = &[$(Self::$variant,)*];
        }

        impl IconNamed for DockIcon {
            fn path(self) -> SharedString {
                match self {
                    $(Self::$variant => concat!("icons/", $stem, ".svg").into(),)*
                }
            }
        }
    };
}

dock_icons! {
    Grip => "grip",
    Download => "download",
    Kanban => "kanban",
    Pen => "pen",
    PencilSparkles => "pencil-sparkles",
    Mail => "mail",
    Phone => "phone",
    MapPin => "map-pin",
    Link => "link",
}

/// The application's asset source: Lucide first, then the DockCV adds.
///
/// `gpui::Application::with_assets` takes exactly one source, so the two sets are
/// composed here rather than registered separately.
#[derive(Clone, Copy)]
pub struct Assets;

impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // Upstream reports a miss as `Err`, not `Ok(None)`, so a `?` here would
        // abort before ever reaching the DockCV set. A miss is not an error at
        // this layer — it just means "try the other set".
        if let Ok(Some(bytes)) = gpui_component_assets::Assets.load(path) {
            return Ok(Some(bytes));
        }
        Ok(DockIcon::ALL
            .iter()
            .find(|icon| icon.path() == path)
            .map(|icon| Cow::Borrowed(icon.bytes())))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_component_assets::Assets.list(path).unwrap_or_default();
        paths.extend(
            DockIcon::ALL
                .iter()
                .map(|icon| icon.path())
                .filter(|p| p.starts_with(path)),
        );
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AssetSource as _;

    #[test]
    fn the_dockcv_adds_are_real_svgs_that_follow_the_theme() {
        for icon in DockIcon::ALL {
            let text = std::str::from_utf8(icon.bytes()).expect("icons are utf-8");
            // Lucide's own files open with an ISC notice. It is kept — the
            // licence asks for it — so the markup starts after the comment.
            let markup = text
                .trim_start()
                .split_once("-->")
                .map_or(text.trim_start(), |(_, rest)| rest.trim_start());
            assert!(markup.starts_with("<svg"), "{icon:?} is not an svg");
            assert!(text.trim_end().ends_with("</svg>"), "{icon:?} is truncated");
            // Upstream's `Icon` tints through `text_color`; a hard-coded stroke
            // would silently ignore the palette.
            assert!(
                text.contains("currentColor"),
                "{icon:?} does not use currentColor and will not follow the theme"
            );
        }
    }

    #[test]
    fn the_source_serves_both_sets() {
        // Ours.
        for icon in DockIcon::ALL {
            assert!(
                Assets.load(&icon.path()).unwrap().is_some(),
                "{icon:?} is not served at {}",
                icon.path()
            );
        }
        // Upstream's — proves the delegation is wired, not just the fallback.
        assert!(Assets.load("icons/star.svg").unwrap().is_some());
        assert!(Assets.load("icons/does-not-exist.svg").unwrap().is_none());
    }

    /// A stem reached by name must actually exist in the bundle — otherwise the
    /// control renders an empty box and nothing else says so.
    #[test]
    fn every_lucide_glyph_used_is_actually_served() {
        for stem in LUCIDE_BY_NAME {
            let path = format!("icons/{stem}.svg");
            assert!(
                Assets.load(&path).unwrap().is_some(),
                "{path} is not in the Lucide bundle"
            );
        }
    }

    /// Every add must be one Lucide genuinely lacks. If upstream gains a glyph we
    /// duplicated, this fails and the duplicate should go.
    #[test]
    fn no_add_duplicates_an_upstream_icon() {
        let upstream = gpui_component_assets::Assets.list("icons/").unwrap();
        for icon in DockIcon::ALL {
            assert!(
                !upstream.contains(&icon.path()),
                "{icon:?} now exists upstream — drop the local copy"
            );
        }
    }
}

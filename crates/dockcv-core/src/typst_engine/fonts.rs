use std::sync::OnceLock;

use typst::foundations::Bytes;
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;

static GLOBAL_FONTS_AND_BOOK: OnceLock<(LazyHash<FontBook>, Vec<Font>)> = OnceLock::new();

pub(crate) fn global_fonts() -> &'static (LazyHash<FontBook>, Vec<Font>) {
    GLOBAL_FONTS_AND_BOOK.get_or_init(|| {
        let mut fonts = Vec::new();
        let mut load = |data: &'static [u8]| {
            let bytes = Bytes::new(data);
            let mut index = 0;
            while let Some(font) = Font::new(bytes.clone(), index) {
                fonts.push(font);
                index += 1;
            }
        };
        // Typst's fallback pack. Behind a feature because it is 9.2 MB and a
        // browser has no use for three maths faces — see this crate's
        // `Cargo.toml`. The document faces below always load.
        #[cfg(feature = "fallback-fonts")]
        for data in typst_assets::fonts() {
            load(data);
        }
        for data in DOCUMENT_FONTS {
            load(data);
        }
        #[cfg(feature = "libertinus")]
        for data in LIBERTINUS {
            load(data);
        }
        (LazyHash::new(FontBook::from_fonts(&fonts)), fonts)
    })
}

/// The browser's faces: the same families, subsetted.
///
/// Selected by **target**, not by a Cargo feature, and that is deliberate.
/// Features unify across a workspace build, so a feature that swapped the font
/// list would swap it for the desktop app too — which is exactly the bug
/// `every_offered_font_is_registered_and_changes_the_render` caught the last
/// time this was tried. A `cfg(target_arch)` cannot leak: nothing else in the
/// workspace compiles to wasm.
///
/// The trade is 3574 KB of faces down to 557 KB — the largest single thing a
/// visitor waits for, since fonts are already-compressed bytes that Brotli
/// cannot help with. What it costs is coverage beyond the repertoire in
/// `scripts/subset-fonts.sh`; `fonts_for_the_browser_cover_what_a_cv_can_contain`
/// is what keeps that honest.
#[cfg(target_arch = "wasm32")]
const DOCUMENT_FONTS: &[&[u8]] = &[
    include_bytes!("../../../../assets/fonts/web/Geist-Regular.subset.ttf"),
    include_bytes!("../../../../assets/fonts/web/Geist-Bold.subset.ttf"),
];

/// Libertinus Serif, for builds without `typst-assets`' font pack.
///
/// `DocumentFont` defaults to this family, so a build that dropped the pack
/// and did not put it back would render the default face as whatever Typst
/// fell back to — silently, which is the whole complaint in L-11. Three faces,
/// not six: regular, bold and italic are what a CV sets.
#[cfg(all(feature = "libertinus", not(target_arch = "wasm32")))]
const LIBERTINUS: &[&[u8]] = &[
    include_bytes!("../../../../assets/fonts/web/LibertinusSerif-Regular.otf"),
    include_bytes!("../../../../assets/fonts/web/LibertinusSerif-Bold.otf"),
    include_bytes!("../../../../assets/fonts/web/LibertinusSerif-Italic.otf"),
];

/// The same three, subsetted, for the browser. See `DOCUMENT_FONTS` above for
/// why this is chosen by target rather than by feature.
#[cfg(all(feature = "libertinus", target_arch = "wasm32"))]
const LIBERTINUS: &[&[u8]] = &[
    include_bytes!("../../../../assets/fonts/web/LibertinusSerif-Regular.subset.otf"),
    include_bytes!("../../../../assets/fonts/web/LibertinusSerif-Bold.subset.otf"),
    include_bytes!("../../../../assets/fonts/web/LibertinusSerif-Italic.subset.otf"),
];

/// Faces registered with the Typst compiler on top of `typst-assets`, so a
/// résumé can be set in a family the user recognises. Bundled, never fetched
/// (US-10) — the same bytes the UI already carries.
#[cfg(not(target_arch = "wasm32"))]
const DOCUMENT_FONTS: &[&[u8]] = &[
    include_bytes!("../../../../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../../../../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../../../../assets/fonts/Geist-Bold.ttf"),
    include_bytes!("../../../../assets/fonts/Newsreader.ttf"),
    include_bytes!("../../../../assets/fonts/Newsreader-Italic.ttf"),
    include_bytes!("../../../../assets/fonts/PTSerif-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/PTSerif-Bold.ttf"),
    include_bytes!("../../../../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/JetBrainsMono-Bold.ttf"),
];

/// The characters in this text that **no bundled face can set**.
///
/// Typst answers a missing glyph the way it answers a missing family —
/// silently, by falling back — so a CV in a script the bundle does not cover
/// comes out with holes in it and nothing on screen says so. A Japanese CV
/// compiled today embeds Libertinus and a maths face and not one glyph of
/// kanji; the page has gaps in it and the text layer disagrees with itself
/// between extractors. That is a thing to tell the author, not a thing to
/// discover from a rejection, and it is the same silent substitution the
/// decisions ledger records as L-11.
///
/// Whitespace is never reported: a space no face covers is still a space.
pub fn characters_no_bundled_face_can_set(text: &str) -> Vec<char> {
    let (_, fonts) = global_fonts();
    let mut missing: Vec<char> = Vec::new();
    for ch in text.chars() {
        if ch.is_whitespace() || ch.is_control() || missing.contains(&ch) {
            continue;
        }
        if !fonts.iter().any(|f| f.info().coverage.contains(ch as u32)) {
            missing.push(ch);
        }
    }
    missing
}

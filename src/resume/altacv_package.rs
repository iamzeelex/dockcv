//! The vendored AltaCV package (MIT), served from memory.
//!
//! DockCV makes no network request at runtime (US-10), so a Typst package it
//! renders through cannot be downloaded — it is copied into the repo and
//! `include_str!`d, exactly like the bundled fonts. Upstream, licence and the
//! rule for updating it are in `assets/typst/altacv/THIRD_PARTY.md`.
//!
//! Paths are the ones Typst resolves: the package sits at `altacv/`, so
//! `lib.typ`\'s `#import "internal/state.typ"` becomes
//! `altacv/internal/state.typ`, and a document reaches it with
//! `#import "altacv/lib.typ"`.

/// Package sources, keyed by the virtual path Typst asks for.
pub const SOURCES: &[(&str, &str)] = &[
    ("altacv/internal/dates.typ", include_str!("../../assets/typst/altacv/internal/dates.typ")),
    ("altacv/internal/defaults.typ", include_str!("../../assets/typst/altacv/internal/defaults.typ")),
    ("altacv/internal/footer.typ", include_str!("../../assets/typst/altacv/internal/footer.typ")),
    ("altacv/internal/header.typ", include_str!("../../assets/typst/altacv/internal/header.typ")),
    ("altacv/internal/icons.typ", include_str!("../../assets/typst/altacv/internal/icons.typ")),
    ("altacv/internal/json-resume.typ", include_str!("../../assets/typst/altacv/internal/json-resume.typ")),
    ("altacv/internal/layout.typ", include_str!("../../assets/typst/altacv/internal/layout.typ")),
    ("altacv/internal/presets.typ", include_str!("../../assets/typst/altacv/internal/presets.typ")),
    ("altacv/internal/primitives.typ", include_str!("../../assets/typst/altacv/internal/primitives.typ")),
    ("altacv/internal/qr.typ", include_str!("../../assets/typst/altacv/internal/qr.typ")),
    ("altacv/internal/ratings.typ", include_str!("../../assets/typst/altacv/internal/ratings.typ")),
    ("altacv/internal/state.typ", include_str!("../../assets/typst/altacv/internal/state.typ")),
    ("altacv/internal/text.typ", include_str!("../../assets/typst/altacv/internal/text.typ")),
    ("altacv/internal/validation.typ", include_str!("../../assets/typst/altacv/internal/validation.typ")),
    ("altacv/lib.typ", include_str!("../../assets/typst/altacv/lib.typ")),
    ("altacv/sections/awards.typ", include_str!("../../assets/typst/altacv/sections/awards.typ")),
    ("altacv/sections/certificates.typ", include_str!("../../assets/typst/altacv/sections/certificates.typ")),
    ("altacv/sections/education.typ", include_str!("../../assets/typst/altacv/sections/education.typ")),
    ("altacv/sections/experience.typ", include_str!("../../assets/typst/altacv/sections/experience.typ")),
    ("altacv/sections/focus-areas.typ", include_str!("../../assets/typst/altacv/sections/focus-areas.typ")),
    ("altacv/sections/languages.typ", include_str!("../../assets/typst/altacv/sections/languages.typ")),
    ("altacv/sections/projects.typ", include_str!("../../assets/typst/altacv/sections/projects.typ")),
    ("altacv/sections/publications.typ", include_str!("../../assets/typst/altacv/sections/publications.typ")),
    ("altacv/sections/references.typ", include_str!("../../assets/typst/altacv/sections/references.typ")),
    ("altacv/sections/skills.typ", include_str!("../../assets/typst/altacv/sections/skills.typ")),
];

/// Everything the package `read`s rather than imports: the avatar placeholder
/// and the label table it takes its section headings from.
///
/// The first sweep collected `.typ` and `.svg` and missed the TOML — a
/// package's data files are as load-bearing as its code, and the omission
/// showed up only as a compile error, not as a missing import.
pub const BYTES: &[(&str, &[u8])] = &[
    ("altacv/assets/avatar-placeholder.svg", include_bytes!("../../assets/typst/altacv/assets/avatar-placeholder.svg")),
    ("altacv/internal/labels-en.toml", include_bytes!("../../assets/typst/altacv/internal/labels-en.toml")),
    ("altacv/assets/icons/book.svg", include_bytes!("../../assets/typst/altacv/assets/icons/book.svg")),
    ("altacv/assets/icons/calendar.svg", include_bytes!("../../assets/typst/altacv/assets/icons/calendar.svg")),
    ("altacv/assets/icons/file-text.svg", include_bytes!("../../assets/typst/altacv/assets/icons/file-text.svg")),
    ("altacv/assets/icons/github.svg", include_bytes!("../../assets/typst/altacv/assets/icons/github.svg")),
    ("altacv/assets/icons/globe.svg", include_bytes!("../../assets/typst/altacv/assets/icons/globe.svg")),
    ("altacv/assets/icons/link.svg", include_bytes!("../../assets/typst/altacv/assets/icons/link.svg")),
    ("altacv/assets/icons/mail.svg", include_bytes!("../../assets/typst/altacv/assets/icons/mail.svg")),
    ("altacv/assets/icons/map-pin.svg", include_bytes!("../../assets/typst/altacv/assets/icons/map-pin.svg")),
    ("altacv/assets/icons/mic.svg", include_bytes!("../../assets/typst/altacv/assets/icons/mic.svg")),
    ("altacv/assets/icons/newspaper.svg", include_bytes!("../../assets/typst/altacv/assets/icons/newspaper.svg")),
    ("altacv/assets/icons/phone.svg", include_bytes!("../../assets/typst/altacv/assets/icons/phone.svg")),
]; 

/// Look up a vendored source by the path Typst asked for.
pub fn source(path: &str) -> Option<&'static str> {
    SOURCES
        .iter()
        .find(|(p, _)| *p == path || p.strip_prefix("altacv/").unwrap_or(p) == path)
        .map(|(_, s)| *s)
}

/// Look up a vendored binary asset by path.
pub fn bytes(path: &str) -> Option<&'static [u8]> {
    BYTES
        .iter()
        .find(|(p, _)| *p == path || p.ends_with(&format!("/{path}")))
        .map(|(_, b)| *b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `#import "…"` inside the package must resolve to a file we
    /// vendored. A missing one is a compile error at render time, in a code
    /// path a user reaches and no test otherwise walks.
    #[test]
    fn every_internal_import_resolves_to_a_vendored_file() {
        for (path, text) in SOURCES {
            let dir = path.rsplit_once('/').map_or("altacv", |(d, _)| d);
            for line in text.lines() {
                let Some(rest) = line.trim().strip_prefix("#import \"") else {
                    continue;
                };
                let Some(target) = rest.split('"').next() else {
                    continue;
                };
                if target.starts_with('@') {
                    continue; // an external package — reported separately
                }
                let mut resolved = format!("{dir}/{target}");
                while let Some(at) = resolved.find("/../") {
                    let head = &resolved[..at];
                    let head = head.rsplit_once('/').map_or("", |(h, _)| h);
                    resolved = format!("{head}{}", &resolved[at + 3..]);
                }
                assert!(
                    source(&resolved).is_some(),
                    "{path} imports {target}, which is not vendored (looked for {resolved})"
                );
            }
        }
    }

    /// What the package needs that is *not* in the repo. Named so the list is a
    /// decision, not a surprise at compile time.
    #[test]
    fn external_package_dependencies_are_known() {
        let mut external: Vec<&str> = SOURCES
            .iter()
            .flat_map(|(_, text)| text.lines())
            .filter_map(|l| l.trim().strip_prefix("#import \"@"))
            .filter_map(|r| r.split('"').next())
            .collect();
        external.sort_unstable();
        external.dedup();
        assert_eq!(
            external.as_slice(),
            // `gairm-import` (JSON Resume parsing) and `zebra` (QR codes)
            // were forked out — both served features DockCV does not offer and
            // both were imported at the top level, so they loaded whether or
            // not the feature was reached. See THIRD_PARTY.md.
            // All three are gone: `gairm-import` and `zebra` served features
            // DockCV does not offer, and FontAwesome — which ships no fonts of
            // its own — was replaced by the Lucide set the app already carries.
            [] as [&str; 0],
            "the package\'s external dependencies changed"
        );
    }
}

//! Typst source export emitter for a [`ResumeDoc`] or [`Resume`].
//!
//! Produces standalone `.typ` source that compiles with the Typst CLI to the
//! identical typeset layout produced by DockCV.

use super::model::{DocumentLanguage, LayoutSettings, Resume, ResumeDoc};
use super::template;

/// Export a [`ResumeDoc`] to a complete, standalone Typst source string.
///
/// Uses the document's own layout settings (page size, margins, font, text scale,
/// leading, section layout overrides).
pub fn export_typst(doc: &ResumeDoc) -> String {
    template::generate_for(doc)
}

/// Export a composed [`Resume`] with explicit [`LayoutSettings`] to standalone Typst source.
///
/// English, because a composed [`Resume`] has no language of its own. A caller
/// that has the document — every caller in the app does — should use
/// [`export_typst_in`] instead, or the `.typ` it writes will compile to a
/// different PDF from the one the preview showed it.
pub fn export_typst_with_layout(resume: &Resume, layout: &LayoutSettings) -> String {
    template::generate_with_layout(resume, layout)
}

/// The same, in the reading's own language.
///
/// The Typst source export is the one format whose output is *the same
/// artifact* as the preview — somebody compiles it and expects our PDF. So it
/// is the one that cannot afford to drop an axis: without the language it
/// writes no `/Lang` into the catalog, which is a PDF/UA-1 conformance the
/// preview has and the export does not, and it prints `Present` on a CV that
/// says `Heute` on screen.
pub fn export_typst_in(
    resume: &Resume,
    layout: &LayoutSettings,
    language: DocumentLanguage,
) -> String {
    template::generate_with_layout_and_language(resume, layout, language)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `.typ` we write and the PDF we preview have to be the same document.
    /// They were not: the export dropped the reading's language, so the file
    /// compiled without a `/Lang` in its catalog — the PDF/UA-1 conformance
    /// B4 exists for — and printed `Present` under a CV that says `Heute`.
    #[test]
    fn the_typst_export_is_the_document_the_preview_showed() {
        use crate::resume::model::{DocumentLanguage, Resume, ResumeDoc};

        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        doc.set_language(DocumentLanguage::German);

        let exported = export_typst_in(&doc.compose(), &doc.layout, doc.language());
        let previewed = crate::resume::template::generate_for(&doc);
        assert_eq!(
            exported, previewed,
            "the file somebody compiles is not the file we showed them"
        );
        assert!(exported.contains("lang: \"de\""));

        // The composed-only entry point cannot know a language and says so by
        // being English rather than by guessing.
        let english = export_typst_with_layout(&doc.compose(), &doc.layout);
        assert!(english.contains("lang: \"en\""));
    }
    use crate::resume::export_walk::sample_resume;
    use crate::typst_engine::TypstEngine;

    /// The `.typ` we hand a Typst user has to be the whole document.
    ///
    /// "Standalone" is the entire promise of this export: it is worth more to
    /// that audience than any template we ship, and it is worthless the moment
    /// the file needs something from a vault they do not have. Compiling it
    /// inside our own engine would not notice, so the source is also read for
    /// the things that would make it depend on this machine.
    #[test]
    fn exported_typst_is_standalone_and_lays_out_the_same_page_count() {
        let doc = ResumeDoc::from_resume(sample_resume(), "Base");
        let source = export_typst(&doc);

        assert!(source.contains("#set page"));
        assert!(source.contains("#render-cv"));

        // Nothing that reaches outside the file itself.
        for reach in [
            "#import \"",
            "#include \"",
            "read(",
            "json(",
            "csv(",
            "image(",
        ] {
            assert!(
                !source.contains(reach),
                "the exported source reaches outside itself with {reach:?}, \
                 so it will not compile on a machine that never had this vault"
            );
        }

        // The file the user takes away lays out as the page they were shown.
        let exported = TypstEngine::new(source);
        let (_, exported_geometry) = exported
            .compile_to_pixels(1.0)
            .expect("the exported source must compile");
        let previewed = TypstEngine::new(crate::resume::template::generate_for(&doc));
        let (_, preview_geometry) = previewed
            .compile_to_pixels(1.0)
            .expect("the preview source must compile");

        assert!(exported_geometry.page_count >= 1);
        assert_eq!(
            exported_geometry.page_count, preview_geometry.page_count,
            "the .typ a user takes away paginates differently from the page they saw"
        );
    }
}

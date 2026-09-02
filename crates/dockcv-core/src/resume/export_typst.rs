//! Typst source export emitter for a [`ResumeDoc`] or [`Resume`].
//!
//! Produces standalone `.typ` source that compiles with the Typst CLI to the
//! identical typeset layout produced by DockCV.

use super::model::{LayoutSettings, Resume, ResumeDoc};
use super::template;

/// Export a [`ResumeDoc`] to a complete, standalone Typst source string.
///
/// Uses the document's own layout settings (page size, margins, font, text scale,
/// leading, section layout overrides).
pub fn export_typst(doc: &ResumeDoc) -> String {
    template::generate_for(doc)
}

/// Export a composed [`Resume`] with explicit [`LayoutSettings`] to standalone Typst source.
pub fn export_typst_with_layout(resume: &Resume, layout: &LayoutSettings) -> String {
    template::generate_with_layout(resume, layout)
}

#[cfg(test)]
mod tests {
    use super::*;
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

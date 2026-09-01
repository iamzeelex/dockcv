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
    use crate::typst_engine::TypstEngine;

    #[test]
    fn export_typst_produces_compilable_source() {
        let doc = ResumeDoc::default();
        let source = export_typst(&doc);

        assert!(source.contains("#set page"));
        assert!(source.contains("#render-cv"));

        let engine = TypstEngine::new(source);
        let result = engine.compile_to_pdf();
        assert!(result.is_ok(), "Typst export must compile cleanly: {:?}", result.err());
        let pdf_bytes = result.unwrap();
        assert!(!pdf_bytes.is_empty());
    }
}

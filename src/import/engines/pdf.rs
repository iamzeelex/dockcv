//! PDF import: text extraction, pure Rust.
//!
//! This deliberately does **not** use `pdfium-render`. That crate binds to a
//! `libpdfium` shared library at runtime — `bind_to_system_library()`, falling
//! back to a path relative to the *working directory* — and DockCV ships neither.
//! The result was that importing a PDF, the first thing a new user does (US-01),
//! failed on any machine without libpdfium installed, and the fallback path broke
//! the moment the app was launched from Finder rather than `cargo run`.
//!
//! A local-first app that embeds its own fonts and compiles Typst in-process
//! should not need a system library to read a file. `pdf-extract` is pure Rust,
//! so the binary stays self-contained.
//!
//! The cost is the first-page thumbnail, which pdfium rendered and this does not.
//! The design's `FIRST-RUN IMPORT` row never draws one — it shows the filename and
//! the list of sections found — so nothing in the mockup is lost.

use std::path::Path;

use crate::import::classifier::classify_raw_text;
use crate::import::model::ImportedDoc;

pub fn import_pdf(path: &Path) -> Result<ImportedDoc, String> {
    let text = extract_text(path)?;
    Ok(classify_raw_text("PDF", &text))
}

/// Pull the document's text out in reading order.
fn extract_text(path: &Path) -> Result<String, String> {
    pdf_extract::extract_text(path).map_err(|e| format!("Could not read this PDF: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip through our own compiler: render the bundled sample to PDF,
    /// then read it back. This proves extraction works on a real document rather
    /// than on a fixture someone has to keep in the repo — and it uses the exact
    /// PDF shape DockCV itself produces.
    #[test]
    fn text_comes_back_out_of_a_pdf_we_generated() {
        use crate::resume::{altacv, template};
        use crate::typst_engine::TypstEngine;

        let resume = altacv::import(altacv::ALTACV_SAMPLE).expect("the sample parses");
        let source = template::generate(&resume);
        let engine = TypstEngine::new(source);
        let bytes = engine.compile_to_pdf().expect("the sample compiles to PDF");

        let dir = std::env::temp_dir().join("dockcv-pdf-extract-test");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("sample.pdf");
        std::fs::write(&file, bytes).expect("write");

        let text = extract_text(&file).expect("extraction succeeds");
        let name = &resume.basics.name;
        assert!(
            text.contains(name),
            "the person's name ({name}) should survive a PDF round-trip; got {} chars",
            text.len()
        );

        let _ = std::fs::remove_file(&file);
    }
}

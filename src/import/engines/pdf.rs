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

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use crate::import::classifier::classify_raw_text;
use crate::import::model::ImportedDoc;

pub fn import_pdf(path: &Path) -> Result<ImportedDoc, String> {
    let text = extract_text(path)?;
    Ok(classify_raw_text("PDF", &text))
}

/// Pull the document's text out in reading order.
///
/// `pdf-extract` answers a PDF construct it does not handle by **panicking**,
/// not by returning `Err` — `panic!("unexpected encoding {:?}")` on a CJK
/// `/Encoding` is one of about a hundred such sites, and a structurally valid
/// PDF is enough to reach them. That panic is not contained by being on a
/// background thread: `async-task` catches it on the worker and resumes the
/// unwind in the awaiting task, which for the import flow is a foreground task
/// on the UI thread. So an ordinary CV exported by an unusual tool took the
/// whole app down on US-01, the first thing a new user does.
///
/// `catch_unwind` turns that back into this function's own error type. The
/// closure borrows nothing that could be left inconsistent — the crate is
/// handed a path and returns an owned `String` — so `AssertUnwindSafe` is
/// carrying a fact here, not a hope.
fn extract_text(path: &Path) -> Result<String, String> {
    match catch_unwind(AssertUnwindSafe(|| pdf_extract::extract_text(path))) {
        Ok(result) => result.map_err(|e| format!("Could not read this PDF: {e}")),
        Err(_) => Err("This PDF uses a construct the reader can't handle. \
             Try exporting it again as PDF from the original app, or import \
             the DOCX instead."
            .to_string()),
    }
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

    /// Build a one-page PDF whose only unusual property is the `/Encoding`
    /// named on its font. Everything else — xref, catalog, page tree, content
    /// stream — is valid, which is the point: this is not corrupt input, it is
    /// ordinary input carrying a construct `pdf-extract` refuses.
    fn one_page_pdf(encoding: &str) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => Object::Name(encoding.as_bytes().to_vec()),
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 24 Tf 72 720 Td (Sofiia Medvedenko) Tj ET".to_vec(),
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("write the fixture");
        bytes
    }

    /// The fixture is only worth anything if the *sane* half of it reads back,
    /// so this pins both ends: `WinAnsiEncoding` extracts, and the CJK
    /// encoding beside it is rejected rather than extracted differently.
    #[test]
    fn a_font_encoding_the_reader_cannot_handle_is_an_error_not_a_crash() {
        let dir = std::env::temp_dir().join(format!("dockcv-pdf-panic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");

        let sane = dir.join("sane.pdf");
        std::fs::write(&sane, one_page_pdf("WinAnsiEncoding")).expect("write");
        assert!(
            extract_text(&sane)
                .expect("an ordinary encoding still extracts")
                .contains("Sofiia"),
            "the fixture itself must be a readable PDF, or this test proves nothing"
        );

        // `UniJIS-UCS2-H` is a real CMap that real Japanese exporters emit.
        // `pdf-extract` answers it with `panic!("unexpected encoding …")`.
        let odd = dir.join("cjk.pdf");
        std::fs::write(&odd, one_page_pdf("UniJIS-UCS2-H")).expect("write");
        let error = extract_text(&odd).expect_err("this must not extract");
        assert!(
            error.contains("can't handle"),
            "a caught panic must reach the user as import guidance, got: {error}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

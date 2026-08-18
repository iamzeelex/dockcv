//! DockCV's engine, callable from JavaScript.
//!
//! One job: take what a person has — a DockCV document, a JSON Résumé, or an
//! AltaCV Typst file — and give back the same typeset pages the desktop app
//! would produce, as SVG.
//!
//! ## Same source, same document
//!
//! The pages come from `template::generate_for`, which is the *same* function
//! the app calls before handing Typst its source. That is what makes "this is
//! the engine from the app" true rather than a marketing line: the web and the
//! desktop differ in how a laid-out page is turned into something you can
//! look at (SVG here, a pixmap there), and in nothing before that.
//!
//! The one honest asterisk: the browser build ships five faces instead of
//! twenty-seven, so a document naming a family it does not carry will fall
//! back. The default serif is present precisely so the common case does not.

use wasm_bindgen::prelude::*;

use dockcv_core::resume::model::{Resume, ResumeDoc};
use dockcv_core::resume::{altacv, template};
use dockcv_core::typst_engine::TypstEngine;

/// Read whatever the visitor pasted.
///
/// Tried in order of how specific the format is, so a guess never wins over a
/// certainty: a DockCV document is TOML with our own shape, a JSON Résumé is
/// the published schema, and AltaCV is Typst source. Each parser rejects the
/// other formats outright, so the order only decides which error the visitor
/// sees when nothing matches.
fn parse(input: &str) -> Result<ResumeDoc, String> {
    if let Ok(doc) = toml::from_str::<ResumeDoc>(input) {
        return Ok(doc);
    }
    if let Ok(resume) = toml::from_str::<Resume>(input) {
        return Ok(ResumeDoc::from_resume(resume, "Base"));
    }
    if let Ok(resume) = serde_json::from_str::<Resume>(input) {
        return Ok(ResumeDoc::from_resume(resume, "Base"));
    }
    if let Some(resume) = altacv::import(input) {
        return Ok(ResumeDoc::from_resume(resume, "Base"));
    }
    Err("Not a DockCV document, a JSON Résumé, or an AltaCV file.".into())
}

/// Typeset a résumé. Returns one SVG string per page.
///
/// Errors are the compiler's own, already turned into sentences by
/// `dockcv-core` — the visitor gets "unexpected `]`" rather than a span dump,
/// the same wording the app's error banner shows.
#[wasm_bindgen]
pub fn render(input: &str) -> Result<Vec<JsValue>, JsValue> {
    let doc = parse(input).map_err(|e| JsValue::from_str(&e))?;
    let engine = TypstEngine::new(template::generate_for(&doc));
    let pages = engine
        .compile_to_svg()
        .map_err(|e| JsValue::from_str(&e))?;
    Ok(pages.into_iter().map(|p| JsValue::from_str(&p)).collect())
}

/// A document to show before the visitor has pasted anything.
///
/// The AltaCV starter — a résumé for a person who does not exist, already in
/// the tree as the fixture the parser tests are built on. A demo needs
/// *something* on the page at first paint, and inventing a second sample so
/// the first could stay test-only would be two things to keep current.
#[wasm_bindgen]
pub fn sample() -> String {
    dockcv_core::resume::altacv::ALTACV_SAMPLE.to_string()
}

/// The version this module was built from, so a page can show what it is
/// running and a stale cache is visible rather than mysterious.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

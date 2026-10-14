//! Resume domain: the data model (source of truth), the Typst codegen that
//! renders it, and importers that recognize external templates into the model.
//!
//! Four families here own a directory, because each was already a family with
//! a prefix standing in for one: [`model`] is the state, [`template`] turns it
//! into Typst, [`export`] turns it into every other format, and [`altacv`]
//! reads the one foreign template we recognize. What stays at this level is
//! vocabulary the whole crate shares — dates, links, outcomes — plus the three
//! modules the editor addresses directly.

pub mod altacv;
pub mod ats;
pub mod dates;
pub mod diagnostics;
mod document_toml;
pub mod edit;
pub mod export;
pub mod language;
pub mod links;
pub mod locales;
pub mod model;
pub mod outcomes;
pub mod posting;
pub mod presets;
pub mod taxonomy;
pub mod template;

pub use document_toml::parse_document_toml;
pub use export::*;
pub use model::ExportRecord;
pub use model::{
    builtin_profile, LayoutProfile, ProfileCatalog, ATS_SAFE_PROFILE, DEFAULT_PROFILE,
};

/// Read whatever resume format was provided.
///
/// Tried in order of how specific the format is: DockCV TOML, raw TOML Resume,
/// JSON Résumé, and AltaCV Typst source.
pub fn parse_any(input: &str) -> Result<model::ResumeDoc, String> {
    if let Ok(doc) = parse_document_toml(input) {
        return Ok(doc);
    }
    if let Ok(resume) = toml::from_str::<model::Resume>(input) {
        return Ok(model::ResumeDoc::from_resume(resume, "Base"));
    }
    if let Ok(resume) = serde_json::from_str::<model::Resume>(input) {
        return Ok(model::ResumeDoc::from_resume(resume, "Base"));
    }
    if let Some(resume) = altacv::import(input) {
        return Ok(model::ResumeDoc::from_resume(resume, "Base"));
    }
    Err("Not a DockCV document, a JSON Résumé, or an AltaCV file.".into())
}

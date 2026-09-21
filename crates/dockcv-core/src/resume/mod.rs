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
pub mod language;
pub mod export;
pub mod links;
pub mod model;
pub mod outcomes;
pub mod presets;
pub mod template;

pub use document_toml::parse_document_toml;
pub use export::*;
pub use model::ExportRecord;
pub use model::{
    builtin_profile, LayoutProfile, ProfileCatalog, ATS_SAFE_PROFILE, DEFAULT_PROFILE,
};

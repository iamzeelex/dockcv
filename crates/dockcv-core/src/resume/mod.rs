//! Resume domain: the data model (source of truth), the Typst codegen that
//! renders it, and importers that recognize external templates into the model.

pub mod altacv;
pub mod altacv_package;
mod applications;
pub mod ats;
pub mod dates;
pub mod diagnostics;
mod document_toml;
mod document_variants;
pub mod edit;
#[cfg(feature = "docx")]
pub mod export_docx;
pub mod export_json_resume;
pub mod export_markdown;
pub mod export_names;
mod export_settings;
pub mod export_text;
pub mod export_typst;
pub mod export_walk;
pub mod export_wrap;
mod language;
mod layout;
mod layout_sections;
pub mod links;
pub mod model;
pub mod outcomes;
pub mod presets;
mod profiles;
pub mod template;
mod versioning;

pub use document_toml::parse_document_toml;
#[cfg(feature = "docx")]
pub use export_docx::{export_docx, export_docx_with_date_format};
pub use export_json_resume::{export_json_resume, export_json_resume_with_meta, ResumeMeta};
pub use export_markdown::{export_markdown, export_markdown_with_date_format};
pub use export_names::{disambiguate_filename, plan_batch, OnCollision, PlannedExport};
pub use export_text::{export_plain_text, export_plain_text_with_date_format};
pub use export_typst::{export_typst, export_typst_with_layout};
pub use model::ExportRecord;
pub use profiles::{
    builtin as builtin_profile, LayoutProfile, ProfileCatalog, ATS_SAFE_PROFILE, DEFAULT_PROFILE,
};

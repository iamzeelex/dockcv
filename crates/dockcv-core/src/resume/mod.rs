//! Resume domain: the data model (source of truth), the Typst codegen that
//! renders it, and importers that recognize external templates into the model.

pub mod altacv;
pub mod altacv_package;
pub mod dates;
pub mod diagnostics;
pub mod edit;
#[cfg(feature = "docx")]
pub mod export_docx;
pub mod export_json_resume;
pub mod export_markdown;
pub mod export_text;
pub mod export_typst;
pub mod model;
pub mod template;

#[cfg(feature = "docx")]
pub use export_docx::{export_docx, export_docx_with_date_format};
pub use export_json_resume::export_json_resume;
pub use export_markdown::{export_markdown, export_markdown_with_date_format};
pub use export_text::{export_plain_text, export_plain_text_with_date_format};
pub use export_typst::{export_typst, export_typst_with_layout};

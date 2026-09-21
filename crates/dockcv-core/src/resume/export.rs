//! Every format a CV leaves in that is not Typst's own PDF.
//!
//! One traversal ([`walk`]) is shared by all of them, so a field added to the
//! model reaches DOCX, Markdown, plain text and JSON Resume together or not at
//! all — the alternative is four emitters that quietly disagree about what a
//! CV contains. [`names`] answers the other half: what the file is called once
//! it exists, and what happens when that name is taken.

#[cfg(feature = "docx")]
pub mod docx;
pub mod json_resume;
pub mod markdown;
pub mod names;
pub mod text;
pub mod typst;
pub mod walk;
pub mod wrap;

#[cfg(feature = "docx")]
pub use docx::{export_docx, export_docx_in};
pub use json_resume::{export_json_resume, export_json_resume_with_meta, ResumeMeta};
pub use markdown::{export_markdown, export_markdown_in};
pub use names::{disambiguate_filename, plan_batch, OnCollision, PlannedExport};
pub use text::{export_plain_text, export_plain_text_in};
pub use typst::{export_typst, export_typst_in, export_typst_with_layout};

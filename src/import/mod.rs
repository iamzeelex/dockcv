//! Document importer subsystem.
//!
//! Provides a local-first, multi-engine pipeline for importing existing CV documents
//! from **PDF** (via `pdf-extract`, pure Rust), **DOCX**, **JSON Resume**,
//! **Plain Text/Markdown**, and a **LinkedIn data export** archive.

pub mod classifier;
pub mod layout;
pub mod model;

pub mod engines {
    pub mod docx;
    pub mod linkedin;
    pub mod pdf;
    pub mod structured;
    pub mod text;
}

use model::ImportedDoc;
use std::path::Path;

const MAX_IMPORT_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB

/// Main entry point for importing any supported resume file.
pub fn import_file(path: &Path) -> Result<ImportedDoc, String> {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > MAX_IMPORT_FILE_SIZE {
            return Err(format!(
                "File exceeds maximum allowed import size of {} MB",
                MAX_IMPORT_FILE_SIZE / (1024 * 1024)
            ));
        }
    }

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "pdf" => engines::pdf::import_pdf(path),
        "zip" => engines::linkedin::import_linkedin(path),
        "docx" => engines::docx::import_docx(path),
        "json" | "typ" => engines::structured::import_structured(path)
            .or_else(|_| engines::text::import_text(path)),
        "txt" | "md" | "markdown" => engines::text::import_text(path),
        _ => engines::structured::import_structured(path)
            .or_else(|_| engines::text::import_text(path))
            .map_err(|_| format!("Unsupported file extension '.{ext}'")),
    }
}

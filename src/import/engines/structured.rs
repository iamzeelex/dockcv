//! Structured Document Import Engine (JSON Resume & Typst AltaCV).

use std::fs;
use std::path::Path;

use crate::import::model::ImportedDoc;
use crate::resume::altacv;
use crate::resume::model::{Resume, ResumeDoc};

pub fn import_structured(path: &Path) -> Result<ImportedDoc, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Could not read file: {e}"))?;

    // Attempt Typst AltaCV parse first
    if let Some(resume) = altacv::import(&content) {
        let doc = ResumeDoc::from_resume(resume, "Base");
        return Ok(ImportedDoc::new("Typst (AltaCV)", doc));
    }

    // Attempt JSON Resume parse
    if let Ok(resume) = serde_json::from_str::<Resume>(&content) {
        let doc = ResumeDoc::from_resume(resume, "Base");
        return Ok(ImportedDoc::new("JSON Resume", doc));
    }

    Err("File does not match JSON Resume or Typst AltaCV schema".to_string())
}

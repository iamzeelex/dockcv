//! Plain Text / Markdown Document Import Engine.

use std::fs;
use std::path::Path;

use crate::import::classifier::classify_raw_text;
use crate::import::model::ImportedDoc;

pub fn import_text(path: &Path) -> Result<ImportedDoc, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Could not read text file: {e}"))?;
    let imported = classify_raw_text("Plain Text / Markdown", &content);
    Ok(imported)
}

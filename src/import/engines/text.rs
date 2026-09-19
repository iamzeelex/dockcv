//! Plain Text / Markdown Document Import Engine.

use std::fs;
use std::path::Path;

use crate::import::classifier::classify_raw_text;
use crate::import::model::ImportedDoc;

pub fn import_text(path: &Path) -> Result<ImportedDoc, String> {
    let bytes = fs::read(path).map_err(|e| format!("Could not read text file: {e}"))?;
    // UTF-16 and a UTF-8 byte-order mark both arrive from Notepad — see
    // `import::decode_text`.
    let content = crate::import::decode_text(bytes)
        .map_err(|why| format!("Could not read text file: {why}"))?;
    let imported = classify_raw_text("Plain Text / Markdown", &content);
    Ok(imported)
}

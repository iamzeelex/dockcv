//! Structured Document Import Engine (JSON Resume & Typst AltaCV).

use std::fs;
use std::path::Path;

use crate::import::engines::json_resume;
use crate::import::error::ImportError;
use crate::import::model::ImportedDoc;
use crate::resume::altacv;
use crate::resume::model::{Resume, ResumeDoc};

pub fn import_structured(path: &Path) -> Result<ImportedDoc, ImportError> {
    let content = fs::read_to_string(path).map_err(|e| {
        ImportError::new("Could not read this file")
            .detail(format!("the system said: {e}"))
            .remedy("Check the file is still where you picked it from")
    })?;

    // Typst AltaCV first: it is the one shape that is unambiguous on sight.
    if let Some(resume) = altacv::import(&content) {
        let doc = ResumeDoc::from_resume(resume, "Base");
        let mut imported = ImportedDoc::new("Typst (AltaCV)", doc);
        imported.observe();
        return Ok(imported);
    }

    // JSON Resume, read against **its** schema. Deserializing the spec straight
    // into our `Resume` is what used to fail on `basics.location`, and the
    // failure was then swallowed into a prose import.
    if let Some(imported) = json_resume::import(&content) {
        return Ok(imported);
    }

    // A document DockCV itself exported, which *is* our shape. Last, because a
    // JSON Resume must never reach it.
    if let Ok(resume) = serde_json::from_str::<Resume>(&content) {
        if !resume.basics.name.trim().is_empty() || !resume.work.is_empty() {
            let doc = ResumeDoc::from_resume(resume, "Base");
            let mut imported = ImportedDoc::new("DockCV JSON", doc);
            imported.observe();
            return Ok(imported);
        }
    }

    Err(ImportError::new("This file is not a CV DockCV can read")
        .detail(
            "It parsed, but it does not carry a name, any jobs or any education — so it is \
                 some other document that happens to be JSON.",
        )
        .remedy("If it is a JSON Resume, check it has a `basics.name` or a `work` list")
        .remedy("If it came out of another CV tool, export it as PDF or DOCX instead"))
}

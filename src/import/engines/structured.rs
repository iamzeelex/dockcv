//! Structured Document Import Engine (JSON Resume & Typst AltaCV).

use std::fs;
use std::path::Path;

use crate::import::engines::json_resume;
use crate::import::error::ImportError;
use crate::import::model::ImportedDoc;
use crate::resume::altacv;
use crate::resume::model::{Resume, ResumeDoc};

pub fn import_structured(path: &Path) -> Result<ImportedDoc, ImportError> {
    let bytes = fs::read(path).map_err(|e| {
        ImportError::new("Could not read this file")
            .detail(format!("the system said: {e}"))
            .remedy("Check the file is still where you picked it from")
    })?;
    let content = crate::import::decode_text(bytes).map_err(|why| {
        ImportError::new("Could not read this file")
            .detail(why)
            .remedy("Open it in the editor it came from and save it as UTF-8 text")
    })?;

    // An empty file is not a document that disappoints us, and saying "it
    // parsed, but it does not carry a name" about nothing at all is the kind of
    // message that makes a person doubt the file rather than the picker.
    if content.trim().is_empty() {
        return Err(ImportError::new("This file is empty")
            .detail("There is nothing in it to read — not a CV, not anything else.")
            .remedy("Check you picked the file you meant, and that it finished downloading"));
    }

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

    // "It parsed, but…" has to be true when we say it. A file cut off halfway
    // through — a download that stopped, half a clipboard — does not parse at
    // all, and telling its owner the document is the wrong shape sends them
    // looking in the wrong place.
    if let Err(why) = serde_json::from_str::<serde_json::Value>(&content) {
        return Err(ImportError::new("This file is not readable JSON")
            .detail(format!(
                "It stops making sense at line {}, column {} — which usually means it was \
                 cut off rather than that it is the wrong kind of document.",
                why.line(),
                why.column()
            ))
            .remedy("If it came from a download or an export, fetch it again")
            .remedy("If you meant a PDF, a Word file or a plain-text CV, pick that instead"));
    }

    Err(ImportError::new("This file is not a CV DockCV can read")
        .detail(
            "It parsed, but it does not carry a name, any jobs or any education — so it is \
                 some other document that happens to be JSON.",
        )
        .remedy("If it is a JSON Resume, check it has a `basics.name` or a `work` list")
        .remedy("If it came out of another CV tool, export it as PDF or DOCX instead"))
}

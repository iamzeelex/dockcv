//! Document importer subsystem.
//!
//! Provides a local-first, multi-engine pipeline for importing existing CV documents
//! from **PDF** (via `pdf-extract`, pure Rust), **DOCX**, **JSON Resume**,
//! **Plain Text/Markdown**, and a **LinkedIn data export** archive.

pub mod classifier;
pub mod error;
pub mod layout;
pub mod model;
pub mod notes;

pub mod engines {
    pub mod docx;
    pub mod json_resume;
    pub mod linkedin;
    pub mod pdf;
    pub mod structured;
    pub mod text;
}

pub use error::ImportError;
use model::ImportedDoc;
use std::path::Path;

const MAX_IMPORT_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB

/// Main entry point for importing any supported resume file.
pub fn import_file(path: &Path) -> Result<ImportedDoc, ImportError> {
    // A guard that skips itself when `metadata` fails is not a guard. A file we
    // cannot stat is one we cannot size, and that is a refusal, not a pass.
    let meta = std::fs::metadata(path).map_err(|e| {
        ImportError::new("Could not open this file")
            .detail(format!("the system said: {e}"))
            .remedy("Check the file is still where you picked it from")
    })?;
    if meta.len() > MAX_IMPORT_FILE_SIZE {
        return Err(ImportError::new(format!(
            "This file is larger than {} MB",
            MAX_IMPORT_FILE_SIZE / (1024 * 1024)
        ))
        .detail("A CV is normally a few hundred kilobytes, so this is probably not one.")
        .remedy("Check you picked the right file"));
    }

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    let outcome = match ext.as_str() {
        "pdf" => engines::pdf::import_pdf(path),
        "zip" => engines::linkedin::import_linkedin(path).map_err(ImportError::from),
        "docx" => engines::docx::import_docx(path).map_err(ImportError::from),
        // **No prose fallback.** A file the user told us is JSON has to fail as
        // JSON. Falling through to the text classifier turned a legible error
        // into a plausible-looking wrong answer: a canonical JSON Resume came
        // out as a CV whose name was `{`, with the whole work history gone and
        // `import_file` returning `Ok`.
        "json" | "typ" => engines::structured::import_structured(path),
        "txt" | "md" | "markdown" => engines::text::import_text(path).map_err(ImportError::from),
        // No extension to go on, so trying both is the only option — and here
        // the fallback is honest, because the user never said what it was.
        _ => engines::structured::import_structured(path)
            .or_else(|_| engines::text::import_text(path).map_err(ImportError::from))
            .map_err(|_| {
                ImportError::new(format!("DockCV does not read '.{ext}' files"))
                    .detail("Nothing is wrong with the file — this build has no engine for it.")
                    .remedy("Export it as PDF, DOCX, Markdown or JSON Resume")
            }),
    };

    // Import is where a bug report is most likely to start, and where the
    // recipient least wants to send their CV to prove it. So: the shape of
    // what came out, and not one line of what was in it.
    match &outcome {
        Ok(imported) => log::info!(
            "imported .{ext} as {}: {} sections, {} jobs, {} unparsed lines",
            imported.format_name,
            imported.doc.sections().len(),
            imported.doc.work.active().len(),
            imported.unparsed.len(),
        ),
        Err(error) => log::error!("import of .{ext} failed: {error}"),
    }
    outcome
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    /// The regression I-01, at the dispatcher.
    ///
    /// `.json` used to fall through to the prose classifier when the structured
    /// engine refused it, which is how a canonical JSON Resume became a CV
    /// named `{`. A file the user told us is JSON has to fail as JSON.
    #[test]
    fn json_that_is_not_a_resume_fails_rather_than_importing_as_prose() {
        let dir = std::env::temp_dir().join(format!("dockcv-import-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("package.json");
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(br#"{"name":"dockcv","version":"0.1.0","scripts":{"build":"cargo build"}}"#)
            .expect("write");
        drop(file);

        let Err(error) = super::import_file(&path) else {
            panic!("a package.json must not import as somebody's CV");
        };
        assert!(
            !error.headline.is_empty() && !error.remedies.is_empty(),
            "a refusal has to say what to do next: {error:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// And the format that *is* one comes in whole, through the same door.
    #[test]
    fn a_json_resume_reaches_the_document_through_import_file() {
        let dir = std::env::temp_dir().join(format!("dockcv-import-jr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("resume.json");
        std::fs::write(
            &path,
            br#"{"basics":{"name":"Sofiia Medvedenko","location":{"city":"Berlin"}},
                 "work":[{"name":"Acme","position":"Staff Engineer","startDate":"2021-01"}]}"#,
        )
        .expect("write");

        let imported = super::import_file(&path).expect("a JSON Resume imports");
        assert_eq!(imported.format_name, "JSON Resume");
        assert_eq!(imported.doc.profile.active().name, "Sofiia Medvedenko");
        assert_eq!(imported.doc.profile.active().location, "Berlin");
        assert_eq!(imported.doc.work.active().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

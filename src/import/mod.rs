//! Document importer subsystem.
//!
//! Provides a local-first, multi-engine pipeline for importing existing CV documents
//! from **PDF** (via `pdf-extract`, pure Rust), **DOCX**, **JSON Resume**,
//! **Plain Text/Markdown**, and a **LinkedIn data export** archive.

pub mod classifier;
pub mod error;
#[cfg(test)]
pub mod foreign_cvs;
pub mod layout;
pub mod model;
pub mod notes;
#[cfg(test)]
mod roundtrip_tests;

pub mod engines {
    pub mod docx;
    pub mod json_resume;
    pub mod linkedin;
    pub mod markdown;
    pub mod pdf;
    pub mod structured;
    pub mod text;
}

pub use error::ImportError;
use model::ImportedDoc;
use std::path::Path;

const MAX_IMPORT_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB

/// Decode a file's bytes as text, whichever of the three encodings a CV
/// arrives in.
///
/// `read_to_string` takes UTF-8 and nothing else, which refused two files that
/// are ordinary outside this project: Notepad's "Unicode" is UTF-16 with a
/// byte-order mark, and its "UTF-8" writes a BOM too — which does not fail, and
/// is worse, because the mark becomes the first character of the person's name
/// and travels into the vault invisibly. Both are recognised here by their
/// mark, which is what a mark is for.
pub(crate) fn decode_text(bytes: Vec<u8>) -> Result<String, String> {
    match bytes.as_slice() {
        [0xff, 0xfe, rest @ ..] => decode_utf16(rest, u16::from_le_bytes),
        [0xfe, 0xff, rest @ ..] => decode_utf16(rest, u16::from_be_bytes),
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8(rest.to_vec())
            .map_err(|e| format!("stream did not contain valid UTF-8: {e}")),
        _ => {
            String::from_utf8(bytes).map_err(|e| format!("stream did not contain valid UTF-8: {e}"))
        }
    }
}

fn decode_utf16(bytes: &[u8], word: fn([u8; 2]) -> u16) -> Result<String, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("this looks like UTF-16 and stops in the middle of a character".into());
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| word([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&units).map_err(|e| format!("this is not valid UTF-16: {e}"))
}

/// Read a text file, or say what stopped us in the same voice as every other
/// import refusal.
fn read_text(path: &Path) -> Result<String, ImportError> {
    let bytes = std::fs::read(path).map_err(|e| {
        ImportError::new("Could not read this file")
            .detail(format!("the system said: {e}"))
            .remedy("Check the file is still where you picked it from")
    })?;
    decode_text(bytes).map_err(|why| {
        ImportError::new("Could not read this file")
            .detail(why)
            .remedy("Open it in the editor it came from and save it as UTF-8 text")
    })
}

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
        // Markdown states its own structure and is read as markup; plain text
        // has none and is recovered from typography. Handing `.md` to the prose
        // path threw the structure away and then guessed it back wrongly — a
        // CV DockCV had exported itself came back with `##` in its section
        // names and its whole work history in one custom section.
        "md" | "markdown" => Ok(engines::markdown::import_markdown(
            "Markdown",
            &read_text(path)?,
        )),
        "txt" => engines::text::import_text(path).map_err(ImportError::from),
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
            "imported .{ext} as {}: {} sections, {} jobs, {} unplaced items",
            imported.format_name,
            imported.doc.sections().len(),
            imported.doc.work.active().len(),
            imported.unplaced.len(),
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
            br#"{"basics":{"name":"Albert Einstein","location":{"city":"Berlin"}},
                 "work":[{"name":"Acme","position":"Staff Engineer","startDate":"2021-01"}]}"#,
        )
        .expect("write");

        let imported = super::import_file(&path).expect("a JSON Resume imports");
        assert_eq!(imported.format_name, "JSON Resume");
        assert_eq!(imported.doc.profile.active().name, "Albert Einstein");
        assert_eq!(imported.doc.profile.active().location, "Berlin");
        assert_eq!(imported.doc.work.active().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

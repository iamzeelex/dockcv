//! PDF import: text extraction, pure Rust.
//!
//! This deliberately does **not** use `pdfium-render`. That crate binds to a
//! `libpdfium` shared library at runtime — `bind_to_system_library()`, falling
//! back to a path relative to the *working directory* — and DockCV ships neither.
//! The result was that importing a PDF, the first thing a new user does (US-01),
//! failed on any machine without libpdfium installed, and the fallback path broke
//! the moment the app was launched from Finder rather than `cargo run`.
//!
//! A local-first app that embeds its own fonts and compiles Typst in-process
//! should not need a system library to read a file. `pdf-extract` is pure Rust,
//! so the binary stays self-contained.
//!
//! The cost is the first-page thumbnail, which pdfium rendered and this does not.
//! The design's `FIRST-RUN IMPORT` row never draws one — it shows the filename and
//! the list of sections found — so nothing in the mockup is lost.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use crate::import::classifier::classify_raw_text;
use crate::import::error::ImportError;
use crate::import::model::ImportedDoc;
use dockcv_core::resume::model::SkillGroup;

pub fn import_pdf(path: &Path) -> Result<ImportedDoc, ImportError> {
    let bytes = std::fs::read(path).map_err(|e| {
        ImportError::new("Could not open this PDF").detail(format!("the system said: {e}"))
    })?;
    let text = extract_text(path).map_err(ImportError::from)?;

    // A scanned or photographed CV *has* a text layer — it is simply empty.
    // Without this the classifier built a default document, `import_pdf`
    // returned `Ok`, and the wizard showed a review screen with nothing on it
    // and a green button underneath. That is the most common PDF that will not
    // import, and it was indistinguishable from success.
    if text.trim().is_empty() {
        return Err(ImportError::new("There is no text in this PDF to read")
            // Short on purpose. Why a file will not open is the least
            // interesting thing on this screen — what to do about it is
            // underneath, and every line spent here is a line between the
            // person and the answer.
            .detail("Nothing is wrong with it — it is a picture of a document, not text.")
            // No OCR advice. It used to say "run the scan through OCR first,
            // then import the result", which asks somebody who has never heard
            // the word to go and find a tool for it — while the panel below
            // now does exactly that job in one click. A remedy that sends a
            // person away from the answer we are already offering is worse
            // than no remedy.
            .remedy("Have an assistant read it — just below")
            .remedy("Import the original .docx, if you still have it")
            .remedy("Export to PDF again from the app you wrote it in, keeping the text")
            .no_text_layer());
    }

    // A PDF records where glyphs landed, never the order they were typed in, so
    // a Hebrew or Arabic CV arrives spelled backwards. This is the only engine
    // that needs the repair — a .docx, a text file and a JSON Resume all store
    // logical order already — and it costs a Latin CV one scan for a
    // right-to-left letter that is never there.
    // When the file says what its lines are, believe it. A tagged PDF carries
    // the answer the flat reading has to infer — which line is a heading, where
    // a paragraph ends, which column a line sits in — and inferring it from
    // glyph positions is what makes a two-column CV read as two columns
    // interleaved. Everything else in this function is the fallback for a file
    // that says nothing.
    // A tagged PDF has already said which lines are headings, and one of them
    // is usually the person. LinkedIn's export — the most common CV file there
    // is — opens with a sidebar of Contact, Top Skills and Languages, so the
    // name is forty lines into the text layer and no rule about "the first line
    // of the document" can reach it. In the tag tree it is an `H1` like the
    // others, and the one that is not the name of a section.
    let text = crate::import::bidi::text_to_logical_order(&text);
    let mut imported = classify_raw_text("PDF", &text);

    if imported.doc.profile.active().name.trim().is_empty() {
        if let Some(name) = name_from_headings(&crate::import::pdf_tags::headings(&bytes)) {
            imported.doc.profile.active_mut().name = name;
        }
    }

    read_the_sidebar(&mut imported, &bytes);

    Ok(imported)
}

/// Pull the document's text out in reading order.
///
/// `pdf-extract` answers a PDF construct it does not handle by **panicking**,
/// not by returning `Err` — `panic!("unexpected encoding {:?}")` on a CJK
/// `/Encoding` is one of about a hundred such sites, and a structurally valid
/// PDF is enough to reach them. That panic is not contained by being on a
/// background thread: `async-task` catches it on the worker and resumes the
/// unwind in the awaiting task, which for the import flow is a foreground task
/// on the UI thread. So an ordinary CV exported by an unusual tool took the
/// whole app down on US-01, the first thing a new user does.
///
/// `catch_unwind` turns that back into this function's own error type. The
/// closure borrows nothing that could be left inconsistent — the crate is
/// handed a path and returns an owned `String` — so `AssertUnwindSafe` is
/// carrying a fact here, not a hope.
fn extract_text(path: &Path) -> Result<String, String> {
    match catch_unwind(AssertUnwindSafe(|| read_pages(path))) {
        Ok(result) => result.map_err(|e| format!("Could not read this PDF: {e}")),
        Err(_) => Err("This PDF uses a construct the reader can't handle. \
             Try exporting it again as PDF from the original app, or import \
             the DOCX instead."
            .to_string()),
    }
}

/// The importer's own reading, for the conformance harness.
///
/// The harness used to model a glyph-sorting reader with
/// `pdf_extract::extract_text_from_mem`, which is the crate's demo sink and
/// not what anything reads a CV with — including us, since [`Lines`] exists.
/// This is the reader a DockCV file actually meets when somebody re-imports
/// it, so it is the one worth a column.
#[cfg(test)]
pub fn read_as_the_importer_does(path: &Path) -> Result<String, String> {
    extract_text(path)
}

fn read_pages(path: &Path) -> Result<String, pdf_extract::OutputError> {
    let mut doc = pdf_extract::Document::load(path)?;
    if doc.is_encrypted() {
        // An empty password is what an owner-password-only document opens
        // with, which is most of the "protected" CVs people are sent.
        doc.decrypt("")
            .map_err(pdf_extract::OutputError::PdfError)?;
    }
    let mut lines = Lines::default();
    // One page at a time, because `output_doc` reads them all through a single
    // reader that caches fonts by their **resource name** — `/F1`, `/F2` — and
    // a resource name is page-local. Typst numbers page two's fonts from one
    // again, so page two's character codes were decoded through page one's
    // font: every page after the first came back as plausible-looking nonsense
    // (`the` as `tag`), and a CV that ran to two pages lost everything below
    // the break. `output_doc_page` builds its own reader per call, which is
    // all the isolation this needs.
    for page in doc.get_pages().keys() {
        pdf_extract::output_doc_page(&doc, &mut lines, *page)?;
    }
    Ok(lines.text)
}

/// The text of a page, with its lines taken from the **baseline**.
///
/// `pdf_extract::PlainTextOutput` decides where a line ends mostly from
/// horizontal motion: it breaks when the pen moves back to the left of where
/// the last glyph ended. That needs the glyph widths, and it reads zero for the
/// subset CID fonts Typst writes — so "where the last glyph ended" collapses to
/// where it *began*, and a heading set flush left never breaks from the
/// paragraph beneath it, which starts at the same x. Every DockCV CV with
/// left-aligned headings came out of its own PDF as
/// `ProfileEngineer turned engineering lead`, with the section boundary gone
/// and the document behind it: one section, no work history.
///
/// A baseline cannot be got wrong the same way. Two glyphs on the same baseline
/// are on the same line whatever the widths say, and a new baseline is a new
/// line. Horizontal position is still read, but only for the one thing it is
/// needed for — telling a gap the producer left instead of a space character
/// from the ordinary advance between two letters.
#[derive(Default)]
struct Lines {
    text: String,
    /// Flips PDF's upward y so that "down the page" is increasing.
    flip: pdf_extract::Transform,
    /// Baseline and pen position of the glyph before this one.
    last_y: f64,
    last_x: f64,
    /// The size the last glyph was drawn at.
    last_size: f64,
    /// Where the last glyph could have ended, when its width was legible.
    last_end: f64,
    started: bool,
}

impl Lines {
    /// A baseline this far from the last one is a new line, measured in the
    /// current font's size. Line leading is at least one whole size; a
    /// superscript rises by about a third of one. Half separates them.
    const NEW_LINE: f64 = 0.5;
    /// And a pen this far past the end of the last glyph skipped something —
    /// a producer that positions its words instead of writing spaces.
    const GAP: f64 = 0.25;
    /// A size this much larger or smaller than the run before it, on the same
    /// baseline, is a different field rather than a continuation of one.
    const NEW_FIELD: f64 = 0.2;
}

impl pdf_extract::OutputDev for Lines {
    fn begin_page(
        &mut self,
        _page: u32,
        media_box: &pdf_extract::MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), pdf_extract::OutputError> {
        self.flip =
            pdf_extract::Transform::row_major(1., 0., 0., -1., 0., media_box.ury - media_box.lly);
        // A page starts a line, and never continues the one the page before it
        // ended on.
        if self.started {
            self.text.push('\n');
        }
        self.started = false;
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &pdf_extract::Transform,
        width: f64,
        _spacing: f64,
        font_size: f64,
        glyph: &str,
    ) -> Result<(), pdf_extract::OutputError> {
        let position = trm.post_transform(&self.flip);
        let (x, y) = (position.m31, position.m32);
        // The size the glyph is actually drawn at, text matrix included.
        let scale = (trm.m11 * trm.m22 - trm.m12 * trm.m21).abs().sqrt();
        let size = if scale > 0.0 {
            scale * font_size
        } else {
            font_size
        };

        if self.started {
            if (y - self.last_y).abs() > size * Self::NEW_LINE {
                self.text.push('\n');
            } else if (size - self.last_size).abs() > self.last_size * Self::NEW_FIELD
                && !self.text.ends_with("  ")
            {
                // A header sets the name large and the title beside it small,
                // on one line — and a text layer has no way to say "these are
                // two things" except how they are set. Two spaces is the run
                // the classifier already reads as a field boundary; one space
                // would have left `Nora Vestergaard Platform Engineer` as
                // somebody's name.
                self.text.push_str("  ");
            } else if x > self.last_end + size * Self::GAP
                && !glyph.starts_with(char::is_whitespace)
                && !self.text.ends_with(char::is_whitespace)
            {
                // Only where there is not a space there already: a document
                // whose widths read as zero reports a gap between every pair of
                // words, and doubling the space it already wrote turned
                // `Copenhagen, Denmark` into `Copenhagen,  Denmark`.
                self.text.push(' ');
            }
        }

        self.text.push_str(glyph);
        self.started = true;
        self.last_y = y;
        self.last_x = x;
        self.last_size = size;
        // Zero is not a width, it is a width we could not read; fall back to
        // the pen position so the gap test compares like with like.
        self.last_end = if width > 0.0 { x + width * size } else { x };
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }
}

/// What the tree knows and the page cannot say: which column a line is in.
///
/// A sidebar is where a CV keeps the things a reader wants most and a parser
/// finds hardest — the contact details, and a bare list of skills with no
/// separators in it. Read off the page they are lines among other lines: the
/// skills in LinkedIn's own export came back as nothing at all, and the
/// telephone number with them. In the tree they are the blocks under a heading
/// inside one cell, which is a fact the file states.
///
/// Only the empty fields are filled. The flat reading is better than this at
/// everything it does do — it joins a line to the one below it, which is what
/// turns two lines into a job — and a tree that overruled it cost a real export
/// every employer it had.
fn read_the_sidebar(imported: &mut ImportedDoc, bytes: &[u8]) {
    use crate::import::classifier::SectionKind;
    use crate::import::classifier_headings::classify_header;

    for section in crate::import::pdf_tags::sections(bytes) {
        if section.items.is_empty() {
            continue;
        }
        match classify_header(&section.heading) {
            SectionKind::Skills if imported.doc.skills.active().is_empty() => {
                let keywords: Vec<String> = section
                    .items
                    .iter()
                    .filter(|item| item.chars().count() <= 60)
                    .cloned()
                    .collect();
                if !keywords.is_empty() {
                    imported.doc.skills.active_mut().push(SkillGroup {
                        name: String::new(),
                        keywords,
                    });
                }
            }
            SectionKind::Contact => {
                let block = section.items.join("\n");
                let profile = imported.doc.profile.active_mut();
                if profile.phone.is_empty() {
                    if let Some(found) = crate::import::classifier_contact::first_phone(&block) {
                        profile.phone = found;
                    }
                }
            }
            _ => {}
        }
    }
}

/// The heading that names a person rather than a section./// The heading that names a person rather than a section.
///
/// Strict on purpose: a heading the taxonomy does not know could be somebody's
/// invented section (`Leadership & Activities`), and filing that as the
/// author's name would be worse than having no name at all. Two to four words,
/// each of them capitalised, no digits, and short.
fn name_from_headings(headings: &[String]) -> Option<String> {
    headings.iter().find_map(|heading| {
        let text = heading.trim();
        let words: Vec<&str> = text.split_whitespace().collect();
        let plausible = (2..=4).contains(&words.len())
            && text.chars().count() <= 48
            && !text.chars().any(|c| c.is_ascii_digit())
            && !text.contains(['@', ':', '/', '&', ','])
            && words
                .iter()
                .all(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
            // Both questions, because they are different ones. `names_a_section`
            // asks whether the *whole line* is a section's name; `classify_header`
            // asks which section a heading belongs to and is happy with a
            // substring — which is how `Top Skills` was a heading nothing called
            // a section, and became somebody's name.
            && !crate::import::classifier_headings::names_a_section(&text.to_lowercase())
            && crate::import::classifier_headings::classify_header(text)
                == crate::import::classifier::SectionKind::Unknown;
        plausible.then(|| text.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip through our own compiler: render the bundled sample to PDF,
    /// then read it back. This proves extraction works on a real document rather
    /// than on a fixture someone has to keep in the repo — and it uses the exact
    /// PDF shape DockCV itself produces.
    #[test]
    fn text_comes_back_out_of_a_pdf_we_generated() {
        use crate::resume::{altacv, template};
        use crate::typst_engine::TypstEngine;

        let resume = altacv::import(altacv::ALTACV_SAMPLE).expect("the sample parses");
        let source = template::generate(&resume);
        let engine = TypstEngine::new(source);
        let bytes = engine.compile_to_pdf().expect("the sample compiles to PDF");

        let dir = std::env::temp_dir().join("dockcv-pdf-extract-test");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("sample.pdf");
        std::fs::write(&file, bytes).expect("write");

        let text = extract_text(&file).expect("extraction succeeds");
        let name = &resume.basics.name;
        assert!(
            text.contains(name),
            "the person's name ({name}) should survive a PDF round-trip; got {} chars",
            text.len()
        );

        let _ = std::fs::remove_file(&file);
    }

    /// A heading set flush left is still a line of its own.
    ///
    /// `pdf_extract`'s own reader ends a line when the pen moves back to the
    /// left of where the last glyph ended, and it reads the glyph widths of a
    /// Typst subset font as zero — so "where the last glyph ended" becomes
    /// "where it began", and a heading at the same x as the paragraph under it
    /// never breaks from it. Every DockCV CV with left-aligned headings came
    /// back as `PROFILEEngineer turned engineering lead`, with the section
    /// boundaries gone and the whole document in one section behind them.
    ///
    /// `rule-to-margin` is the style that is *always* flush left, whatever the
    /// alignment says, so it is the one asserted here.
    #[test]
    fn a_left_aligned_heading_is_a_line_of_its_own() {
        use crate::resume::model::{
            Basics, HeadingLayout, HeadingStyle, LayoutSettings, Resume, ResumeDate, Work,
        };
        use crate::resume::template;
        use crate::typst_engine::TypstEngine;

        let resume = Resume {
            basics: Basics {
                name: "Albert Einstein".into(),
                summary: "Physicist, latterly of Princeton, with a long-standing interest in \
                          the photoelectric effect and in gravitation."
                    .into(),
                ..Default::default()
            },
            work: vec![Work {
                name: "Patent Office".into(),
                position: "Examiner".into(),
                start_date: ResumeDate::new("1902-06"),
                end_date: ResumeDate::new("1909-10"),
                highlights: vec!["Reviewed applications for electromechanical devices.".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let layout = LayoutSettings {
            headings: HeadingLayout {
                style: HeadingStyle::RuleToMargin,
                ..Default::default()
            },
            ..Default::default()
        };
        let source = template::generate_with_layout(&resume, &layout);
        let bytes = TypstEngine::new(source)
            .compile_to_pdf()
            .expect("the document compiles");

        let dir = std::env::temp_dir().join(format!("dockcv-pdf-heading-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("left.pdf");
        std::fs::write(&file, bytes).expect("write");
        let text = extract_text(&file).expect("extraction succeeds");
        let _ = std::fs::remove_dir_all(&dir);

        for heading in ["PROFILE", "WORK EXPERIENCE"] {
            assert!(
                text.lines().any(|l| l.trim() == heading),
                "{heading:?} is not on a line of its own; got:\n{text}"
            );
        }
        assert!(
            !text.contains("PROFILEPhysicist"),
            "the heading was fused to the paragraph under it:\n{text}"
        );
    }

    /// The header, read back off the page it was printed on.
    ///
    /// A header set with commas — `Copenhagen, Denmark, you@example.com, …` —
    /// is where every field of it went wrong at once. The city was lost,
    /// because a place has a comma inside it and splitting on commas left
    /// `Copenhagen` and `Denmark` as two parts, neither of which looks like
    /// one. The name swallowed the title beside it, because a text layer sets
    /// them on one line and says nothing about their being two things. And the
    /// person's own site was listed twice — once in the Website field and once
    /// again among the profiles — because the first address in the block took
    /// the field, and the first address is a GitHub.
    #[test]
    fn a_comma_separated_header_comes_back_whole() {
        use crate::resume::model::{
            Basics, HeaderLayout, LayoutSettings, NetworkProfile, Resume, SkillSeparator,
        };
        use crate::resume::template;
        use crate::typst_engine::TypstEngine;

        let resume = Resume {
            basics: Basics {
                name: "Albert Einstein".into(),
                label: "Principal Systems Architect".into(),
                email: "albert@example.com".into(),
                phone: "+45 28 44 10 92".into(),
                location: "Copenhagen, Denmark".into(),
                url: "einstein.example.com".into(),
                profiles: vec![NetworkProfile {
                    network: "GitHub".into(),
                    username: "aeinstein".into(),
                    url: "github.com/aeinstein".into(),
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let layout = LayoutSettings {
            header: HeaderLayout {
                separator: SkillSeparator::Comma,
                ..Default::default()
            },
            ..Default::default()
        };

        let bytes = TypstEngine::new(template::generate_with_layout(&resume, &layout))
            .compile_to_pdf()
            .expect("the document compiles");
        let dir = std::env::temp_dir().join(format!("dockcv-pdf-header-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("header.pdf");
        std::fs::write(&file, bytes).expect("write");
        let imported = import_pdf(&file).expect("the PDF imports");
        let _ = std::fs::remove_dir_all(&dir);

        let b = imported.doc.profile.active();
        assert_eq!(b.name, "Albert Einstein", "the title was read as the name");
        assert_eq!(b.label, "Principal Systems Architect");
        assert_eq!(b.email, "albert@example.com");
        assert_eq!(b.location, "Copenhagen, Denmark");
        assert_eq!(
            b.url, "einstein.example.com",
            "a profile took the site field"
        );
        assert!(
            b.profiles.iter().any(|p| p.url == "github.com/aeinstein"),
            "the GitHub profile was lost: {:?}",
            b.profiles
        );
        assert!(
            !b.profiles.iter().any(|p| p.url == b.url),
            "the site is listed twice, once as itself and once as a profile: {:?}",
            b.profiles
        );
    }

    /// Page two reads as page two, not as page one's alphabet.
    ///
    /// `pdf_extract::output_doc` walks every page through one reader whose font
    /// cache is keyed by the **resource name** — `/F1`, `/F2` — and a resource
    /// name means nothing outside the page that declares it. Typst numbers each
    /// page's fonts from one, so page two's character codes were decoded
    /// through page one's font: not gibberish anyone would notice a parser
    /// choking on, but plausible words — `the` came out as `tag` — and every
    /// section below the page break was quietly lost. A CV that runs to two
    /// pages is an ordinary CV.
    #[test]
    fn a_second_page_is_decoded_with_its_own_fonts() {
        use crate::resume::model::{Basics, Resume, ResumeDate, Work};
        use crate::resume::template;
        use crate::typst_engine::TypstEngine;

        // Long enough to break the page, and every employer a single token so
        // a wrap cannot be mistaken for a mis-decode.
        let work: Vec<Work> = (1..=34)
            .map(|n| Work {
                name: format!("Employer{n:02}"),
                position: "Engineer".into(),
                start_date: ResumeDate::new("2010-01"),
                end_date: ResumeDate::new("2011-01"),
                highlights: vec![
                    "Built the thing, ran the thing, and wrote down what the thing cost."
                        .to_string(),
                    "Then did it again somewhere else, with a smaller budget and more people."
                        .to_string(),
                ],
                ..Default::default()
            })
            .collect();
        let resume = Resume {
            basics: Basics {
                name: "Albert Einstein".into(),
                ..Default::default()
            },
            work,
            ..Default::default()
        };

        let bytes = TypstEngine::new(template::generate(&resume))
            .compile_to_pdf()
            .expect("the document compiles");
        let dir = std::env::temp_dir().join(format!("dockcv-pdf-pages-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("long.pdf");
        std::fs::write(&file, bytes).expect("write");

        let pages = pdf_extract::Document::load(&file)
            .expect("the PDF loads")
            .get_pages()
            .len();
        let text = extract_text(&file).expect("extraction succeeds");
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            pages >= 2,
            "the fixture has to break the page to test anything; it made {pages}"
        );
        for n in 1..=34 {
            let employer = format!("Employer{n:02}");
            assert!(
                text.contains(&employer),
                "{employer} is not in the text layer, so a page was read with \
                 the wrong font; got:\n{text}"
            );
        }
    }

    /// The regression I-02: a scanned CV has a text layer and it is empty.
    /// Before this guard `import_pdf` returned `Ok`, the classifier built a
    /// default document, and the wizard showed a review screen with nothing on
    /// it and a green button underneath.
    #[test]
    fn a_pdf_with_no_text_says_so_and_says_it_is_not_the_users_fault() {
        let dir = std::env::temp_dir().join(format!("dockcv-pdf-blank-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let blank = dir.join("scan.pdf");
        // Same fixture, no text operators — a page that draws nothing, which is
        // what a flattened scan's text layer amounts to.
        std::fs::write(&blank, one_page_pdf_with_content(b"")).expect("write");

        let Err(error) = import_pdf(&blank) else {
            panic!("a textless PDF must not import as an empty CV");
        };
        assert!(
            error.headline.to_lowercase().contains("no text"),
            "the headline has to name the cause, got: {}",
            error.headline
        );
        assert!(
            error.detail.to_lowercase().contains("nothing is wrong"),
            "the user must be told the file is not at fault, got: {}",
            error.detail
        );
        // The flag, not the wording, is what puts the assistant hand-off on
        // screen (`views/import_assistant.rs`). Asserted here because the
        // route out of this dead end disappears silently if it is ever lost,
        // and nothing else in the build would notice.
        assert!(
            error.is_unreadable_image(),
            "a picture of a document has to be marked as one"
        );
        assert!(
            error.remedies.len() >= 2,
            "a dead end needs somewhere to go, got {:?}",
            error.remedies
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// I-15. A two-column CV — a sidebar beside the body — is a very common
    /// shape, and nothing in the importer detects columns: `layout.rs` derives
    /// **one** measure from the document's line lengths.
    ///
    /// This test does not assert that columns work. It pins what actually
    /// happens, which is what the finding asked for: the audit recorded the case
    /// as *unknown*, and an unknown in the first minute of the product is worth
    /// converting into a known even when the answer is "it depends on the
    /// exporter".
    ///
    /// What it establishes: reading order follows the **content stream**, not
    /// the geometry. A typesetter that writes one column and then the other
    /// gives back one column and then the other, which reads correctly. One that
    /// interleaves by visual row gives back interleaved lines, and nothing here
    /// puts them back. The fixture writes the worst case — row-interleaved — so
    /// the failure mode is recorded rather than assumed.
    #[test]
    fn a_two_column_page_is_read_in_content_stream_order() {
        let dir = std::env::temp_dir().join(format!("dockcv-pdf-cols-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");

        // Sidebar at x=60, body at x=300, written row by row as a naive
        // exporter would.
        let mut content = String::new();
        for (row, (left, right)) in [
            ("Albert Einstein", "EXPERIENCE"),
            ("s@example.com", "Staff Engineer, Acme"),
            ("Berlin", "Cut p99 latency in half"),
        ]
        .into_iter()
        .enumerate()
        {
            let y = 720 - (row as i32 * 24);
            content.push_str(&format!("BT /F1 12 Tf 60 {y} Td ({left}) Tj ET\n"));
            content.push_str(&format!("BT /F1 12 Tf 300 {y} Td ({right}) Tj ET\n"));
        }

        let file = dir.join("two-column.pdf");
        std::fs::write(&file, one_page_pdf_with_content(content.as_bytes())).expect("write");
        let text = extract_text(&file).expect("extraction succeeds");

        // Every piece survives — nothing is dropped by the column layout.
        for fragment in [
            "Albert Einstein",
            "s@example.com",
            "EXPERIENCE",
            "Staff Engineer, Acme",
            "Cut p99 latency in half",
        ] {
            assert!(text.contains(fragment), "lost {fragment:?} from:\n{text}");
        }

        // …but the sidebar and the body are interleaved, because that is the
        // order they were written in. A person's email lands between two lines
        // of their work history. This is the limitation, stated:
        let name_at = text.find("Albert Einstein").expect("name");
        let heading_at = text.find("EXPERIENCE").expect("heading");
        let email_at = text.find("s@example.com").expect("email");
        assert!(
            name_at < heading_at && heading_at < email_at,
            "reading order is the content stream's, not the page's:\n{text}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Build a one-page PDF whose only unusual property is the `/Encoding`
    /// named on its font. Everything else — xref, catalog, page tree, content
    /// stream — is valid, which is the point: this is not corrupt input, it is
    /// ordinary input carrying a construct `pdf-extract` refuses.
    fn one_page_pdf(encoding: &str) -> Vec<u8> {
        one_page_pdf_inner(encoding, b"BT /F1 24 Tf 72 720 Td (Albert Einstein) Tj ET")
    }

    /// The same page with a content stream of the caller's choosing.
    fn one_page_pdf_with_content(content: &[u8]) -> Vec<u8> {
        one_page_pdf_inner("WinAnsiEncoding", content)
    }

    fn one_page_pdf_inner(encoding: &str, content: &[u8]) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => Object::Name(encoding.as_bytes().to_vec()),
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("write the fixture");
        bytes
    }

    /// The fixture is only worth anything if the *sane* half of it reads back,
    /// so this pins both ends: `WinAnsiEncoding` extracts, and the CJK
    /// encoding beside it is rejected rather than extracted differently.
    #[test]
    fn a_font_encoding_the_reader_cannot_handle_is_an_error_not_a_crash() {
        let dir = std::env::temp_dir().join(format!("dockcv-pdf-panic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");

        let sane = dir.join("sane.pdf");
        std::fs::write(&sane, one_page_pdf("WinAnsiEncoding")).expect("write");
        assert!(
            extract_text(&sane)
                .expect("an ordinary encoding still extracts")
                .contains("Albert"),
            "the fixture itself must be a readable PDF, or this test proves nothing"
        );

        // `UniJIS-UCS2-H` is a real CMap that real Japanese exporters emit.
        // `pdf-extract` answers it with `panic!("unexpected encoding …")`.
        let odd = dir.join("cjk.pdf");
        std::fs::write(&odd, one_page_pdf("UniJIS-UCS2-H")).expect("write");
        let error = extract_text(&odd).expect_err("this must not extract");
        assert!(
            error.contains("can't handle"),
            "a caught panic must reach the user as import guidance, got: {error}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

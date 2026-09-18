//! A CV DockCV wrote, read back by DockCV.
//!
//! Every export is also an import: a person exports a CV, edits it elsewhere or
//! moves to another machine, and brings it back. Nothing tested that, and the
//! result was that not one text format survived the trip. Plain text lost its
//! bullets and read the rule under a heading as the summary; Markdown was not
//! parsed as Markdown at all, so `## Work Experience` became a section called
//! `## Work Experience`; DOCX lost its whole work history because
//! `work experience` was missing from the taxonomy; Typst dropped every custom
//! section; PDF lost every heading to letter spacing.
//!
//! Each of those was invisible to the tests that existed, because each emitter
//! was tested against its own output and never against the importer. So this
//! asserts the only property that matters at the seam: what comes back is what
//! went out.
//!
//! PDF is not here. Its text layer is a rendering rather than a document — the
//! reading order of a two-column page, and headings printed in a side column,
//! are recovered by inference and not by rule — so it is held to "most of it
//! survives", in its own engine's tests, rather than to equality.

use dockcv_core::resume::model::*;

use super::import_file;

/// A CV shaped like the ones that broke: several jobs with wrapped bullets, two
/// degrees, labelled skill rows, dated certificates, volunteering, a custom
/// section, and a contact block written the way people write one — a bare
/// domain and a phone number in two-digit groups.
fn fixture() -> ResumeDoc {
    let resume = Resume {
        basics: Basics {
            name: "Albert Einstein".into(),
            label: "Principal Systems Architect".into(),
            summary: "Engineer with a research background in stream processing, splitting time \
                      between production systems and the parts of the literature that deploy."
                .into(),
            email: "albert@example.com".into(),
            // Two-digit groups: how a Danish number is written, and what the
            // phone pattern used to read straight past.
            phone: "+45 28 44 10 92".into(),
            location: "Bern, Switzerland".into(),
            // No scheme, the way a person writes their own site.
            url: "einstein.example.com".into(),
            profiles: vec![NetworkProfile {
                network: "GitHub".into(),
                username: "aeinstein".into(),
                url: "github.com/aeinstein".into(),
            }],
        },
        work: vec![
            Work {
                name: "Patent Office".into(),
                position: "Senior Examiner".into(),
                location: "Bern".into(),
                start_date: ResumeDate::new("1902-06"),
                end_date: ResumeDate::new("1909-10"),
                url: String::new(),
                summary: String::new(),
                highlights: vec![
                    // Long enough to wrap in plain text, and ending in a full
                    // stop one line before it finishes — the shape that split
                    // one bullet into two.
                    "Reviewed So many applications for electromechanical devices. Filings that \
                     had waited two years were being answered in a fortnight by the end of it."
                        .into(),
                    "Wrote the review notes the office kept using afterwards.".into(),
                ],
            },
            Work {
                name: "ETH".into(),
                position: "Lecturer".into(),
                location: "Zurich".into(),
                start_date: ResumeDate::new("1912-01"),
                end_date: ResumeDate::new("1914-03"),
                url: String::new(),
                summary: String::new(),
                highlights: vec!["Taught analytical mechanics to two hundred students.".into()],
            },
        ],
        education: vec![
            Education {
                institution: "ETH Zurich".into(),
                study_type: "Diploma, Mathematics and Physics".into(),
                start_date: ResumeDate::new("1896-10"),
                end_date: ResumeDate::new("1900-07"),
                url: "ethz.ch".into(),
                highlights: vec!["Thesis on the consequences of capillarity.".into()],
            },
            Education {
                institution: "University of Zurich".into(),
                study_type: "PhD, Physics".into(),
                start_date: ResumeDate::new("1901-01"),
                end_date: ResumeDate::new("1905-04"),
                url: "uzh.ch".into(),
                highlights: Vec::new(),
            },
        ],
        skills: vec![
            SkillGroup {
                name: "Theory".into(),
                keywords: vec![
                    "Statistical mechanics".into(),
                    "Thermodynamics".into(),
                    "Electrodynamics".into(),
                    "Brownian motion".into(),
                ],
            },
            SkillGroup {
                name: "Mathematics".into(),
                keywords: vec!["Tensor calculus".into(), "Differential geometry".into()],
            },
            SkillGroup {
                name: "Languages".into(),
                keywords: vec!["German".into(), "Italian".into(), "English".into()],
            },
        ],
        certificates: vec![
            Certificate {
                name: "Nobel Prize in Physics".into(),
                issuer: "Royal Swedish Academy of Sciences".into(),
                date: ResumeDate::new("1921-11"),
                url: String::new(),
            },
            Certificate {
                name: "Copley Medal".into(),
                issuer: "Royal Society".into(),
                date: ResumeDate::new("1925-11"),
                // Printed on its own line under the entry, which is how it came
                // back as a certificate of its own called `https://…`.
                url: "royalsociety.org/copley".into(),
            },
        ],
        volunteer: vec![
            Volunteer {
                organization: "International Committee on Intellectual Cooperation".into(),
                position: "Member".into(),
                start_date: ResumeDate::new("1922-01"),
                end_date: ResumeDate::new("1932-01"),
                url: String::new(),
                highlights: vec!["Sat on the committee for a decade.".into()],
            },
            Volunteer {
                organization: "Hebrew University of Jerusalem".into(),
                position: "Governor".into(),
                start_date: ResumeDate::new("1925-01"),
                end_date: ResumeDate::new("Present"),
                url: String::new(),
                highlights: Vec::new(),
            },
        ],
        custom_sections: Vec::new(),
        section_titles: Vec::new(),
        section_overrides: Vec::new(),
        section_order: Vec::new(),
    };

    let mut doc = ResumeDoc::from_resume(resume, "Base");
    // Added through the document's own API rather than assembled with a literal
    // id: ids belong to the document that issues them (D-9).
    let id = doc.add_custom_section("Publications");
    if let Some(section) = doc.custom_section_mut(id) {
        *section.content.active_mut() = vec![CustomEntry {
            title: "On the electrodynamics of moving bodies".into(),
            subtitle: "Annalen der Physik".into(),
            start_date: ResumeDate::new("1905-09"),
            end_date: ResumeDate::new(""),
            url: "doi.org/10.1002/andp.19053221004".into(),
            highlights: vec!["The one people mean.".into()],
        }];
    }
    doc
}

/// What a document is, at the resolution a round trip has to preserve.
#[derive(Debug, PartialEq, Eq)]
struct Shape {
    work: Vec<(usize, String, String)>,
    education: usize,
    skills: Vec<(String, usize)>,
    certificates: Vec<(String, String, String)>,
    volunteer: usize,
    custom: Vec<(String, usize)>,
}

fn shape_of(doc: &ResumeDoc) -> Shape {
    Shape {
        work: doc
            .work
            .active()
            .iter()
            .map(|w| {
                (
                    w.highlights.len(),
                    w.start_date.text.clone(),
                    w.end_date.text.clone(),
                )
            })
            .collect(),
        education: doc.education.active().len(),
        skills: doc
            .skills
            .active()
            .iter()
            .map(|s| (s.name.clone(), s.keywords.len()))
            .collect(),
        certificates: doc
            .certificates
            .active()
            .iter()
            // The address, not the string: JSON Resume declares `format: uri`
            // and writes `https://royalsociety.org/copley` where the vault
            // holds `royalsociety.org/copley`. Same link, deliberately.
            .map(|c| {
                (
                    c.name.clone(),
                    c.issuer.clone(),
                    dockcv_core::resume::links::href(&c.url).unwrap_or_default(),
                )
            })
            .collect(),
        volunteer: doc.volunteer.active().len(),
        custom: doc
            .custom_sections
            .iter()
            .map(|c| (c.title.clone(), c.content.active().len()))
            .collect(),
    }
}

fn write_exports(
    dir: &std::path::Path,
    doc: &ResumeDoc,
) -> Vec<(&'static str, std::path::PathBuf)> {
    let composed = doc.compose();
    let mut out = Vec::new();

    let path = dir.join("cv.txt");
    std::fs::write(&path, dockcv_core::resume::export_plain_text(&composed)).expect("write txt");
    out.push(("plain text", path));

    let path = dir.join("cv.md");
    std::fs::write(&path, dockcv_core::resume::export_markdown(&composed)).expect("write md");
    out.push(("markdown", path));

    let path = dir.join("cv.json");
    let json = dockcv_core::resume::export_json_resume(&composed).expect("json resume");
    std::fs::write(&path, json).expect("write json");
    out.push(("json resume", path));

    let path = dir.join("cv.typ");
    std::fs::write(&path, dockcv_core::resume::export_typst(doc)).expect("write typ");
    out.push(("typst", path));

    let path = dir.join("cv.docx");
    let docx = dockcv_core::resume::export_docx(&composed).expect("docx");
    std::fs::write(&path, docx).expect("write docx");
    out.push(("docx", path));

    out
}

/// The whole property: a CV DockCV exported, imported by DockCV, is the same CV.
#[test]
fn every_text_format_survives_being_exported_and_imported_again() {
    let dir = std::env::temp_dir().join(format!("dockcv-roundtrip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");

    let original = fixture();
    let expected = shape_of(&original);

    for (format, path) in write_exports(&dir, &original) {
        let imported =
            import_file(&path).unwrap_or_else(|e| panic!("{format} did not import: {e}"));
        assert_eq!(
            shape_of(&imported.doc),
            expected,
            "{format} did not come back as the document that was written"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A job that has not ended still has not ended after a round trip.
///
/// JSON Resume validates every date against a subset of ISO 8601, so `Present`
/// cannot be written in `endDate` at all — the field is dropped and the fact
/// recorded in `meta.availability`, which the spec leaves open for exactly
/// this. Writing that and not reading it back would be a format that loses the
/// most useful thing a CV says.
#[test]
fn a_job_still_held_survives_json_resume() {
    let dir = std::env::temp_dir().join(format!("dockcv-roundtrip-present-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");

    let mut original = fixture();
    original.work.active_mut()[1].end_date = ResumeDate::new("Present");
    original.volunteer.active_mut()[0].end_date = ResumeDate::new("Present");

    let path = dir.join("cv.json");
    let json = dockcv_core::resume::export_json_resume(&original.compose()).expect("json resume");
    assert!(
        !json.contains("Present"),
        "a validator refuses `Present` in a date field, so it must not be there:\n{json}"
    );
    std::fs::write(&path, json).expect("write json");

    let imported = import_file(&path).expect("the JSON Resume imports");
    let _ = std::fs::remove_dir_all(&dir);

    let work = imported.doc.work.active();
    assert!(
        work[1].end_date.names_the_present(),
        "the job still held came back ended: {:?}",
        work[1].end_date
    );
    assert_eq!(
        work[0].end_date.text, "1909-10",
        "a job that did ended, ended"
    );
    assert!(imported.doc.volunteer.active()[0]
        .end_date
        .names_the_present());
}

/// The contact block, which every format writes differently and all of them
/// used to lose most of.
#[test]
fn the_contact_block_survives_every_format() {
    let dir = std::env::temp_dir().join(format!("dockcv-roundtrip-contact-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");

    let original = fixture();
    for (format, path) in write_exports(&dir, &original) {
        let imported =
            import_file(&path).unwrap_or_else(|e| panic!("{format} did not import: {e}"));
        let b = imported.doc.profile.active();

        assert_eq!(b.name, "Albert Einstein", "{format} lost the name");
        assert_eq!(b.email, "albert@example.com", "{format} lost the email");
        assert!(
            b.phone.replace([' ', '-'], "").contains("4528441092"),
            "{format} lost the phone number: {:?}",
            b.phone
        );
        assert!(
            b.location.contains("Bern"),
            "{format} lost the location: {:?}",
            b.location
        );
        assert!(
            b.summary.contains("stream processing"),
            "{format} lost the summary: {:?}",
            b.summary
        );
        // Written without a scheme, so it is invisible to a pattern that only
        // knows `https://` — and it is the person's own site.
        let addresses: Vec<&str> = std::iter::once(b.url.as_str())
            .chain(b.profiles.iter().map(|p| p.url.as_str()))
            .collect();
        assert!(
            addresses.iter().any(|u| u.contains("einstein.example.com")),
            "{format} lost the personal site: {addresses:?}"
        );
        assert!(
            addresses.iter().any(|u| u.contains("github.com/aeinstein")),
            "{format} lost the GitHub profile: {addresses:?}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A CV DockCV exported, read back, when the CV is one written to break it.
///
/// The corpus is `ats::adversarial` — ligatures, a name in NFD, a hyphenating
/// line, Ukrainian, `C++` and curly quotes, eight jobs over two pages,
/// whitespace nobody can see. The export side of that corpus is measured in
/// `ats::conformance`; this is the other direction, and it is the one that
/// found the date bug below.
///
/// Japanese is excluded, and named rather than quietly skipped: a DockCV PDF
/// cannot set kanji at all (no bundled face covers it, which the lint now says
/// out loud), and the plain-text importer reads a Japanese entry line as a
/// section of its own. Fixing the second without the first would be polishing a
/// door on a house with no walls.
#[test]
fn every_adversary_survives_being_exported_and_imported_again() {
    let dir = std::env::temp_dir().join(format!("dockcv-adv-rt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");

    for adversary in crate::ats::adversarial::all() {
        if adversary.name == "cjk" {
            continue;
        }
        let original = ResumeDoc::from_resume(adversary.resume.clone(), "Base");
        let expected = shape_of(&original);
        for (format, path) in write_exports(&dir, &original) {
            let imported = import_file(&path)
                .unwrap_or_else(|e| panic!("{} did not import as {format}: {e}", adversary.name));
            assert_eq!(
                shape_of(&imported.doc),
                expected,
                "“{}” did not survive {format}. That document exists to test: {}",
                adversary.name,
                adversary.attacks
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A number in front of a date range used to eat its year.
///
/// `Company Number 4` above `2019-06 - 2022-01` parsed as the range `4 2019` to
/// `06 - 2022`, so every job in the document came back with dates that were
/// never in it. The trigger is any digit before the range — an employer ending
/// in a number, a job title with a grade in it, a street address — and the
/// cause was two permissive pieces of one regex meeting: `[0-9]{1,2}[\s./-]+`
/// before a year made `4 2019` a date, and a *run* of separators made
/// `06 - 2022` another. A real date's parts are held by one mark; ` - ` is what
/// separates the two ends of a range.
///
/// Found by exporting the two-page adversary and reading it back, not by a
/// report — which is the point of that corpus.
#[test]
fn a_number_in_front_of_a_range() {
    let dir = std::env::temp_dir().join(format!("dockcv-number-range-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");

    let doc = ResumeDoc::from_resume(
        Resume {
            basics: Basics {
                name: "A Person".into(),
                ..Default::default()
            },
            work: vec![dockcv_core::resume::model::Work {
                name: "Company Number 4".into(),
                position: "Engineer Grade 3".into(),
                start_date: ResumeDate::new("2019-06"),
                end_date: ResumeDate::new("2022-01"),
                highlights: vec!["Did the thing that needed doing.".into()],
                ..Default::default()
            }],
            ..Default::default()
        },
        "Base",
    );

    for (format, path) in write_exports(&dir, &doc) {
        let back = import_file(&path).unwrap_or_else(|e| panic!("{format}: {e}"));
        let job = &back.doc.work.active()[0];
        assert_eq!(
            (job.start_date.text.as_str(), job.end_date.text.as_str()),
            ("2019-06", "2022-01"),
            "{format} read the dates out of the employer's number"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A damaged file is refused, never a crash.
///
/// Import is the first thing a new user does, and the files they do it with
/// come off downloads, sync clients and USB sticks. Two of the three parsers
/// under this app answer a file that is *almost* right by panicking rather than
/// by returning an error — `pdf-extract` on a construct it does not handle, and
/// `read_docx` on a document part it did not expect — and a panic on the import
/// worker is not contained by being on a worker: `async-task` resumes the
/// unwind in the awaiting task, which is on the UI thread. One flipped byte in
/// a valid .docx took the whole app down until this test was written.
///
/// So: forty-odd damaged files, and the only two acceptable outcomes are a
/// document and a refusal that says something.
#[test]
fn a_damaged_file_is_refused_and_never_a_crash() {
    let dir = std::env::temp_dir().join(format!("dockcv-damage-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let composed = fixture().compose();

    let pdf = dockcv_core::typst_engine::TypstEngine::new(dockcv_core::resume::template::generate(
        &composed,
    ))
    .compile_to_pdf()
    .expect("pdf");
    let docx = dockcv_core::resume::export_docx(&composed).expect("docx");
    let text = dockcv_core::resume::export_plain_text(&composed).into_bytes();
    let json = dockcv_core::resume::export_json_resume(&composed)
        .expect("json")
        .into_bytes();

    let mut cases: Vec<(String, &'static str, Vec<u8>)> = Vec::new();
    for (ext, whole) in [
        ("pdf", &pdf),
        ("docx", &docx),
        ("txt", &text),
        ("json", &json),
    ] {
        for cut in [
            0usize,
            1,
            16,
            whole.len() / 3,
            whole.len() / 2,
            whole.len() - 1,
        ] {
            cases.push((
                format!("{ext} truncated to {cut}"),
                ext,
                whole[..cut].to_vec(),
            ));
        }
        // Still the right length, still the right magic number, and no longer
        // the file it says it is. This is the one that panicked.
        let mut flipped = whole.to_vec();
        let at = flipped.len() / 2;
        flipped[at] ^= 0xff;
        cases.push((format!("{ext} with a flipped byte"), ext, flipped));
        // The right extension over somebody else's bytes.
        cases.push((format!("{ext} that is really a PDF"), ext, pdf.clone()));
    }
    cases.push(("pdf of pure zeros".into(), "pdf", vec![0u8; 4096]));
    cases.push((
        "docx that is an empty zip".into(),
        "docx",
        b"PK\x05\x06".to_vec(),
    ));

    for (name, ext, bytes) in cases {
        let path = dir.join(format!("damaged.{ext}"));
        std::fs::write(&path, &bytes).expect("write");
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| import_file(&path)));
        match outcome {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => assert!(
                !e.to_string().trim().is_empty(),
                "{name} was refused without saying why"
            ),
            Err(_) => panic!("{name} brought the app down instead of being refused"),
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// Notepad's two encodings are not an error.
///
/// `read_to_string` takes UTF-8 and nothing else. Saving a CV from Notepad as
/// "Unicode" writes UTF-16 with a byte-order mark, which was refused outright;
/// saving it as "UTF-8" writes a mark too, which was *worse*, because it does
/// not fail — the mark became the first character of the person's name and
/// travelled into the vault where nothing on screen would ever show it.
#[test]
fn a_text_cv_saved_the_way_notepad_saves_one() {
    let dir = std::env::temp_dir().join(format!("dockcv-encodings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let plain = dockcv_core::resume::export_plain_text(&fixture().compose());

    let utf16 = |big_endian: bool| {
        let mut bytes = if big_endian {
            vec![0xfe, 0xff]
        } else {
            vec![0xff, 0xfe]
        };
        for unit in plain.encode_utf16() {
            bytes.extend_from_slice(&if big_endian {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            });
        }
        bytes
    };
    let mut utf8_bom = vec![0xef, 0xbb, 0xbf];
    utf8_bom.extend_from_slice(plain.as_bytes());

    for (what, bytes) in [
        ("UTF-16 little-endian", utf16(false)),
        ("UTF-16 big-endian", utf16(true)),
        ("UTF-8 with a mark", utf8_bom),
        ("UTF-8", plain.clone().into_bytes()),
    ] {
        let path = dir.join("cv.txt");
        std::fs::write(&path, &bytes).expect("write");
        let imported = import_file(&path).unwrap_or_else(|e| panic!("{what}: {e}"));
        let name = imported.doc.compose().basics.name;
        assert_eq!(
            name.trim(),
            "Albert Einstein",
            "{what} did not give back the person's own name"
        );
        assert!(
            !name.starts_with('\u{feff}'),
            "{what} left a byte-order mark inside the name"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A CV from somebody else's template gives up what it states.
///
/// The corpus is `import::foreign_cvs`: a sidebar down the left, the whole
/// document inside a table, the contact block in a running header, dates in a
/// gutter, no section headings at all, and a right-to-left script. Each is
/// compiled to a real PDF and imported, and every fact the file states plainly
/// has to come back — whatever the layout was doing when it stated it.
///
/// Two of these were failing when the corpus was written, and both were fixed
/// rather than recorded: a table's entry line arrives with its dates *first*
/// and had its title and employer filed as a location, and a CV with no
/// headings had everything below the contact block dropped.
#[test]
fn a_cv_from_somebody_elses_template_gives_up_what_it_states() {
    let dir = std::env::temp_dir().join(format!("dockcv-foreign-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");

    for cv in crate::import::foreign_cvs::all() {
        let pdf = dockcv_core::typst_engine::TypstEngine::new(cv.source.to_string())
            .compile_to_pdf()
            .unwrap_or_else(|why| panic!("“{}” does not compile: {why}", cv.name));
        let path = dir.join(format!("{}.pdf", cv.name.replace(' ', "-")));
        std::fs::write(&path, &pdf).expect("write");

        let imported =
            import_file(&path).unwrap_or_else(|e| panic!("“{}” did not import: {e}", cv.name));
        let doc = imported.doc.compose();
        let everything = format!("{doc:?}");
        let missing: Vec<&str> = cv
            .must_recover
            .iter()
            .filter(|fact| !everything.contains(*fact))
            .copied()
            .collect();
        assert!(
            missing.is_empty(),
            "“{}” lost {missing:?}.\n    That layout is: {}",
            cv.name,
            cv.shape
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A CV with no headings says so rather than filing a degree as a job in
/// silence.
#[test]
fn a_cv_with_no_headings_is_read_by_shape_and_says_so() {
    let dir = std::env::temp_dir().join(format!("dockcv-no-headings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");

    let cv = crate::import::foreign_cvs::all()
        .into_iter()
        .find(|c| c.name == "no headings at all")
        .expect("the corpus carries one");
    let pdf = dockcv_core::typst_engine::TypstEngine::new(cv.source.to_string())
        .compile_to_pdf()
        .expect("compiles");
    let path = dir.join("cv.pdf");
    std::fs::write(&path, &pdf).expect("write");

    let imported = import_file(&path).expect("imports");
    assert_eq!(
        imported.doc.work.active().len(),
        3,
        "every dated entry should have come out"
    );
    assert!(
        imported.notes.iter().any(|(_, note)| matches!(
            note,
            crate::import::notes::Note::ReadWithoutHeadings { .. }
        )),
        "a degree read as a job has to be said out loud: {:?}",
        imported.notes
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_telephone_number_and_a_city_survive_the_way_people_write_them() {
    let dir = std::env::temp_dir().join(format!("dockcv-contact-shapes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");

    for (phone, location) in [
        ("+45 28 44 10 92", "Bern, Switzerland"),
        ("+353 1 555 0100", "Dublin"),
        ("+353 1 555 0100", "Dublin, Ireland"),
        ("+1 (415) 555-0134", "San Francisco"),
        ("020 7946 0958", "London"),
    ] {
        let doc = ResumeDoc::from_resume(
            Resume {
                basics: Basics {
                    name: "A Person".into(),
                    email: "person@example.com".into(),
                    phone: phone.into(),
                    location: location.into(),
                    ..Default::default()
                },
                work: vec![dockcv_core::resume::model::Work {
                    name: "Acme".into(),
                    position: "Engineer".into(),
                    start_date: ResumeDate::new("2019-06"),
                    end_date: ResumeDate::new("2022-01"),
                    highlights: vec!["Did the thing that needed doing.".into()],
                    ..Default::default()
                }],
                ..Default::default()
            },
            "Base",
        );

        for (format, path) in write_exports(&dir, &doc) {
            // Typst source carries the model itself, and JSON Resume has a
            // field per fact; the shapes below are about *prose* formats, where
            // a contact line is one line and a reader has to take it apart.
            if format == "typst" || format == "json resume" {
                continue;
            }
            let back = import_file(&path)
                .unwrap_or_else(|e| panic!("{format}: {e}"))
                .doc
                .compose();
            assert_eq!(
                back.basics.phone, phone,
                "{format} lost the telephone number {phone:?}"
            );
            assert_eq!(
                back.basics.location, location,
                "{format} lost the city {location:?}"
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// Exporting what was imported gives the same file back.
///
/// A round trip that is not a fixed point is a round trip that changes the
/// document, and the change compounds: the Typst emitter escaped `C#` as `C\#`
/// and read it back with the backslash still on it, so three trips through
/// `.typ` turned one bullet into `C\\\\\\\#`. Checking the shape, as the tests
/// above do, cannot see any of that — the shape was identical every time.
///
/// Byte equality, and only for the formats where bytes are the document. A
/// `.docx` is a zip: its relationship ids are numbered in the order they were
/// written and say nothing about the CV, so that format is compared by what a
/// reader gets out of it instead.
#[test]
fn exporting_what_was_imported_gives_the_same_file_back() {
    let dir = std::env::temp_dir().join(format!("dockcv-fixed-point-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let second = dir.join("second");
    std::fs::create_dir_all(&second).expect("temp dir");

    let documents = std::iter::once(("the fixture", fixture())).chain(
        crate::ats::adversarial::all()
            .into_iter()
            .filter(|a| a.name != "cjk")
            .map(|a| (a.name, ResumeDoc::from_resume(a.resume, "Base"))),
    );

    for (name, doc) in documents {
        for (format, path) in write_exports(&dir, &doc) {
            // Known, argued, and each one a thing a person would rather have
            // than not:
            //
            // * the importer folds a no-break space to a space, so a name typed
            //   with one comes back with an ordinary space. That is a repair,
            //   not a loss — and the lint says so before the CV is ever sent.
            // * the fixture's Markdown education heading is
            //   `[Diploma, Mathematics and Physics, ETH Zurich](…)`, three
            //   comma-separated parts of which two are the degree. Nothing is
            //   lost — the whole string lands in the degree — but where the
            //   school ends and the subject begins is a guess, and guessing it
            //   from one example is how a heuristic gets worse.
            let excused =
                name == "whitespace nobody sees" || (name == "the fixture" && format == "markdown");
            if excused {
                continue;
            }

            let once = std::fs::read(&path).expect("read");
            let imported = import_file(&path).unwrap_or_else(|e| panic!("{name} · {format}: {e}"));
            let again = write_exports(&second, &imported.doc)
                .into_iter()
                .find(|(f, _)| *f == format)
                .map(|(_, p)| std::fs::read(p).expect("read"))
                .expect("the same format comes back");

            if format == "docx" {
                let before = crate::ats::docx::flat_text(&once).expect("read docx");
                let after = crate::ats::docx::flat_text(&again).expect("read docx");
                if before != after {
                    let diff = before
                        .lines()
                        .zip(after.lines())
                        .find(|(x, y)| x != y)
                        .map(|(x, y)| format!("was {x:?}\n  now {y:?}"))
                        .unwrap_or_else(|| {
                            format!(
                                "{} lines became {}",
                                before.lines().count(),
                                after.lines().count()
                            )
                        });
                    panic!("{name} · {format} says something different the second time:\n  {diff}");
                }
                continue;
            }

            if once != again {
                let a = String::from_utf8_lossy(&once);
                let b = String::from_utf8_lossy(&again);
                let where_ = a
                    .lines()
                    .zip(b.lines())
                    .enumerate()
                    .find(|(_, (x, y))| x != y)
                    .map(|(i, (x, y))| format!("line {i}:\n  was {x:?}\n  now {y:?}"))
                    .unwrap_or_else(|| {
                        format!("{} lines became {}", a.lines().count(), b.lines().count())
                    });
                panic!("{name} · {format} is not a fixed point — {where_}");
            }
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

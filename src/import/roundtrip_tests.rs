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

//! One traversal of a composed [`Resume`], shared by every export emitter.
//!
//! Plain text, Markdown and DOCX are three renderings of the same walk, and
//! written separately they diverge within a month — the fourth section anyone
//! adds reaches two of the three, and the one it misses ships a CV with a hole
//! in it that no test looks for. So the order sections come in, whether a
//! section has anything to say, and what its heading is called live here, once,
//! and an emitter decides only how to draw what it is handed.
//!
//! What is deliberately *not* here: the emitters' own formatting. A heading is
//! `EDUCATION` in text, `## Education` in Markdown and a bold run in DOCX, and
//! pushing that behind a trait would buy nothing but indirection.

use crate::resume::dates::DateStyle;
use crate::resume::model::{Resume, ResumeDate, ResumeDoc, SectionKind};

/// The sections of `resume`, in the order they print.
///
/// The user's own order when they have set one — `section_order` is the whole
/// point of the editor's drag handles — and the shipping order otherwise, with
/// custom sections after the six built-ins.
pub fn ordered_sections(resume: &Resume) -> Vec<SectionKind> {
    if !resume.section_order.is_empty() {
        return resume.section_order.clone();
    }

    let mut list = vec![
        SectionKind::Work,
        SectionKind::Education,
        SectionKind::Skills,
        SectionKind::Certificates,
        SectionKind::Organizations,
    ];
    for cs in &resume.custom_sections {
        list.push(SectionKind::Custom(cs.id));
    }
    list
}

/// Whether a section would print nothing, so an emitter can skip its heading
/// rather than leave `EDUCATION` above a blank line.
pub fn is_section_empty(resume: &Resume, kind: SectionKind) -> bool {
    match kind {
        SectionKind::Profile => resume.basics.summary.trim().is_empty(),
        SectionKind::Work => resume.work.is_empty(),
        SectionKind::Education => resume.education.is_empty(),
        SectionKind::Skills => resume.skills.is_empty(),
        SectionKind::Certificates => resume.certificates.is_empty(),
        SectionKind::Organizations => resume.volunteer.is_empty(),
        SectionKind::Custom(id) => resume
            .custom_sections
            .iter()
            .find(|cs| cs.id == id)
            .map(|cs| cs.entries.is_empty())
            .unwrap_or(true),
    }
}

/// The heading a section prints under: the user's rename if there is one, the
/// custom section's own title, or the shipping name.
pub fn resolve_section_title(resume: &Resume, kind: SectionKind) -> String {
    if let SectionKind::Custom(id) = kind {
        return resume
            .custom_sections
            .iter()
            .find(|cs| cs.id == id)
            .map(|cs| cs.title.clone())
            .unwrap_or_default();
    }

    if let Some((_, title)) = resume.section_titles.iter().find(|(k, _)| *k == kind) {
        return title.clone();
    }

    ResumeDoc::default_section_title(kind).to_string()
}

/// `start – end`, either end alone, or nothing — in the document's own date
/// format, so an export never prints a date the page does not.
pub fn format_date_range(start: &ResumeDate, end: &ResumeDate, dates: DateStyle) -> String {
    let start_str = start.display_in(dates.format, dates.language);
    let end_str = end.display_in(dates.format, dates.language);
    if !start_str.is_empty() && !end_str.is_empty() {
        format!("{start_str} - {end_str}")
    } else if !start_str.is_empty() {
        // An empty end date is how this model says "still there" — the page has
        // read it that way since the beginning (`template.rs::daterange`) and
        // every other export read it as a job that ended at an unstated time.
        // A parser cannot tell the difference between the two from a bare start
        // date, and "is this person available" is a question it is built to
        // answer, so the word the page prints is the word they all print now.
        format!("{start_str} - {}", dates.language.present())
    } else if !end_str.is_empty() {
        end_str
    } else {
        String::new()
    }
}

/// The one composed résumé every emitter's tests are written against.
///
/// Shared rather than copied: four hand-maintained copies of this had already
/// drifted — different dates, different highlight counts, a custom section in
/// two of them and not the others — which is how an emitter comes to be tested
/// against a document no other emitter ever sees.
///
/// Every section kind is populated, so a test that walks `SectionKind` finds
/// content for each one.
#[cfg(test)]
pub(crate) fn sample_resume() -> crate::resume::model::Resume {
    use crate::resume::model::*;

    Resume {
        basics: Basics {
            name: "Albert Einstein".into(),
            label: "Principal Systems Architect".into(),
            summary: "Experienced *systems engineer* specializing in _distributed_ storage."
                .into(),
            email: "albert@example.com".into(),
            phone: "+1 555-0199".into(),
            location: "San Francisco, CA, US".into(),
            url: "https://example.com".into(),
            profiles: vec![NetworkProfile {
                network: "GitHub".into(),
                username: "aeinstein".into(),
                url: "https://github.com/aeinstein".into(),
            }],
        },
        work: vec![Work {
            name: "Tech Corp".into(),
            position: "Staff Software Engineer".into(),
            location: "Mountain View, CA".into(),
            url: String::new(),
            start_date: ResumeDate::new("2021-03-01"),
            end_date: ResumeDate::new("2024-01-01"),
            summary: "Lead storage engine architecture.".into(),
            highlights: vec![
                // Long on purpose: the plain-text emitter wraps, and a bullet
                // that fits on one line proves nothing about the wrapping.
                "Designed high-throughput commit log processing 50M ops/sec with sub-millisecond p99 latency."
                    .into(),
                "Shipped #link(\"https://dockcv.com\")[DockCV] to *8* senior engineers.".into(),
            ],
        }],
        education: vec![Education {
            institution: "State University".into(),
            study_type: "B.S. in Computer Science".into(),
            start_date: ResumeDate::new("2015-09-01"),
            end_date: ResumeDate::new("2019-06-01"),
            url: "https://university.edu".into(),
            highlights: vec!["Graduated *Summa Cum Laude* with honors.".into()],
        }],
        skills: vec![SkillGroup {
            name: "Languages".into(),
            keywords: vec!["Rust".into(), "C++".into(), "Go".into()],
        }],
        certificates: vec![Certificate {
            name: "AWS Solutions Architect".into(),
            issuer: "Amazon Web Services".into(),
            date: ResumeDate::new("2022-05-15"),
            url: "https://aws.amazon.com".into(),
        }],
        volunteer: vec![Volunteer {
            organization: "Open Source Collective".into(),
            position: "Core Maintainer".into(),
            url: String::new(),
            start_date: ResumeDate::new("2020-01-01"),
            end_date: ResumeDate::new("2023-01-01"),
            highlights: vec!["Maintain networking libraries.".into()],
        }],
        custom_sections: vec![
            ComposedCustomSection {
                id: CustomSectionId::from_u32(1),
                title: "Publications".into(),
                entries: vec![CustomEntry {
                    title: "High Performance Storage in Rust".into(),
                    subtitle: "ACM Systems Conference".into(),
                    start_date: ResumeDate::new("2023-11-01"),
                    end_date: ResumeDate::new(""),
                    url: "https://doi.org/10.1145/example".into(),
                    highlights: vec!["Awarded Best Paper.".into()],
                }],
            },
            ComposedCustomSection {
                id: CustomSectionId::from_u32(2),
                title: "Side Projects".into(),
                entries: vec![CustomEntry {
                    title: "DockCV".into(),
                    subtitle: "Modern CV builder in Rust".into(),
                    start_date: ResumeDate::new("2024-01-01"),
                    end_date: ResumeDate::new("Present"),
                    url: "https://dockcv.com".into(),
                    highlights: vec!["Fast Typst-backed CV desktop app.".into()],
                }],
            },
        ],
        section_titles: Vec::new(),
        section_overrides: Vec::new(),
        section_order: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    /// Every text emitter goes through here, so this is the one place a
    /// German CV could stop being German. It did: the shape was threaded and
    /// the language was not, so DOCX, plain text and Markdown printed
    /// `Mar 2021 - Present` under a page reading `Mär 2021 - Heute`.
    #[test]
    fn a_date_range_is_written_in_the_readings_own_language() {
        use crate::resume::model::{DateFormat, DateStyle, DocumentLanguage, ResumeDate};

        let start = ResumeDate::new("2021-03-04");
        let open = ResumeDate::new("");

        let english = format_date_range(&start, &open, DateStyle::plain(DateFormat::DayMonShortYear));
        assert!(english.contains("Mar"), "{english}");
        assert!(english.ends_with("Present"), "{english}");

        let german = format_date_range(
            &start,
            &open,
            DateStyle::new(DateFormat::DayMonShortYear, DocumentLanguage::German),
        );
        assert!(german.contains("Mär"), "{german}");
        assert!(german.ends_with("Heute"), "{german}");
    }

    use super::*;
    use crate::resume::model::{CustomSectionId, SectionKind};

    /// Every section kind reaches every emitter.
    ///
    /// The point of one traversal is that adding a seventh section kind cannot
    /// reach two emitters and miss the third. The `match` below is exhaustive,
    /// so a new variant fails to compile until someone says what it contains;
    /// the assertions then check that each emitter actually printed it, which
    /// is the part a list of kinds on its own never proves.
    #[test]
    fn every_section_kind_reaches_every_emitter() {
        let resume = sample_resume();

        let text = crate::resume::export::text::export_plain_text(&resume);
        let markdown = crate::resume::export::markdown::export_markdown(&resume);
        #[cfg(feature = "docx")]
        let docx = {
            let bytes = crate::resume::export::docx::export_docx(&resume).expect("docx");
            docx_text(&bytes)
        };

        for kind in ordered_sections(&resume) {
            assert!(
                !is_section_empty(&resume, kind),
                "the shared fixture has nothing for {kind:?}, so no emitter can be checked on it"
            );

            // Exhaustive on purpose: a new `SectionKind` stops the build here.
            let heading = match kind {
                SectionKind::Profile
                | SectionKind::Work
                | SectionKind::Education
                | SectionKind::Skills
                | SectionKind::Certificates
                | SectionKind::Organizations
                | SectionKind::Custom(_) => resolve_section_title(&resume, kind),
            };

            assert!(
                text.contains(&heading.to_uppercase()),
                "plain text dropped {heading:?}"
            );
            assert!(
                markdown.contains(&format!("## {heading}")),
                "markdown dropped {heading:?}"
            );
            #[cfg(feature = "docx")]
            assert!(
                docx.contains(&heading.to_uppercase()),
                "docx dropped {heading:?}"
            );
        }
    }

    /// A section the fixture leaves empty prints no heading anywhere — an
    /// `EDUCATION` above a blank line is worse than no section at all.
    #[test]
    fn an_empty_section_prints_no_heading() {
        let mut resume = sample_resume();
        resume.education.clear();
        assert!(is_section_empty(&resume, SectionKind::Education));

        let text = crate::resume::export::text::export_plain_text(&resume);
        let markdown = crate::resume::export::markdown::export_markdown(&resume);
        assert!(!text.contains("EDUCATION"));
        assert!(!markdown.contains("## Education"));
    }

    #[test]
    fn the_user_s_own_section_order_wins() {
        let mut resume = sample_resume();
        resume.section_order = vec![
            SectionKind::Skills,
            SectionKind::Work,
            SectionKind::Custom(CustomSectionId::from_u32(1)),
        ];
        assert_eq!(
            ordered_sections(&resume),
            vec![
                SectionKind::Skills,
                SectionKind::Work,
                SectionKind::Custom(CustomSectionId::from_u32(1)),
            ]
        );

        // A renamed heading is the name that prints, in every emitter.
        resume.section_titles = vec![(SectionKind::Work, "Relevant Experience".into())];
        assert_eq!(
            resolve_section_title(&resume, SectionKind::Work),
            "Relevant Experience"
        );
    }

    #[cfg(feature = "docx")]
    fn docx_text(bytes: &[u8]) -> String {
        let docx = docx_rs::read_docx(bytes).expect("docx-rs reads what we wrote");
        let mut out = String::new();
        for child in docx.document.children {
            if let docx_rs::DocumentChild::Paragraph(p) = child {
                for p_child in p.children {
                    match p_child {
                        docx_rs::ParagraphChild::Run(r) => {
                            for r_child in r.children {
                                if let docx_rs::RunChild::Text(t) = r_child {
                                    out.push_str(&t.text);
                                }
                            }
                        }
                        docx_rs::ParagraphChild::Hyperlink(h) => {
                            for h_child in h.children {
                                if let docx_rs::ParagraphChild::Run(r) = h_child {
                                    for r_child in r.children {
                                        if let docx_rs::RunChild::Text(t) = r_child {
                                            out.push_str(&t.text);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                out.push('\n');
            }
        }
        out
    }

    /// Every emitter carries an entry link in its own idiom — plain text in
    /// parentheses, Markdown as `### [Title](target)`, DOCX as a hyperlink
    /// relationship, JSON Resume as a `url`, Typst as an `href` beside the
    /// printed text.
    ///
    /// The URLs below are written the way people write their own — `dtu.dk`,
    /// not `https://dtu.dk` — because that is what the vault holds and because
    /// the version of this test that used absolute URLs passed while every
    /// link in a real CV was dead. What prints stays as typed; what is followed
    /// is absolute. See [`crate::resume::links`].
    #[test]
    fn entry_links_reach_all_track_a_exporters_in_their_own_idiom() {
        let mut resume = sample_resume();
        resume.work[0].url = "techcorp.example.com".into();
        resume.education[0].url = "university.edu".into();
        resume.volunteer[0].url = "opensource.example.org".into();

        // 1. Plain Text: heading (url)
        let text = crate::resume::export::text::export_plain_text(&resume);
        assert!(
            text.contains("Staff Software Engineer, Tech Corp (Mountain View, CA)")
                && text.contains("(techcorp.example.com)"),
            "Plain text must include work url in parentheses after heading, got:\n{text}"
        );
        assert!(
            text.contains("B.S. in Computer Science, State University (university.edu)"),
            "Plain text must include education url in parentheses after heading, got:\n{text}"
        );
        assert!(
            text.contains("Core Maintainer, Open Source Collective (opensource.example.org)"),
            "Plain text must include volunteer url in parentheses after heading, got:\n{text}"
        );

        // 2. Markdown: ### [Heading](url)
        let md = crate::resume::export::markdown::export_markdown(&resume);
        assert!(
            md.contains("### [Staff Software Engineer, Tech Corp](https://techcorp.example.com) (Mountain View, CA)"),
            "Markdown must format work heading as inline link, got:\n{md}"
        );
        assert!(
            md.contains("### [B.S. in Computer Science, State University](https://university.edu)"),
            "Markdown must format education heading as inline link, got:\n{md}"
        );
        assert!(
            md.contains(
                "### [Core Maintainer, Open Source Collective](https://opensource.example.org)"
            ),
            "Markdown must format volunteer heading as inline link, got:\n{md}"
        );

        // 3. DOCX: contains text and links
        #[cfg(feature = "docx")]
        {
            let docx_bytes = crate::resume::export::docx::export_docx(&resume).expect("docx export");
            let read = docx_rs::read_docx(&docx_bytes).expect("docx read");
            let mut hyperlink_texts: Vec<String> = Vec::new();
            for child in read.document.children {
                if let docx_rs::DocumentChild::Paragraph(p) = child {
                    for pc in p.children {
                        if let docx_rs::ParagraphChild::Hyperlink(h) = pc {
                            let mut text = String::new();
                            for hc in &h.children {
                                if let docx_rs::ParagraphChild::Run(r) = hc {
                                    for rc in &r.children {
                                        if let docx_rs::RunChild::Text(t) = rc {
                                            text.push_str(&t.text);
                                        }
                                    }
                                }
                            }
                            hyperlink_texts.push(text);
                        }
                    }
                }
            }
            assert!(
                hyperlink_texts
                    .iter()
                    .any(|t| t.contains("Staff Software Engineer, Tech Corp")),
                "DOCX must contain work heading as hyperlink: {hyperlink_texts:?}"
            );
            assert!(
                hyperlink_texts
                    .iter()
                    .any(|t| t.contains("State University")),
                "DOCX must contain education heading as hyperlink: {hyperlink_texts:?}"
            );
            assert!(
                hyperlink_texts
                    .iter()
                    .any(|t| t.contains("Open Source Collective")),
                "DOCX must contain volunteer heading as hyperlink: {hyperlink_texts:?}"
            );

            // Verify the actual external URLs are stored in the Word relationship parts:
            let mut archive =
                zip::ZipArchive::new(std::io::Cursor::new(&docx_bytes)).expect("docx zip archive");
            let mut rels = String::new();
            std::io::Read::read_to_string(
                &mut archive
                    .by_name("word/_rels/document.xml.rels")
                    .expect("docx rels"),
                &mut rels,
            )
            .expect("read rels");
            // The full target, not a substring a bare `techcorp.example.com`
            // would also satisfy: Word resolves a relative target against the
            // document's own folder and finds nothing there.
            for target in [
                "https://techcorp.example.com",
                "https://university.edu",
                "https://opensource.example.org",
            ] {
                assert!(
                    rels.contains(&format!("Target=\"{target}\"")),
                    "DOCX rels must point at {target}, got:\n{rels}"
                );
            }
        }

        // 4. JSON Resume: exports work and volunteer url
        let json_resume =
            crate::resume::export::json_resume::export_json_resume(&resume).expect("json resume");
        assert!(
            json_resume.contains("https://techcorp.example.com"),
            "JSON Resume must contain work url"
        );
        assert!(
            json_resume.contains("https://opensource.example.org"),
            "JSON Resume must contain volunteer url"
        );

        // 5. Typst: the printed text and the followed target are separate keys.
        // That the target then survives into a PDF annotation is asserted where
        // the PDF is made — `typst_engine::font_tests`.
        let typst_src = crate::resume::template::generate(&resume);
        assert!(
            typst_src.contains(r#"url: "techcorp.example.com""#),
            "Typst dict must print the url as the user typed it, got:\n{typst_src}"
        );
        assert!(
            typst_src.contains(r#"href: "https://techcorp.example.com""#),
            "Typst dict must carry an absolute target beside it, got:\n{typst_src}"
        );
    }
}

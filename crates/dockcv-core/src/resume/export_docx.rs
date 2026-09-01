//! Microsoft Word (.docx) export emitter for a composed [`Resume`].
//!
//! Generates a clean Word document using `docx-rs`, structured for maximum ATS
//! readability (clear headings, styled runs, bullet lists, no layout tables).

use std::io::Cursor;

use docx_rs::{Docx, DocxError, Paragraph, Run};

use super::dates::DateFormat;
use super::export_text::strip_typst_markup;
use super::model::{
    Basics, Certificate, ComposedCustomSection, CustomEntry, Education, Resume, ResumeDate,
    ResumeDoc, SectionKind, SkillGroup, Volunteer, Work,
};

/// Export a composed [`Resume`] to DOCX binary bytes.
pub fn export_docx(resume: &Resume) -> Result<Vec<u8>, DocxError> {
    export_docx_with_date_format(resume, DateFormat::default())
}

/// Export a composed [`Resume`] to DOCX binary bytes with an explicit date format.
pub fn export_docx_with_date_format(
    resume: &Resume,
    date_format: DateFormat,
) -> Result<Vec<u8>, DocxError> {
    let mut docx = Docx::new();

    // 1. Header / Basics
    write_docx_basics(&mut docx, &resume.basics);

    // 2. Sections in order
    let sections = ordered_sections(resume);
    for kind in sections {
        if is_section_empty(resume, kind) {
            continue;
        }

        // Section heading
        let heading_hidden = resume
            .section_overrides
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, o)| o.no_heading)
            .unwrap_or(false);

        if !heading_hidden {
            let title = resolve_section_title(resume, kind);
            if !title.trim().is_empty() {
                docx = docx.add_paragraph(
                    Paragraph::new()
                        .add_run(Run::new().add_text(title.to_uppercase()).bold().size(26)),
                );
            }
        }

        // Section content
        match kind {
            SectionKind::Profile => {
                // Profile summary is handled with basics
            }
            SectionKind::Work => {
                docx = write_docx_work(docx, &resume.work, date_format);
            }
            SectionKind::Education => {
                docx = write_docx_education(docx, &resume.education, date_format);
            }
            SectionKind::Skills => {
                docx = write_docx_skills(docx, &resume.skills);
            }
            SectionKind::Certificates => {
                docx = write_docx_certificates(docx, &resume.certificates, date_format);
            }
            SectionKind::Organizations => {
                docx = write_docx_volunteer(docx, &resume.volunteer, date_format);
            }
            SectionKind::Custom(id) => {
                if let Some(cs) = resume.custom_sections.iter().find(|s| s.id == id) {
                    docx = write_docx_custom(docx, cs, date_format);
                }
            }
        }
    }

    let mut buf = Cursor::new(Vec::new());
    docx.build().pack(&mut buf)?;
    Ok(buf.into_inner())
}

fn write_docx_basics(docx: &mut Docx, b: &Basics) {
    if !b.name.is_empty() {
        *docx = std::mem::take(docx).add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(&b.name).bold().size(36)),
        );
    }

    if !b.label.is_empty() {
        *docx = std::mem::take(docx).add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(&b.label).bold().size(24)),
        );
    }

    let mut contact_parts = Vec::new();
    if !b.email.is_empty() {
        contact_parts.push(b.email.clone());
    }
    if !b.phone.is_empty() {
        contact_parts.push(b.phone.clone());
    }
    if !b.location.is_empty() {
        contact_parts.push(b.location.clone());
    }
    if !b.url.is_empty() {
        contact_parts.push(b.url.clone());
    }

    if !contact_parts.is_empty() {
        let contact_line = contact_parts.join("  |  ");
        *docx = std::mem::take(docx).add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(contact_line).size(20)),
        );
    }

    if !b.profiles.is_empty() {
        let prof_parts: Vec<String> = b
            .profiles
            .iter()
            .map(|p| {
                if !p.url.is_empty() && !p.network.is_empty() {
                    format!("{}: {}", p.network, p.url)
                } else if !p.url.is_empty() {
                    p.url.clone()
                } else if !p.username.is_empty() {
                    format!("{}: {}", p.network, p.username)
                } else {
                    p.network.clone()
                }
            })
            .filter(|s| !s.is_empty())
            .collect();
        if !prof_parts.is_empty() {
            *docx = std::mem::take(docx).add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(prof_parts.join("  |  ")).size(20)),
            );
        }
    }

    if !b.summary.is_empty() {
        let clean_summary = strip_typst_markup(&b.summary);
        *docx = std::mem::take(docx).add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(clean_summary).size(22)),
        );
    }
}

fn write_docx_work(mut docx: Docx, work: &[Work], date_format: DateFormat) -> Docx {
    for w in work {
        let role = if !w.position.is_empty() && !w.name.is_empty() {
            format!("{}, {}", w.position, w.name)
        } else if !w.position.is_empty() {
            w.position.clone()
        } else {
            w.name.clone()
        };

        let mut title_run = Run::new().add_text(role).bold().size(22);
        if !w.location.is_empty() {
            title_run = title_run.add_text(format!(" ({})", w.location));
        }

        docx = docx.add_paragraph(Paragraph::new().add_run(title_run));

        let date_str = format_date_range(&w.start_date, &w.end_date, date_format);
        if !date_str.is_empty() {
            docx = docx.add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(date_str).italic().size(20)),
            );
        }

        if !w.summary.is_empty() {
            let clean = strip_typst_markup(&w.summary);
            docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(clean).size(22)));
        }

        for hl in &w.highlights {
            let clean = strip_typst_markup(hl);
            docx = docx.add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text("• ").size(22))
                    .add_run(Run::new().add_text(clean).size(22)),
            );
        }
    }
    docx
}

fn write_docx_education(mut docx: Docx, edu: &[Education], date_format: DateFormat) -> Docx {
    for e in edu {
        let heading = if !e.study_type.is_empty() && !e.institution.is_empty() {
            format!("{}, {}", e.study_type, e.institution)
        } else if !e.study_type.is_empty() {
            e.study_type.clone()
        } else {
            e.institution.clone()
        };

        docx = docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(heading).bold().size(22)),
        );

        let date_str = format_date_range(&e.start_date, &e.end_date, date_format);
        if !date_str.is_empty() {
            docx = docx.add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(date_str).italic().size(20)),
            );
        }

        if !e.url.is_empty() {
            docx = docx.add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(&e.url).size(20)),
            );
        }

        for hl in &e.highlights {
            let clean = strip_typst_markup(hl);
            docx = docx.add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text("• ").size(22))
                    .add_run(Run::new().add_text(clean).size(22)),
            );
        }
    }
    docx
}

fn write_docx_skills(mut docx: Docx, skills: &[SkillGroup]) -> Docx {
    for sg in skills {
        if sg.keywords.is_empty() {
            continue;
        }
        let kw_list = sg.keywords.join(", ");
        let p = if !sg.name.is_empty() {
            Paragraph::new()
                .add_run(Run::new().add_text(format!("{}: ", sg.name)).bold().size(22))
                .add_run(Run::new().add_text(kw_list).size(22))
        } else {
            Paragraph::new().add_run(Run::new().add_text(kw_list).size(22))
        };
        docx = docx.add_paragraph(p);
    }
    docx
}

fn write_docx_certificates(
    mut docx: Docx,
    certs: &[Certificate],
    date_format: DateFormat,
) -> Docx {
    for c in certs {
        let mut p = Paragraph::new().add_run(Run::new().add_text(&c.name).bold().size(22));
        if !c.issuer.is_empty() {
            p = p.add_run(Run::new().add_text(format!(" — {}", c.issuer)).size(22));
        }
        let date_str = c.date.display(date_format);
        if !date_str.is_empty() {
            p = p.add_run(
                Run::new()
                    .add_text(format!(" ({date_str})"))
                    .italic()
                    .size(20),
            );
        }
        if !c.url.is_empty() {
            p = p.add_run(Run::new().add_text(format!(" ({})", c.url)).size(20));
        }
        docx = docx.add_paragraph(p);
    }
    docx
}

fn write_docx_volunteer(mut docx: Docx, vol: &[Volunteer], date_format: DateFormat) -> Docx {
    for v in vol {
        let heading = if !v.position.is_empty() && !v.organization.is_empty() {
            format!("{}, {}", v.position, v.organization)
        } else if !v.position.is_empty() {
            v.position.clone()
        } else {
            v.organization.clone()
        };

        docx = docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(heading).bold().size(22)),
        );

        let date_str = format_date_range(&v.start_date, &v.end_date, date_format);
        if !date_str.is_empty() {
            docx = docx.add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(date_str).italic().size(20)),
            );
        }

        for hl in &v.highlights {
            let clean = strip_typst_markup(hl);
            docx = docx.add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text("• ").size(22))
                    .add_run(Run::new().add_text(clean).size(22)),
            );
        }
    }
    docx
}

fn write_docx_custom(
    mut docx: Docx,
    cs: &ComposedCustomSection,
    date_format: DateFormat,
) -> Docx {
    for e in &cs.entries {
        docx = write_docx_custom_entry(docx, e, date_format);
    }
    docx
}

fn write_docx_custom_entry(mut docx: Docx, e: &CustomEntry, date_format: DateFormat) -> Docx {
    let heading = if !e.title.is_empty() && !e.subtitle.is_empty() {
        format!("{} — {}", e.title, e.subtitle)
    } else if !e.title.is_empty() {
        e.title.clone()
    } else {
        e.subtitle.clone()
    };

    if !heading.is_empty() {
        docx = docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(heading).bold().size(22)),
        );
    }

    let date_str = format_date_range(&e.start_date, &e.end_date, date_format);
    if !date_str.is_empty() {
        docx = docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(date_str).italic().size(20)),
        );
    }

    if !e.url.is_empty() {
        docx = docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(&e.url).size(20)),
        );
    }

    for hl in &e.highlights {
        let clean = strip_typst_markup(hl);
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("• ").size(22))
                .add_run(Run::new().add_text(clean).size(22)),
        );
    }

    docx
}

fn format_date_range(start: &ResumeDate, end: &ResumeDate, date_format: DateFormat) -> String {
    let start_str = start.display(date_format);
    let end_str = end.display(date_format);
    if !start_str.is_empty() && !end_str.is_empty() {
        format!("{start_str} – {end_str}")
    } else if !start_str.is_empty() {
        start_str
    } else if !end_str.is_empty() {
        end_str
    } else {
        String::new()
    }
}

fn ordered_sections(resume: &Resume) -> Vec<SectionKind> {
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

fn is_section_empty(resume: &Resume, kind: SectionKind) -> bool {
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

fn resolve_section_title(resume: &Resume, kind: SectionKind) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::*;

    fn sample_resume() -> Resume {
        Resume {
            basics: Basics {
                name: "Alexey Belochenko".into(),
                label: "Principal Systems Architect".into(),
                summary: "Experienced *systems engineer* specializing in distributed storage."
                    .into(),
                email: "alexey@example.com".into(),
                phone: "+1 555-0199".into(),
                location: "San Francisco, CA".into(),
                url: "https://example.com".into(),
                profiles: vec![NetworkProfile {
                    network: "GitHub".into(),
                    username: "zeelex".into(),
                    url: "https://github.com/zeelex".into(),
                }],
            },
            work: vec![Work {
                name: "Tech Corp".into(),
                position: "Staff Software Engineer".into(),
                location: "Mountain View, CA".into(),
                start_date: ResumeDate::new("2021-03"),
                end_date: ResumeDate::new("Present"),
                summary: "Lead storage engine architecture.".into(),
                highlights: vec![
                    "Designed high-throughput commit log processing 50M ops/sec with sub-millisecond p99 latency."
                        .into(),
                    "Mentored 8 senior engineers and authored 5 architecture RFCs.".into(),
                ],
            }],
            education: vec![Education {
                institution: "State University".into(),
                study_type: "B.S. in Computer Science".into(),
                start_date: ResumeDate::new("2015-09"),
                end_date: ResumeDate::new("2019-06"),
                url: "https://university.edu".into(),
                highlights: vec!["Graduated Summa Cum Laude with honors.".into()],
            }],
            skills: vec![SkillGroup {
                name: "Languages".into(),
                keywords: vec!["Rust".into(), "C++".into(), "Go".into()],
            }],
            certificates: vec![Certificate {
                name: "AWS Solutions Architect".into(),
                issuer: "Amazon Web Services".into(),
                date: ResumeDate::new("2022-05"),
                url: "https://aws.amazon.com".into(),
            }],
            volunteer: vec![Volunteer {
                organization: "Open Source Collective".into(),
                position: "Core Maintainer".into(),
                start_date: ResumeDate::new("2020-01"),
                end_date: ResumeDate::new("Present"),
                highlights: vec!["Maintain high-performance networking libraries.".into()],
            }],
            custom_sections: vec![ComposedCustomSection {
                id: CustomSectionId::from_u32(1),
                title: "Publications".into(),
                entries: vec![CustomEntry {
                    title: "High Performance Storage in Rust".into(),
                    subtitle: "ACM Systems Conference".into(),
                    start_date: ResumeDate::new("2023-11"),
                    end_date: ResumeDate::new(""),
                    url: "https://doi.org/10.1145/example".into(),
                    highlights: vec!["Awarded Best Paper.".into()],
                }],
            }],
            section_titles: Vec::new(),
            section_overrides: Vec::new(),
            section_order: Vec::new(),
        }
    }

    #[test]
    fn docx_export_and_readback_verifies_headings_and_bullets() {
        let resume = sample_resume();
        let bytes = export_docx(&resume).expect("DOCX generation should succeed");
        assert!(!bytes.is_empty());

        let docx = docx_rs::read_docx(&bytes).expect("DOCX should be readable by docx-rs");
        let mut text_content = String::new();

        for child in docx.document.children {
            if let docx_rs::DocumentChild::Paragraph(p) = child {
                for p_child in p.children {
                    if let docx_rs::ParagraphChild::Run(r) = p_child {
                        for r_child in r.children {
                            if let docx_rs::RunChild::Text(t) = r_child {
                                text_content.push_str(&t.text);
                                text_content.push(' ');
                            }
                        }
                    }
                }
            }
        }

        assert!(text_content.contains("Alexey Belochenko"));
        assert!(text_content.contains("WORK EXPERIENCE"));
        assert!(text_content.contains("Staff Software Engineer"));
        assert!(text_content.contains("Designed high-throughput commit log"));
        assert!(text_content.contains("EDUCATION"));
        assert!(text_content.contains("State University"));
        assert!(text_content.contains("SKILLS"));
        assert!(text_content.contains("Rust"));
        assert!(text_content.contains("PUBLICATIONS"));
    }
}

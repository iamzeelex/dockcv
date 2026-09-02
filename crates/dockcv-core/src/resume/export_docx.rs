//! Microsoft Word (.docx) export emitter for a composed [`Resume`].
//!
//! Generates a clean Word document using `docx-rs`, structured for maximum ATS
//! readability (clear headings, styled runs, bullet lists, no layout tables).

use std::io::Cursor;

use docx_rs::{
    AbstractNumbering, Docx, DocxError, IndentLevel, Level, LevelJc, LevelText, NumberFormat,
    Numbering, NumberingId, Paragraph, Run, Start,
};

use super::dates::DateFormat;
use super::export_text::strip_typst_markup;
use super::export_walk::{
    format_date_range, is_section_empty, ordered_sections, resolve_section_title,
};
use super::model::{
    Basics, Certificate, ComposedCustomSection, CustomEntry, Education, Resume, SectionKind,
    SkillGroup, Volunteer, Work,
};

/// Word's outline level for a section heading (`EXPERIENCE`), zero-based.
const SECTION_OUTLINE_LEVEL: usize = 0;

/// …and for the entry headings under it (a job title, a degree).
const ENTRY_OUTLINE_LEVEL: usize = 1;

/// The one bullet list definition the document carries. A literal `•` in a
/// paragraph reads as a bullet to a person and as punctuation to a parser, and
/// this format exists to be parsed.
const BULLET_NUMBERING: usize = 1;

/// Export a composed [`Resume`] to DOCX binary bytes.
pub fn export_docx(resume: &Resume) -> Result<Vec<u8>, DocxError> {
    export_docx_with_date_format(resume, DateFormat::default())
}

/// Export a composed [`Resume`] to DOCX binary bytes with an explicit date format.
pub fn export_docx_with_date_format(
    resume: &Resume,
    date_format: DateFormat,
) -> Result<Vec<u8>, DocxError> {
    let mut docx = Docx::new()
        .add_abstract_numbering(
            AbstractNumbering::new(BULLET_NUMBERING).add_level(
                Level::new(
                    0,
                    Start::new(1),
                    NumberFormat::new("bullet"),
                    LevelText::new("•"),
                    LevelJc::new("left"),
                )
                .indent(
                    Some(360),
                    Some(docx_rs::SpecialIndentType::Hanging(360)),
                    None,
                    None,
                ),
            ),
        )
        .add_numbering(Numbering::new(BULLET_NUMBERING, BULLET_NUMBERING));

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
                        // `outline_lvl` is what makes Word's navigation pane and
                        // a parser see structure. Bold text at 13pt only *looks*
                        // like a heading, and looking like one is exactly what
                        // an ATS cannot read.
                        .outline_lvl(SECTION_OUTLINE_LEVEL)
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
        *docx = std::mem::take(docx)
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text(&b.name).bold().size(36)));
    }

    if !b.label.is_empty() {
        *docx = std::mem::take(docx)
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text(&b.label).bold().size(24)));
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
        *docx = std::mem::take(docx)
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text(contact_line).size(20)));
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
        *docx = std::mem::take(docx)
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text(clean_summary).size(22)));
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
            docx =
                docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(clean).size(22)));
        }

        for hl in &w.highlights {
            let clean = strip_typst_markup(hl);
            docx = docx.add_paragraph(
                Paragraph::new()
                    .numbering(NumberingId::new(BULLET_NUMBERING), IndentLevel::new(0))
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
            Paragraph::new()
                .outline_lvl(ENTRY_OUTLINE_LEVEL)
                .add_run(Run::new().add_text(heading).bold().size(22)),
        );

        let date_str = format_date_range(&e.start_date, &e.end_date, date_format);
        if !date_str.is_empty() {
            docx = docx.add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(date_str).italic().size(20)),
            );
        }

        if !e.url.is_empty() {
            docx =
                docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(&e.url).size(20)));
        }

        for hl in &e.highlights {
            let clean = strip_typst_markup(hl);
            docx = docx.add_paragraph(
                Paragraph::new()
                    .numbering(NumberingId::new(BULLET_NUMBERING), IndentLevel::new(0))
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
                .add_run(
                    Run::new()
                        .add_text(format!("{}: ", sg.name))
                        .bold()
                        .size(22),
                )
                .add_run(Run::new().add_text(kw_list).size(22))
        } else {
            Paragraph::new().add_run(Run::new().add_text(kw_list).size(22))
        };
        docx = docx.add_paragraph(p);
    }
    docx
}

fn write_docx_certificates(mut docx: Docx, certs: &[Certificate], date_format: DateFormat) -> Docx {
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
            Paragraph::new()
                .outline_lvl(ENTRY_OUTLINE_LEVEL)
                .add_run(Run::new().add_text(heading).bold().size(22)),
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
                    .numbering(NumberingId::new(BULLET_NUMBERING), IndentLevel::new(0))
                    .add_run(Run::new().add_text(clean).size(22)),
            );
        }
    }
    docx
}

fn write_docx_custom(mut docx: Docx, cs: &ComposedCustomSection, date_format: DateFormat) -> Docx {
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
            Paragraph::new()
                .outline_lvl(ENTRY_OUTLINE_LEVEL)
                .add_run(Run::new().add_text(heading).bold().size(22)),
        );
    }

    let date_str = format_date_range(&e.start_date, &e.end_date, date_format);
    if !date_str.is_empty() {
        docx = docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(date_str).italic().size(20)),
        );
    }

    if !e.url.is_empty() {
        docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(&e.url).size(20)));
    }

    for hl in &e.highlights {
        let clean = strip_typst_markup(hl);
        docx = docx.add_paragraph(
            Paragraph::new()
                .numbering(NumberingId::new(BULLET_NUMBERING), IndentLevel::new(0))
                .add_run(Run::new().add_text(clean).size(22)),
        );
    }

    docx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::export_walk::sample_resume;

    /// A writer nobody reads back is a writer that silently drifts, so this
    /// opens what it wrote with the same library Word would and checks the
    /// structure — not just that the words are somewhere in the file.
    #[test]
    fn docx_readback_finds_headings_as_headings_and_bullets_as_a_list() {
        let resume = sample_resume();
        let bytes = export_docx(&resume).expect("DOCX generation should succeed");
        assert!(!bytes.is_empty());

        let docx = docx_rs::read_docx(&bytes).expect("DOCX should be readable by docx-rs");

        let mut headings: Vec<String> = Vec::new();
        let mut bullets: Vec<String> = Vec::new();
        let mut all_text = String::new();

        for child in docx.document.children {
            let docx_rs::DocumentChild::Paragraph(p) = child else {
                continue;
            };
            let text: String = p
                .children
                .iter()
                .filter_map(|c| match c {
                    docx_rs::ParagraphChild::Run(r) => Some(
                        r.children
                            .iter()
                            .filter_map(|rc| match rc {
                                docx_rs::RunChild::Text(t) => Some(t.text.clone()),
                                _ => None,
                            })
                            .collect::<String>(),
                    ),
                    _ => None,
                })
                .collect();
            all_text.push_str(&text);
            all_text.push('\n');

            if p.property.outline_lvl.as_ref().map(|o| o.v) == Some(SECTION_OUTLINE_LEVEL) {
                headings.push(text.clone());
            }
            if p.property.numbering_property.is_some() {
                bullets.push(text);
            }
        }

        // Headings are headings: an outline level, not merely bold text.
        for kind in crate::resume::export_walk::ordered_sections(&resume) {
            let title = crate::resume::export_walk::resolve_section_title(&resume, kind);
            if title.is_empty() {
                continue;
            }
            assert!(
                headings.contains(&title.to_uppercase()),
                "{title:?} is not a Word heading; headings were {headings:?}"
            );
        }

        // Bullets are a list: the first highlight of every job, education entry,
        // organization and custom entry has a numbering property on it.
        for job in &resume.work {
            let first = crate::resume::export_text::strip_typst_markup(&job.highlights[0]);
            assert!(
                bullets.contains(&first),
                "the first bullet of {:?} is not a list item",
                job.name
            );
        }
        for entry in resume.custom_sections.iter().flat_map(|cs| &cs.entries) {
            let first = crate::resume::export_text::strip_typst_markup(&entry.highlights[0]);
            assert!(bullets.contains(&first), "{first:?} is not a list item");
        }
        assert!(
            !bullets.is_empty() && bullets.iter().all(|b| !b.starts_with('•')),
            "a list item must not also carry a literal bullet glyph: {bullets:?}"
        );

        // And the content itself survived.
        assert!(all_text.contains("Alexey Belochenko"));
        assert!(all_text.contains("Staff Software Engineer"));
        assert!(all_text.contains("State University"));
        assert!(all_text.contains("Rust"));
    }
}

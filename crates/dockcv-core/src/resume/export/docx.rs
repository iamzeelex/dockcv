//! Microsoft Word (.docx) export emitter for a composed [`Resume`].
//!
//! Generates a clean Word document using `docx-rs`, structured for maximum ATS
//! readability (clear headings, styled runs, bullet lists, no layout tables).

use std::io::Cursor;

use docx_rs::{
    AbstractNumbering, Docx, DocxError, Hyperlink, HyperlinkType, IndentLevel, Level, LevelJc,
    LevelText, NumberFormat, Numbering, NumberingId, Paragraph, Run, Start, Style, StyleType,
};

use crate::resume::dates::DateStyle;
use super::text::strip_typst_markup;
use super::walk::{
    format_date_range, is_section_empty, ordered_sections, resolve_section_title,
};
use crate::resume::links;
use crate::resume::model::{
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

// Word records what a paragraph *is* two ways, and readers do not agree on
// which one to look at. `outlineLvl` is what the navigation pane uses; a
// paragraph *style* named `Heading1` is what most parsers key on — including
// DockCV's own DOCX importer, which reads styles and never looked at the
// outline level this file used to carry alone. Writing both costs a few lines
// of styles.xml and makes the heading visible to either kind of reader.
const SECTION_STYLE: &str = "Heading1";
const ENTRY_STYLE: &str = "Heading2";
const BULLET_STYLE: &str = "ListParagraph";
// The name is the document's title, not its first section. A CV that styles it
// `Heading1` is one our own importer has to guess its way out of (`kind_of`),
// and so does everybody else's.
const NAME_STYLE: &str = "Title";

/// Export a composed [`Resume`] to DOCX binary bytes.
pub fn export_docx(resume: &Resume) -> Result<Vec<u8>, DocxError> {
    export_docx_in(resume, DateStyle::default())
}

/// Export a composed [`Resume`] to DOCX binary bytes with an explicit date format.
pub fn export_docx_in(
    resume: &Resume,
    dates: DateStyle,
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
        .add_numbering(Numbering::new(BULLET_NUMBERING, BULLET_NUMBERING))
        // Names are Word's own spelling of the built-in styles, because that is
        // what a reader matches on: python-docx reports `w:name`, not the id.
        // No formatting on any of them — the runs state theirs, and a style
        // that also set a size would change how the document looks.
        .add_style(
            Style::new(SECTION_STYLE, StyleType::Paragraph)
                .name("heading 1")
                .based_on("Normal"),
        )
        .add_style(
            Style::new(ENTRY_STYLE, StyleType::Paragraph)
                .name("heading 2")
                .based_on("Normal"),
        )
        .add_style(
            Style::new(BULLET_STYLE, StyleType::Paragraph)
                .name("List Paragraph")
                .based_on("Normal"),
        )
        .add_style(
            Style::new(NAME_STYLE, StyleType::Paragraph)
                .name("Title")
                .based_on("Normal"),
        );

    // 1. Header / Basics
    //
    // The summary's own heading travels with it rather than with the section
    // loop below: `ordered_sections` does not carry Profile, because the
    // summary is written here beside the contact block, and a heading emitted
    // in the loop would land *under* the paragraph it names. The page has
    // printed one all along (`template.rs::section("profile", …)`) and this
    // file printed none — a parser looking for where the summary starts found
    // nothing in the Word version of the same CV.
    let profile_heading = if section_heading_hidden(resume, SectionKind::Profile) {
        None
    } else {
        let title = resolve_section_title(resume, SectionKind::Profile);
        Some(title).filter(|t| !t.trim().is_empty())
    };
    write_docx_basics(&mut docx, &resume.basics, profile_heading.as_deref());

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
                        .style(SECTION_STYLE)
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
                docx = write_docx_work(docx, &resume.work, dates);
            }
            SectionKind::Education => {
                docx = write_docx_education(docx, &resume.education, dates);
            }
            SectionKind::Skills => {
                docx = write_docx_skills(docx, &resume.skills);
            }
            SectionKind::Certificates => {
                docx = write_docx_certificates(docx, &resume.certificates, dates);
            }
            SectionKind::Organizations => {
                docx = write_docx_volunteer(docx, &resume.volunteer, dates);
            }
            SectionKind::Custom(id) => {
                if let Some(cs) = resume.custom_sections.iter().find(|s| s.id == id) {
                    docx = write_docx_custom(docx, cs, dates);
                }
            }
        }
    }

    let mut buf = Cursor::new(Vec::new());
    docx.build().pack(&mut buf)?;
    Ok(buf.into_inner())
}

/// Whether the document asks for this section's heading to be left off.
fn section_heading_hidden(resume: &Resume, kind: SectionKind) -> bool {
    resume
        .section_overrides
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, o)| o.no_heading)
        .unwrap_or(false)
}

fn write_docx_basics(docx: &mut Docx, b: &Basics, profile_heading: Option<&str>) {
    if !b.name.is_empty() {
        *docx = std::mem::take(docx).add_paragraph(
            Paragraph::new()
                .style(NAME_STYLE)
                .add_run(Run::new().add_text(&b.name).bold().size(36)),
        );
    }

    if !b.label.is_empty() {
        *docx = std::mem::take(docx)
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text(&b.label).bold().size(24)));
    }

    // The contact row is the part of a CV a recruiter actually clicks, and in
    // DOCX it was flat text while the PDF's was live — a parity hole G1 left.
    // Each part carries its own target, so the address prints as written and
    // resolves as a URI (`resume::links`).
    let contact_parts: Vec<(String, Option<String>)> = [
        (b.email.clone(), links::mailto(&b.email)),
        (b.phone.clone(), links::tel(&b.phone)),
        (b.location.clone(), None),
        (b.url.clone(), links::href(&b.url)),
    ]
    .into_iter()
    .filter(|(shown, _)| !shown.is_empty())
    .collect();
    write_linked_line(docx, &contact_parts);

    let prof_parts: Vec<(String, Option<String>)> = b
        .profiles
        .iter()
        .map(|p| {
            let shown = if !p.url.is_empty() && !p.network.is_empty() {
                format!("{}: {}", p.network, p.url)
            } else if !p.url.is_empty() {
                p.url.clone()
            } else if !p.username.is_empty() {
                format!("{}: {}", p.network, p.username)
            } else {
                p.network.clone()
            };
            (shown, links::href(&p.url))
        })
        .filter(|(shown, _)| !shown.is_empty())
        .collect();
    write_linked_line(docx, &prof_parts);

    if !b.summary.is_empty() {
        if let Some(title) = profile_heading {
            *docx = std::mem::take(docx).add_paragraph(
                Paragraph::new()
                    .outline_lvl(SECTION_OUTLINE_LEVEL)
                    .style(SECTION_STYLE)
                    .add_run(Run::new().add_text(title.to_uppercase()).bold().size(26)),
            );
        }
        let clean_summary = strip_typst_markup(&b.summary);
        *docx = std::mem::take(docx)
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text(clean_summary).size(22)));
    }
}

/// One `a  |  b  |  c` line whose parts are hyperlinks where they have targets.
///
/// Built run by run rather than by joining strings first, because a hyperlink
/// in OOXML is a sibling of the runs around it, not a span inside one — the
/// separators have to be their own runs for the links to keep their own extent.
fn write_linked_line(docx: &mut Docx, parts: &[(String, Option<String>)]) {
    if parts.is_empty() {
        return;
    }
    let mut p = Paragraph::new();
    for (i, (shown, href)) in parts.iter().enumerate() {
        if i > 0 {
            p = p.add_run(Run::new().add_text("  |  ").size(20));
        }
        p = match href {
            Some(href) => p.add_hyperlink(
                Hyperlink::new(href, HyperlinkType::External)
                    .add_run(Run::new().add_text(shown).size(20)),
            ),
            None => p.add_run(Run::new().add_text(shown).size(20)),
        };
    }
    *docx = std::mem::take(docx).add_paragraph(p);
}

fn write_docx_work(mut docx: Docx, work: &[Work], dates: DateStyle) -> Docx {
    for w in work {
        let role = if !w.position.is_empty() && !w.name.is_empty() {
            format!("{}, {}", w.position, w.name)
        } else if !w.position.is_empty() {
            w.position.clone()
        } else {
            w.name.clone()
        };

        let mut p = Paragraph::new()
            .outline_lvl(ENTRY_OUTLINE_LEVEL)
            .style(ENTRY_STYLE);
        // `Hyperlink` writes the target into the relationship part
        // verbatim, so a bare `dtu.dk` becomes a *relative* target and
        // Word looks for a file of that name next to the document.
        if let Some(href) = links::href(&w.url) {
            p = p.add_hyperlink(
                Hyperlink::new(href, HyperlinkType::External)
                    .add_run(Run::new().add_text(role).bold().size(22)),
            );
        } else {
            p = p.add_run(Run::new().add_text(role).bold().size(22));
        }
        if !w.location.is_empty() {
            p = p.add_run(
                Run::new()
                    .add_text(format!(" ({})", w.location))
                    .bold()
                    .size(22),
            );
        }

        docx = docx.add_paragraph(p);

        let date_str = format_date_range(&w.start_date, &w.end_date, dates);
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
                    .style(BULLET_STYLE)
                    .style(BULLET_STYLE)
                    .add_run(Run::new().add_text(clean).size(22)),
            );
        }
    }
    docx
}

fn write_docx_education(mut docx: Docx, edu: &[Education], dates: DateStyle) -> Docx {
    for e in edu {
        let heading = if !e.study_type.is_empty() && !e.institution.is_empty() {
            format!("{}, {}", e.study_type, e.institution)
        } else if !e.study_type.is_empty() {
            e.study_type.clone()
        } else {
            e.institution.clone()
        };

        let mut p = Paragraph::new()
            .outline_lvl(ENTRY_OUTLINE_LEVEL)
            .style(ENTRY_STYLE);
        if let Some(href) = links::href(&e.url) {
            p = p.add_hyperlink(
                Hyperlink::new(href, HyperlinkType::External)
                    .add_run(Run::new().add_text(heading).bold().size(22)),
            );
        } else {
            p = p.add_run(Run::new().add_text(heading).bold().size(22));
        }

        docx = docx.add_paragraph(p);

        let date_str = format_date_range(&e.start_date, &e.end_date, dates);
        if !date_str.is_empty() {
            docx = docx.add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(date_str).italic().size(20)),
            );
        }

        for hl in &e.highlights {
            let clean = strip_typst_markup(hl);
            docx = docx.add_paragraph(
                Paragraph::new()
                    .numbering(NumberingId::new(BULLET_NUMBERING), IndentLevel::new(0))
                    .style(BULLET_STYLE)
                    .style(BULLET_STYLE)
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

fn write_docx_certificates(mut docx: Docx, certs: &[Certificate], dates: DateStyle) -> Docx {
    for c in certs {
        let mut p = Paragraph::new();
        if let Some(href) = links::href(&c.url) {
            p = p.add_hyperlink(
                Hyperlink::new(href, HyperlinkType::External)
                    .add_run(Run::new().add_text(&c.name).bold().size(22)),
            );
        } else {
            p = p.add_run(Run::new().add_text(&c.name).bold().size(22));
        }
        if !c.issuer.is_empty() {
            p = p.add_run(Run::new().add_text(format!(" — {}", c.issuer)).size(22));
        }
        let date_str = c.date.display_in(dates.format, dates.language);
        if !date_str.is_empty() {
            p = p.add_run(
                Run::new()
                    .add_text(format!(" ({date_str})"))
                    .italic()
                    .size(20),
            );
        }
        docx = docx.add_paragraph(p);
    }
    docx
}

fn write_docx_volunteer(mut docx: Docx, vol: &[Volunteer], dates: DateStyle) -> Docx {
    for v in vol {
        let heading = if !v.position.is_empty() && !v.organization.is_empty() {
            format!("{}, {}", v.position, v.organization)
        } else if !v.position.is_empty() {
            v.position.clone()
        } else {
            v.organization.clone()
        };

        let mut p = Paragraph::new()
            .outline_lvl(ENTRY_OUTLINE_LEVEL)
            .style(ENTRY_STYLE);
        if let Some(href) = links::href(&v.url) {
            p = p.add_hyperlink(
                Hyperlink::new(href, HyperlinkType::External)
                    .add_run(Run::new().add_text(heading).bold().size(22)),
            );
        } else {
            p = p.add_run(Run::new().add_text(heading).bold().size(22));
        }
        docx = docx.add_paragraph(p);

        let date_str = format_date_range(&v.start_date, &v.end_date, dates);
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
                    .style(BULLET_STYLE)
                    .style(BULLET_STYLE)
                    .add_run(Run::new().add_text(clean).size(22)),
            );
        }
    }
    docx
}

fn write_docx_custom(mut docx: Docx, cs: &ComposedCustomSection, dates: DateStyle) -> Docx {
    for e in &cs.entries {
        docx = write_docx_custom_entry(docx, e, dates);
    }
    docx
}

fn write_docx_custom_entry(mut docx: Docx, e: &CustomEntry, dates: DateStyle) -> Docx {
    let heading = if !e.title.is_empty() && !e.subtitle.is_empty() {
        format!("{} — {}", e.title, e.subtitle)
    } else if !e.title.is_empty() {
        e.title.clone()
    } else {
        e.subtitle.clone()
    };

    if !heading.is_empty() {
        let mut p = Paragraph::new()
            .outline_lvl(ENTRY_OUTLINE_LEVEL)
            .style(ENTRY_STYLE);
        if let Some(href) = links::href(&e.url) {
            p = p.add_hyperlink(
                Hyperlink::new(href, HyperlinkType::External)
                    .add_run(Run::new().add_text(heading).bold().size(22)),
            );
        } else {
            p = p.add_run(Run::new().add_text(heading).bold().size(22));
        }
        docx = docx.add_paragraph(p);
    }

    let date_str = format_date_range(&e.start_date, &e.end_date, dates);
    if !date_str.is_empty() {
        docx = docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(date_str).italic().size(20)),
        );
    }

    for hl in &e.highlights {
        let clean = strip_typst_markup(hl);
        docx = docx.add_paragraph(
            Paragraph::new()
                .numbering(NumberingId::new(BULLET_NUMBERING), IndentLevel::new(0))
                .style(BULLET_STYLE)
                .add_run(Run::new().add_text(clean).size(22)),
        );
    }

    docx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::export::walk::sample_resume;

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
            let mut text = String::new();
            for c in &p.children {
                match c {
                    docx_rs::ParagraphChild::Run(r) => {
                        for rc in &r.children {
                            if let docx_rs::RunChild::Text(t) = rc {
                                text.push_str(&t.text);
                            }
                        }
                    }
                    docx_rs::ParagraphChild::Hyperlink(h) => {
                        for rc in &h.children {
                            if let docx_rs::ParagraphChild::Run(r) = rc {
                                for rcc in &r.children {
                                    if let docx_rs::RunChild::Text(t) = rcc {
                                        text.push_str(&t.text);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
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
        for kind in crate::resume::export::walk::ordered_sections(&resume) {
            let title = crate::resume::export::walk::resolve_section_title(&resume, kind);
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
            let first = crate::resume::export::text::strip_typst_markup(&job.highlights[0]);
            assert!(
                bullets.contains(&first),
                "the first bullet of {:?} is not a list item",
                job.name
            );
        }
        for entry in resume.custom_sections.iter().flat_map(|cs| &cs.entries) {
            let first = crate::resume::export::text::strip_typst_markup(&entry.highlights[0]);
            assert!(bullets.contains(&first), "{first:?} is not a list item");
        }
        assert!(
            !bullets.is_empty() && bullets.iter().all(|b| !b.starts_with('•')),
            "a list item must not also carry a literal bullet glyph: {bullets:?}"
        );

        // And the content itself survived.
        assert!(all_text.contains("Albert Einstein"));
        assert!(all_text.contains("Staff Software Engineer"));
        assert!(all_text.contains("State University"));
        assert!(all_text.contains("Rust"));
    }
}

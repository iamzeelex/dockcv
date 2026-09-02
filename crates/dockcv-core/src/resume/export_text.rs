//! Plain text export emitter for a composed [`Resume`].
//!
//! A deterministic walk of the document in reading order: header first, then
//! each section in resolved `section_order`, hard-wrapped at 72 columns, with
//! no fancy glyphs or box-drawing characters.
//!
//! The wrapping itself is [`super::export_wrap`], which measures columns
//! rather than bytes and finds break opportunities in scripts that do not
//! separate words with spaces. This is the format an ATS is most likely to
//! read cleanly, and a CV is as often in Russian or Japanese as in English.

use std::fmt::Write as _;

use super::dates::DateFormat;
use super::export_walk::{
    format_date_range, is_section_empty, ordered_sections, resolve_section_title,
};
use super::export_wrap;
use super::model::{
    Basics, Certificate, ComposedCustomSection, CustomEntry, Education, Resume, SectionKind,
    SkillGroup, Volunteer, Work,
};

/// The column plain text wraps at.
///
/// Only prose reaches the wrapper — the summary and the bullets. Headings, job
/// titles and dates are short by construction, so this number is a decision
/// about reading paragraphs and nothing else.
///
/// Three things make it 72 rather than something else:
///
/// * **Measure.** Prose reads best somewhere between 45 and 75 characters a
///   line. A bullet is indented four columns, so 72 gives it a measure of 68 —
///   inside the band, with the summary at 72 near its top.
/// * **Slack under 80.** Eight columns is what a `> ` quote prefix costs when
///   this file is pasted into a reply, plus the column a terminal keeps for its
///   cursor. It is why 72 is the number in RFC 2822 and in the git commit
///   convention, and the reason applies here unchanged.
/// * **Scripts that are two columns wide.** At 72 a Japanese line holds 36
///   characters, which is close to the norm for Japanese body text. That fell
///   out of measuring width instead of bytes (see [`super::export_wrap`]) and is
///   worth keeping in mind before anyone tunes this number for English alone.
///
/// Deliberately not a setting. It is one more control on a screen that has
/// plenty, for a decision almost nobody holds an opinion about, and a CV that
/// wraps differently from the one you sent last week is a difference nobody
/// asked for.
pub const WRAP_WIDTH: usize = 72;

/// Export a composed [`Resume`] into clean, plain-text format.
pub fn export_plain_text(resume: &Resume) -> String {
    export_plain_text_with_date_format(resume, DateFormat::default())
}

/// Export a composed [`Resume`] with an explicit date format.
pub fn export_plain_text_with_date_format(resume: &Resume, date_format: DateFormat) -> String {
    let mut out = String::new();

    // 1. Header / Basics
    write_basics(&mut out, &resume.basics);

    // 2. Sections in order
    let sections = ordered_sections(resume);
    for kind in sections {
        if is_section_empty(resume, kind) {
            continue;
        }

        // Section header
        let heading_hidden = resume
            .section_overrides
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, o)| o.no_heading)
            .unwrap_or(false);

        if !heading_hidden {
            let title = resolve_section_title(resume, kind);
            if !title.trim().is_empty() {
                out.push('\n');
                let upper_title = title.to_uppercase();
                let _ = writeln!(out, "{upper_title}");
                let _ = writeln!(out, "{}", "-".repeat(upper_title.len()));
            }
        }

        // Section content
        match kind {
            SectionKind::Profile => {
                // Profile summary is rendered with basics; if there is additional text, handle it here
            }
            SectionKind::Work => {
                write_work_section(&mut out, &resume.work, date_format);
            }
            SectionKind::Education => {
                write_education_section(&mut out, &resume.education, date_format);
            }
            SectionKind::Skills => {
                write_skills_section(&mut out, &resume.skills);
            }
            SectionKind::Certificates => {
                write_certificates_section(&mut out, &resume.certificates, date_format);
            }
            SectionKind::Organizations => {
                write_volunteer_section(&mut out, &resume.volunteer, date_format);
            }
            SectionKind::Custom(id) => {
                if let Some(cs) = resume.custom_sections.iter().find(|s| s.id == id) {
                    write_custom_section(&mut out, cs, date_format);
                }
            }
        }
    }

    out
}

fn write_basics(out: &mut String, b: &Basics) {
    if !b.name.is_empty() {
        let _ = writeln!(out, "{}", b.name);
    }
    if !b.label.is_empty() {
        let _ = writeln!(out, "{}", b.label);
    }

    // Contact line
    let mut contacts = Vec::new();
    if !b.email.is_empty() {
        contacts.push(b.email.as_str());
    }
    if !b.phone.is_empty() {
        contacts.push(b.phone.as_str());
    }
    if !b.location.is_empty() {
        contacts.push(b.location.as_str());
    }
    if !b.url.is_empty() {
        contacts.push(b.url.as_str());
    }

    if !contacts.is_empty() {
        let contact_line = contacts.join(" | ");
        write_wrapped(out, &contact_line, 0, 0);
    }

    for p in &b.profiles {
        let mut prof_parts = Vec::new();
        if !p.network.is_empty() {
            prof_parts.push(p.network.as_str());
        }
        if !p.username.is_empty() {
            prof_parts.push(p.username.as_str());
        }
        if !p.url.is_empty() {
            prof_parts.push(p.url.as_str());
        }
        if !prof_parts.is_empty() {
            write_wrapped(out, &prof_parts.join(": "), 0, 0);
        }
    }

    if !b.summary.is_empty() {
        out.push('\n');
        let clean_summary = strip_typst_markup(&b.summary);
        write_wrapped(out, &clean_summary, 0, 0);
    }
}

fn write_work_section(out: &mut String, work: &[Work], date_format: DateFormat) {
    for (i, w) in work.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let role = if !w.position.is_empty() && !w.name.is_empty() {
            format!("{}, {}", w.position, w.name)
        } else if !w.position.is_empty() {
            w.position.clone()
        } else {
            w.name.clone()
        };

        let mut line = role;
        if !w.location.is_empty() {
            line.push_str(&format!(" ({})", w.location));
        }
        write_wrapped(out, &line, 0, 0);

        let date_str = format_date_range(&w.start_date, &w.end_date, date_format);
        if !date_str.is_empty() {
            let _ = writeln!(out, "{date_str}");
        }

        if !w.summary.is_empty() {
            let clean = strip_typst_markup(&w.summary);
            write_wrapped(out, &clean, 0, 0);
        }

        for hl in &w.highlights {
            let clean = strip_typst_markup(hl);
            write_bullet(out, &clean);
        }
    }
}

fn write_education_section(out: &mut String, edu: &[Education], date_format: DateFormat) {
    for (i, e) in edu.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let heading = if !e.study_type.is_empty() && !e.institution.is_empty() {
            format!("{}, {}", e.study_type, e.institution)
        } else if !e.study_type.is_empty() {
            e.study_type.clone()
        } else {
            e.institution.clone()
        };

        write_wrapped(out, &heading, 0, 0);

        let date_str = format_date_range(&e.start_date, &e.end_date, date_format);
        if !date_str.is_empty() {
            let _ = writeln!(out, "{date_str}");
        }

        if !e.url.is_empty() {
            let _ = writeln!(out, "{}", e.url);
        }

        for hl in &e.highlights {
            let clean = strip_typst_markup(hl);
            write_bullet(out, &clean);
        }
    }
}

fn write_skills_section(out: &mut String, skills: &[SkillGroup]) {
    for sg in skills {
        if sg.keywords.is_empty() {
            continue;
        }
        let kw_list = sg.keywords.join(", ");
        if !sg.name.is_empty() {
            let text = format!("{}: {}", sg.name, kw_list);
            write_wrapped(out, &text, 0, 2);
        } else {
            write_wrapped(out, &kw_list, 0, 0);
        }
    }
}

fn write_certificates_section(out: &mut String, certs: &[Certificate], date_format: DateFormat) {
    for c in certs {
        let mut line = c.name.clone();
        if !c.issuer.is_empty() {
            line.push_str(&format!(" - {}", c.issuer));
        }
        let date_str = c.date.display(date_format);
        if !date_str.is_empty() {
            line.push_str(&format!(" ({date_str})"));
        }
        write_wrapped(out, &line, 0, 0);
        if !c.url.is_empty() {
            let _ = writeln!(out, "  {}", c.url);
        }
    }
}

fn write_volunteer_section(out: &mut String, vol: &[Volunteer], date_format: DateFormat) {
    for (i, v) in vol.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let heading = if !v.position.is_empty() && !v.organization.is_empty() {
            format!("{}, {}", v.position, v.organization)
        } else if !v.position.is_empty() {
            v.position.clone()
        } else {
            v.organization.clone()
        };

        write_wrapped(out, &heading, 0, 0);

        let date_str = format_date_range(&v.start_date, &v.end_date, date_format);
        if !date_str.is_empty() {
            let _ = writeln!(out, "{date_str}");
        }

        for hl in &v.highlights {
            let clean = strip_typst_markup(hl);
            write_bullet(out, &clean);
        }
    }
}

fn write_custom_section(out: &mut String, cs: &ComposedCustomSection, date_format: DateFormat) {
    for (i, e) in cs.entries.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        write_custom_entry(out, e, date_format);
    }
}

fn write_custom_entry(out: &mut String, e: &CustomEntry, date_format: DateFormat) {
    let heading = if !e.title.is_empty() && !e.subtitle.is_empty() {
        format!("{} - {}", e.title, e.subtitle)
    } else if !e.title.is_empty() {
        e.title.clone()
    } else {
        e.subtitle.clone()
    };

    if !heading.is_empty() {
        write_wrapped(out, &heading, 0, 0);
    }

    let date_str = format_date_range(&e.start_date, &e.end_date, date_format);
    if !date_str.is_empty() {
        let _ = writeln!(out, "{date_str}");
    }

    if !e.url.is_empty() {
        let _ = writeln!(out, "{}", e.url);
    }

    for hl in &e.highlights {
        let clean = strip_typst_markup(hl);
        write_bullet(out, &clean);
    }
}

/// Strip common Typst formatting markup (`*`, `_`, `#link(..)[..]`, etc.)
pub fn strip_typst_markup(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Check for escaped characters
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                '*' | '_' | '#' | '$' | '[' | ']' | '\\' => {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                ' ' => {
                    out.push(' ');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }

        // Check for #link("...") or #link("...")[...]
        if chars[i] == '#' && input[i..].starts_with("#link(") {
            // Find closing paren of link
            if let Some(close_paren) = input[i..].find(')') {
                let link_end = i + close_paren;
                let url_content = input[i + 6..link_end].trim_matches('"');
                if link_end + 1 < input.len() && input[link_end + 1..].starts_with('[') {
                    // It has [label]
                    if let Some(close_bracket) = input[link_end + 1..].find(']') {
                        let label = &input[link_end + 2..link_end + 1 + close_bracket];
                        if label == url_content {
                            out.push_str(label);
                        } else {
                            out.push_str(&format!("{label} ({url_content})"));
                        }
                        i = link_end + 2 + close_bracket;
                        continue;
                    }
                } else {
                    out.push_str(url_content);
                    i = link_end + 1;
                    continue;
                }
            }
        }

        // Strip bold / italic delimiters
        if chars[i] == '*' || chars[i] == '_' {
            i += 1;
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

/// A bullet, hard-wrapped with its continuation lines hanging under the text
/// rather than under the marker.
fn write_bullet(out: &mut String, text: &str) {
    export_wrap::wrap_into(out, text, "  * ", 4, WRAP_WIDTH);
}

/// A paragraph, hard-wrapped, with `first_indent` spaces on its first line and
/// `rest_indent` on the rest.
fn write_wrapped(out: &mut String, text: &str, first_indent: usize, rest_indent: usize) {
    let first_prefix = " ".repeat(first_indent);
    export_wrap::wrap_into(out, text, &first_prefix, rest_indent, WRAP_WIDTH);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::export_walk::sample_resume;
    use crate::resume::model::*;

    #[test]
    fn plain_text_export_contains_all_sections_and_respects_wrap_width() {
        let resume = sample_resume();
        let exported = export_plain_text(&resume);

        assert!(exported.contains("Albert Einstein"));
        assert!(exported.contains("Principal Systems Architect"));
        assert!(exported.contains("albert@example.com"));
        assert!(exported.contains("WORK EXPERIENCE"));
        assert!(exported.contains("EDUCATION"));
        assert!(exported.contains("SKILLS"));
        assert!(exported.contains("CERTIFICATIONS"));
        assert!(exported.contains("ORGANIZATIONS"));
        assert!(exported.contains("PUBLICATIONS"));

        assert_within_the_column(&exported);
    }

    /// The same document in Russian and Japanese. Byte length would put the
    /// first at roughly half the column and the second at a third of it, so
    /// this is the test that fails if the wrapping ever goes back to counting
    /// bytes — and it checks the whole emitter, not just the wrapper.
    #[test]
    fn a_cv_that_is_not_in_english_still_fits_the_column() {
        let mut resume = sample_resume();
        resume.basics.name = "Альберт Эйнштейн".into();
        resume.basics.label = "Ведущий системный архитектор".into();
        resume.basics.summary = "Опытный системный инженер, специализирующийся на распределённых \
             хранилищах данных и высоконагруженных системах."
            .into();
        resume.work[0].highlights = vec![
            "Спроектировал журнал фиксации, обрабатывающий пятьдесят миллионов \
             операций в секунду с задержкой p99 менее одной миллисекунды."
                .into(),
            "分散ストレージシステムの設計と実装を担当し、毎秒五千万件の書き込みを処理する\
             高性能なコミットログを構築しました。"
                .into(),
        ];

        let exported = export_plain_text(&resume);
        assert!(exported.contains("Альберт Эйнштейн"));
        assert!(exported.contains("Спроектировал"));
        assert_within_the_column(&exported);
    }

    #[track_caller]
    fn assert_within_the_column(exported: &str) {
        for (i, line) in exported.lines().enumerate() {
            let columns = crate::resume::export_wrap::width(line);
            assert!(
                columns <= WRAP_WIDTH,
                "line {i} is {columns} columns wide, over {WRAP_WIDTH}: {line:?}"
            );
        }
    }

    #[test]
    fn plain_text_export_respects_section_order_and_overrides() {
        let mut resume = sample_resume();
        resume.section_order = vec![
            SectionKind::Skills,
            SectionKind::Work,
            SectionKind::Custom(CustomSectionId::from_u32(1)),
        ];
        resume.section_titles = vec![(SectionKind::Work, "Relevant Experience".into())];

        let exported = export_plain_text(&resume);
        let skills_pos = exported.find("SKILLS").unwrap();
        let work_pos = exported.find("RELEVANT EXPERIENCE").unwrap();
        let pub_pos = exported.find("PUBLICATIONS").unwrap();

        assert!(skills_pos < work_pos);
        assert!(work_pos < pub_pos);
    }

    #[test]
    fn typst_markup_is_stripped_cleanly() {
        assert_eq!(
            strip_typst_markup("Experienced *systems engineer* with _distributed_ systems."),
            "Experienced systems engineer with distributed systems."
        );
        assert_eq!(
            strip_typst_markup(r#"Read \#1 and \$2 in \*bold\*"#),
            "Read #1 and $2 in *bold*"
        );
        assert_eq!(
            strip_typst_markup(r#"Visit #link("https://dockcv.com")[DockCV]"#),
            "Visit DockCV (https://dockcv.com)"
        );
    }
}

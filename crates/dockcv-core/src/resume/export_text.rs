//! Plain text export emitter for a composed [`Resume`].
//!
//! A deterministic walk of the document in reading order: header first, then
//! each section in resolved `section_order`, hard-wrapped at 72 columns, with
//! no fancy glyphs or box-drawing characters.

use std::fmt::Write as _;

use super::dates::DateFormat;
use super::export_walk::{
    format_date_range, is_section_empty, ordered_sections, resolve_section_title,
};
use super::model::{
    Basics, Certificate, ComposedCustomSection, CustomEntry, Education, Resume, SectionKind,
    SkillGroup, Volunteer, Work,
};

/// The hard-wrap column limit for plain-text export.
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

fn write_bullet(out: &mut String, text: &str) {
    let prefix = "  * ";
    let hanging_indent = 4;
    write_wrapped_with_prefix(out, text, prefix, hanging_indent);
}

fn write_wrapped(out: &mut String, text: &str, first_indent: usize, rest_indent: usize) {
    let first_prefix = " ".repeat(first_indent);
    write_wrapped_with_prefix(out, text, &first_prefix, rest_indent);
}

fn write_wrapped_with_prefix(out: &mut String, text: &str, first_prefix: &str, rest_indent: usize) {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return;
    }

    let rest_prefix = " ".repeat(rest_indent);
    let mut current_line = String::from(first_prefix);

    for word in words {
        let line_len_with_word = if current_line.trim().is_empty() {
            current_line.len() + word.len()
        } else {
            current_line.len() + 1 + word.len()
        };

        if line_len_with_word > WRAP_WIDTH && !current_line.trim().is_empty() {
            let _ = writeln!(out, "{current_line}");
            current_line = rest_prefix.clone();
            current_line.push_str(word);
        } else {
            if !current_line.trim().is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        }
    }

    if !current_line.trim().is_empty() {
        let _ = writeln!(out, "{current_line}");
    }
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

        // Verify that no line exceeds WRAP_WIDTH
        for (i, line) in exported.lines().enumerate() {
            assert!(
                line.len() <= WRAP_WIDTH,
                "Line {i} exceeds wrap width ({}/{}): {:?}",
                line.len(),
                WRAP_WIDTH,
                line
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

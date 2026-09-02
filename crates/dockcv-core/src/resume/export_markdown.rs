//! Markdown export emitter for a composed [`Resume`].
//!
//! Converts a composed [`Resume`] to clean GitHub-Flavored Markdown, translating
//! Typst inline markup into standard Markdown and formatting sections according
//! to `section_order`, `section_titles`, and `no_heading` overrides.

use std::fmt::Write as _;

use super::dates::DateFormat;
use super::export_walk::{
    format_date_range, is_section_empty, ordered_sections, resolve_section_title,
};
use super::model::{
    Basics, Certificate, ComposedCustomSection, CustomEntry, Education, Resume, SectionKind,
    SkillGroup, Volunteer, Work,
};

/// Export a composed [`Resume`] to GitHub-Flavored Markdown.
pub fn export_markdown(resume: &Resume) -> String {
    export_markdown_with_date_format(resume, DateFormat::default())
}

/// Export a composed [`Resume`] to GitHub-Flavored Markdown with an explicit date format.
pub fn export_markdown_with_date_format(resume: &Resume, date_format: DateFormat) -> String {
    let mut out = String::new();

    // 1. Header / Basics
    write_markdown_basics(&mut out, &resume.basics);

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
                let _ = writeln!(out, "## {title}");
                out.push('\n');
            }
        }

        // Section content
        match kind {
            SectionKind::Profile => {
                // Profile summary is handled with basics
            }
            SectionKind::Work => {
                write_markdown_work(&mut out, &resume.work, date_format);
            }
            SectionKind::Education => {
                write_markdown_education(&mut out, &resume.education, date_format);
            }
            SectionKind::Skills => {
                write_markdown_skills(&mut out, &resume.skills);
            }
            SectionKind::Certificates => {
                write_markdown_certificates(&mut out, &resume.certificates, date_format);
            }
            SectionKind::Organizations => {
                write_markdown_volunteer(&mut out, &resume.volunteer, date_format);
            }
            SectionKind::Custom(id) => {
                if let Some(cs) = resume.custom_sections.iter().find(|s| s.id == id) {
                    write_markdown_custom(&mut out, cs, date_format);
                }
            }
        }
    }

    out
}

fn write_markdown_basics(out: &mut String, b: &Basics) {
    if !b.name.is_empty() {
        let _ = writeln!(out, "# {}", b.name);
    }
    if !b.label.is_empty() {
        let _ = writeln!(out, "**{}**", b.label);
    }

    // Contact line
    let mut contacts = Vec::new();
    if !b.email.is_empty() {
        contacts.push(format!("[{}]({})", b.email, format_mailto(&b.email)));
    }
    if !b.phone.is_empty() {
        contacts.push(format!("[{}]({})", b.phone, format_tel(&b.phone)));
    }
    if !b.location.is_empty() {
        contacts.push(b.location.clone());
    }
    if !b.url.is_empty() {
        contacts.push(format!("[{}]({})", b.url, b.url));
    }

    if !contacts.is_empty() {
        let _ = writeln!(out, "{}", contacts.join(" · "));
    }

    if !b.profiles.is_empty() {
        let prof_links: Vec<String> = b
            .profiles
            .iter()
            .map(|p| {
                if !p.url.is_empty() && !p.network.is_empty() {
                    format!("[{}]({})", p.network, p.url)
                } else if !p.url.is_empty() {
                    format!("[{}]({})", p.url, p.url)
                } else if !p.username.is_empty() {
                    format!("{}: {}", p.network, p.username)
                } else {
                    p.network.clone()
                }
            })
            .filter(|s| !s.is_empty())
            .collect();
        if !prof_links.is_empty() {
            let _ = writeln!(out, "{}", prof_links.join(" · "));
        }
    }

    if !b.summary.is_empty() {
        out.push('\n');
        let md_summary = typst_to_markdown(&b.summary);
        let _ = writeln!(out, "{md_summary}");
    }
}

fn format_mailto(email: &str) -> String {
    if email.starts_with("mailto:") {
        email.to_string()
    } else {
        format!("mailto:{email}")
    }
}

fn format_tel(phone: &str) -> String {
    if phone.starts_with("tel:") {
        phone.to_string()
    } else {
        format!(
            "tel:{}",
            phone
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '(' && *c != ')' && *c != '-')
                .collect::<String>()
        )
    }
}

fn write_markdown_work(out: &mut String, work: &[Work], date_format: DateFormat) {
    for (i, w) in work.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let role = if !w.position.is_empty() && !w.name.is_empty() {
            format!("### {}, {}", w.position, w.name)
        } else if !w.position.is_empty() {
            format!("### {}", w.position)
        } else {
            format!("### {}", w.name)
        };

        let mut heading = role;
        if !w.location.is_empty() {
            heading.push_str(&format!(" ({})", w.location));
        }
        let _ = writeln!(out, "{heading}");

        let date_str = format_date_range(&w.start_date, &w.end_date, date_format);
        if !date_str.is_empty() {
            let _ = writeln!(out, "*{date_str}*\n");
        } else {
            out.push('\n');
        }

        if !w.summary.is_empty() {
            let md = typst_to_markdown(&w.summary);
            let _ = writeln!(out, "{md}\n");
        }

        for hl in &w.highlights {
            let md = typst_to_markdown(hl);
            let _ = writeln!(out, "- {md}");
        }
    }
}

fn write_markdown_education(out: &mut String, edu: &[Education], date_format: DateFormat) {
    for (i, e) in edu.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let heading = if !e.study_type.is_empty() && !e.institution.is_empty() {
            format!("### {}, {}", e.study_type, e.institution)
        } else if !e.study_type.is_empty() {
            format!("### {}", e.study_type)
        } else {
            format!("### {}", e.institution)
        };

        let _ = writeln!(out, "{heading}");

        let date_str = format_date_range(&e.start_date, &e.end_date, date_format);
        if !date_str.is_empty() {
            let _ = writeln!(out, "*{date_str}*\n");
        } else {
            out.push('\n');
        }

        if !e.url.is_empty() {
            let _ = writeln!(out, "[{}]({})\n", e.url, e.url);
        }

        for hl in &e.highlights {
            let md = typst_to_markdown(hl);
            let _ = writeln!(out, "- {md}");
        }
    }
}

fn write_markdown_skills(out: &mut String, skills: &[SkillGroup]) {
    for sg in skills {
        if sg.keywords.is_empty() {
            continue;
        }
        let kw_list = sg.keywords.join(", ");
        if !sg.name.is_empty() {
            let _ = writeln!(out, "- **{}:** {kw_list}", sg.name);
        } else {
            let _ = writeln!(out, "- {kw_list}");
        }
    }
}

fn write_markdown_certificates(out: &mut String, certs: &[Certificate], date_format: DateFormat) {
    for c in certs {
        let mut line = format!("- **{}**", c.name);
        if !c.issuer.is_empty() {
            line.push_str(&format!(" — {}", c.issuer));
        }
        let date_str = c.date.display(date_format);
        if !date_str.is_empty() {
            line.push_str(&format!(" (*{date_str}*)"));
        }
        if !c.url.is_empty() {
            line.push_str(&format!(" [Link]({})", c.url));
        }
        let _ = writeln!(out, "{line}");
    }
}

fn write_markdown_volunteer(out: &mut String, vol: &[Volunteer], date_format: DateFormat) {
    for (i, v) in vol.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let heading = if !v.position.is_empty() && !v.organization.is_empty() {
            format!("### {}, {}", v.position, v.organization)
        } else if !v.position.is_empty() {
            format!("### {}", v.position)
        } else {
            format!("### {}", v.organization)
        };

        let _ = writeln!(out, "{heading}");

        let date_str = format_date_range(&v.start_date, &v.end_date, date_format);
        if !date_str.is_empty() {
            let _ = writeln!(out, "*{date_str}*\n");
        } else {
            out.push('\n');
        }

        for hl in &v.highlights {
            let md = typst_to_markdown(hl);
            let _ = writeln!(out, "- {md}");
        }
    }
}

fn write_markdown_custom(out: &mut String, cs: &ComposedCustomSection, date_format: DateFormat) {
    for (i, e) in cs.entries.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        write_markdown_custom_entry(out, e, date_format);
    }
}

fn write_markdown_custom_entry(out: &mut String, e: &CustomEntry, date_format: DateFormat) {
    let heading = if !e.title.is_empty() && !e.subtitle.is_empty() {
        format!("### {} — {}", e.title, e.subtitle)
    } else if !e.title.is_empty() {
        format!("### {}", e.title)
    } else {
        format!("### {}", e.subtitle)
    };

    if !heading.is_empty() {
        let _ = writeln!(out, "{heading}");
    }

    let date_str = format_date_range(&e.start_date, &e.end_date, date_format);
    if !date_str.is_empty() {
        let _ = writeln!(out, "*{date_str}*\n");
    } else {
        out.push('\n');
    }

    if !e.url.is_empty() {
        let _ = writeln!(out, "[{}]({})\n", e.url, e.url);
    }

    for hl in &e.highlights {
        let md = typst_to_markdown(hl);
        let _ = writeln!(out, "- {md}");
    }
}

/// Convert Typst inline markup (`*bold*`, `_italic_`, `#link("url")[label]`) to Markdown.
pub fn typst_to_markdown(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Escaped characters
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

        // #link("...") or #link("...")[...]
        if chars[i] == '#' && input[i..].starts_with("#link(") {
            if let Some(close_paren) = input[i..].find(')') {
                let link_end = i + close_paren;
                let url_content = input[i + 6..link_end].trim_matches('"');
                if link_end + 1 < input.len() && input[link_end + 1..].starts_with('[') {
                    if let Some(close_bracket) = input[link_end + 1..].find(']') {
                        let label = &input[link_end + 2..link_end + 1 + close_bracket];
                        out.push_str(&format!("[{label}]({url_content})"));
                        i = link_end + 2 + close_bracket;
                        continue;
                    }
                } else {
                    out.push_str(&format!("[{url_content}]({url_content})"));
                    i = link_end + 1;
                    continue;
                }
            }
        }

        // Convert Typst bold `*word*` to Markdown `**word**`
        if chars[i] == '*' {
            out.push_str("**");
            i += 1;
            continue;
        }

        // Convert Typst italic `_word_` to Markdown `*word*`
        if chars[i] == '_' {
            out.push('*');
            i += 1;
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::export_walk::sample_resume;
    use crate::resume::model::*;

    #[test]
    fn markdown_export_contains_all_markdown_headers_and_links() {
        let resume = sample_resume();
        let md = export_markdown(&resume);

        assert!(md.contains("# Alexey Belochenko"));
        assert!(md.contains("**Principal Systems Architect**"));
        assert!(md.contains("[alexey@example.com](mailto:alexey@example.com)"));
        assert!(md.contains("[GitHub](https://github.com/zeelex)"));
        assert!(md.contains("## Work Experience"));
        assert!(md.contains("### Staff Software Engineer, Tech Corp (Mountain View, CA)"));
        assert!(md.contains("[DockCV](https://dockcv.com)"));
        assert!(md.contains("## Education"));
        assert!(md.contains("### B.S. in Computer Science, State University"));
        assert!(md.contains("## Skills"));
        assert!(md.contains("- **Languages:** Rust, C++, Go"));
        assert!(md.contains("## Certifications"));
        assert!(md.contains("- **AWS Solutions Architect** — Amazon Web Services"));
        assert!(md.contains("## Organizations"));
        assert!(md.contains("### Core Maintainer, Open Source Collective"));
        assert!(md.contains("## Publications"));
        assert!(md.contains("### High Performance Storage in Rust — ACM Systems Conference"));
    }

    #[test]
    fn typst_to_markdown_conversions() {
        assert_eq!(
            typst_to_markdown("Build *fast* and _reliable_ systems."),
            "Build **fast** and *reliable* systems."
        );
        assert_eq!(
            typst_to_markdown(r#"See #link("https://example.com")[Example] for info."#),
            "See [Example](https://example.com) for info."
        );
        assert_eq!(
            typst_to_markdown(r#"Escaped \*star\* and \_underscore\_"#),
            "Escaped *star* and _underscore_"
        );
    }

    #[test]
    fn markdown_export_respects_custom_order_and_titles() {
        let mut resume = sample_resume();
        resume.section_order = vec![SectionKind::Skills, SectionKind::Work];
        resume.section_titles = vec![(SectionKind::Work, "Selected Work".into())];

        let md = export_markdown(&resume);
        let skills_idx = md.find("## Skills").unwrap();
        let work_idx = md.find("## Selected Work").unwrap();
        assert!(skills_idx < work_idx);
    }
}

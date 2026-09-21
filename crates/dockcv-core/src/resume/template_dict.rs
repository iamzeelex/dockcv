//! The model, serialized back into a Typst dictionary.
//!
//! Split from `template.rs` (C15). This half turns a composed [`Resume`] into
//! the `#let cv = (..)` literal `renderer.typ` reads, and owns the writers
//! underneath it — quoting, indenting, and the one piece of real judgement
//! here, which is [`neutralize_into`]: what of an author's markup reaches the
//! page as markup and what is escaped.


use crate::resume::links;
use crate::resume::model::{DateFormat, DocumentLanguage, Resume, ResumeDoc, SectionKind};

use super::template_page::{renderer_key, section_key};

pub(super) fn resume_to_dict_into(s: &mut String, r: &Resume, dates: DateFormat, language: DocumentLanguage) {
    s.push_str("(\n");

    // --- basics ---
    //
    // `(:)` rather than `(`…`)`: in Typst an empty *dictionary* is `(:)` and
    // `()` is an empty **array**. A document whose profile has no fields yet —
    // a blank CV, which is exactly what "Skip — start blank" produces —
    // emitted `basics: (\n)`, so the renderer's `cv.basics.at("name", …)`
    // indexed an array with a string and the preview failed to compile with
    // "expected integer, found string". Pre-existing; found while chasing the
    // section-order bug, because nothing had ever compiled an empty document.
    let basics_start = s.len();
    s.push_str("  basics: (\n");
    let b = &r.basics;
    field(s, 4, "name", &b.name);
    field(s, 4, "label", &b.label);
    content(s, 4, "summary", &b.summary);
    field(s, 4, "email", &b.email);
    if let Some(href) = links::mailto(&b.email) {
        field(s, 4, "emailHref", &href);
    }
    field(s, 4, "phone", &b.phone);
    if let Some(href) = links::tel(&b.phone) {
        field(s, 4, "phoneHref", &href);
    }
    field(s, 4, "location", &b.location);
    field(s, 4, "url", &b.url);
    if let Some(href) = links::href(&b.url) {
        field(s, 4, "urlHref", &href);
    }
    if !b.profiles.is_empty() {
        s.push_str("    profiles: (\n");
        for p in &b.profiles {
            s.push_str("      (network: ");
            write_quoted(s, &p.network);
            s.push_str(", username: ");
            write_quoted(s, &p.username);
            s.push_str(", url: ");
            write_quoted(s, &p.url);
            s.push_str(", href: ");
            write_quoted(s, links::href(&p.url).as_deref().unwrap_or_default());
            s.push_str("),\n");
        }
        s.push_str("    ),\n");
    }
    if s.len() == basics_start + "  basics: (\n".len() {
        s.truncate(basics_start);
        s.push_str("  basics: (:),\n");
    } else {
        s.push_str("  ),\n");
    }

    // --- work ---
    if !r.work.is_empty() {
        s.push_str("  work: (\n");
        for w in &r.work {
            s.push_str("    (\n");
            field(s, 6, "name", &w.name);
            field(s, 6, "position", &w.position);
            field(s, 6, "location", &w.location);
            field(s, 6, "startDate", &w.start_date.display_in(dates, language));
            field(s, 6, "endDate", &w.end_date.display_in(dates, language));
            field(s, 6, "url", &w.url);
            href_field(s, 6, &w.url);
            content(s, 6, "summary", &w.summary);
            highlights(s, 6, &w.highlights);
            s.push_str("    ),\n");
        }
        s.push_str("  ),\n");
    }

    // --- education ---
    if !r.education.is_empty() {
        s.push_str("  education: (\n");
        for e in &r.education {
            s.push_str("    (\n");
            field(s, 6, "institution", &e.institution);
            field(s, 6, "studyType", &e.study_type);
            field(s, 6, "startDate", &e.start_date.display_in(dates, language));
            field(s, 6, "endDate", &e.end_date.display_in(dates, language));
            field(s, 6, "url", &e.url);
            href_field(s, 6, &e.url);
            highlights(s, 6, &e.highlights);
            s.push_str("    ),\n");
        }
        s.push_str("  ),\n");
    }

    // --- skills ---
    if !r.skills.is_empty() {
        s.push_str("  skills: (\n");
        for sk in &r.skills {
            s.push_str("    (name: ");
            write_quoted(s, &sk.name);
            s.push_str(", keywords: ");
            string_array(s, &sk.keywords);
            s.push_str("),\n");
        }
        s.push_str("  ),\n");
    }

    // --- certificates ---
    if !r.certificates.is_empty() {
        s.push_str("  certificates: (\n");
        for c in &r.certificates {
            s.push_str("    (name: ");
            write_quoted(s, &c.name);
            s.push_str(", issuer: ");
            write_quoted(s, &c.issuer);
            s.push_str(", date: ");
            write_quoted(s, &c.date.display_in(dates, language));
            s.push_str(", url: ");
            write_quoted(s, &c.url);
            s.push_str(", href: ");
            write_quoted(s, links::href(&c.url).as_deref().unwrap_or_default());
            s.push_str("),\n");
        }
        s.push_str("  ),\n");
    }

    // --- volunteer / organizations ---
    if !r.volunteer.is_empty() {
        s.push_str("  volunteer: (\n");
        for v in &r.volunteer {
            s.push_str("    (\n");
            field(s, 6, "organization", &v.organization);
            field(s, 6, "position", &v.position);
            field(s, 6, "startDate", &v.start_date.display_in(dates, language));
            field(s, 6, "endDate", &v.end_date.display_in(dates, language));
            field(s, 6, "url", &v.url);
            href_field(s, 6, &v.url);
            highlights(s, 6, &v.highlights);
            s.push_str("    ),\n");
        }
        s.push_str("  ),\n");
    }

    // --- section order ---
    //
    // Emitted as renderer keys so `render-cv` can dispatch on them, and only
    // when the order differs from the shipped one — an untouched document then
    // produces exactly the source it produced before this existed.
    //
    // A custom section is named by its id (`custom7`), never counted to. The
    // order list walks *every* section including the hidden ones, while the
    // array beside it holds only the visible ones — so any scheme that
    // counted would go off by one the moment a hidden custom section sat
    // above a visible one, and put the survivor where the hidden one had been.
    let order_keys: Vec<String> = r
        .section_order
        .iter()
        .filter_map(|kind| match kind {
            SectionKind::Custom(id) => Some(format!("custom{}", id.as_u32())),
            other => renderer_key(*other).map(|k| k.to_string()),
        })
        .collect();
    // The renderer's own fallback, spelled the same way: the built-ins in
    // their shipped order, then the custom sections it was actually handed.
    let default_order: Vec<String> = ResumeDoc::SECTIONS
        .iter()
        .filter_map(|k| renderer_key(*k).map(|s| s.to_string()))
        .chain(
            r.custom_sections
                .iter()
                .map(|cs| format!("custom{}", cs.id.as_u32())),
        )
        .collect();
    if !order_keys.is_empty() && order_keys != default_order {
        s.push_str("  order: (");
        for key in &order_keys {
            s.push_str(&format!("\"{key}\", "));
        }
        s.push_str("),\n");
    }

    // --- printed headings (O-14) ---
    //
    // Only overrides are emitted; the renderer falls back to the shipped default
    // per key, so an untouched document produces no `sectionTitles` entry at all
    // and its generated source is unchanged.
    let overrides: Vec<(&str, &String)> = r
        .section_titles
        .iter()
        .filter_map(|(kind, title)| {
            let key = section_key(*kind)?;
            (title.as_str() != ResumeDoc::default_section_title(*kind)).then_some((key, title))
        })
        .collect();
    if !overrides.is_empty() {
        s.push_str("  sectionTitles: (\n");
        for (key, title) in overrides {
            field(s, 4, key, title);
        }
        s.push_str("  ),\n");
    }

    // --- custom sections (D-9) ---
    if !r.custom_sections.is_empty() {
        s.push_str("  customSections: (\n");
        for cs in &r.custom_sections {
            s.push_str("    (\n");
            s.push_str(&format!("      id: {},\n", cs.id.as_u32()));
            field(s, 6, "title", &cs.title);
            if !cs.entries.is_empty() {
                s.push_str("      entries: (\n");
                for e in &cs.entries {
                    s.push_str("        (\n");
                    field(s, 10, "title", &e.title);
                    field(s, 10, "subtitle", &e.subtitle);
                    field(
                        s,
                        10,
                        "startDate",
                        &e.start_date.display_in(dates, language),
                    );
                    field(s, 10, "endDate", &e.end_date.display_in(dates, language));
                    field(s, 10, "url", &e.url);
                    href_field(s, 10, &e.url);
                    highlights(s, 10, &e.highlights);
                    s.push_str("        ),\n");
                }
                s.push_str("      ),\n");
            }
            s.push_str("    ),\n");
        }
        s.push_str("  ),\n");
    }

    s.push(')');
}

pub(super) fn write_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push(' ');
    }
}

/// A quoted Typst string literal written directly into buffer.
pub(super) fn write_quoted(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out.push('"');
}

/// Emit `href: "…"` beside a `url` field, when what the user typed can be made
/// into something a viewer will follow.
///
/// The two are separate keys because they are separate values: the page prints
/// `dtu.dk` and follows `https://dtu.dk`. See [`crate::resume::links`] for why
/// the shorter form on its own is a dead link.
pub(super) fn href_field(out: &mut String, indent: usize, raw: &str) {
    if let Some(href) = links::href(raw) {
        field(out, indent, "href", &href);
    }
}

/// Emit `key: "value",` only when non-empty.
pub(super) fn field(out: &mut String, indent: usize, key: &str, value: &str) {
    use std::fmt::Write;
    if value.is_empty() {
        return;
    }
    write_indent(out, indent);
    let _ = write!(out, "{key}: ");
    write_quoted(out, value);
    out.push_str(",\n");
}

/// Escape the Typst syntax a résumé's own prose collides with.
///
/// Content blocks keep emphasis live (`*bold*`, `_italic_`) because authors do
/// write it. What they never mean is the *referencing* and *executing* syntax:
/// `albert@example.com` is an email, not `@label`; `C#` is a language, not code mode;
/// `$2M ARR` is a number, not math. Each of those parses, fails, and takes the
/// whole document down with it — so a real bullet loses a hypothetical
/// `#emph[..]` rather than the user losing their preview. `[`/`]` close the
/// block early and are escaped for the same reason.
pub(super) fn neutralize_into(out: &mut String, markup: &str) {
    for ch in markup.chars() {
        if matches!(ch, '\\' | '@' | '#' | '$' | '[' | ']') {
            out.push('\\');
        }
        out.push(ch);
    }
}

#[cfg(test)] // the String-returning shape; the app calls the `_into` form
pub(super) fn neutralize(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    neutralize_into(&mut out, markup);
    out
}

/// Emit `key: [markup],` only when the markup is non-empty.
pub(super) fn content(out: &mut String, indent: usize, key: &str, markup: &str) {
    let trimmed = markup.trim();
    if trimmed.is_empty() {
        return;
    }
    write_indent(out, indent);
    out.push_str(key);
    out.push_str(": [");
    neutralize_into(out, trimmed);
    out.push_str("],\n");
}

/// Emit a `highlights: ([..], [..],)` array of content blocks.
pub(super) fn highlights(out: &mut String, indent: usize, items: &[String]) {
    if items.is_empty() {
        return;
    }
    write_indent(out, indent);
    out.push_str("highlights: (\n");
    for item in items {
        write_indent(out, indent + 2);
        out.push('[');
        neutralize_into(out, item.trim());
        out.push_str("],\n");
    }
    write_indent(out, indent);
    out.push_str("),\n");
}

/// A Typst array of strings. The trailing comma keeps a single-element value an
/// array rather than a parenthesized expression.
pub(super) fn string_array(out: &mut String, items: &[String]) {
    out.push('(');
    for item in items {
        write_quoted(out, item);
        out.push_str(", ");
    }
    out.push(')');
}
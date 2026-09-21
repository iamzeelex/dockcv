//! The `#set` lines at the top of a generated document, and the section
//! settings under them.
//!
//! Split from `template.rs` (C15). This half decides how the *page* behaves —
//! size, margins, type, the per-section overrides — and is generated fresh per
//! document from its `LayoutSettings` rather than baked into `renderer.typ`,
//! which is the whole of C1/US-07.

use std::fmt::Write as _;

use crate::resume::model::{
    DocumentLanguage, LayoutSettings, Resume, SectionKind, SectionOverrides, TypeSizes,
};

/// The document metadata every export carries.
///
/// Internal preset names, variant names, vault paths and private notes must NEVER
/// leak into exported document metadata.
pub(super) fn document_metadata_into(out: &mut String, resume: &Resume) {
    let name = resume.basics.name.trim();
    let label = resume.basics.label.trim();
    let title = if !name.is_empty() && !label.is_empty() {
        format!("{name} - {label}")
    } else if !name.is_empty() {
        format!("Resume - {name}")
    } else {
        "Resume".to_string()
    };
    let clean_title = title.replace('\\', "\\\\").replace('"', "\\\"");
    let clean_author = name.replace('\\', "\\\\").replace('"', "\\\"");
    if clean_author.is_empty() {
        let _ = writeln!(out, "#set document(title: \"{clean_title}\")");
    } else {
        let _ = writeln!(
            out,
            "#set document(title: \"{clean_title}\", author: \"{clean_author}\")"
        );
    }
}

/// The sections that print no heading, as renderer keys.
///
/// Emitted beside the page setup rather than inside the `cv` dict because
/// `section` is a plain helper that never sees `cv` — and beside it rather
/// than *in* it because this is the document's data, not one of its layout
/// knobs. Absent overrides produce an empty array, so a document nobody has
/// touched emits `#let no-heading = ()` and renders as it always did.
pub(super) fn no_heading_into(out: &mut String, resume: &Resume) {
    out.push_str("#let no-heading = (");
    for (kind, _) in resume
        .section_overrides
        .iter()
        .filter(|(_, o)| o.no_heading)
    {
        if let Some(key) = section_layout_key(*kind) {
            let _ = write!(out, "\"{key}\", ");
        }
    }
    out.push_str(")\n");
}

/// The sections that depart from the document's layout, each already resolved.
///
/// Only departures are emitted, and only the halves that departed: a section
/// that restyles its heading gets a `heading` entry and no `entry` one, so the
/// renderer's fallback keeps carrying the document's own value. A document
/// nobody has customised emits `#let section-layout = (:)` and renders exactly
/// as it did.
///
/// Resolution happens here rather than in Typst on purpose — the merge rules
/// are the interesting part, and in Typst they would sit in the one place the
/// test suite cannot reach.
pub(super) fn section_layout_into(out: &mut String, resume: &Resume, layout: &LayoutSettings) {
    let rows: Vec<(String, SectionOverrides)> = resume
        .section_overrides
        .iter()
        .filter(|(_, o)| o.touches_heading() || o.touches_entries())
        .filter_map(|(kind, o)| section_layout_key(*kind).map(|key| (key, *o)))
        .collect();

    if rows.is_empty() {
        out.push_str("#let section-layout = (:)\n");
        return;
    }

    out.push_str("#let section-layout = (\n");
    for (key, overrides) in rows {
        let _ = writeln!(out, "  \"{key}\": (");
        if overrides.touches_heading() {
            let h = overrides.headings(layout.headings);
            let _ = writeln!(
                out,
                "    heading: (style: \"{}\", case: \"{}\", align: \"{}\"),",
                h.style.keyword(),
                h.case.keyword(),
                h.align.keyword()
            );
        }
        if overrides.touches_entries() {
            let e = overrides.entries(layout.entries);
            let _ = writeln!(
                out,
                "    entry: (position: \"{}\", order: \"{}\", subtitle: \"{}\", \
                 meta: \"{}\", bullet: \"{}\", indent: {}),",
                e.meta_position.keyword(),
                e.meta_order.keyword(),
                e.subtitle.keyword(),
                e.meta.keyword(),
                e.bullet.marker(),
                e.indent_body
            );
        }
        out.push_str("  ),\n");
    }
    out.push_str(")\n");
}

/// The key a section is addressed by in `section-layout` and `no-heading` —
/// the renderer key for a built-in, the id for a custom one.
pub(super) fn section_layout_key(kind: SectionKind) -> Option<String> {
    match kind {
        SectionKind::Custom(id) => Some(format!("custom{}", id.as_u32())),
        other => renderer_key(other).map(|k| k.to_string()),
    }
}

pub(super) fn page_setup_into(out: &mut String, layout: &LayoutSettings, language: DocumentLanguage) {
    use std::fmt::Write;
    let paper = layout.page_size.typst_paper_name();
    let size_pt = layout.base_size_pt();
    let at = |delta: f32| fmt_measure(TypeSizes::resolve(size_pt, delta));
    let _ = write!(
        out,
        r##"#set page(paper: "{paper}", fill: white, margin: (x: {x}mm, top: {top}mm, bottom: {bottom}mm))
#set text(font: "{font}", size: {size}pt, fill: rgb("#1a1a1a"), lang: "{lang}", hyphenate: false)
// `lang` is the reading's own (C5): it is what Typst hyphenates, quotes and
// breaks lines by, and what a screen reader announces the page in.
// Hyphenation off, and measured rather than preferred. Typst hyphenates by
// default when a paragraph is justified, and it does it correctly: the break
// is a *soft* hyphen, U+00AD, which is exactly what the character is for. No
// extractor strips it. All seven readings of a CV whose bullet broke the word
// `counterparties` across a line lost that whole sentence — content order,
// sorted, the structure tree, `pdftotext` in all three modes, pdfminer and
// PDFBox alike. A CV is a page of short prose at a wide measure, so the
// typographic cost is a little more air between words; the cost of leaving it
// on is a bullet no parser can read. See `src/ats/adversarial.rs`.
#set par(justify: true, leading: {leading}em)

// Section bars are `heading` elements so the exported PDF carries an `/H2`
// per section in its structure tree — the one thing that tells a parser where
// a section begins without it having to guess from the geometry. The styling
// is the renderer's own, applied inside the element, so this rule hands the
// body straight back rather than adding Typst's default heading shape on top.
#show heading: it => it.body

// Used only when a dated entry has no end date. Parsed month names are
// localized in Rust before they enter the dictionary below.
#let present-label = "{present}"

// Everything that is not body text, sized from the base above rather than in
// absolute points — so `text_scale_pct` scales the document instead of only
// its paragraphs. `size-meta` and `size-pill` are not controls: they are
// derived, and only listed here so the renderer never states a size twice.
#let size-name = {name}pt
#let size-title = {title}pt
#let size-heading = {heading}pt
#let size-entry = {entry}pt
#let size-meta = {meta}pt
#let size-pill = {pill}pt

// How the Skills section is set. One dict rather than five bindings: they are
// one decision, and the renderer reads them together.
// The bar above each section.
#let heading-style = "{heading_style}"
#let heading-case = "{heading_case}"
#let heading-align = "{heading_align}"

// The block above the first section.
#let header-align = "{header_align}"
#let header-contacts = "{header_contacts}"
#let header-separator = "{header_separator}"
#let show-link-marks = {show_link_marks}

// How a dated entry is set — a job, a degree, a certificate.
#let entry-meta-position = "{entry_meta_position}"
#let entry-meta-order = "{entry_meta_order}"
#let entry-subtitle = "{entry_subtitle}"
#let entry-meta = "{entry_meta}"
#let entry-bullet = "{entry_bullet}"
#let entry-indent = {entry_indent}

#let skills = (
  style: "{skills_style}",
  sep: "{skills_sep}",
  mark_before: "{mark_before}",
  mark_after: "{mark_after}",
  gap: {skills_gap}pt,
  bullets: {skills_bullets},
)
"##,
        font = layout.font.family(),
        lang = language.code(),
        present = language.present(),
        name = at(layout.sizes.name_pt),
        title = at(layout.sizes.title_pt),
        heading = at(layout.sizes.heading_pt),
        entry = at(layout.sizes.entry_pt),
        meta = at(TypeSizes::META_PT),
        pill = at(TypeSizes::PILL_PT),
        skills_style = layout.skills.style.keyword(),
        skills_sep = layout.skills.separator.printed(),
        mark_before = layout.skills.mark.wraps().0,
        mark_after = layout.skills.mark.wraps().1,
        skills_gap = fmt_measure(layout.skills.spacing.gap_pt()),
        skills_bullets = layout.skills.bullets,
        entry_meta_position = layout.entries.meta_position.keyword(),
        entry_meta_order = layout.entries.meta_order.keyword(),
        entry_subtitle = layout.entries.subtitle.keyword(),
        entry_meta = layout.entries.meta.keyword(),
        entry_bullet = layout.entries.bullet.marker(),
        entry_indent = layout.entries.indent_body,
        heading_style = layout.headings.style.keyword(),
        heading_case = layout.headings.case.keyword(),
        heading_align = layout.headings.align.keyword(),
        header_align = layout.header.align.keyword(),
        header_contacts = layout.header.contacts.keyword(),
        header_separator = layout.header.separator.printed(),
        show_link_marks = layout.show_link_marks,
        x = fmt_measure(layout.margins.x_mm),
        top = fmt_measure(layout.margins.top_mm),
        bottom = fmt_measure(layout.margins.bottom_mm),
        size = fmt_measure(size_pt),
        leading = fmt_measure(layout.leading_em),
    );
}

/// Format a measurement to at most 2 decimal places with no trailing zeros,
/// so a default layout (`16mm`, `10pt`, `0.62em`) reads exactly as the old
/// hard-coded constants did rather than gaining spurious digits.
pub(super) fn fmt_measure(value: f32) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let mut s = format!("{rounded:.2}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

/// The key `render-cv`'s dispatch loop matches on for a built-in section.
/// Custom sections are keyed by their id (`custom7`) instead, since they have
/// no fixed name — see the `order_keys` comment for why identity and not
/// position.
pub(super) fn renderer_key(kind: SectionKind) -> Option<&'static str> {
    use SectionKind::*;
    Some(match kind {
        Profile => "profile",
        Work => "work",
        Education => "education",
        Skills => "skills",
        Certificates => "certificates",
        Organizations => "organizations",
        Custom(_) => return None,
    })
}

/// The dict key a built-in section's heading is stored under. Custom sections
/// carry their own title and are not part of this table.
pub(super) fn section_key(kind: SectionKind) -> Option<&'static str> {
    use SectionKind::*;
    Some(match kind {
        Profile => "Profile",
        Work => "Work",
        Education => "Education",
        Skills => "Skills",
        Certificates => "Certificates",
        Organizations => "Organizations",
        Custom(_) => return None,
    })
}
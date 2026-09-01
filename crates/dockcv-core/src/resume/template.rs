//! Codegen: turn a [`Resume`] into a self-contained Typst document.
//!
//! The generated source is `page_setup(layout)` (the `#set page`/`#set text`/
//! `#set par` lines, generated per document from its
//! [`LayoutSettings`](crate::resume::model::LayoutSettings) rather than
//! hard-coded — C1, US-07) + `RENDERER` (the rest of our renderer, written in
//! Typst, layout-independent) + `#let cv = (..)` (the model serialized back
//! to a Typst dictionary) + `#render-cv(cv)`. Everything lives in one
//! in-memory file so it compiles with the bundled fonts and no
//! package/network access — we do **not** download the AltaCV package to
//! render. (C7 vendors its MIT source into the repo instead; that changes which
//! renderer this dictionary is handed to, not the fact that nothing is fetched.)
//!
//! Plain fields become quoted Typst strings (so no markup escaping is needed);
//! free-text fields (`summary`, `highlights`) are emitted as `[..]` content
//! blocks, keeping the author's emphasis markup — see `neutralize` for the
//! syntax that is escaped instead, and why.

use std::fmt::Write as _;

use crate::resume::model::{
    DateFormat, LayoutSettings, Resume, ResumeDoc, SectionKind, SectionOverrides, TypeSizes,
};

/// The renderer body, written in Typst: helper functions plus `render-cv`.
/// Page/text setup is *not* here — it is generated per-document by
/// [`page_setup`] from the document's own [`LayoutSettings`], not baked into
/// this constant (C1, US-07). This is still one self-contained block: no
/// packages, no network, bundled fonts only (US-10).
const RENDERER: &str = r##"
#let muted = luma(110)
// Light enough to read as a hairline rather than as a second row of type.
#let hairline = 0.5pt + luma(150)

// A section's own layout, or the document's where it does not depart.
//
// `section-layout` holds only the sections that differ, and each one it holds
// is already fully resolved — no merging happens here. The alternative, a
// sparse dict merged against the defaults in Typst, puts the resolution rules
// in the one place they cannot be unit-tested.
#let heading-of(key) = section-layout.at(key, default: (:)).at(
  "heading",
  default: (style: heading-style, case: heading-case, align: heading-align),
)
#let entry-of(key) = section-layout.at(key, default: (:)).at(
  "entry",
  default: (
    position: entry-meta-position,
    order: entry-meta-order,
    subtitle: entry-subtitle,
    meta: entry-meta,
    bullet: entry-bullet,
    indent: entry-indent,
  ),
)

// The bar above each section. Takes the section's own key, because printing
// a heading at all is a per-section decision (`no-heading`) — most CVs that
// open with a summary print no "PROFILE" over it.
//
// Every branch is a whole `block` of its own rather than one block with the
// differences inside it: the spacing above and below a heading is part of the
// style, and nesting a block inside a block to share those two numbers would
// add Typst's own block spacing to every variant.
#let section(key, title) = if key in no-heading {
  // The space the bar would have taken, so the section below still reads as a
  // new section. Weak, so a headingless first section does not open the
  // document with a gap.
  v(12pt, weak: true)
} else {
  let h = heading-of(key)
  let words = if h.case == "upper" { upper(title) } else { title }
  let body = text(
    weight: "bold",
    size: size-heading,
    // Letter-spacing is a decision about capitals — it is what stops a run of
    // them setting solid. On mixed case it only loosens the word.
    tracking: if h.case == "upper" { 1pt } else { 0pt },
    words,
  )
  let al = if h.align == "left" { left } else { center }

  if h.style == "band" {
    block(
      width: 100%, above: 12pt, below: 6pt,
      fill: luma(238), inset: (x: 8pt, y: 4pt), radius: 2pt,
      align(al, body),
    )
  } else if h.style == "boxed" {
    block(
      width: 100%, above: 12pt, below: 6pt,
      stroke: hairline, inset: (x: 8pt, y: 4pt), radius: 2pt,
      align(al, body),
    )
  } else if h.style == "rule" {
    block(width: 100%, above: 12pt, below: 6pt, {
      align(al, body)
      v(2pt)
      line(length: 100%, stroke: hairline)
    })
  } else if h.style == "rule-to-margin" {
    // The rule takes whatever the words leave, so this style costs no line of
    // its own — on a full CV that is a section's worth of page back.
    block(width: 100%, above: 12pt, below: 6pt, grid(
      columns: (auto, 1fr), column-gutter: 8pt,
      align: (left + horizon, horizon),
      body, line(length: 100%, stroke: hairline),
    ))
  } else if h.style == "underline" {
    block(width: 100%, above: 12pt, below: 6pt,
      align(al, underline(offset: 3pt, stroke: hairline, body)))
  } else {
    block(width: 100%, above: 12pt, below: 6pt, align(al, body))
  }
}

#let daterange(start, end) = {
  if start == "" and end == "" { "" }
  else if end == "" { start + " – Present" }
  else { start + " – " + end }
}

#let meta(items) = {
  let parts = items.filter(x => x != none and x != "")
  text(fill: muted, size: size-meta, parts.join("  |  "))
}

// The three ways a run of text can be emphasised, as one helper, so the
// subtitle and the date/location line ask for it the same way.
#let styled(how, body) = {
  if how == "bold" { text(weight: "bold", body) }
  else if how == "italic" { emph(body) }
  else { body }
}

// A dated entry: title, subtitle, and the date/location pair.
//
// `trailing` arrives in document order (date, location); `entry-meta-order`
// decides whether it prints that way. Empty parts are dropped by `meta`, so a
// job with no location reads the same under either order.
#let entry(el, title, subtitle, trailing) = {
  let ordered = if el.order == "location-first" { trailing.rev() } else { trailing }
  let head = text(size: size-entry, {
    text(weight: "bold", title)
    if subtitle != "" { styled(el.subtitle, ", " + subtitle) }
  })
  if el.position == "below" {
    // Its own line: the title is never squeezed by a long date range, at the
    // cost of a line per entry.
    head
    linebreak()
    styled(el.meta, meta(ordered))
  } else {
    grid(
      columns: (1fr, auto), column-gutter: 10pt,
      align: (left + bottom, right + bottom),
      head,
      styled(el.meta, meta(ordered)),
    )
  }
}

// An entry's bullets. The marker is a setting, and `indent` decides whether
// the block sits under the title or starts again at the margin.
#let bullets(el, items) = {
  let inner = if el.bullet == "" {
    // No marker: the indent is what says "these belong to the entry above".
    for item in items [#pad(left: 0.4em, item)]
  } else {
    list(marker: [#el.bullet], ..items)
  }
  if el.indent { pad(left: 0.9em, inner) } else { inner }
}

#let render-cv(cv) = {
  let b = cv.at("basics", default: (:))

  let head-align = if header-align == "left" { left } else { center }

  align(head-align, {
    text(size: size-name, weight: "bold", b.at("name", default: ""))
    if b.at("label", default: "") != "" {
      h(8pt)
      text(size: size-title, style: "italic", fill: muted, b.at("label", default: ""))
    }
  })
  v(2pt)

  let links = b.at("profiles", default: ()).map(p => p.at("url", default: ""))
  // Empty parts are dropped here rather than in each branch below, so a CV
  // with no phone leaves no gap and no stray separator whichever shape is
  // chosen.
  let details = (
    b.at("location", default: ""),
    b.at("email", default: ""),
    b.at("phone", default: ""),
    b.at("url", default: ""),
    ..links,
  ).filter(x => x != none and x != "")

  if details.len() > 0 {
    if header-contacts == "stacked" {
      align(head-align, text(fill: muted, size: size-meta,
        details.map(d => [#d]).join(linebreak())))
    } else if header-contacts == "columns" {
      // Half the rows of `stacked`, still one item each. A grid fills
      // row-major, so the items go in as they are.
      text(fill: muted, size: size-meta, grid(
        columns: (1fr, 1fr),
        column-gutter: 12pt,
        row-gutter: 2pt,
        align: (head-align, head-align),
        ..details,
      ))
    } else {
      align(head-align, text(fill: muted, size: size-meta,
        details.join(header-separator)))
    }
  }

  let summary = b.at("summary", default: none)
  let titles = cv.at("sectionTitles", default: (:))
  let heading(key, fallback) = titles.at(key, default: fallback)

  // Each section is a closure so the document's own order (`order`, below)
  // decides the sequence. Before this they were emitted inline, one after
  // another, which meant `ResumeDoc::section_order` — a real, saved,
  // drag-reorderable field — reached the sidebar and stopped there: the PDF
  // always printed the built-in order no matter what the user arranged.
  let render-profile() = {
    if summary != none { section("profile", heading("Profile", "Profile")); summary }
  }

  let render-work() = {
  let work = cv.at("work", default: ())
  if work.len() > 0 {
    section("work", heading("Work", "Work Experience"))
    let el = entry-of("work")
    for w in work {
      entry(
        el,
        w.at("position", default: ""),
        w.at("name", default: ""),
        (daterange(w.at("startDate", default: ""), w.at("endDate", default: "")),
         w.at("location", default: "")),
      )
      let s = w.at("summary", default: none)
      if s != none { if el.indent { pad(left: 0.9em, s) } else { s } }
      let hs = w.at("highlights", default: ())
      if hs.len() > 0 { bullets(el, hs) }
      v(4pt)
    }
  }
  }

  let render-education() = {
  let edu = cv.at("education", default: ())
  if edu.len() > 0 {
    section("education", heading("Education", "Education"))
    let el = entry-of("education")
    for e in edu {
      entry(
        el,
        e.at("studyType", default: ""),
        e.at("institution", default: ""),
        (daterange(e.at("startDate", default: ""), e.at("endDate", default: "")),),
      )
      let hs = e.at("highlights", default: ())
      if hs.len() > 0 { bullets(el, hs) }
      v(3pt)
    }
  }
  }

  // One pill. Same fill and radius as the section bars above, so a bubbled
  // skills block reads as part of this document rather than as something
  // borrowed from another template.
  let pill(body, strong: false) = box(
    fill: luma(if strong { 224 } else { 240 }),
    inset: (x: 5pt, y: 2.5pt),
    radius: 3pt,
    outset: (y: 2pt),
    text(size: size-pill, weight: if strong { "bold" } else { "regular" }, body),
  )

  let render-skills() = {
  // `skills` is the settings dict from the preamble; the section's own data
  // is `groups`, named apart so the two cannot be confused.
  let groups = cv.at("skills", default: ())
  if groups.len() > 0 {
    section("skills", heading("Skills", "Skills"))

    // A group with no name is a flat list — LinkedIn exports have no
    // categories at all, and a CV need not invent one. Every branch below has
    // to survive that, which is why each checks the label rather than
    // assuming it.
    let marked(label) = text(
      weight: "bold",
      skills.mark_before + label + skills.mark_after,
    )
    // The separator is drawn muted so it recedes: at sixty terms the rules
    // are structure, not content, and printing them at full weight makes the
    // section look like a table of pipes.
    let joined(items) = items.join(text(fill: muted, skills.sep))
    let lead = if skills.bullets { text(fill: muted, "• ") } else { none }

    if skills.style == "compact" {
      // Categories dropped: one flowing list, in document order. The densest
      // arrangement there is, and honest for data that never had categories.
      let all = groups.map(g => g.at("keywords", default: ())).flatten()
      if lead != none { lead }
      joined(all)
      v(skills.gap)
    } else if skills.style == "bubbles" {
      for g in groups {
        let label = g.at("name", default: "")
        let kws = g.at("keywords", default: ())
        if kws.len() > 0 {
          // `hanging-indent` so a wrapped row of pills lines up under the
          // first pill rather than under the category.
          par(hanging-indent: 1em, justify: false, {
            if label != "" { pill(label, strong: true); h(4pt) }
            kws.map(k => pill(k)).join(h(3pt))
          })
          v(skills.gap + 1pt)
        }
      }
    } else if skills.style == "grid" {
      // Categories as a column: with several groups they line up, which is
      // what makes this different from `rows`, where each paragraph indents
      // under its own category.
      let rows = groups.filter(g => g.at("keywords", default: ()).len() > 0)
      grid(
        columns: (auto, 1fr),
        column-gutter: 12pt,
        row-gutter: skills.gap + 3pt,
        ..rows.map(g => (
          {
            if lead != none { lead }
            marked(g.at("name", default: ""))
          },
          joined(g.at("keywords", default: ())),
        )).flatten(),
      )
      v(skills.gap)
    } else {
      // Rows: category and keywords in one paragraph, wrapping under a
      // hanging indent so a long group does not start again at the margin
      // and read as a new one.
      for g in groups {
        let label = g.at("name", default: "")
        let kws = g.at("keywords", default: ())
        if kws.len() > 0 {
          par(hanging-indent: 1em, {
            if lead != none { lead }
            if label != "" { marked(label); h(0.35em) }
            joined(kws)
          })
          v(skills.gap)
        }
      }
    }
  }
  }

  let render-certificates() = {
  let certs = cv.at("certificates", default: ())
  if certs.len() > 0 {
    section("certificates", heading("Certificates", "Certifications"))
    let el = entry-of("certificates")
    for c in certs {
      entry(
        el,
        c.at("name", default: ""),
        c.at("issuer", default: ""),
        (c.at("date", default: ""),),
      )
      // The link was stored, saved and editable, and the page never printed
      // it — a value that reaches the model and not the output. Custom
      // sections have shown theirs all along; this is the same line.
      let u = c.at("url", default: "")
      if u != "" { meta((u,)) }
      v(2pt)
    }
  }
  }

  let render-organizations() = {
  let orgs = cv.at("volunteer", default: ())
  if orgs.len() > 0 {
    section("organizations", heading("Organizations", "Organizations"))
    let el = entry-of("organizations")
    for o in orgs {
      entry(
        el,
        o.at("position", default: ""),
        o.at("organization", default: ""),
        (daterange(o.at("startDate", default: ""), o.at("endDate", default: "")),),
      )
      let hs = o.at("highlights", default: ())
      if hs.len() > 0 { bullets(el, hs) }
      v(3pt)
    }
  }
  }

  // User-added sections (D-9): one generic block per section, laid out with
  // the same `section`/`entry`/`list` helpers the six built-ins use above,
  // so a custom section reads as part of the document, not a bolt-on.
  let customs = cv.at("customSections", default: ())
  // By id, not by position: `order` names a section, and a hidden one simply
  // matches nothing. Indexing this array instead meant that hiding a custom
  // section shifted every later key by one and moved the survivor into the
  // hidden one's place on the page.
  let render-custom(id) = {
    let found = customs.filter(c => c.at("id", default: -1) == id)
    if found.len() > 0 {
      let cs = found.first()
      let items = cs.at("entries", default: ())
      if items.len() > 0 {
        section("custom" + str(id), cs.at("title", default: ""))
        let el = entry-of("custom" + str(id))
        for it in items {
          entry(
            el,
            it.at("title", default: ""),
            it.at("subtitle", default: ""),
            (daterange(it.at("startDate", default: ""), it.at("endDate", default: "")),),
          )
          let hs = it.at("highlights", default: ())
          if hs.len() > 0 { bullets(el, hs) }
          let u = it.at("url", default: "")
          if u != "" { meta((u,)) }
          v(3pt)
        }
      }
    }
  }

  // The document's own section order. Absent (a bare `Resume` with no
  // `ResumeDoc` behind it, e.g. the importer's preview) falls back to the
  // order the six built-ins ship in, followed by every custom section.
  // One line on purpose: Typst ends a statement at the newline, so a leading
  // `+` on a continuation line parses as unary plus on an array.
  let default-order = ("profile", "work", "education", "skills", "certificates", "organizations") + customs.map(c => "custom" + str(c.at("id", default: 0)))
  let order = cv.at("order", default: default-order)

  for key in order {
    if key == "profile" { render-profile() }
    else if key == "work" { render-work() }
    else if key == "education" { render-education() }
    else if key == "skills" { render-skills() }
    else if key == "certificates" { render-certificates() }
    else if key == "organizations" { render-organizations() }
    else if key.starts-with("custom") { render-custom(int(key.slice(6))) }
  }
}
"##;

/// Build the full Typst document for a resume, using the default layout
/// (A4, the original margins/text-scale/leading `PREAMBLE` used to hard-code).
///
/// Kept alongside [`generate_with_layout`] as the no-layout-opinion entry
/// point: the AltaCV importer and the PDF-import preview both compile a bare
/// [`Resume`] with no [`crate::resume::model::ResumeDoc`] (and so no stored
/// layout) in hand.
pub fn generate(resume: &Resume) -> String {
    generate_with_layout(resume, &LayoutSettings::default())
}

/// Build the Typst document for a **document**, with the layout it carries.
///
/// This is what the app calls. It exists because the obvious call —
/// `generate(&doc.compose())` — silently renders with
/// `LayoutSettings::default()`, and six call sites did exactly that: the font
/// picker, the page size, the margins, the text scale and the date format all
/// reached the model, were saved to the vault, and then never arrived in the
/// preview. Taking the whole `ResumeDoc` makes the layout impossible to drop.
pub fn generate_for(doc: &ResumeDoc) -> String {
    generate_with_layout(&doc.compose(), &doc.layout)
}

/// Build the full Typst document for a resume with an explicit layout. This
/// is what a document editor should call — `resume/model.rs::ResumeDoc`
/// carries `layout` precisely so callers have real settings to pass here
/// instead of the default.
///
/// `layout` is sanitized before it ever reaches Typst source: a corrupted or
/// hand-edited vault file can supply a zero text scale or a negative margin,
/// and this is where that gets caught, not as a Typst compile error.
pub fn generate_with_layout(resume: &Resume, layout: &LayoutSettings) -> String {
    let mut out = String::with_capacity(8192);
    document_metadata_into(&mut out, resume);
    page_setup_into(&mut out, &layout.sanitized());
    no_heading_into(&mut out, resume);
    section_layout_into(&mut out, resume, &layout.sanitized());
    out.push('\n');
    out.push_str(RENDERER);
    out.push_str("\n#let cv = ");
    resume_to_dict_into(&mut out, resume, layout.date_format);
    out.push_str("\n#render-cv(cv)\n");
    out
}

/// Set PDF document metadata (Task A12).
///
/// Internal preset names, variant names, vault paths and private notes must NEVER
/// leak into exported document metadata.
fn document_metadata_into(out: &mut String, resume: &Resume) {
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
fn no_heading_into(out: &mut String, resume: &Resume) {
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
fn section_layout_into(out: &mut String, resume: &Resume, layout: &LayoutSettings) {
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
fn section_layout_key(kind: SectionKind) -> Option<String> {
    match kind {
        SectionKind::Custom(id) => Some(format!("custom{}", id.as_u32())),
        other => renderer_key(other).map(|k| k.to_string()),
    }
}

fn page_setup_into(out: &mut String, layout: &LayoutSettings) {
    use std::fmt::Write;
    let paper = layout.page_size.typst_paper_name();
    let size_pt = layout.base_size_pt();
    let at = |delta: f32| fmt_measure(TypeSizes::resolve(size_pt, delta));
    let _ = write!(
        out,
        r##"#set page(paper: "{paper}", fill: white, margin: (x: {x}mm, top: {top}mm, bottom: {bottom}mm))
#set text(font: "{font}", size: {size}pt, fill: rgb("#1a1a1a"))
#set par(justify: true, leading: {leading}em)

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
fn fmt_measure(value: f32) -> String {
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
fn renderer_key(kind: SectionKind) -> Option<&'static str> {
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
fn section_key(kind: SectionKind) -> Option<&'static str> {
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

fn resume_to_dict_into(s: &mut String, r: &Resume, dates: DateFormat) {
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
    field(s, 4, "phone", &b.phone);
    field(s, 4, "location", &b.location);
    field(s, 4, "url", &b.url);
    if !b.profiles.is_empty() {
        s.push_str("    profiles: (\n");
        for p in &b.profiles {
            s.push_str("      (network: ");
            write_quoted(s, &p.network);
            s.push_str(", username: ");
            write_quoted(s, &p.username);
            s.push_str(", url: ");
            write_quoted(s, &p.url);
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
            field(s, 6, "startDate", &w.start_date.display(dates));
            field(s, 6, "endDate", &w.end_date.display(dates));
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
            field(s, 6, "startDate", &e.start_date.display(dates));
            field(s, 6, "endDate", &e.end_date.display(dates));
            field(s, 6, "url", &e.url);
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
            write_quoted(s, &c.date.display(dates));
            s.push_str(", url: ");
            write_quoted(s, &c.url);
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
            field(s, 6, "startDate", &v.start_date.display(dates));
            field(s, 6, "endDate", &v.end_date.display(dates));
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
                    field(s, 10, "startDate", &e.start_date.display(dates));
                    field(s, 10, "endDate", &e.end_date.display(dates));
                    field(s, 10, "url", &e.url);
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

fn write_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push(' ');
    }
}

/// A quoted Typst string literal written directly into buffer.
fn write_quoted(out: &mut String, value: &str) {
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

/// Emit `key: "value",` only when non-empty.
fn field(out: &mut String, indent: usize, key: &str, value: &str) {
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
/// `hi@zeelex.me` is an email, not `@label`; `C#` is a language, not code mode;
/// `$2M ARR` is a number, not math. Each of those parses, fails, and takes the
/// whole document down with it — so a real bullet loses a hypothetical
/// `#emph[..]` rather than the user losing their preview. `[`/`]` close the
/// block early and are escaped for the same reason.
fn neutralize_into(out: &mut String, markup: &str) {
    for ch in markup.chars() {
        if matches!(ch, '\\' | '@' | '#' | '$' | '[' | ']') {
            out.push('\\');
        }
        out.push(ch);
    }
}

#[cfg(test)] // the String-returning shape; the app calls the `_into` form
fn neutralize(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    neutralize_into(&mut out, markup);
    out
}

/// Emit `key: [markup],` only when the markup is non-empty.
fn content(out: &mut String, indent: usize, key: &str, markup: &str) {
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
fn highlights(out: &mut String, indent: usize, items: &[String]) {
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
fn string_array(out: &mut String, items: &[String]) {
    out.push('(');
    for item in items {
        write_quoted(out, item);
        out.push_str(", ");
    }
    out.push(')');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Margins, PageSize};
    /// A skills style is only real if it reaches the page. Every one of them
    /// **compiles**, and each produces a different number of laid-out items —
    /// asserting on the generated source would pass for a style the compiler
    /// then ignored, which is the mistake E-32 records (the whole layout rail
    /// was inert while every source-level test was green).
    #[test]
    fn every_skills_style_compiles_and_lays_out_differently() {
        use crate::resume::model::{SkillsLayout, SkillsStyle};
        use crate::typst_engine::TypstEngine;

        let mut resume = Resume::default();
        resume.skills = vec![
            crate::resume::model::SkillGroup {
                name: "Languages".into(),
                keywords: vec!["Rust".into(), "Python".into(), "TypeScript".into()],
            },
            crate::resume::model::SkillGroup {
                name: "Infrastructure".into(),
                keywords: vec!["Kubernetes".into(), "Kafka".into()],
            },
        ];

        let mut heights = Vec::new();
        for style in SkillsStyle::ALL {
            let layout = LayoutSettings {
                skills: SkillsLayout {
                    style,
                    ..SkillsLayout::default()
                },
                entries: Default::default(),
                header: Default::default(),
                ..LayoutSettings::default()
            };
            let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
            let pdf = engine
                .compile_to_pdf()
                .unwrap_or_else(|e| panic!("{} did not compile: {e}", style.label()));
            assert!(
                pdf.starts_with(b"%PDF"),
                "{} produced no PDF",
                style.label()
            );
            heights.push((style, pdf.len()));
        }

        // Not all four need differ from each other — `grid` and `inline` are
        // close by design — but the two that rearrange the words must differ
        // from the default, or nothing was actually applied.
        let of = |s: SkillsStyle| heights.iter().find(|(k, _)| *k == s).unwrap().1;
        assert_ne!(
            of(SkillsStyle::Rows),
            of(SkillsStyle::Bubbles),
            "bubbles rendered identically to inline — the style never reached the page"
        );
        assert_ne!(
            of(SkillsStyle::Rows),
            of(SkillsStyle::Compact),
            "compact rendered identically to inline"
        );
    }

    /// The first per-section override: a section that prints no heading.
    ///
    /// Held to three rules. It has to take the heading off the page; it has
    /// to leave the section's own content there (the whole point is a summary
    /// with no "PROFILE" over it, not a missing summary); and a document that
    /// never used it has to render exactly as it did.
    #[test]
    fn a_section_can_print_no_heading_without_losing_its_content() {
        use crate::resume::model::{Basics, Work};
        use crate::typst_engine::TypstEngine;

        let resume = Resume {
            basics: Basics {
                name: "Sofiia Medvedenko".into(),
                summary: "Backend engineer with eight years of experience.".into(),
                ..Default::default()
            },
            work: vec![Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut doc = ResumeDoc::from_resume(resume.clone(), "Base");

        let ink = |doc: &ResumeDoc| {
            TypstEngine::new(generate(&doc.compose()))
                .compile_to_pixels(1.0)
                .expect("compiles")
                .0
                .rgba
                .chunks_exact(4)
                .filter(|px| px[0] < 200 || px[1] < 200 || px[2] < 200)
                .count()
        };

        let with_heading = ink(&doc);
        doc.set_heading_printed(SectionKind::Profile, false);
        let without = ink(&doc);
        assert!(
            without < with_heading,
            "hiding the Profile heading drew as much ink as printing it \
             ({without} vs {with_heading})"
        );

        // The summary itself is still there — a rule that would otherwise be
        // satisfied by dropping the section altogether. Compared against the
        // same document with nothing *in* the section, because Profile is the
        // one section `set_hidden` refuses to touch (a CV without a name is
        // not a shorter CV), so there is no "hidden" version to compare with.
        let mut gutted = doc.clone();
        gutted.profile.active_mut().summary.clear();
        assert!(
            ink(&gutted) < without,
            "the section lost its content, not just its heading"
        );

        // Off again, and the document is byte for byte what it was: the row
        // is removed rather than stored as a row of defaults.
        doc.set_heading_printed(SectionKind::Profile, true);
        assert!(
            doc.section_overrides.is_empty(),
            "turning the override off left a row behind: {:?}",
            doc.section_overrides
        );
        assert_eq!(
            generate(&doc.compose()),
            generate(&ResumeDoc::from_resume(resume, "Base").compose()),
            "a document that used and un-used the flag is not what it started as"
        );
    }

    /// Every section can drop its heading, not just Profile — including a
    /// custom one, whose key is its id and not its position.
    #[test]
    fn the_no_heading_flag_reaches_the_source_for_every_kind_of_section() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let custom = doc.add_custom_section("Publications");

        assert!(
            generate(&doc.compose()).contains("#let no-heading = ()"),
            "an untouched document should carry an empty list"
        );

        for (section, key) in [
            (SectionKind::Profile, "profile"),
            (SectionKind::Skills, "skills"),
            (SectionKind::Organizations, "organizations"),
            (SectionKind::Custom(custom), "custom0"),
        ] {
            doc.set_heading_printed(section, false);
            let source = generate(&doc.compose());
            assert!(
                source.contains(&format!("\"{key}\"")),
                "{section:?} did not reach the source as `{key}`"
            );
            doc.set_heading_printed(section, true);
        }
    }

    /// Every heading style has to compile, put ink on the page, and differ
    /// from every other one — six branches of Typst is six chances for a
    /// variant to silently fall through to the same drawing.
    #[test]
    fn every_heading_style_compiles_and_draws_something_different() {
        use crate::resume::model::{HeadingLayout, HeadingStyle, Work};
        use crate::typst_engine::TypstEngine;
        use std::collections::HashMap;

        let resume = Resume {
            work: vec![Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                start_date: "2022-01".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut seen: HashMap<Vec<u8>, &'static str> = HashMap::new();
        for style in HeadingStyle::ALL {
            let layout = LayoutSettings {
                headings: HeadingLayout {
                    style,
                    ..Default::default()
                },
                ..LayoutSettings::default()
            };
            let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
            let pixels = engine
                .compile_to_pixels(1.0)
                .unwrap_or_else(|e| panic!("{} did not compile: {e}", style.label()))
                .0
                .rgba;
            // `Plain` is the one style with nothing but type, so it is the
            // floor every other style has to draw more than.
            if let Some(other) = seen.insert(pixels, style.label()) {
                panic!("{} rendered identically to {other}", style.label());
            }
        }
    }

    /// The heading controls, on the same two rules as the rest of the rail:
    /// each changes the page, and the default leaves a document alone.
    #[test]
    fn every_heading_control_changes_the_page_and_the_default_changes_nothing() {
        use crate::resume::model::{HeaderAlign, HeadingCase, HeadingLayout, HeadingStyle, Work};
        use crate::typst_engine::TypstEngine;

        let resume = Resume {
            work: vec![Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let pixels = |headings: HeadingLayout| {
            let layout = LayoutSettings {
                headings,
                ..LayoutSettings::default()
            };
            let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba
        };

        let base = pixels(HeadingLayout::default());
        let engine = TypstEngine::new(generate(&resume));
        assert_eq!(
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba,
            base,
            "the default heading is not what `generate` produces"
        );

        let variants: [(&str, HeadingLayout); 3] = [
            (
                "plain style",
                HeadingLayout {
                    style: HeadingStyle::Plain,
                    ..Default::default()
                },
            ),
            (
                "as-typed case",
                HeadingLayout {
                    case: HeadingCase::AsTyped,
                    ..Default::default()
                },
            ),
            (
                "left aligned",
                HeadingLayout {
                    align: HeaderAlign::Left,
                    ..Default::default()
                },
            ),
        ];
        for (what, headings) in variants {
            assert_ne!(
                pixels(headings),
                base,
                "{what} rendered identically to the default — the control never reached the page"
            );
        }

        // Alignment is hidden for the one style whose words cannot move, so
        // it had better be true that they cannot. If this ever fails the rail
        // is hiding a control that does something.
        assert!(!HeadingStyle::RuleToMargin.can_align());
        assert_eq!(
            pixels(HeadingLayout {
                style: HeadingStyle::RuleToMargin,
                align: HeaderAlign::Center,
                ..Default::default()
            }),
            pixels(HeadingLayout {
                style: HeadingStyle::RuleToMargin,
                align: HeaderAlign::Left,
                ..Default::default()
            }),
            "alignment moved a rule-to-margin heading — the rail hides a live control"
        );
    }

    /// Letter-spacing belongs to the capitals, so dropping the capitals has
    /// to drop it too — otherwise "Work Experience" comes out gappy and the
    /// case control looks broken rather than chosen.
    ///
    /// Isolated on the page rather than in the source: the title is typed in
    /// capitals already, so `upper()` is a no-op and the **only** thing left
    /// between the two renders is the tracking. An earlier version of this
    /// test matched the renderer's source text and broke the moment the branch
    /// was rewritten with identical behaviour (E-36).
    #[test]
    fn tracking_follows_the_capitals() {
        use crate::resume::model::{HeadingCase, Work};
        use crate::typst_engine::TypstEngine;

        let mut doc = ResumeDoc::from_resume(
            Resume {
                work: vec![Work {
                    name: "Acme".into(),
                    position: "Staff Engineer".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            "Base",
        );
        doc.set_section_title(SectionKind::Work, "WORK");

        let pixels = |case: HeadingCase| {
            let mut doc = doc.clone();
            doc.layout.headings.case = case;
            // `generate_for`, not `generate`: the latter renders under the
            // *default* layout, so a test that sets one and calls it proves
            // nothing about the setting.
            TypstEngine::new(generate_for(&doc))
                .compile_to_pixels(1.0)
                .expect("compiles")
                .0
                .rgba
        };

        assert!(
            pixels(HeadingCase::Upper) != pixels(HeadingCase::AsTyped),
            "a heading already typed in capitals rendered the same either way — \
             the letter-spacing is no longer tied to the case"
        );
    }

    /// A per-section departure has to reach the page, and reach **only** that
    /// section — the whole risk of this feature is a setting that leaks.
    #[test]
    fn a_sections_own_heading_reaches_the_page_and_leaves_the_others_alone() {
        use crate::resume::model::{Education, HeadingStyle, SectionOverrides, Work};
        use crate::typst_engine::TypstEngine;

        let doc = ResumeDoc::from_resume(
            Resume {
                work: vec![Work {
                    name: "Acme".into(),
                    position: "Staff Engineer".into(),
                    ..Default::default()
                }],
                education: vec![Education {
                    institution: "Tallaght".into(),
                    study_type: "M.Sc.".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            "Base",
        );

        let pixels = |d: &ResumeDoc| {
            TypstEngine::new(generate_for(d))
                .compile_to_pixels(1.0)
                .expect("compiles")
                .0
                .rgba
        };
        let base = pixels(&doc);

        // One section restyled.
        let mut one = doc.clone();
        one.set_section_overrides(
            SectionKind::Work,
            SectionOverrides {
                heading_style: Some(HeadingStyle::Plain),
                ..Default::default()
            },
        );
        assert!(pixels(&one) != base, "the override never reached the page");

        // The same style applied to the whole document must differ from the
        // one-section version — otherwise "only that section" is unproven.
        let mut all = doc.clone();
        all.layout.headings.style = HeadingStyle::Plain;
        assert!(
            pixels(&one) != pixels(&all),
            "restyling one section rendered the same as restyling every section"
        );

        // And the section that was left alone is genuinely untouched: turning
        // the *document* to the same style leaves Work where the override
        // already put it, so only Education can account for the difference.
        let mut both = one.clone();
        both.layout.headings.style = HeadingStyle::Plain;
        assert_eq!(
            pixels(&both),
            pixels(&all),
            "an override and the document agreeing did not converge"
        );
    }

    /// Overrides are per **field**, not per struct.
    ///
    /// A section that departs on style must still follow the document on
    /// capitalisation — otherwise restyling one section silently freezes two
    /// other decisions, and the next document-wide change skips it for
    /// reasons the user never chose.
    #[test]
    fn overriding_one_field_does_not_pin_the_others() {
        use crate::resume::model::{HeadingCase, HeadingStyle, SectionOverrides, Work};
        use crate::typst_engine::TypstEngine;

        let mut doc = ResumeDoc::from_resume(
            Resume {
                work: vec![Work {
                    name: "Acme".into(),
                    position: "Staff Engineer".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            "Base",
        );
        doc.set_section_overrides(
            SectionKind::Work,
            SectionOverrides {
                heading_style: Some(HeadingStyle::Plain),
                ..Default::default()
            },
        );

        // The model's own answer…
        assert_eq!(
            doc.headings_for(SectionKind::Work).style,
            HeadingStyle::Plain
        );
        assert_eq!(doc.headings_for(SectionKind::Work).case, HeadingCase::Upper);
        doc.layout.headings.case = HeadingCase::AsTyped;
        assert_eq!(
            doc.headings_for(SectionKind::Work).case,
            HeadingCase::AsTyped,
            "the section stopped following the document on a field it never set"
        );
        assert_eq!(
            doc.headings_for(SectionKind::Work).style,
            HeadingStyle::Plain,
            "the field it did set was lost"
        );

        // …and the page's, since a resolution that never reaches Typst is not
        // a resolution (E-32).
        let pixels = |d: &ResumeDoc| {
            TypstEngine::new(generate_for(d))
                .compile_to_pixels(1.0)
                .expect("compiles")
                .0
                .rgba
        };
        let following = pixels(&doc);
        let mut pinned = doc.clone();
        pinned.layout.headings.case = HeadingCase::Upper;
        assert!(
            following != pixels(&pinned),
            "the document's capitalisation did not move the overridden section"
        );
    }

    /// An untouched document emits an empty table and is unchanged.
    #[test]
    fn no_overrides_means_an_empty_table_and_the_document_it_always_was() {
        use crate::resume::model::{HeadingStyle, SectionOverrides};

        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let before = generate_for(&doc);
        assert!(before.contains("#let section-layout = (:)"));

        doc.set_section_overrides(
            SectionKind::Skills,
            SectionOverrides {
                heading_style: Some(HeadingStyle::Boxed),
                ..Default::default()
            },
        );
        let with = generate_for(&doc);
        assert!(with.contains("\"skills\": ("), "{with}");
        assert!(
            with.contains("heading: (style: \"boxed\""),
            "the resolved heading is not in the source"
        );
        assert!(
            !with.contains("    entry: ("),
            "a section that only restyled its heading pinned its entries too"
        );

        // Back to following the document, byte for byte.
        doc.set_section_overrides(SectionKind::Skills, SectionOverrides::default());
        assert_eq!(generate_for(&doc), before);
    }

    /// The header controls, held to the same two rules as the entry ones:
    /// each has to change the page, and the default has to leave a document
    /// exactly as it was.
    #[test]
    fn every_header_control_changes_the_page_and_the_default_changes_nothing() {
        use crate::resume::model::{
            Basics, ContactLayout, HeaderAlign, HeaderLayout, SkillSeparator,
        };
        use crate::typst_engine::TypstEngine;

        let resume = Resume {
            basics: Basics {
                name: "Sofiia Medvedenko".into(),
                label: "Staff Engineer".into(),
                location: "Barcelona, Spain".into(),
                email: "s@example.com".into(),
                phone: "+34 000 000 000".into(),
                url: "https://example.com".into(),
                ..Default::default()
            },
            ..Default::default()
        };

        let pixels = |header: HeaderLayout| {
            let layout = LayoutSettings {
                header,
                ..LayoutSettings::default()
            };
            let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba
        };

        let base = pixels(HeaderLayout::default());
        let engine = TypstEngine::new(generate(&resume));
        assert_eq!(
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba,
            base,
            "the default header is not what `generate` produces"
        );

        let variants: [(&str, HeaderLayout); 4] = [
            (
                "left aligned",
                HeaderLayout {
                    align: HeaderAlign::Left,
                    ..Default::default()
                },
            ),
            (
                "stacked contacts",
                HeaderLayout {
                    contacts: ContactLayout::Stacked,
                    ..Default::default()
                },
            ),
            (
                "two columns",
                HeaderLayout {
                    contacts: ContactLayout::Columns,
                    ..Default::default()
                },
            ),
            (
                "bullet separator",
                HeaderLayout {
                    separator: SkillSeparator::Bullet,
                    ..Default::default()
                },
            ),
        ];
        for (what, header) in variants {
            assert_ne!(
                pixels(header),
                base,
                "{what} rendered identically to the default — the control never reached the page"
            );
        }

        // The separator is only a choice when the details share a line, which
        // is why the rail hides it otherwise. If that ever stops being true
        // here, the rail is hiding a live control.
        assert_eq!(
            pixels(HeaderLayout {
                contacts: ContactLayout::Stacked,
                separator: SkillSeparator::Bullet,
                ..Default::default()
            }),
            pixels(HeaderLayout {
                contacts: ContactLayout::Stacked,
                ..Default::default()
            }),
            "the separator changed a stacked header — the rail hides a control that does something"
        );
    }

    /// The per-element sizes, held to the two rules the other layout groups
    /// are: each control has to reach the page, and a document nobody has
    /// touched has to render exactly as it did before the controls existed.
    ///
    /// The second half is checked on the *numbers that reach Typst* rather
    /// than on pixels, because the equality it guards is arithmetic: at the
    /// default base of 10pt the six offsets have to resolve to the six
    /// literals the renderer used to spell out. (That the substitution is
    /// otherwise a no-op was confirmed once by rendering the sample against
    /// both trees — identical bytes at 100%.)
    #[test]
    fn every_type_size_changes_the_page_and_the_default_resolves_to_the_old_literals() {
        use crate::resume::model::{Basics, TypeSizes, Work};
        use crate::typst_engine::TypstEngine;

        let source = generate(&Resume::default());
        for expected in [
            "#let size-name = 20pt",
            "#let size-title = 12pt",
            "#let size-heading = 9pt",
            "#let size-entry = 10pt",
            "#let size-meta = 9pt",
            "#let size-pill = 8.5pt",
        ] {
            assert!(
                source.contains(expected),
                "a default document no longer sets `{expected}` — every CV \
                 written before this control existed has been re-typeset"
            );
        }

        let resume = Resume {
            basics: Basics {
                name: "Sofiia Medvedenko".into(),
                label: "Staff Engineer".into(),
                ..Default::default()
            },
            work: vec![Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                start_date: "2022-01".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let pixels = |sizes: TypeSizes| {
            let layout = LayoutSettings {
                sizes,
                ..LayoutSettings::default()
            };
            let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba
        };

        let base = pixels(TypeSizes::default());
        let variants: [(&str, TypeSizes); 4] = [
            (
                "name",
                TypeSizes {
                    name_pt: 14.0,
                    ..Default::default()
                },
            ),
            (
                "professional title",
                TypeSizes {
                    title_pt: 5.0,
                    ..Default::default()
                },
            ),
            (
                "section headings",
                TypeSizes {
                    heading_pt: 3.0,
                    ..Default::default()
                },
            ),
            (
                "entry title",
                TypeSizes {
                    entry_pt: 3.0,
                    ..Default::default()
                },
            ),
        ];
        for (what, sizes) in variants {
            assert_ne!(
                pixels(sizes),
                base,
                "the {what} size rendered identically to the default — the \
                 control never reached the page"
            );
        }
    }

    /// Text scale scales the *document*, not only its paragraphs.
    ///
    /// This is the behaviour the offsets bought and the one thing about them
    /// that is not backwards-compatible: before, the name was a flat 20pt at
    /// every scale, so turning the body down to 85% widened the size contrast
    /// instead of shrinking the page. Rendered documents at a non-default
    /// scale therefore changed once, deliberately.
    #[test]
    fn every_size_follows_the_base_rather_than_standing_still() {
        let smaller = generate_with_layout(
            &Resume::default(),
            &LayoutSettings {
                text_scale_pct: 90,
                ..LayoutSettings::default()
            },
        );
        // 9pt base: 9+10, 9+2, 9−1, 9+0, 9−1, 9−1.5.
        for expected in [
            "#let size-name = 19pt",
            "#let size-title = 11pt",
            "#let size-heading = 8pt",
            "#let size-entry = 9pt",
            "#let size-pill = 7.5pt",
        ] {
            assert!(
                smaller.contains(expected),
                "at 90% the document does not set `{expected}` — some element \
                 is pinned to an absolute size again"
            );
        }
    }

    /// Every entry control has to reach the page, and the default has to
    /// leave a document rendering exactly as it did before the controls
    /// existed — the second half is what stops a layout feature from quietly
    /// re-typesetting everyone's CV.
    ///
    /// Compared on pixels rather than on the generated source: E-32 was a
    /// whole layout rail that reached the model, was saved, and never arrived
    /// on the page while every source-level test stayed green.
    #[test]
    fn every_entry_control_changes_the_page_and_the_default_changes_nothing() {
        use crate::resume::model::{
            BulletGlyph, Emphasis, EntryLayout, MetaOrder, MetaPosition, Work,
        };
        use crate::typst_engine::TypstEngine;

        let resume = Resume {
            work: vec![Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                location: "Barcelona, Spain".into(),
                // Dates matter to this fixture: `meta` drops empty parts, so
                // an undated entry has one thing to print and reversing the
                // order of one thing proves nothing.
                start_date: "2022-01".into(),
                end_date: "2024-06".into(),
                summary: "Recovered reliable state from unreliable measurement.".into(),
                highlights: vec![
                    "Cut p99 latency in half.".into(),
                    "Rewrote the ingest.".into(),
                ],
            }],
            ..Default::default()
        };

        let pixels = |entries: EntryLayout| {
            let layout = LayoutSettings {
                entries,
                ..LayoutSettings::default()
            };
            let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba
        };

        let base = pixels(EntryLayout::default());

        // The default must be the old rendering, byte for byte.
        let engine = TypstEngine::new(generate(&resume));
        assert_eq!(
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba,
            base,
            "the default entry layout is not what `generate` produces"
        );

        let variants: [(&str, EntryLayout); 6] = [
            (
                "meta below",
                EntryLayout {
                    meta_position: MetaPosition::Below,
                    ..Default::default()
                },
            ),
            (
                "place first",
                EntryLayout {
                    meta_order: MetaOrder::LocationFirst,
                    ..Default::default()
                },
            ),
            (
                "subtitle bold",
                EntryLayout {
                    subtitle: Emphasis::Bold,
                    ..Default::default()
                },
            ),
            (
                "meta bold",
                EntryLayout {
                    meta: Emphasis::Bold,
                    ..Default::default()
                },
            ),
            (
                "dash bullets",
                EntryLayout {
                    bullet: BulletGlyph::Dash,
                    ..Default::default()
                },
            ),
            (
                "indented body",
                EntryLayout {
                    indent_body: true,
                    ..Default::default()
                },
            ),
        ];

        for (what, entries) in variants {
            assert_ne!(
                pixels(entries),
                base,
                "{what} rendered identically to the default — the control never reached the page"
            );
        }
    }

    /// The point of the options: a Skills section with sixty terms is the
    /// thing that pushes a CV onto a second page, and the dense settings have
    /// to actually *be* denser on the laid-out page — not merely different.
    ///
    /// Measured in points of used page height, from the same geometry the
    /// overflow chip reads, because "the source changed" would pass for
    /// settings the compiler ignored (E-32).
    #[test]
    fn the_dense_settings_take_less_page_than_the_roomy_ones() {
        use crate::resume::model::{CategoryMark, RowSpacing, SkillSeparator, SkillsLayout};
        use crate::typst_engine::TypstEngine;

        let mut resume = Resume::default();
        resume.skills = (0..8)
            .map(|i| crate::resume::model::SkillGroup {
                name: format!("Category number {i}"),
                keywords: (0..9).map(|k| format!("Technology {i}-{k}")).collect(),
            })
            .collect();

        let used = |skills: SkillsLayout| {
            let layout = LayoutSettings {
                skills,
                ..LayoutSettings::default()
            };
            let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
            let (_, geometry) = engine.compile_to_pixels(1.0).expect("compiles");
            geometry.page_count as f64 * 1000.0 + geometry.last_page_used_pt
        };

        let roomy = used(SkillsLayout::default());
        let dense = used(SkillsLayout {
            separator: SkillSeparator::Rule,
            mark: CategoryMark::Dash,
            spacing: RowSpacing::Tight,
            ..SkillsLayout::default()
        });

        assert!(
            dense < roomy,
            "the dense settings used {dense} against {roomy} — they cost space instead of saving it"
        );
    }

    /// A group with no category name is the ordinary shape of a LinkedIn
    /// export, and every style has to survive it — a `bubbles` run that
    /// emitted an empty pill, or a `grid` with a blank first column, would be
    /// the bare-colon bug (E-36) wearing a new hat.
    #[test]
    fn every_skills_style_survives_a_group_with_no_category() {
        use crate::resume::model::{SkillsLayout, SkillsStyle};
        use crate::typst_engine::TypstEngine;

        let mut resume = Resume::default();
        resume.skills = vec![crate::resume::model::SkillGroup {
            name: String::new(),
            keywords: vec!["Rust".into(), "Kafka".into()],
        }];

        for style in SkillsStyle::ALL {
            let layout = LayoutSettings {
                skills: SkillsLayout {
                    style,
                    ..SkillsLayout::default()
                },
                entries: Default::default(),
                header: Default::default(),
                ..LayoutSettings::default()
            };
            let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
            assert!(
                engine.compile_to_pdf().is_ok(),
                "{} failed on an unnamed group",
                style.label()
            );
        }
    }

    #[test]
    fn default_layout_renders_the_old_hard_coded_values() {
        let source = generate_with_layout(&Resume::default(), &LayoutSettings::default());
        assert!(source.contains(r#"paper: "a4""#));
        assert!(source.contains("margin: (x: 16mm, top: 14mm, bottom: 14mm)"));
        assert!(source.contains("size: 10pt"));
        assert!(source.contains("leading: 0.62em"));
        // generate() with no layout in hand must match the explicit default.
        assert_eq!(source, generate(&Resume::default()));
    }

    #[test]
    fn letter_uses_the_us_letter_paper_preset() {
        let layout = LayoutSettings {
            page_size: PageSize::Letter,
            font: Default::default(),
            date_format: Default::default(),
            ..LayoutSettings::default()
        };
        let source = generate_with_layout(&Resume::default(), &layout);
        assert!(source.contains(r#"paper: "us-letter""#));
    }

    #[test]
    fn text_scale_and_leading_render_into_the_preamble() {
        let layout = LayoutSettings {
            text_scale_pct: 107,
            leading_em: 0.7,
            margins: Margins {
                x_mm: 20.0,
                top_mm: 18.0,
                bottom_mm: 18.0,
            },
            ..LayoutSettings::default()
        };
        let source = generate_with_layout(&Resume::default(), &layout);
        assert!(source.contains("size: 10.7pt"), "{source}");
        assert!(source.contains("leading: 0.7em"), "{source}");
        assert!(source.contains("margin: (x: 20mm, top: 18mm, bottom: 18mm)"));
    }

    #[test]
    fn invalid_layout_is_sanitized_before_it_reaches_typst_source() {
        // A hand-edited or corrupted vault file could hold any of these; the
        // generated source must still be renderable, not a Typst error.
        let layout = LayoutSettings {
            page_size: PageSize::A4,
            font: Default::default(),
            date_format: Default::default(),
            skills: Default::default(),
            entries: Default::default(),
            header: Default::default(),
            headings: Default::default(),
            sizes: TypeSizes {
                name_pt: 900.0,
                title_pt: -900.0,
                heading_pt: 0.0,
                entry_pt: 0.0,
            },
            text_scale_pct: 0,
            leading_em: -1.0,
            margins: Margins {
                x_mm: -5.0,
                top_mm: 900.0,
                bottom_mm: 900.0,
            },
        };
        let source = generate_with_layout(&Resume::default(), &layout);
        assert!(!source.contains("size: 0pt"));
        assert!(!source.contains("leading: -1em"));
        assert!(!source.contains("margin: (x: -5mm"));
    }

    /// The page-size switch has to survive the compiler, not just the string.
    ///
    /// Asserting that `"us-letter"` appears in the generated source proves the
    /// preset name is spelled right; it does not prove Typst accepts it or that
    /// the page actually changed. A4 is 297mm tall and Letter 279.4mm, so a real
    /// compile must report measurably different page heights.
    /// The bug this guards: a bullet carrying the CV's own email compiled to
    /// `label <zeelex.mecritical> does not exist`, because Typst reads `@x` as a
    /// reference. `C#` fails the same way through code mode. Asserting on the
    /// generated source would not have caught it — only the compiler knows.
    #[test]
    fn prose_that_looks_like_typst_syntax_still_compiles() {
        use crate::resume::model::Work;
        use crate::typst_engine::TypstEngine;

        let resume = Resume {
            work: vec![Work {
                position: "Engineer".into(),
                highlights: vec![
                    "Reachable at hi@zeelex.me for critical incidents".into(),
                    "Ported the C# service and its #tags to Rust".into(),
                    "Cut latency [p99] by 40%".into(),
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        let engine = TypstEngine::new(generate(&resume));
        let attempt = engine.compile_with_diagnostics(1.0);
        assert!(
            attempt.result.is_ok(),
            "an email in a bullet must not break the document: {:?}",
            attempt.diagnostics
        );
    }

    /// The bug this guards: every call site in the app rendered with
    /// `LayoutSettings::default()`, so the font picker, page size, margins,
    /// text scale and date format all reached the model, were saved, and never
    /// arrived in the preview. The engine's own tests passed throughout —
    /// they called `generate_with_layout` directly, which is not the path the
    /// app took.
    #[test]
    fn the_documents_own_layout_reaches_the_source_the_app_renders() {
        use crate::resume::model::{DocumentFont, PageSize};

        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        doc.layout.font = DocumentFont::ALL
            .iter()
            .copied()
            .find(|f| *f != DocumentFont::default())
            .expect("more than one font is offered");
        doc.layout.page_size = PageSize::Letter;

        let source = generate_for(&doc);
        assert!(
            source.contains(doc.layout.font.family()),
            "the chosen font never reached the source"
        );
        assert!(
            source.contains(r#"paper: "us-letter""#),
            "page size was dropped"
        );
        assert_ne!(
            source,
            generate(&doc.compose()),
            "generate_for must differ from the default-layout path"
        );
    }

    /// A skill group with no name printed a bare colon in front of its list.
    /// LinkedIn exports have no categories at all, so this is the ordinary
    /// shape, not an edge case.
    #[test]
    fn an_unnamed_skill_group_prints_no_label_and_no_colon() {
        use crate::resume::model::SkillGroup;

        let named = Resume {
            skills: vec![SkillGroup {
                name: "Languages".into(),
                keywords: vec!["Rust".into()],
            }],
            ..Default::default()
        };
        let unnamed = Resume {
            skills: vec![SkillGroup {
                name: String::new(),
                keywords: vec!["Rust".into()],
            }],
            ..Default::default()
        };

        assert!(generate(&named).contains(r#"name: "Languages""#));
        // The empty name *is* emitted — the decision belongs to the renderer,
        // not the codegen, and the guard is one branch in `RENDERER`. A source
        // test cannot see a glyph, so what is asserted here is that the branch
        // exists and that both shapes compile; the missing colon is visible on
        // the page.
        let source = generate(&unnamed);
        let at = source.rfind("skills: (").expect("a skills array");
        assert!(source[at..].contains(r#"name: """#));

        for resume in [&named, &unnamed] {
            let engine = crate::typst_engine::TypstEngine::new(generate(resume));
            assert!(
                engine.compile_with_diagnostics(1.0).result.is_ok(),
                "skills must render either way"
            );
        }

        // The actual bug (E-36) was a *printed* `:` in front of a list with no
        // category, and neither the model nor the generated source can show
        // that. The page can: with no name, the mark must make no difference
        // at all, so rendering with a colon and with no mark has to produce
        // the same pixels. This used to be a substring test against the
        // renderer's source, which said nothing about the output and broke the
        // moment the branch was rewritten with the same behaviour.
        let pixels_with = |mark: crate::resume::model::CategoryMark| {
            let layout = LayoutSettings {
                skills: crate::resume::model::SkillsLayout {
                    mark,
                    ..Default::default()
                },
                ..LayoutSettings::default()
            };
            let engine =
                crate::typst_engine::TypstEngine::new(generate_with_layout(&unnamed, &layout));
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba
        };
        assert_eq!(
            pixels_with(crate::resume::model::CategoryMark::Colon),
            pixels_with(crate::resume::model::CategoryMark::None),
            "an unnamed group printed its mark — the bare colon is back"
        );

        // And with a name, the mark must change the page, or the control is
        // decoration.
        let named_pixels = |mark: crate::resume::model::CategoryMark| {
            let layout = LayoutSettings {
                skills: crate::resume::model::SkillsLayout {
                    mark,
                    ..Default::default()
                },
                ..LayoutSettings::default()
            };
            let engine =
                crate::typst_engine::TypstEngine::new(generate_with_layout(&named, &layout));
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba
        };
        assert_ne!(
            named_pixels(crate::resume::model::CategoryMark::Colon),
            named_pixels(crate::resume::model::CategoryMark::Dash),
            "the category mark never reached the page"
        );
    }

    /// A certificate's link was emitted into the dictionary and dropped by the
    /// renderer — stored, saved, editable, invisible. Asserting on the model
    /// would have passed; this asserts on the generated source, which is where
    /// it went missing.
    #[test]
    fn a_certificates_link_reaches_the_page() {
        use crate::resume::model::Certificate;

        let resume = Resume {
            certificates: vec![Certificate {
                name: "Reservoir Engineering".into(),
                issuer: "SPE".into(),
                url: "https://spe.org/cert/42".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let source = generate(&resume);
        assert!(
            source.contains("https://spe.org/cert/42"),
            "the url is not emitted"
        );
        assert!(
            RENDERER.contains(r#"let u = c.at("url", default: "")"#),
            "the renderer no longer reads a certificate's url"
        );
        let engine = crate::typst_engine::TypstEngine::new(source);
        assert!(engine.compile_with_diagnostics(1.0).result.is_ok());
    }

    #[test]
    fn letter_and_a4_compile_to_genuinely_different_pages() {
        use crate::resume::altacv;
        use crate::typst_engine::TypstEngine;

        let resume = altacv::import(altacv::ALTACV_SAMPLE).expect("the sample parses");

        let height_of = |page_size| {
            let layout = LayoutSettings {
                page_size,
                font: Default::default(),
                date_format: Default::default(),
                ..LayoutSettings::default()
            };
            let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
            let (_, geometry) = engine
                .compile_to_pixels(1.0)
                .unwrap_or_else(|e| panic!("{page_size:?} must compile: {e}"));
            geometry.page_height_pt
        };

        let a4 = height_of(PageSize::A4);
        let letter = height_of(PageSize::Letter);

        assert!(
            a4 > letter,
            "A4 ({a4}pt) should be taller than Letter ({letter}pt)"
        );
        // 297mm - 279.4mm = 17.6mm ≈ 49.9pt. Generous tolerance: the point is that
        // the setting reached the compiler, not that we re-derive the constant.
        assert!(
            (a4 - letter - 49.9).abs() < 2.0,
            "the gap should be ~49.9pt, got {:.1}pt",
            a4 - letter
        );
    }

    #[test]
    fn fmt_measure_strips_trailing_zeros() {
        assert_eq!(fmt_measure(16.0), "16");
        assert_eq!(fmt_measure(0.62), "0.62");
        assert_eq!(fmt_measure(10.7), "10.7");
        assert_eq!(fmt_measure(0.0), "0");
    }

    /// A document with no custom sections (every résumé written before D-9)
    /// must not gain a `customSections` entry in its `#let cv = (..)` dict —
    /// `RENDERER` itself references the key name unconditionally (it's fixed
    /// Typst source, present regardless of data), so the check is on the
    /// *data* the dict-builder emits, not on the bare substring.
    /// Renaming must reach the exported document, not just the panel — and an
    /// untouched document must generate exactly what it did before.
    #[test]
    fn a_renamed_section_reaches_the_generated_source() {
        use crate::resume::model::{ResumeDoc, SectionKind};

        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        doc.work.active_mut().push(Default::default());

        let before = generate(&doc.compose());
        // The *renderer* mentions `sectionTitles` (it reads the key); what must
        // be absent is the emitted data table, which carries the leading indent.
        assert!(
            !before.contains("  sectionTitles: ("),
            "an untouched document must not gain a titles table"
        );

        doc.set_section_title(SectionKind::Work, "Engineering");
        let after = generate(&doc.compose());
        assert!(
            after.contains("Engineering"),
            "the new heading must reach the source"
        );
        assert_ne!(before, after);

        // Blanking it clears the override rather than printing an empty heading.
        doc.set_section_title(SectionKind::Work, "   ");
        assert_eq!(
            generate(&doc.compose()),
            before,
            "a blank name returns to the default"
        );
    }

    #[test]
    fn no_custom_sections_means_no_custom_sections_key_in_the_dict() {
        let source = generate(&Resume::default());
        assert!(!source.contains("customSections: ("));
    }

    /// A custom section (D-9) reaches the generated Typst source with its
    /// title and entry fields, and — the real proof, not just string
    /// matching — the document still compiles.
    #[test]
    fn custom_section_renders_and_compiles() {
        use crate::resume::model::CustomEntry;
        use crate::typst_engine::TypstEngine;

        // Built through the document rather than by hand: an id comes from
        // `ResumeDoc`'s counter, and this is the path the app actually walks.
        let mut resume = Resume::default();
        resume.basics.name = "Test Person".into();
        let mut doc = ResumeDoc::from_resume(resume, "Base");
        let id = doc.add_custom_section("Publications");
        let content = doc.custom_section_mut(id).unwrap().content.active_mut();
        content.clear();
        content.push(CustomEntry {
            title: "A Paper on Something".into(),
            subtitle: "Journal of Examples".into(),
            start_date: "2024".into(),
            end_date: Default::default(),
            url: "https://example.com/paper".into(),
            highlights: vec!["Peer reviewed".into()],
        });

        let source = generate(&doc.compose());
        assert!(source.contains("customSections"));
        assert!(source.contains("A Paper on Something"));
        assert!(source.contains("Publications"));

        let engine = TypstEngine::new(source);
        let (_, geometry) = engine
            .compile_to_pixels(1.0)
            .expect("a document with a custom section must compile");
        assert_eq!(geometry.page_count, 1);
    }
}

#[cfg(test)]
mod section_order_tests {
    use super::*;
    use crate::resume::model::{Resume, ResumeDoc, SectionKind};

    /// The bug this guards, found by the user in the running app: sections
    /// reordered in the sidebar did not move in the rendered PDF. Order was a
    /// saved document field that reached `ResumeDoc::sections()` and then died
    /// at the `compose()` boundary — `Resume` had nowhere to carry it and the
    /// Typst renderer emitted a hard-coded sequence. A test asserting the
    /// *sidebar* order would have passed the whole time; this asserts the
    /// generated source, which is what actually prints.
    #[test]
    fn reordering_sections_reaches_the_generated_source() {
        let mut doc = ResumeDoc::from_resume(
            crate::resume::altacv::import(crate::resume::altacv::ALTACV_SAMPLE).unwrap(),
            "Base",
        );

        // Untouched: no `order` line at all, so existing documents generate
        // byte-identical source to before this feature.
        let before = generate(&doc.compose());
        assert!(
            !before.contains("order: ("),
            "default order must not be emitted"
        );

        // Move Education up one slot, so it sits above Work.
        doc.move_section(SectionKind::Education, -1);
        assert_eq!(
            doc.sections()[1],
            SectionKind::Education,
            "fixture: Education should now sit directly after Profile"
        );

        let after = generate(&doc.compose());
        assert!(
            after.contains("order: ("),
            "a reordered document must emit its order"
        );
        let order_line = after
            .lines()
            .find(|l| l.trim_start().starts_with("order: ("))
            .expect("order line");
        let education_at = order_line.find("education").expect("education in order");
        let work_at = order_line.find("\"work\"").expect("work in order");
        assert!(
            education_at < work_at,
            "education must precede work in the emitted order: {order_line}"
        );
    }

    /// Hiding one custom section must not move another one.
    ///
    /// The renderer used to address custom sections by their **position** in
    /// the emitted array, while the `order` list beside it is built by walking
    /// every section including the hidden ones. The two walks agree right up
    /// until a hidden custom section sits above a visible one — then the keys
    /// are off by one, and the survivor is drawn at the hidden section's
    /// place instead of its own. Nothing errored; the CV was just wrong.
    ///
    /// The invariant, stated so it cannot drift: hiding a section has to
    /// render exactly like that section having nothing in it.
    #[test]
    fn hiding_a_custom_section_leaves_the_others_where_they_are() {
        use crate::resume::model::CustomEntry;
        use crate::typst_engine::TypstEngine;

        // The document needs real content between the two custom sections:
        // on an empty CV every other section renders nothing, so "first" and
        // "last" collapse to the same place and a positional bug is invisible.
        let seeded = || Resume {
            basics: crate::resume::model::Basics {
                name: "Sofiia Medvedenko".into(),
                ..Default::default()
            },
            work: vec![crate::resume::model::Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                ..Default::default()
            }],
            ..Resume::default()
        };

        let build = || {
            let mut doc = ResumeDoc::from_resume(seeded(), "Base");
            let upper = doc.add_custom_section("Talks");
            let lower = doc.add_custom_section("Publications");
            for (id, title) in [(upper, "A talk"), (lower, "A paper")] {
                let content = doc.custom_section_mut(id).unwrap().content.active_mut();
                content.clear();
                content.push(CustomEntry {
                    title: title.into(),
                    ..Default::default()
                });
            }
            // Off the default order, so the `order` list is actually emitted
            // — that is the code path the keys are read on.
            for _ in 0..doc.sections().len() {
                doc.move_section(SectionKind::Custom(upper), -1);
            }
            (doc, upper, lower)
        };

        let pixels = |doc: &ResumeDoc| {
            TypstEngine::new(generate(&doc.compose()))
                .compile_to_pixels(1.0)
                .expect("compiles")
                .0
                .rgba
        };

        let (mut hidden_doc, upper, lower) = build();
        hidden_doc.set_hidden(SectionKind::Custom(upper), true);

        let (mut emptied_doc, upper_b, _) = build();
        emptied_doc
            .custom_section_mut(upper_b)
            .unwrap()
            .content
            .active_mut()
            .clear();

        assert!(
            pixels(&hidden_doc) == pixels(&emptied_doc),
            "hiding the section above moved the one below it — the renderer is \
             counting custom sections instead of naming them"
        );

        // And the survivor is genuinely on the page, so the equality above
        // cannot be satisfied by both documents drawing nothing.
        let (mut both_gone, upper_c, lower_c) = build();
        both_gone.set_hidden(SectionKind::Custom(upper_c), true);
        both_gone.set_hidden(SectionKind::Custom(lower_c), true);
        assert!(
            pixels(&hidden_doc) != pixels(&both_gone),
            "the surviving section drew nothing, so the comparison above proves nothing"
        );
        let _ = lower;
    }

    /// A custom section keeps its place in the order, and its renderer key
    /// matches its position in the emitted `customSections` array.
    #[test]
    fn a_custom_section_keeps_its_position_in_the_order() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let id = doc.add_custom_section("Publications");
        doc.custom_section_mut(id)
            .unwrap()
            .content
            .active_mut()
            .push(crate::resume::model::CustomEntry {
                title: "A paper".into(),
                ..Default::default()
            });

        // Pull it to the very top.
        for _ in 0..doc.sections().len() {
            doc.move_section(SectionKind::Custom(id), -1);
        }
        assert_eq!(doc.sections()[0], SectionKind::Custom(id));

        let source = generate(&doc.compose());
        let order_line = source
            .lines()
            .find(|l| l.trim_start().starts_with("order: ("))
            .expect("order line");
        assert!(
            order_line.trim_start().starts_with("order: (\"custom0\""),
            "the custom section leads the order: {order_line}"
        );
    }
}

#[cfg(test)]
mod empty_document_tests {
    use super::*;
    use crate::resume::model::Resume;
    use crate::typst_engine::TypstEngine;

    /// A brand-new blank CV must compile. It did not: an all-empty profile
    /// emitted `basics: (\n)`, which Typst reads as an empty **array** rather
    /// than an empty dictionary (`(:)`), so the renderer's
    /// `cv.basics.at("name", …)` indexed an array with a string and the whole
    /// preview failed with "expected integer, found string".
    ///
    /// This is the first screen after "Skip — start blank", and nothing in the
    /// suite had ever compiled a document with no content in it.
    #[test]
    fn a_blank_document_still_compiles() {
        let source = generate(&Resume::default());
        assert!(
            source.contains("basics: (:)"),
            "an empty profile must emit an empty dictionary, not an empty array"
        );

        let mut engine = TypstEngine::new(String::new());
        engine.set_source(source);
        engine
            .compile_to_pixels(1.0)
            .expect("a blank CV must compile");
    }
}

#[cfg(test)]
mod date_format_tests {
    use super::*;
    use crate::resume::model::{DateFormat, LayoutSettings, Resume, Work};

    /// The document's date format has to reach the *generated source*, not
    /// just sit in the model — the same boundary that swallowed section order
    /// (E-12). And what the user typed must survive in the file while only
    /// its rendering changes.
    #[test]
    fn the_date_format_reaches_the_generated_source() {
        let resume = Resume {
            work: vec![Work {
                position: "Senior Engineer".into(),
                name: "Acme".into(),
                start_date: "2022-01".into(),
                end_date: "2024-06-15".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let with = |format: DateFormat| {
            generate_with_layout(
                &resume,
                &LayoutSettings {
                    date_format: format,
                    ..Default::default()
                },
            )
        };

        let iso = with(DateFormat::Iso);
        assert!(iso.contains("2022-01"), "iso keeps the stored shape");
        assert!(iso.contains("2024-06-15"));

        let uk = with(DateFormat::DayMonShortYear);
        assert!(
            uk.contains("Jan 2022"),
            "month-only degrades, no invented day"
        );
        assert!(uk.contains("15 Jun 2024"));
        assert!(!uk.contains("2024-06-15"), "the ISO form must be gone");

        let us = with(DateFormat::MonthDayOrdinalYear);
        assert!(us.contains("January 2022"));
        assert!(us.contains("June 15th, 2024"));
    }

    /// Text that is not a date reaches the page untouched whatever the format
    /// is set to — the promise `resume::dates` is built around.
    #[test]
    fn unparseable_dates_print_as_written_under_every_format() {
        let resume = Resume {
            work: vec![Work {
                position: "Engineer".into(),
                start_date: "Summer 2021".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        for format in DateFormat::ALL {
            let source = generate_with_layout(
                &resume,
                &LayoutSettings {
                    date_format: format,
                    ..Default::default()
                },
            );
            assert!(
                source.contains("Summer 2021"),
                "{format:?} dropped text it could not parse"
            );
        }
    }

    #[test]
    fn neutralize_escapes_backslashes_and_special_markup() {
        assert_eq!(
            neutralize(r"C:\#1 @user [test] $100"),
            r"C:\\\#1 \@user \[test\] \$100"
        );
    }

    /// Task A12: Document metadata is set cleanly and never leaks internal preset names,
    /// variant names or private vault state.
    #[test]
    fn pdf_metadata_provenance_stays_clean_and_never_leaks_presets() {
        use crate::resume::model::{Basics, Preset};
        let resume = Resume {
            basics: Basics {
                name: "Alexey Belochenko".into(),
                label: "Principal Systems Architect".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let doc = ResumeDoc {
            presets: vec![
                Preset {
                    name: "FAANG · concise".into(),
                    selection: vec![],
                    hidden: vec![],
                },
                Preset {
                    name: "Startup · long".into(),
                    selection: vec![],
                    hidden: vec![],
                },
            ],
            ..ResumeDoc::from_resume(resume.clone(), "Base")
        };

        let typst_source = generate_for(&doc);
        assert!(typst_source.contains(r#"#set document(title: "Alexey Belochenko - Principal Systems Architect", author: "Alexey Belochenko")"#));
        assert!(!typst_source.contains("FAANG"));
        assert!(!typst_source.contains("concise"));
        assert!(!typst_source.contains("Startup"));
    }
}

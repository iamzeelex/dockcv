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

use crate::resume::model::{DateFormat, LayoutSettings, Resume, ResumeDoc, SectionKind};

/// The renderer body, written in Typst: helper functions plus `render-cv`.
/// Page/text setup is *not* here — it is generated per-document by
/// [`page_setup`] from the document's own [`LayoutSettings`], not baked into
/// this constant (C1, US-07). This is still one self-contained block: no
/// packages, no network, bundled fonts only (US-10).
const RENDERER: &str = r##"
#let muted = luma(110)

#let section(title) = block(
  width: 100%, above: 12pt, below: 6pt,
  fill: luma(238), inset: (x: 8pt, y: 4pt), radius: 2pt,
  align(center, text(weight: "bold", size: 9pt, tracking: 1pt, upper(title))),
)

#let daterange(start, end) = {
  if start == "" and end == "" { "" }
  else if end == "" { start + " – Present" }
  else { start + " – " + end }
}

#let meta(items) = {
  let parts = items.filter(x => x != none and x != "")
  text(fill: muted, size: 9pt, parts.join("  |  "))
}

#let entry(title, subtitle, trailing) = grid(
  columns: (1fr, auto), column-gutter: 10pt,
  align: (left + bottom, right + bottom),
  {
    text(weight: "bold", title)
    if subtitle != "" { emph(", " + subtitle) }
  },
  meta(trailing),
)

#let render-cv(cv) = {
  let b = cv.at("basics", default: (:))

  align(center, {
    text(size: 20pt, weight: "bold", b.at("name", default: ""))
    if b.at("label", default: "") != "" {
      h(8pt)
      text(size: 12pt, style: "italic", fill: muted, b.at("label", default: ""))
    }
  })
  v(2pt)
  let links = b.at("profiles", default: ()).map(p => p.at("url", default: ""))
  align(center, meta((
    b.at("location", default: ""),
    b.at("email", default: ""),
    b.at("phone", default: ""),
    b.at("url", default: ""),
    ..links,
  )))

  let summary = b.at("summary", default: none)
  let titles = cv.at("sectionTitles", default: (:))
  let heading(key, fallback) = titles.at(key, default: fallback)

  // Each section is a closure so the document's own order (`order`, below)
  // decides the sequence. Before this they were emitted inline, one after
  // another, which meant `ResumeDoc::section_order` — a real, saved,
  // drag-reorderable field — reached the sidebar and stopped there: the PDF
  // always printed the built-in order no matter what the user arranged.
  let render-profile() = {
    if summary != none { section(heading("Profile", "Profile")); summary }
  }

  let render-work() = {
  let work = cv.at("work", default: ())
  if work.len() > 0 {
    section(heading("Work", "Work Experience"))
    for w in work {
      entry(
        w.at("position", default: ""),
        w.at("name", default: ""),
        (daterange(w.at("startDate", default: ""), w.at("endDate", default: "")),
         w.at("location", default: "")),
      )
      let s = w.at("summary", default: none)
      if s != none { s }
      let hs = w.at("highlights", default: ())
      if hs.len() > 0 { list(..hs) }
      v(4pt)
    }
  }
  }

  let render-education() = {
  let edu = cv.at("education", default: ())
  if edu.len() > 0 {
    section(heading("Education", "Education"))
    for e in edu {
      entry(
        e.at("studyType", default: ""),
        e.at("institution", default: ""),
        (daterange(e.at("startDate", default: ""), e.at("endDate", default: "")),),
      )
      let hs = e.at("highlights", default: ())
      if hs.len() > 0 { list(..hs) }
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
    text(size: 8.5pt, weight: if strong { "bold" } else { "regular" }, body),
  )

  let render-skills() = {
  // `skills` is the settings dict from the preamble; the section's own data
  // is `groups`, named apart so the two cannot be confused.
  let groups = cv.at("skills", default: ())
  if groups.len() > 0 {
    section(heading("Skills", "Skills"))

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
    section(heading("Certificates", "Certifications"))
    for c in certs {
      entry(
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
    section(heading("Organizations", "Organizations"))
    for o in orgs {
      entry(
        o.at("position", default: ""),
        o.at("organization", default: ""),
        (daterange(o.at("startDate", default: ""), o.at("endDate", default: "")),),
      )
      let hs = o.at("highlights", default: ())
      if hs.len() > 0 { list(..hs) }
      v(3pt)
    }
  }
  }

  // User-added sections (D-9): one generic block per section, laid out with
  // the same `section`/`entry`/`list` helpers the six built-ins use above,
  // so a custom section reads as part of the document, not a bolt-on.
  let customs = cv.at("customSections", default: ())
  let render-custom(idx) = {
    if idx < customs.len() {
      let cs = customs.at(idx)
      let items = cs.at("entries", default: ())
      if items.len() > 0 {
        section(cs.at("title", default: ""))
        for it in items {
          entry(
            it.at("title", default: ""),
            it.at("subtitle", default: ""),
            (daterange(it.at("startDate", default: ""), it.at("endDate", default: "")),),
          )
          let hs = it.at("highlights", default: ())
          if hs.len() > 0 { list(..hs) }
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
  let default-order = ("profile", "work", "education", "skills", "certificates", "organizations") + range(customs.len()).map(i => "custom" + str(i))
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
    page_setup_into(&mut out, &layout.sanitized());
    out.push('\n');
    out.push_str(RENDERER);
    out.push_str("\n#let cv = ");
    resume_to_dict_into(&mut out, resume, layout.date_format);
    out.push_str("\n#render-cv(cv)\n");
    out
}

fn page_setup_into(out: &mut String, layout: &LayoutSettings) {
    use std::fmt::Write;
    let paper = layout.page_size.typst_paper_name();
    let size_pt = 10.0 * layout.text_scale_pct as f32 / 100.0;
    let _ = write!(
        out,
        r##"#set page(paper: "{paper}", fill: white, margin: (x: {x}mm, top: {top}mm, bottom: {bottom}mm))
#set text(font: "{font}", size: {size}pt, fill: rgb("#1a1a1a"))
#set par(justify: true, leading: {leading}em)

// How the Skills section is set. One dict rather than five bindings: they are
// one decision, and the renderer reads them together.
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
        skills_style = layout.skills.style.keyword(),
        skills_sep = layout.skills.separator.printed(),
        mark_before = layout.skills.mark.wraps().0,
        mark_after = layout.skills.mark.wraps().1,
        skills_gap = fmt_measure(layout.skills.spacing.gap_pt()),
        skills_bullets = layout.skills.bullets,
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
/// Custom sections are keyed positionally (`custom0`, `custom1`, …) instead,
/// since they have no fixed name.
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
    // `compose` emits custom sections already in document order, so the nth
    // custom section in the order list is `custom{n}` in that array.
    let mut custom_seen = 0usize;
    let order_keys: Vec<String> = r
        .section_order
        .iter()
        .filter_map(|kind| match kind {
            SectionKind::Custom(_) => {
                let key = format!("custom{custom_seen}");
                custom_seen += 1;
                Some(key)
            }
            other => renderer_key(*other).map(|k| k.to_string()),
        })
        .collect();
    let default_order: Vec<String> = ResumeDoc::SECTIONS
        .iter()
        .filter_map(|k| renderer_key(*k).map(|s| s.to_string()))
        .chain((0..custom_seen).map(|i| format!("custom{i}")))
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
                ..LayoutSettings::default()
            };
            let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
            let pdf = engine
                .compile_to_pdf()
                .unwrap_or_else(|e| panic!("{} did not compile: {e}", style.label()));
            assert!(pdf.starts_with(b"%PDF"), "{} produced no PDF", style.label());
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
        use crate::typst_engine::TypstEngine;
        use crate::resume::model::Work;

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
        assert!(source.contains(r#"paper: "us-letter""#), "page size was dropped");
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
        assert!(source.contains("https://spe.org/cert/42"), "the url is not emitted");
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
        use crate::resume::model::{ComposedCustomSection, CustomEntry};
        use crate::typst_engine::TypstEngine;

        let mut resume = Resume::default();
        resume.basics.name = "Test Person".into();
        resume.custom_sections.push(ComposedCustomSection {
            title: "Publications".into(),
            entries: vec![CustomEntry {
                title: "A Paper on Something".into(),
                subtitle: "Journal of Examples".into(),
                start_date: "2024".into(),
                end_date: Default::default(),
                url: "https://example.com/paper".into(),
                highlights: vec!["Peer reviewed".into()],
            }],
        });

        let source = generate(&resume);
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
        assert!(!before.contains("order: ("), "default order must not be emitted");

        // Move Education up one slot, so it sits above Work.
        doc.move_section(SectionKind::Education, -1);
        assert_eq!(
            doc.sections()[1],
            SectionKind::Education,
            "fixture: Education should now sit directly after Profile"
        );

        let after = generate(&doc.compose());
        assert!(after.contains("order: ("), "a reordered document must emit its order");
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

    /// A custom section keeps its place in the order, and its renderer key
    /// matches its position in the emitted `customSections` array.
    #[test]
    fn a_custom_section_keeps_its_position_in_the_order() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let id = doc.add_custom_section("Publications");
        doc.custom_section_mut(id).unwrap().content.active_mut().push(
            crate::resume::model::CustomEntry {
                title: "A paper".into(),
                ..Default::default()
            },
        );

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
        engine.compile_to_pixels(1.0).expect("a blank CV must compile");
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
        assert!(uk.contains("Jan 2022"), "month-only degrades, no invented day");
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
        assert_eq!(neutralize(r"C:\#1 @user [test] $100"), r"C:\\\#1 \@user \[test\] \$100");
    }
}

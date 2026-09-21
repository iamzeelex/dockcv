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
  // A real `heading` element, not a styled block: it is what puts an `/H2`
  // in the PDF's structure tree, so a tag-aware parser reads the section
  // boundary from the document rather than inferring it from geometry. The
  // `#show heading` rule in the preamble hands the styling straight back, so
  // nothing about the page changes.
  let body = heading(level: 2, outlined: false, text(
    weight: "bold",
    size: size-heading,
    // Letter-spacing is a decision about capitals — it is what stops a run of
    // them setting solid. On mixed case it only loosens the word.
    //
    // Measured rather than chosen: at 0.1em and above, four of the eight
    // extractors the conformance harness reads with return `E D U C AT I O N`
    // and lose every section heading in the document — pdf-extract, pdfminer,
    // and `pdftotext` in two of its three modes. 0.05em and 0.08em are clean
    // across every heading size and text scale the app offers. This is the
    // lower of the two, and `em` rather than `pt` so a document set at 120%
    // does not walk back over the cliff. See `src/ats/conformance.rs`.
    tracking: if h.case == "upper" { 0.05em } else { 0pt },
    words,
  ))
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
  else if end == "" { start + " – " + present-label }
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

// Text and target are two different values and are kept that way: the page
// prints `vestergaard.dev` because that is what its owner wrote, and follows
// `https://vestergaard.dev` because the shorter form is a relative reference
// that no viewer can resolve. `resume::links` prepares every `href`; an empty
// one means the field held something that is not an address, and the text
// prints without pretending to be a link.
#let followable(shown, href) = {
  // In a box, so the line breaker cannot take the address apart. UAX #14
  // offers a break after every `/`, and a header narrow enough to need one put
  // `linkedin.com/in/` at the end of a line and `nora-vestergaard` at the start
  // of the next — an address a reader cannot copy and a PDF reader brings back
  // as two unrelated fields.
  if shown == "" { none }
  else if href == "" { box(shown) }
  else { box(link(href)[#shown]) }
}

// The small indicator saying there is something to follow.
//
// Drawn in Geist explicitly, never in the document's own font: Newsreader and
// PT Serif lack U+2197, and Typst answers a missing glyph by falling back to
// system fonts rather than by erroring — exactly the silent substitution L-11
// was written against. Geist is always bundled and covers it.
#let link-mark = if show-link-marks {
  text(font: "Geist", size: 0.85em, " ↗")
} else {
  none
}

// A dated entry: title, subtitle, and the date/location pair.
//
// `trailing` arrives in document order (date, location); `entry-meta-order`
// decides whether it prints that way. Empty parts are dropped by `meta`, so a
// job with no location reads the same under either order.
#let entry(el, title, subtitle, trailing, href: "") = {
  let ordered = if el.order == "location-first" { trailing.rev() } else { trailing }
  let head-inner = {
    text(weight: "bold", title)
    if subtitle != "" { styled(el.subtitle, ", " + subtitle) }
  }
  let head = text(size: size-entry, {
    if href != "" {
      link(href)[#head-inner#link-mark]
    } else {
      head-inner
    }
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
    // Level 1, and the section bars below are level 2: the name is this
    // document's title, and a heading tree that starts at level 2 is the one
    // thing PDF/UA-1 refuses the file for. The `#show heading` rule keeps the
    // element from bringing any styling of its own, so the name sets exactly
    // as it did.
    heading(level: 1, outlined: false,
      text(size: size-name, weight: "bold", b.at("name", default: "")))
    if b.at("label", default: "") != "" {
      h(8pt)
      text(size: size-title, style: "italic", fill: muted, b.at("label", default: ""))
    }
  })
  v(2pt)

  let email-item = followable(b.at("email", default: ""), b.at("emailHref", default: ""))
  let phone-item = followable(b.at("phone", default: ""), b.at("phoneHref", default: ""))
  let url-item = followable(b.at("url", default: ""), b.at("urlHref", default: ""))
  let profile-items = b.at("profiles", default: ()).map(p =>
    followable(p.at("url", default: ""), p.at("href", default: "")))
  // Empty parts are dropped here rather than in each branch below, so a CV
  // with no phone leaves no gap and no stray separator whichever shape is
  // chosen.
  let details = (
    b.at("location", default: ""),
    email-item,
    phone-item,
    url-item,
    ..profile-items,
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
  // Named apart from Typst's `heading` element, which `section` now emits.
  let section-title(key, fallback) = titles.at(key, default: fallback)

  // Each section is a closure so the document's own order (`order`, below)
  // decides the sequence. Before this they were emitted inline, one after
  // another, which meant `ResumeDoc::section_order` — a real, saved,
  // drag-reorderable field — reached the sidebar and stopped there: the PDF
  // always printed the built-in order no matter what the user arranged.
  let render-profile() = {
    if summary != none { section("profile", section-title("Profile", "Profile")); summary }
  }

  let render-work() = {
  let work = cv.at("work", default: ())
  if work.len() > 0 {
    section("work", section-title("Work", "Work Experience"))
    let el = entry-of("work")
    for w in work {
      entry(
        el,
        w.at("position", default: ""),
        w.at("name", default: ""),
        (daterange(w.at("startDate", default: ""), w.at("endDate", default: "")),
         w.at("location", default: "")),
        href: w.at("href", default: ""),
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
    section("education", section-title("Education", "Education"))
    let el = entry-of("education")
    for e in edu {
      entry(
        el,
        e.at("studyType", default: ""),
        e.at("institution", default: ""),
        (daterange(e.at("startDate", default: ""), e.at("endDate", default: "")),),
        href: e.at("href", default: ""),
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
    section("skills", section-title("Skills", "Skills"))

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
    section("certificates", section-title("Certificates", "Certifications"))
    let el = entry-of("certificates")
    for c in certs {
      let u = c.at("url", default: "")
      entry(
        el,
        c.at("name", default: ""),
        c.at("issuer", default: ""),
        (c.at("date", default: ""),),
        href: c.at("href", default: ""),
      )
      // The link was stored, saved and editable, and the page never printed
      // it — a value that reaches the model and not the output. Custom
      // sections have shown theirs all along; this is the same line. It is
      // followable too: a printed URL is the thing a reader clicks first.
      if u != "" { meta((followable(u, c.at("href", default: "")),)) }
      v(2pt)
    }
  }
  }

  let render-organizations() = {
  let orgs = cv.at("volunteer", default: ())
  if orgs.len() > 0 {
    section("organizations", section-title("Organizations", "Organizations"))
    let el = entry-of("organizations")
    for o in orgs {
      entry(
        el,
        o.at("position", default: ""),
        o.at("organization", default: ""),
        (daterange(o.at("startDate", default: ""), o.at("endDate", default: "")),),
        href: o.at("href", default: ""),
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
          let u = it.at("url", default: "")
          entry(
            el,
            it.at("title", default: ""),
            it.at("subtitle", default: ""),
            (daterange(it.at("startDate", default: ""), it.at("endDate", default: "")),),
            href: it.at("href", default: ""),
          )
          let hs = it.at("highlights", default: ())
          if hs.len() > 0 { bullets(el, hs) }
          if u != "" { meta((followable(u, it.at("href", default: "")),)) }
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

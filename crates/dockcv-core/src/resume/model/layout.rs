//! Document-wide page geometry and typography.

use serde::{Deserialize, Serialize};

use crate::resume::dates::DateFormat;
use super::layout_sections::{EntryLayout, HeaderLayout, HeadingLayout, SkillsLayout};

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------
//
// Everything here used to be a constant baked into `resume/template.rs`'s
// `PREAMBLE`. It travels with the document instead (see `ResumeDoc::layout`'s
// doc comment) — a named enum for page size (never a bare string; the Typst
// paper-preset name is derived from it, not stored), and physical-unit
// scalars for margins and type scale rather than a pre-formatted Typst
// snippet, which would put Typst syntax in the data model.
//
// Values here are not assumed valid: `LayoutSettings::sanitized` clamps them
// before `resume/template.rs` ever formats them into Typst source, so a
// corrupted or hand-edited vault file can't produce an unreadable document or
// a Typst compile error from a zero/negative measurement.

/// Page size for the rendered document. Named rather than a raw string or
/// dimensions pair — CLAUDE.md's "no unnamed-tuple-shaped data" rule — and
/// deliberately small: the two sizes a résumé is ever printed to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageSize {
    #[default]
    A4,
    Letter,
}

impl PageSize {
    /// The Typst paper-preset name for `#set page(paper: ..)`. Checked
    /// against `typst-library` 0.15's own paper table (`page.rs`) rather than
    /// guessed — Typst's Letter preset is named `"us-letter"`, not `"letter"`.
    pub fn typst_paper_name(self) -> &'static str {
        match self {
            PageSize::A4 => "a4",
            PageSize::Letter => "us-letter",
        }
    }

    /// Physical page dimensions in millimeters, `(width, height)` — used only
    /// to clamp margins to something the page can still hold.
    /// Page width in typographic points — what the preview needs to work out
    /// how many pixels per point it must rasterize at to be sharp at the size
    /// it is actually drawing the sheet.
    pub fn width_pt(self) -> f32 {
        // 1 in = 25.4 mm = 72 pt.
        self.dimensions_mm().0 * 72.0 / 25.4
    }

    /// Page height in typographic points. The companion to [`Self::width_pt`],
    /// and what "fit the whole page in the pane" needs: one sheet's proportion,
    /// not the rendered stack's, which is as tall as the CV is long.
    pub fn height_pt(self) -> f32 {
        self.dimensions_mm().1 * 72.0 / 25.4
    }

    fn dimensions_mm(self) -> (f32, f32) {
        match self {
            PageSize::A4 => (210.0, 297.0),
            PageSize::Letter => (215.9, 279.4),
        }
    }
}

/// The families a document can be set in — every one of them bundled, so a
/// CV renders the same on any machine and the app still makes no network call
/// (US-10).
///
/// System fonts are deliberately **not** offered. A résumé set in a face the
/// next machine does not have is a document that silently reflows, and the
/// whole point of File-over-App is that the vault is portable. If system
/// fonts arrive later they need an explicit "this file needs a font you may
/// not have" story, not a silent picker entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentFont {
    /// Typst's own serif, and what every document used before this existed.
    #[default]
    LibertinusSerif,
    /// The display serif the app itself is set in — warmer, more editorial.
    Newsreader,
    /// A classic screen-and-print serif.
    PtSerif,
    /// The interface sans. Half of all CV templates are sans; until this
    /// existed, none of ours could be.
    Geist,
    /// For a CV that wants to look like a terminal. Rare, and asked for.
    JetBrainsMono,
}

impl DocumentFont {
    /// The family name Typst matches on — must equal the name inside the
    /// font file, not a label of our choosing.
    pub fn family(self) -> &'static str {
        match self {
            Self::LibertinusSerif => "Libertinus Serif",
            // The family name inside the file, not the file's own name:
            // Newsreader ships as an optical-size family and calls itself
            // "Newsreader 16pt". A test asserts every entry here resolves,
            // because Typst answers a missing family by silently falling back
            // rather than failing — the picker would have "worked" and
            // changed nothing.
            Self::Newsreader => "Newsreader 16pt",
            Self::PtSerif => "PT Serif",
            Self::Geist => "Geist",
            Self::JetBrainsMono => "JetBrains Mono",
        }
    }

    /// What the picker calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::LibertinusSerif => "Libertinus Serif",
            Self::Newsreader => "Newsreader",
            Self::PtSerif => "PT Serif",
            Self::Geist => "Geist Sans",
            Self::JetBrainsMono => "JetBrains Mono",
        }
    }

    pub const ALL: [DocumentFont; 5] = [
        Self::LibertinusSerif,
        Self::Newsreader,
        Self::PtSerif,
        Self::Geist,
        Self::JetBrainsMono,
    ];
}

/// Page margins, in millimeters. Kept as the three edges the original
/// `PREAMBLE` hard-coded (`x` symmetric left/right, `top`, `bottom`) rather
/// than collapsed to one uniform value: a single Margins slider is a
/// plausible *UI* simplification over this (see the Typst-controls spec
/// §10, left open there), but the stored shape keeps a vault written before
/// this field existed rendering with the exact same margins it always had.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Margins {
    pub x_mm: f32,
    pub top_mm: f32,
    pub bottom_mm: f32,
}

impl Margins {
    /// Set every edge to `mm`.
    ///
    /// The design draws **one** "Margins" slider while the model keeps three
    /// edges (O-10). Both are right: a hand-edited file may legitimately want
    /// an asymmetric page, and the rail's one control cannot express that — so
    /// moving it unifies the three. That is a change the user asked for by
    /// dragging a control labelled "Margins", not a silent flattening, and
    /// [`Margins::is_uniform`] lets the readout say so before they do.
    pub fn set_uniform(&mut self, mm: f32) {
        self.x_mm = mm;
        self.top_mm = mm;
        self.bottom_mm = mm;
    }

    /// Whether all three edges agree — i.e. whether one slider can honestly
    /// describe this page.
    pub fn is_uniform(&self) -> bool {
        const EPSILON: f32 = 0.01;
        (self.x_mm - self.top_mm).abs() < EPSILON && (self.x_mm - self.bottom_mm).abs() < EPSILON
    }
}

impl Default for Margins {
    /// Matches the old `PREAMBLE` constant: `margin: (x: 1.6cm, top: 1.4cm,
    /// bottom: 1.4cm)`.
    fn default() -> Self {
        Self {
            x_mm: 16.0,
            top_mm: 14.0,
            bottom_mm: 14.0,
        }
    }
}

/// The size of each element that is not body text, as **points added to the
/// document's base size**.
///
/// Deltas rather than absolutes, so that "Text scale" means what its name
/// says. Before this the name was a flat `20pt` while the body scaled with
/// the control, so a CV set to 85% had a *larger* size contrast than the same
/// CV at 100%: the scale control was quietly editing the hierarchy instead of
/// the size. Storing offsets makes the hierarchy the user's decision and the
/// scale a multiplier over all of it.
///
/// Every default reproduces the number the template used to hard-code, at the
/// default base of 10pt: name 10+10=20, title 10+2=12, heading 10−1=9, entry
/// title 10+0=10.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeSizes {
    #[serde(default = "TypeSizes::default_name")]
    pub name_pt: f32,
    /// The professional title beside the name.
    #[serde(default = "TypeSizes::default_title")]
    pub title_pt: f32,
    /// The bar above each section.
    #[serde(default = "TypeSizes::default_heading")]
    pub heading_pt: f32,
    /// A dated entry's title line — job title, degree, certificate.
    #[serde(default)]
    pub entry_pt: f32,
}

impl Default for TypeSizes {
    fn default() -> Self {
        Self {
            name_pt: Self::default_name(),
            title_pt: Self::default_title(),
            heading_pt: Self::default_heading(),
            entry_pt: 0.0,
        }
    }
}

impl TypeSizes {
    fn default_name() -> f32 {
        10.0
    }
    fn default_title() -> f32 {
        2.0
    }
    fn default_heading() -> f32 {
        -1.0
    }

    /// Below this nothing survives being printed, and it is also what keeps
    /// `base + delta` positive when the base is at its floor and the offset
    /// at its own — a negative text size is a Typst compile error.
    pub const MIN_PT: f32 = 4.0;
    /// How far an element may be pushed from the body size. One range for all
    /// four: the floor above is what protects legibility, so this only has to
    /// stop a hand-edited file from asking for a 90pt name.
    pub const DELTA_RANGE: (f32, f32) = (-4.0, 18.0);
    /// What one press of the rail's `+`/`−` moves. Half a point, because the
    /// difference between a 12pt and a 12.5pt title is visible on a page and
    /// a whole point is a coarser adjustment than this control is for.
    pub const STEP_PT: f32 = 0.5;

    /// The date/location line, and the pills a bubbled Skills section is made
    /// of. Not controls — they are *derived* from the body size and always sit
    /// just under it — but expressed the same way, so they follow the base
    /// like everything else. At the default base they are still the 9pt and
    /// 8.5pt the template used to hard-code.
    pub const META_PT: f32 = -1.0;
    pub const PILL_PT: f32 = -1.5;

    /// The size an element is actually set at, given the document's base.
    pub fn resolve(base_pt: f32, delta_pt: f32) -> f32 {
        (base_pt + delta_pt).max(Self::MIN_PT)
    }

    fn sanitized(&self) -> Self {
        let (lo, hi) = Self::DELTA_RANGE;
        Self {
            name_pt: self.name_pt.clamp(lo, hi),
            title_pt: self.title_pt.clamp(lo, hi),
            heading_pt: self.heading_pt.clamp(lo, hi),
            entry_pt: self.entry_pt.clamp(lo, hi),
        }
    }
}

/// Page layout and type scale for the rendered document. See `ResumeDoc::layout`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutSettings {
    pub page_size: PageSize,
    /// The family the document is set in.
    ///
    /// `#[serde(default)]` keeps every document written before this existed
    /// rendering byte-identically: the default is the serif Typst was already
    /// using, so nothing shifts under a user who never touched the control.
    #[serde(default)]
    pub font: DocumentFont,
    /// How every date in the document is printed.
    ///
    /// A document-wide setting rather than per entry: a CV whose roles are
    /// dated `2022-01` on one line and `Jan 2022` on the next looks careless,
    /// and that inconsistency is exactly what free-text dates produced. The
    /// text a user types stays theirs (see `resume::dates`); this decides how
    /// it is *rendered*.
    #[serde(default)]
    pub date_format: DateFormat,
    /// How the Skills section is set. `#[serde(default)]` so a document
    /// written before this existed keeps the shape it had.
    #[serde(default)]
    pub skills: SkillsLayout,
    /// How a dated entry is set. `#[serde(default)]` so a document written
    /// before this existed keeps the shape it had.
    #[serde(default)]
    pub entries: EntryLayout,
    /// How the header is set. `#[serde(default)]` so a document written
    /// before this existed keeps the shape it had.
    #[serde(default)]
    pub header: HeaderLayout,
    /// How the bar above each section is set. `#[serde(default)]` so a
    /// document written before this existed keeps the band it had.
    #[serde(default)]
    pub headings: HeadingLayout,
    /// Whether linked entry titles carry the small `↗` mark.
    ///
    /// The default keeps every existing document pixel-identical. ATS-safe
    /// turns it off because the glyph is otherwise real text in PDF content
    /// order, where parsers read it as part of the institution or employer.
    #[serde(default = "default_show_link_marks")]
    pub show_link_marks: bool,
    /// The size of the name, the professional title, the section bars and
    /// an entry's title. `#[serde(default)]` so a document written before
    /// this existed keeps the sizes the template hard-coded.
    #[serde(default)]
    pub sizes: TypeSizes,
    /// Body text size as a percentage of the template's base size (10pt).
    /// 100 is the old hard-coded default; the layout rail's own readout
    /// (the Typst-controls spec — "107%") is this same unit.
    pub text_scale_pct: u16,
    /// Paragraph leading, as an em multiple of the (scaled) text size.
    /// Matches the old hard-coded `#set par(leading: 0.62em)`.
    pub leading_em: f32,
    pub margins: Margins,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            page_size: PageSize::default(),
            font: DocumentFont::default(),
            date_format: DateFormat::default(),
            skills: SkillsLayout::default(),
            entries: EntryLayout::default(),
            header: HeaderLayout::default(),
            headings: HeadingLayout::default(),
            show_link_marks: true,
            sizes: TypeSizes::default(),
            text_scale_pct: 100,
            leading_em: 0.62,
            margins: Margins::default(),
        }
    }
}

impl LayoutSettings {
    /// Text scale bounds: below 50% a résumé is unreadable, above 200% it
    /// cannot hold a page of content — the same "a setting the user cannot
    /// get wrong" reasoning the Typst-controls spec asks for.
    /// What the **layout rail's** sliders offer, which is deliberately much
    /// narrower than the clamps below.
    ///
    /// The clamps exist so a hand-edited file cannot produce something Typst
    /// refuses or a human cannot read: a 102 mm margin is *valid*, and absurd
    /// on a résumé. A slider whose travel is mostly unusable values is a bad
    /// control — its useful band would be a few pixels wide. So the rail
    /// offers the band people actually work in, and the clamps stay as the
    /// outer guard for files edited by hand.
    pub const MARGIN_MM_UI_RANGE: (f32, f32) = (8.0, 30.0);
    pub const TEXT_SCALE_PCT_UI_RANGE: (u16, u16) = (85, 120);

    const TEXT_SCALE_PCT_RANGE: (u16, u16) = (50, 200);
    /// Leading bounds: 0 or negative is a Typst compile error (leading must
    /// be a positive length); above 1.5em reads as double-spaced.
    const LEADING_EM_RANGE: (f32, f32) = (0.3, 1.5);
    /// Floor under which a margin is visually gone; the per-page ceiling is
    /// computed from the page size in `sanitized`.
    const MIN_MARGIN_MM: f32 = 3.0;

    /// The size body text is set at, in points — `text_scale_pct` applied to
    /// the template's 10pt base. Every other size in the document is this
    /// plus a [`TypeSizes`] offset, so the rail's readouts and the generated
    /// Typst have to agree on it.
    pub fn base_size_pt(&self) -> f32 {
        10.0 * self.text_scale_pct as f32 / 100.0
    }

    /// A copy of these settings with every value clamped into a range Typst
    /// can render and a human can read — called once, at the point
    /// `resume/template.rs` turns settings into Typst source, so nothing
    /// downstream ever has to re-check.
    pub fn sanitized(&self) -> Self {
        let (min_scale, max_scale) = Self::TEXT_SCALE_PCT_RANGE;
        let (min_leading, max_leading) = Self::LEADING_EM_RANGE;
        let (page_w, page_h) = self.page_size.dimensions_mm();
        // Leave at least a third of the page as printable area either way.
        let max_x = (page_w / 2.0 - Self::MIN_MARGIN_MM).max(Self::MIN_MARGIN_MM);
        let max_vertical = (page_h / 3.0).max(Self::MIN_MARGIN_MM);

        Self {
            page_size: self.page_size,
            font: self.font,
            date_format: self.date_format,
            // Nothing to clamp: every variant is a valid arrangement.
            skills: self.skills,
            entries: self.entries,
            header: self.header,
            // Nothing to clamp: every combination is a valid heading.
            headings: self.headings,
            show_link_marks: self.show_link_marks,
            sizes: self.sizes.sanitized(),
            text_scale_pct: self.text_scale_pct.clamp(min_scale, max_scale),
            leading_em: self.leading_em.clamp(min_leading, max_leading),
            margins: Margins {
                x_mm: self.margins.x_mm.clamp(Self::MIN_MARGIN_MM, max_x),
                top_mm: self.margins.top_mm.clamp(Self::MIN_MARGIN_MM, max_vertical),
                bottom_mm: self
                    .margins
                    .bottom_mm
                    .clamp(Self::MIN_MARGIN_MM, max_vertical),
            },
        }
    }
}

fn default_show_link_marks() -> bool {
    true
}


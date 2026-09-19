//! The resume data model — the single source of truth for the editor.
//!
//! It deliberately mirrors a subset of the [JSON Resume](https://jsonresume.org)
//! schema, which is also the shape AltaCV 1.5.0 uses for its `#let cv = (..)`
//! dictionary. Keeping the same vocabulary means the AltaCV importer maps 1:1
//! and our own template renders the exact same data.
//!
//! Free-text fields that may contain Typst markup (`summary`, `highlights`) are
//! stored as raw markup strings; plain fields (names, dates, URLs) are stored
//! verbatim and quoted on the way back into Typst.

use serde::{Deserialize, Serialize};

pub use super::applications::*;
pub use super::dates::{DateFormat, ResumeDate};
pub use super::export_settings::*;
pub use super::language::DocumentLanguage;
pub use super::layout::*;
pub use super::layout_sections::*;
pub use super::profiles::*;
pub use super::versioning::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Resume {
    pub basics: Basics,
    pub work: Vec<Work>,
    pub education: Vec<Education>,
    pub skills: Vec<SkillGroup>,
    pub certificates: Vec<Certificate>,
    /// JSON Resume `volunteer` — surfaced in the UI as "Organizations".
    pub volunteer: Vec<Volunteer>,
    /// User-added sections beyond the six above (D-9), each already resolved
    /// to its active variant's entries — this is the *composed*, render-ready
    /// shape; `ResumeDoc::custom_sections` is where the versioning lives.
    #[serde(default)]
    pub custom_sections: Vec<ComposedCustomSection>,
    /// The heading to print for each built-in section, already resolved from the
    /// user's overrides. Renaming a section is only real if it reaches the PDF.
    #[serde(default)]
    pub section_titles: Vec<(SectionKind, String)>,
    /// Where a section departs from the document's layout, already resolved.
    /// Only the sections that actually differ appear.
    #[serde(default)]
    pub section_overrides: Vec<(SectionKind, SectionOverrides)>,
    /// The order sections print in, already resolved from the document.
    ///
    /// `Resume` is the flat, render-ready shape, and until this field existed
    /// it had nowhere to put order — so `ResumeDoc::section_order` reached the
    /// sidebar and died at this boundary, and the PDF always printed the
    /// built-in sequence. Empty means "the order the built-ins ship in".
    #[serde(default)]
    pub section_order: Vec<SectionKind>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Basics {
    pub name: String,
    /// Job title / tagline.
    pub label: String,
    /// Profile blurb (Typst markup).
    pub summary: String,
    pub email: String,
    pub phone: String,
    pub location: String,
    pub url: String,
    pub profiles: Vec<NetworkProfile>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NetworkProfile {
    pub network: String,
    pub username: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Work {
    /// Employer name.
    pub name: String,
    pub position: String,
    pub location: String,
    pub start_date: ResumeDate,
    pub end_date: ResumeDate,
    /// Optional employer or project URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// Optional one-line summary (Typst markup).
    pub summary: String,
    /// Bullet points (Typst markup each).
    pub highlights: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Education {
    pub institution: String,
    pub study_type: String,
    pub start_date: ResumeDate,
    pub end_date: ResumeDate,
    pub url: String,
    /// Coursework, thesis, honours — the same `highlights` vocabulary `Work`,
    /// `Volunteer` and `CustomEntry` already use. Added because a CV's course
    /// list had nowhere to land and was being dropped on import; `default` so
    /// every document written before this still loads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SkillGroup {
    pub name: String,
    pub keywords: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Certificate {
    pub name: String,
    pub issuer: String,
    pub date: ResumeDate,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Volunteer {
    pub organization: String,
    pub position: String,
    pub start_date: ResumeDate,
    pub end_date: ResumeDate,
    /// Optional organization or initiative URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    pub highlights: Vec<String>,
}

// ---------------------------------------------------------------------------
// Custom sections (D-9, the roadmap)
// ---------------------------------------------------------------------------
//
// The six built-in sections above each have a shape suited to their content
// (`Work` isn't `Certificate` isn't `SkillGroup`). A user-added section can't
// borrow one of those wholesale — a Publications section isn't Work history —
// so it gets one generic entry shape instead, deliberately not a bag of
// `Option<String>`: `title`/`subtitle`/date range/`url`/`highlights` is the
// vocabulary `Work`, `Certificate` and `Volunteer` already share, wide enough
// to hold a publication (title, journal, date, DOI, abstract notes), a
// language (name, proficiency level as the subtitle, notes), an award (name,
// issuing body, date, notes), a talk (title, venue, date, link) or a patent
// (title, patent number as the subtitle, date, link) without inventing a
// field per section type. Empty string means "not used", exactly like the
// built-in entry types already do — no `Option` wrapping.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CustomEntry {
    /// The entry's headline: publication title, language name, award name,
    /// talk title, patent title.
    pub title: String,
    /// Secondary line: journal/publisher, proficiency level, issuing body,
    /// venue, patent number.
    pub subtitle: String,
    pub start_date: ResumeDate,
    pub end_date: ResumeDate,
    pub url: String,
    /// Bullet points (Typst markup each), same vocabulary as
    /// `Work::highlights` / `Volunteer::highlights`.
    pub highlights: Vec<String>,
}

/// A stable identifier for a user-added section. Newtype rather than a
/// section's position in `ResumeDoc::custom_sections` — a `Vec` index would
/// silently re-point at a different section the moment an earlier one is
/// deleted, and this id is held onto by `section_order`, `Preset::selection`
/// and `FieldId`, all three of which must survive that deletion honestly (see
/// `ResumeDoc::add_custom_section`'s doc comment for how uniqueness is kept).
/// `Copy` because it lives inside `FieldId`, which is also `Copy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustomSectionId(u32);

impl CustomSectionId {
    /// An id from a raw number, for a section that is not yet part of a
    /// document — a fixture, or one being read out of a file.
    ///
    /// Crate-private on purpose: ids are handed out by `next_custom_section_id`
    /// and never reissued (D-9), and a public constructor is how two live
    /// sections come to share one. An id made here is provisional; the moment
    /// the section joins a document, [`ResumeDoc::from_resume`] issues it a real
    /// one from that document's own counter.
    pub(crate) const fn from_u32(id: u32) -> Self {
        Self(id)
    }

    /// The number behind the id, for the one job that needs it: naming the
    /// section in the generated Typst (`custom7`). Nothing else should care
    /// what an id *is*.
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// A user-added section: a stable id, a user-editable title (the title lives
/// on the section, never on the id — a preset holds selections, never
/// content), and versioned content like every built-in section.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomSection {
    pub id: CustomSectionId,
    pub title: String,
    pub content: Versioned<Vec<CustomEntry>>,
}

/// A custom section resolved to its active variant's entries — the shape
/// [`Resume::compose`] emits for the renderer, mirroring how the six
/// built-in sections are already flattened to their active content.
///
/// Deliberately not `Default`: an id is issued by [`ResumeDoc`]'s counter and
/// a defaulted one would be `CustomSectionId(0)`, which is a real section's.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComposedCustomSection {
    /// The section's own id, carried through to the renderer so it is
    /// addressed by identity rather than by where it happens to sit. Before
    /// this the renderer indexed the array positionally, and hiding a custom
    /// section that sat above another one moved the survivor into its place.
    pub id: CustomSectionId,
    pub title: String,
    pub entries: Vec<CustomEntry>,
}

/// The professional diary: a running log of work achievements the user can
/// later turn into work-experience bullets. Stored at `<vault>/diary.toml`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Diary {
    pub entries: Vec<DiaryEntry>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiaryEntry {
    /// ISO date (YYYY-MM-DD) the entry was logged.
    pub date: String,
    /// The achievement text (Typst markup allowed).
    pub text: String,
    /// The role this win belongs to, as one label — `"Acme Corp · Senior SWE"`.
    ///
    /// The design's whole premise for this surface is "log it in one line and
    /// **tag it to a role**": an eight-year career spans employers and titles,
    /// and a win is only useful later if you know which of them it belongs to.
    /// One string rather than an employer/title pair because the user picks it
    /// from their own work history, where the pairing already lives — and a
    /// role typed here must not pretend to be a second source of truth about
    /// where they worked. Empty for an untagged entry.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    /// Free tags (`#performance`, `#mentoring`), stored without the `#`.
    ///
    /// These are the user's own words, never derived: the design draws a
    /// metric chip (`↓ 50% p99`) beside them, and that one is deliberately
    /// *not* modelled here — a number in a résumé must trace to something the
    /// user typed (US-14), and extracting one from prose is the AI layer's
    /// job, under review, not a field the diary fills in silently.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Marked as containing something that must not leave the vault verbatim.
    ///
    /// US-36, and the reason it is P0: the persona may work at a bank, a
    /// hospital or a government body, where the *fact* of a win is fine and the
    /// wording around it is not. *"Personal-data leak at client ACME"* is a
    /// real diary entry and an unemployable CV bullet. The story's rule is
    /// blunt: a confidential entry is **never** offered to a CV verbatim, only
    /// as an abstracted metric.
    ///
    /// The internal wording stays here, in the diary, which is the whole point:
    /// you keep the record you need for a performance review and a different,
    /// abstracted sentence goes outward. Nothing about this field redacts or
    /// rewrites anything — it marks, and the surfaces that could leak it check.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confidential: bool,
    /// Documents this win has been promoted into, as file stems.
    ///
    /// The other half of `Use in a CV` (US-06): six months later the question
    /// is *did I ever use this?*, and answering it by opening every CV is the
    /// reason people stop keeping a diary. The same idea as the library's
    /// `used in N CVs`, except this one is recorded rather than derived — a
    /// bullet is a bare `String` in the model, so there is nothing in a
    /// document to match a diary entry back to.
    ///
    /// A label, not a link: the document can be renamed or deleted afterwards
    /// and this still tells the truth about what was done at the time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub used_in: Vec<String>,
    /// The document that was open when this was captured, as its file stem.
    ///
    /// The review's whole complaint about `Use in a CV →` (P-05) is that the most
    /// valuable link in the product was drawn as a promise and never built. An
    /// entry captured from the editor knows which CV it came from, so it records
    /// that at capture time rather than asking the user to reconstruct it later.
    /// `None` for entries typed straight into the Diary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_doc: Option<String>,
}

/// The user's reusable block library — their "me", shared across every résumé
/// in the vault. Résumés are assembled by copying blocks out of these pools
/// (and blocks are added to them from résumés). Stored at `<vault>/library.toml`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Library {
    pub work: Vec<Work>,
    pub education: Vec<Education>,
    pub skills: Vec<SkillGroup>,
    pub certificates: Vec<Certificate>,
    pub volunteer: Vec<Volunteer>,
}

/// Keeps `next_custom_section_id` out of every document that has no custom
/// sections. TOML is the wire format and `git diff` readability is a product
/// requirement — a counter at zero is noise in every file on disk.
/// A stored override that is blank must not win over the default — it would
/// print an empty heading into the exported PDF.
fn title_is_blank(kind: &SectionKind, titles: &[(SectionKind, String)]) -> bool {
    titles
        .iter()
        .find(|(k, _)| k == kind)
        .is_some_and(|(_, t)| t.trim().is_empty())
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// A resume with every section independently versioned, plus document-wide
/// presets over those variants.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResumeDoc {
    pub profile: Versioned<Basics>,
    pub work: Versioned<Vec<Work>>,
    pub education: Versioned<Vec<Education>>,
    pub skills: Versioned<Vec<SkillGroup>>,
    pub certificates: Versioned<Vec<Certificate>>,
    pub volunteer: Versioned<Vec<Volunteer>>,
    pub presets: Vec<Preset>,
    /// Language of the working reading.
    ///
    /// English is represented by absence so every pre-C5 document keeps its
    /// exact meaning and the common case stays out of TOML. Presets pin their
    /// own copy and restore it wholesale, just like headings and order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// The order sections appear in, which the user can change.
    ///
    /// Order is **data**, not a constant: a Platform CV leads with Skills, an
    /// academic one with Education. Empty means "the default order" — that keeps
    /// documents written before this field existed loading unchanged, and keeps
    /// the common case out of every file on disk.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub section_order: Vec<SectionKind>,
    /// Page geometry and type scale for the rendered/exported document.
    ///
    /// Page size in particular is not cosmetic: an EU application wants A4, a
    /// US one wants Letter, and the same person applies to both — this is a
    /// property of the CV, not an app-wide preference (the product review
    /// §1, US-07). `#[serde(default)]` reproduces exactly the values
    /// `resume/template.rs`'s old hard-coded `PREAMBLE` used, so a document
    /// written before this field existed renders unchanged.
    #[serde(default)]
    pub layout: LayoutSettings,
    /// A vault-wide layout profile used by the working copy.
    ///
    /// `None` means [`Self::layout`]. The named profile is resolved at render
    /// time rather than copied here, so updating one profile updates every CV
    /// that names it without eleven layout values drifting apart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_profile: Option<String>,
    /// How exports of this document are named, and where they last went.
    #[serde(default, skip_serializing_if = "ExportSettings::is_default")]
    pub export: ExportSettings,
    /// Id to hand out to the *next* custom section added (D-9) — a
    /// monotonically increasing counter, never rewound, so a deleted
    /// section's id is never reissued. Declared before `custom_sections`
    /// (a scalar before a table array, same reasoning as `Versioned`'s
    /// `active`/`variants` ordering). `#[serde(default)]` starts a document
    /// written before this field existed at 0, which is correct: it has no
    /// custom sections yet to collide with.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_custom_section_id: u32,
    /// User-added sections beyond the six built-ins (D-9). Empty for every
    /// document written before this field existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_sections: Vec<CustomSection>,
    /// Headings the user renamed, overriding the built-in defaults.
    ///
    /// `SectionKind` stays the document's spine — the Typst renderer and every
    /// `match` key off it — while the *printed* heading belongs to the user. A
    /// platform CV may want "Engineering", an academic one "Appointments".
    ///
    /// Stored as pairs rather than a map for the same reason `Preset::selection`
    /// is: `SectionKind` is not a natural TOML key, and pairs stay readable in a
    /// diff. Absent means the default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub section_titles: Vec<(SectionKind, String)>,
    /// Sections currently left out of the rendered document.
    ///
    /// The *current* state; a [`Preset`] pins its own copy and restores it on
    /// apply, exactly as it does for variant selections. Kept as a list rather
    /// than a `hidden: bool` per section because `SectionKind` is not a
    /// natural TOML key — the same reason `section_titles` is a list of pairs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden_sections: Vec<SectionKind>,
    /// Where a section departs from `layout`.
    ///
    /// Pairs, and on the document rather than inside [`LayoutSettings`], for
    /// the same two reasons the two tables above are: `SectionKind` is not a
    /// natural TOML key, and every other per-section table already lives
    /// here. `LayoutSettings` also stays `Copy` this way, which a good deal of
    /// the editor relies on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub section_overrides: Vec<(SectionKind, SectionOverrides)>,
    /// Every export of this document, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub export_history: Vec<ExportRecord>,
}

impl ResumeDoc {
    /// Every section, in the order they ship in. [`ResumeDoc::sections`] is what
    /// screens should iterate — it honours the user's own order.
    pub const SECTIONS: [SectionKind; 6] = [
        SectionKind::Profile,
        SectionKind::Work,
        SectionKind::Education,
        SectionKind::Skills,
        SectionKind::Certificates,
        SectionKind::Organizations,
    ];

    /// The sections in the order this document shows them.
    ///
    /// Falls back to [`Self::SECTIONS`], and repairs a stored order that has gone
    /// stale — a missing section is appended, a duplicate or unknown one dropped.
    /// A document that silently lost a section because its order was malformed
    /// would be data loss with no error to see.
    pub fn sections(&self) -> Vec<SectionKind> {
        self.sections_with_order(&self.section_order)
    }

    /// Resolve any stored order against this document's current section set.
    ///
    /// Presets carry their own order in C8, but malformed and pre-C8 data must
    /// obey the same repair rules as the document's working copy. Keeping the
    /// resolver here makes it impossible for the editor and a preset preview
    /// to disagree about where a missing or duplicated section belongs.
    pub(crate) fn sections_with_order(&self, order: &[SectionKind]) -> Vec<SectionKind> {
        // A `Custom` id only "exists" if a section with that id is still in
        // `custom_sections` — a stale reference (the section was deleted) is
        // exactly the "unknown" case this method already repairs away.
        let is_known = |kind: &SectionKind| match *kind {
            SectionKind::Custom(id) => self.custom_sections.iter().any(|s| s.id == id),
            builtin => Self::SECTIONS.contains(&builtin),
        };

        let mut out: Vec<SectionKind> =
            Vec::with_capacity(Self::SECTIONS.len() + self.custom_sections.len());
        for kind in order {
            if is_known(kind) && !out.contains(kind) {
                out.push(*kind);
            }
        }
        for kind in Self::SECTIONS {
            if !out.contains(&kind) {
                out.push(kind);
            }
        }
        for section in &self.custom_sections {
            let kind = SectionKind::Custom(section.id);
            if !out.contains(&kind) {
                out.push(kind);
            }
        }
        out
    }

    /// The heading this document prints for a section.
    ///
    /// The user's override if they set one, otherwise the built-in default. A
    /// custom section's title lives on the section itself.
    pub fn section_title(&self, kind: SectionKind) -> String {
        self.section_title_with(kind, &self.section_titles)
    }

    /// Resolve a heading through an arbitrary override table.
    ///
    /// A preset's `titles` table has exactly the same meaning as the working
    /// copy's `section_titles`; both the matrix and active-preset derivation
    /// use this resolver so an absent override always means the same default.
    pub(crate) fn section_title_with(
        &self,
        kind: SectionKind,
        titles: &[(SectionKind, String)],
    ) -> String {
        if let Some((_, title)) = titles
            .iter()
            .find(|(k, _)| *k == kind && !title_is_blank(k, titles))
        {
            return title.clone();
        }
        if let SectionKind::Custom(id) = kind {
            return self
                .custom_section(id)
                .map(|s| s.title.clone())
                .unwrap_or_default();
        }
        Self::default_section_title(kind).to_string()
    }

    /// The shipped heading for a built-in section.
    pub fn default_section_title(kind: SectionKind) -> &'static str {
        use SectionKind::*;
        match kind {
            Profile => "Profile",
            Work => "Work Experience",
            Education => "Education",
            Skills => "Skills",
            Certificates => "Certifications",
            Organizations => "Organizations",
            Custom(_) => "",
        }
    }

    /// Rename a section's printed heading. An empty or whitespace-only name
    /// clears the override rather than printing a blank heading.
    pub fn set_section_title(&mut self, kind: SectionKind, title: impl Into<String>) {
        let title = title.into();
        if let SectionKind::Custom(id) = kind {
            if let Some(section) = self.custom_section_mut(id) {
                section.title = title;
            }
            return;
        }
        self.section_titles.retain(|(k, _)| *k != kind);
        if !title.trim().is_empty() {
            self.section_titles.push((kind, title));
        }
    }

    /// What `section` overrides from the document's layout. Absent means it
    /// follows the document, which is what every section does until the user
    /// says otherwise.
    pub fn section_overrides(&self, section: SectionKind) -> SectionOverrides {
        self.section_overrides
            .iter()
            .find(|(k, _)| *k == section)
            .map(|(_, o)| *o)
            .unwrap_or_default()
    }

    /// Set what `section` overrides. A row that carries nothing is removed
    /// rather than stored, so "follow the document" is the absence of a row
    /// and not a row full of defaults.
    pub fn set_section_overrides(&mut self, section: SectionKind, overrides: SectionOverrides) {
        self.section_overrides.retain(|(k, _)| *k != section);
        if !overrides.is_empty() {
            self.section_overrides.push((section, overrides));
        }
    }

    /// Whether `section` prints a heading above it.
    pub fn prints_heading(&self, section: SectionKind) -> bool {
        !self.section_overrides(section).no_heading
    }

    /// `section`'s heading, with the document's values wherever it does not
    /// depart from them.
    pub fn headings_for(&self, section: SectionKind) -> HeadingLayout {
        self.section_overrides(section)
            .headings(self.layout.headings)
    }

    /// `section`'s dated entries, resolved the same way.
    pub fn entries_for(&self, section: SectionKind) -> EntryLayout {
        self.section_overrides(section).entries(self.layout.entries)
    }

    /// Show or hide `section`'s heading, leaving the section itself in place.
    /// Distinct from [`Self::set_hidden`], which drops the whole section.
    pub fn set_heading_printed(&mut self, section: SectionKind, printed: bool) {
        let mut overrides = self.section_overrides(section);
        overrides.no_heading = !printed;
        self.set_section_overrides(section, overrides);
    }

    /// Move a section one place up or down, persisting the new order.
    pub fn move_section(&mut self, kind: SectionKind, delta: isize) {
        let mut order = self.sections();
        let Some(from) = order.iter().position(|k| *k == kind) else {
            return;
        };
        let to = from as isize + delta;
        if to < 0 || to as usize >= order.len() {
            return;
        }
        order.swap(from, to as usize);
        self.section_order = order;
    }
}

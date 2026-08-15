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

pub use super::dates::{DateFormat, ResumeDate};

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
    pub highlights: Vec<String>,
}

// ---------------------------------------------------------------------------
// Custom sections (D-9, `docs/ROADMAP.md`)
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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ComposedCustomSection {
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
    /// wording around it is not — "утечка ПДн у клиента ACME" is a real diary
    /// entry and an unemployable CV bullet. The story's rule is that a
    /// confidential entry is "**никогда** не предлагается в CV дословно —
    /// только как абстрагированная метрика".
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

// ---------------------------------------------------------------------------
// Applications (roadmap D4, `docs/user-review.md` US-04/US-05, P-03/P-04)
// ---------------------------------------------------------------------------
//
// A board, not a tracker: `Rejected` is a first-class column, not something
// swept out of the list, because the conversion figures the Library screen
// wants (`4 sent → 1 interview → 1 offer`) are meaningless without the
// denominator Rejected holds (review P-04). Stored at `<vault>/applications.toml`.

/// The board's five columns.
///
/// Serializes to a lowercase string (`status = "wishlist"`) — this file is
/// meant to be hand-edited. A typo or an unrecognised value falls back to
/// `Wishlist` via `#[serde(other)]` rather than failing to parse the whole
/// file: one bad status line should demote a card to the board's default
/// column, not take out every application in the vault. `#[serde(other)]`
/// must name the enum's last variant (a serde-derive requirement), which is
/// why `Wishlist` — semantically the first column — is declared last here;
/// `ApplicationStatus::default()` is what every other reader (counts,
/// conversion) should use to mean "wishlist", not this declaration order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApplicationStatus {
    Applied,
    Interviewing,
    Offer,
    Rejected,
    #[default]
    #[serde(other)]
    Wishlist,
}

fn wishlist_word() -> String {
    "wishlist".to_string()
}

impl ApplicationStatus {
    /// The spelling this build writes. Lowercase, matching serde's own
    /// `rename_all` so a file written before this existed round-trips
    /// unchanged.
    pub fn word(self) -> &'static str {
        match self {
            ApplicationStatus::Wishlist => "wishlist",
            ApplicationStatus::Applied => "applied",
            ApplicationStatus::Interviewing => "interviewing",
            ApplicationStatus::Offer => "offer",
            ApplicationStatus::Rejected => "rejected",
        }
    }

    /// The status a word names, or `None` when this build does not know it —
    /// which is the distinction the whole round-trip rests on.
    pub fn from_word(word: &str) -> Option<Self> {
        match word.trim().to_lowercase().as_str() {
            "wishlist" => Some(ApplicationStatus::Wishlist),
            "applied" => Some(ApplicationStatus::Applied),
            "interviewing" => Some(ApplicationStatus::Interviewing),
            "offer" => Some(ApplicationStatus::Offer),
            "rejected" => Some(ApplicationStatus::Rejected),
            _ => None,
        }
    }

    /// Ordinal depth of a *pipeline* stage, used only to track how far an
    /// application has ever gotten (`Application::furthest`).
    ///
    /// `Rejected` deliberately has no depth: it is a terminal outcome you can
    /// land in from any stage, not a stage deeper than `Offer`, and it must
    /// never be allowed to raise `furthest` — see `Application::advance_to`.
    /// A private helper rather than `Ord` on the enum itself, because the
    /// enum's declaration order is already pinned by `#[serde(other)]`
    /// needing `Wishlist` last, which is not the order this depth uses.
    fn depth(self) -> Option<u8> {
        match self {
            ApplicationStatus::Wishlist => Some(0),
            ApplicationStatus::Applied => Some(1),
            ApplicationStatus::Interviewing => Some(2),
            ApplicationStatus::Offer => Some(3),
            ApplicationStatus::Rejected => None,
        }
    }
}

/// The next thing that has to happen, as data rather than a caption. The
/// review is explicit (P-04): `Decide by Fri` drawn as a label is a caption
/// you cannot build a reminder from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NextStep {
    /// What has to happen: "Onsite", "Take-home due", "Decide by".
    pub label: String,
    /// ISO date (YYYY-MM-DD).
    pub date: String,
    /// 24h "14:00", empty when the step is a day rather than a moment.
    pub time: String,
}

/// A PDF as it was actually sent. Never a live reference to a preset:
/// editing the CV in July must not rewrite what a company saw in March (US-04).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 1-based per application — the mockup's `snapshot v1`, `v2`.
    pub version: u32,
    /// ISO date the snapshot was taken.
    pub date: String,
    /// The preset name at that moment. A label recording history, not a
    /// lookup key — the preset itself may be renamed or deleted later.
    pub preset: String,
    /// File name inside `<vault>/snapshots/`. The bytes are a real file the
    /// user can open in Finder, per File-over-App.
    pub file: String,
}

/// One card on the Applications board.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Application {
    pub company: String,
    pub role: String,
    /// The `status` word exactly as the file spells it — and the only form
    /// that round-trips.
    ///
    /// `ApplicationStatus` takes `#[serde(other)]`, so a word this build does
    /// not know reads as `Wishlist`; that is the right trade, since one bad
    /// word should cost a card rather than the whole board. What was *not*
    /// right is what happened next: the enum was written back, so a
    /// hand-edited `status = "ofer"` was silently rewritten to `"wishlist"`
    /// and an offer stopped having ever existed. The word is kept verbatim,
    /// This is the **only** stored form: [`Application::status`] reads it,
    /// [`Application::advance_to`] writes it, and there is no second field to
    /// drift out of step with it.
    #[serde(rename = "status", default = "wishlist_word")]
    pub status_word: String,
    /// The deepest stage this application ever reached, which is not the
    /// same as where it sits now: most interviews end in a rejection, and a
    /// funnel counted from `status` alone would erase every one of them
    /// (review P-04). Never moves backwards — a card dragged back to
    /// `Applied` by mistake has still been interviewed, and that happened
    /// whether or not the board still says so. Set only through
    /// `Application::advance_to`, never assigned directly.
    #[serde(default)]
    pub furthest: ApplicationStatus,
    /// ISO date the card was created — the mockup's "saved 4d ago".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created: String,
    /// ISO date it was actually sent. `None` while it is still a wishlist entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<String>,
    /// The document it was sent from, as its file stem — a label, not a live
    /// reference; a document can be renamed or deleted and the card must
    /// still tell the truth about what was sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_doc: Option<String>,
    /// The preset name at the time of sending. Empty when nothing was
    /// attributed (e.g. a wishlist entry with no CV picked yet).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preset: String,
    /// The posting.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_step: Option<NextStep>,
    /// Free text: "$168k base · negotiating". Deliberately not a number —
    /// compensation is a negotiation state, not a figure to do arithmetic on.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub compensation: String,
    /// `Some("role filled")` vs `None`, which the card renders "no reason
    /// given". Only meaningful with status `Rejected`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<Snapshot>,
}

impl Application {
    /// The one way `status` should change. Sets `status`, and raises
    /// `furthest` to the deeper of its current value and the new status —
    /// never backwards, and `Rejected` (which has no `depth()`) never raises
    /// it at all, since a rejection is a terminal outcome, not a deeper
    /// pipeline stage than `Offer`.
    /// Where this application sits now.
    ///
    /// A word this build does not know reads as `Wishlist` — one bad word
    /// costs a card rather than the whole board — while
    /// [`Application::status_word`] keeps what the file actually said.
    pub fn status(&self) -> ApplicationStatus {
        ApplicationStatus::from_word(&self.status_word).unwrap_or(ApplicationStatus::Wishlist)
    }

    /// Whether this build understood the file's `status` word.
    pub fn status_is_recognised(&self) -> bool {
        ApplicationStatus::from_word(&self.status_word).is_some()
    }

    pub fn advance_to(&mut self, status: ApplicationStatus) {
        // Whatever the file used to say, the user has now said otherwise —
        // this is the one place an unrecognised word is allowed to be lost.
        self.status_word = status.word().to_string();
        if let Some(new_depth) = status.depth() {
            let furthest_depth = self.furthest.depth().unwrap_or(0);
            if new_depth > furthest_depth {
                self.furthest = status;
            }
        }
    }
}

/// A conversion funnel for one preset — the Library screen's
/// `FAANG · concise — 4 sent → 1 interview → 1 offer` line.
///
/// The three counts are cumulative funnel stages, not disjoint buckets: an
/// application that reached `Offer` was necessarily sent and interviewed, so
/// it counts in `sent`, `interviews` and `offers` alike. `Applications::conversion`
/// is what computes this — see its doc comment for the exact rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PresetConversion {
    pub preset: String,
    pub sent: usize,
    pub interviews: usize,
    pub offers: usize,
}

/// The application board: every company/role the user is tracking. Stored at
/// `<vault>/applications.toml`, mirroring Diary and Library.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Applications {
    pub entries: Vec<Application>,
}

impl Applications {
    /// Number of cards in a given column.
    pub fn count(&self, status: ApplicationStatus) -> usize {
        self.entries.iter().filter(|a| a.status() == status).count()
    }

    /// Everything not `Rejected` — the header line's "N active".
    pub fn active(&self) -> usize {
        self.entries
            .iter()
            .filter(|a| a.status() != ApplicationStatus::Rejected)
            .count()
    }

    /// Per-preset conversion funnels, one entry per distinct non-empty preset
    /// name that appears on at least one application.
    ///
    /// Counting rule (decided, not a UI choice): `sent`, `interviews` and
    /// `offers` are cumulative funnel stages read off the card's *deepest
    /// stage ever reached* (`furthest`), not disjoint buckets and — this is
    /// the point — not the card's *current* status either. Most interviews
    /// end in a rejection (review P-04); counting from `status` would erase
    /// every one of them and understate `interviews` by exactly the cases
    /// that matter most. `Applied` → `sent`; `Interviewing` → `sent` +
    /// `interviews`; `Offer` → `sent` + `interviews` + `offers`. `Wishlist`
    /// cards, and any card with an empty preset (nothing to attribute), are
    /// excluded entirely.
    pub fn conversion(&self) -> Vec<PresetConversion> {
        let mut order: Vec<String> = Vec::new();
        let mut totals: Vec<PresetConversion> = Vec::new();

        for entry in &self.entries {
            if entry.preset.is_empty() || entry.furthest == ApplicationStatus::Wishlist {
                continue;
            }
            let idx = match order.iter().position(|p| p == &entry.preset) {
                Some(i) => i,
                None => {
                    order.push(entry.preset.clone());
                    totals.push(PresetConversion {
                        preset: entry.preset.clone(),
                        sent: 0,
                        interviews: 0,
                        offers: 0,
                    });
                    totals.len() - 1
                }
            };
            let conv = &mut totals[idx];
            match entry.furthest {
                ApplicationStatus::Applied => conv.sent += 1,
                ApplicationStatus::Interviewing => {
                    conv.sent += 1;
                    conv.interviews += 1;
                }
                ApplicationStatus::Offer => {
                    conv.sent += 1;
                    conv.interviews += 1;
                    conv.offers += 1;
                }
                ApplicationStatus::Wishlist => unreachable!("excluded above"),
                // `furthest` should never actually be `Rejected` — it has no
                // `depth()`, so nothing in this module ever assigns it —
                // but a hand-edited file could set it directly. Treat that
                // the same as no progress rather than invent a stage this
                // entry has no honest record of reaching (US-14).
                ApplicationStatus::Rejected => {}
            }
        }

        totals
    }

    /// Repairs `furthest` for every entry so it is at least as deep as the
    /// entry's current `status`.
    ///
    /// This is the migration for `applications.toml` files written before
    /// `furthest` existed: it deserializes to `Wishlist` (its `#[default]`),
    /// which would otherwise zero out `conversion()`'s funnel for every
    /// pre-existing row. `Rejected` is excepted for the same reason
    /// `Application::advance_to` excepts it — it has no `depth()` to raise
    /// `furthest` to. Called by `vault::load_applications` after
    /// deserializing; safe to call again on an already-normalized board.
    pub fn normalize(&mut self) {
        for entry in &mut self.entries {
            if let Some(status_depth) = entry.status().depth() {
                let furthest_depth = entry.furthest.depth().unwrap_or(0);
                if status_depth > furthest_depth {
                    entry.furthest = entry.status();
                }
            }
        }
    }
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

// ---------------------------------------------------------------------------
// Per-section versioning
// ---------------------------------------------------------------------------
//
// Each section is versioned independently: it carries several named variants
// (e.g. a generic Work history and one tailored for a specific employer), and
// exactly one is active. The rendered document is the composition of every
// section's active variant — see [`ResumeDoc::compose`].

/// A single named variant of a section's content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Variant<T> {
    pub name: String,
    pub data: T,
}

/// A section's variants with one always active. Invariant: `variants` is
/// non-empty and `active` is always in range.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Versioned<T> {
    // `active` (a scalar) is declared before `variants` (a table array) so TOML
    // serialization emits it before the `[[...]]` sections.
    pub active: usize,
    pub variants: Vec<Variant<T>>,
}

impl<T: Default + Clone> Default for Versioned<T> {
    fn default() -> Self {
        Self::single("Base", T::default())
    }
}

impl<T: Clone> Versioned<T> {
    pub fn single(name: impl Into<String>, data: T) -> Self {
        Self {
            variants: vec![Variant {
                name: name.into(),
                data,
            }],
            active: 0,
        }
    }

    pub fn active(&self) -> &T {
        &self.variants[self.active].data
    }

    pub fn active_mut(&mut self) -> &mut T {
        &mut self.variants[self.active].data
    }

    pub fn active_name(&self) -> &str {
        &self.variants[self.active].name
    }

    pub fn active_name_mut(&mut self) -> &mut String {
        &mut self.variants[self.active].name
    }

    pub fn names(&self) -> Vec<String> {
        self.variants.iter().map(|v| v.name.clone()).collect()
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.variants.len() {
            self.active = index;
        }
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.variants.iter().position(|v| v.name == name)
    }

    /// Activate the variant with the given name, if present.
    pub fn set_active_by_name(&mut self, name: &str) {
        if let Some(index) = self.index_of(name) {
            self.active = index;
        }
    }

    /// Duplicate the active variant and switch to the copy.
    pub fn duplicate_active(&mut self) {
        let mut copy = self.variants[self.active].clone();
        copy.name = format!("{} copy", copy.name);
        self.variants.push(copy);
        self.active = self.variants.len() - 1;
    }

    /// Remove a variant; never removes the last one. Keeps `active` in range.
    pub fn remove(&mut self, index: usize) {
        if self.variants.len() <= 1 || index >= self.variants.len() {
            return;
        }
        self.variants.remove(index);
        if self.active >= self.variants.len() {
            self.active = self.variants.len() - 1;
        }
    }
}

/// Keeps `next_custom_section_id` out of every document that has no custom
/// sections. TOML is the wire format and `git diff` readability is a product
/// requirement — a counter at zero is noise in every file on disk.
/// A stored override that is blank must not win over the default — it would
/// print an empty heading into the exported PDF.
fn title_is_blank(kind: &SectionKind, doc: &ResumeDoc) -> bool {
    doc.section_titles
        .iter()
        .find(|(k, _)| k == kind)
        .is_some_and(|(_, t)| t.trim().is_empty())
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// Identifies a section for variant operations (switch/add/remove/rename).
///
/// `Custom(CustomSectionId)` is the *only* extension point for user-added
/// sections (D-9, `docs/ROADMAP.md`) — deliberately one new variant rather
/// than an open enum. `SectionKind` is `Copy + Hash`, is a `HashMap` key
/// (`views/preset_matrix.rs`), and is embedded in `FieldId` (also `Copy`), so
/// a `String` cannot live in it; the id is `Copy` for the same reason, and
/// the section's own user-editable title lives on [`CustomSection`], never
/// on the id. One extra variant means the compiler forces every existing
/// `match` on `SectionKind` to decide what a custom section means there,
/// instead of a silent fallback arm quietly ignoring it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SectionKind {
    Profile,
    Work,
    Education,
    Skills,
    Certificates,
    Organizations,
    Custom(CustomSectionId),
}

/// A section that could be made shorter without writing anything new.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrimCandidate {
    pub section: SectionKind,
    /// The leaner variant that already exists.
    pub variant: String,
    /// Characters of printed text switching to it would remove.
    pub saved_chars: usize,
}

/// A named, document-wide preset: a chosen variant (by name) for each section.
/// Applying it switches every section's active variant in one click —
/// e.g. a "GE Vernova" preset that picks the tailored Profile and Work variants
/// while leaving Education on its shared one.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub selection: Vec<(SectionKind, String)>,
    /// Sections this preset leaves out of the document entirely.
    ///
    /// Visibility is part of what a preset *selects*, not a document-level
    /// flag (O-13, confirmed against the design row: the same Certificates
    /// section reads `— hidden —` under one preset and `Base · shown` under
    /// another). A preset that hides nothing writes no `hidden` line at all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<SectionKind>,
}

/// Pin `section` to `variant` in this preset, replacing any existing pin.
///
/// A preset holds selections and never content, so this is the only kind of
/// edit it has — and the Preset Matrix screen is where a user makes it.
impl Preset {
    pub fn set(&mut self, section: SectionKind, variant: impl Into<String>) {
        let variant = variant.into();
        match self.selection.iter_mut().find(|(s, _)| *s == section) {
            Some((_, existing)) => *existing = variant,
            None => self.selection.push((section, variant)),
        }
    }

    /// The variant this preset pins for `section`, if it pins one at all.
    pub fn variant_for(&self, section: SectionKind) -> Option<&str> {
        self.selection
            .iter()
            .find(|(s, _)| *s == section)
            .map(|(_, v)| v.as_str())
    }
}

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
/// plausible *UI* simplification over this (see `docs/design/typst-controls.md`
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
        (self.x_mm - self.top_mm).abs() < EPSILON
            && (self.x_mm - self.bottom_mm).abs() < EPSILON
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
    /// Body text size as a percentage of the template's base size (10pt).
    /// 100 is the old hard-coded default; the layout rail's own readout
    /// (`docs/design/typst-controls.md` — "107%") is this same unit.
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
            text_scale_pct: 100,
            leading_em: 0.62,
            margins: Margins::default(),
        }
    }
}

impl LayoutSettings {
    /// Text scale bounds: below 50% a résumé is unreadable, above 200% it
    /// cannot hold a page of content — the same "a setting the user cannot
    /// get wrong" reasoning `docs/design/typst-controls.md` asks for.
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
    /// property of the CV, not an app-wide preference (`docs/user-review.md`
    /// §1, US-07). `#[serde(default)]` reproduces exactly the values
    /// `resume/template.rs`'s old hard-coded `PREAMBLE` used, so a document
    /// written before this field existed renders unchanged.
    #[serde(default)]
    pub layout: LayoutSettings,
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
        // A `Custom` id only "exists" if a section with that id is still in
        // `custom_sections` — a stale reference (the section was deleted) is
        // exactly the "unknown" case this method already repairs away.
        let is_known = |kind: &SectionKind| match *kind {
            SectionKind::Custom(id) => self.custom_sections.iter().any(|s| s.id == id),
            builtin => Self::SECTIONS.contains(&builtin),
        };

        let mut out: Vec<SectionKind> =
            Vec::with_capacity(Self::SECTIONS.len() + self.custom_sections.len());
        for kind in &self.section_order {
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
        if let Some((_, title)) = self
            .section_titles
            .iter()
            .find(|(k, _)| *k == kind && !title_is_blank(k, self))
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

    /// Wrap a flat resume as a document with one variant per section.
    pub fn from_resume(r: Resume, base_name: impl Into<String> + Clone) -> Self {
        Self {
            profile: Versioned::single(base_name.clone(), r.basics),
            work: Versioned::single(base_name.clone(), r.work),
            education: Versioned::single(base_name.clone(), r.education),
            skills: Versioned::single(base_name.clone(), r.skills),
            certificates: Versioned::single(base_name.clone(), r.certificates),
            volunteer: Versioned::single(base_name, r.volunteer),
            presets: Vec::new(),
            section_order: Vec::new(),
            section_titles: Vec::new(),
            layout: LayoutSettings::default(),
            next_custom_section_id: 0,
            custom_sections: Vec::new(),
            hidden_sections: Vec::new(),
        }
    }

    /// Look up a custom section by its stable id.
    pub fn custom_section(&self, id: CustomSectionId) -> Option<&CustomSection> {
        self.custom_sections.iter().find(|s| s.id == id)
    }

    /// Look up a custom section by its stable id, mutably.
    pub fn custom_section_mut(&mut self, id: CustomSectionId) -> Option<&mut CustomSection> {
        self.custom_sections.iter_mut().find(|s| s.id == id)
    }

    /// Add a new custom section (D-9) with the given title and one empty
    /// "Base" variant, and return its stable id.
    ///
    /// Ids come from `next_custom_section_id`, a counter that only ever
    /// increases — deleting a section (`remove_custom_section`) does not
    /// rewind it, so a ***previously issued id is never handed out again***
    /// within this document. That is what keeps `section_order`,
    /// `Preset::selection` and any `FieldId` holding this id honest: none of
    /// them can be silently re-pointed at a different section after a
    /// deletion, because an id is a counter value, never a `Vec` position.
    pub fn add_custom_section(&mut self, title: impl Into<String>) -> CustomSectionId {
        let id = CustomSectionId(self.next_custom_section_id);
        self.next_custom_section_id += 1;
        // Seeded with one placeholder entry, deliberately.
        //
        // The Typst renderer skips a custom section with no entries — correctly,
        // since an empty heading would print into the exported PDF. But that made
        // "+ Add" look broken: the section appeared in the panel and the preview
        // did not change at all, because the generated source was byte-identical
        // and the recompile was (rightly) skipped. Seeding matches what every
        // other "+" in the editor already does — `ListId::Work.add` pushes a
        // "New role" — so the user sees the section land and has something to type
        // into.
        self.custom_sections.push(CustomSection {
            id,
            title: title.into(),
            content: Versioned::single(
                "Base",
                vec![CustomEntry {
                    title: "New entry".into(),
                    ..CustomEntry::default()
                }],
            ),
        });
        id
    }

    /// Remove a custom section (every variant of it). Any stale reference
    /// left behind in `section_order` or a `Preset` is repaired away by
    /// [`Self::sections`] / read through [`Self::variant_name`]'s safe
    /// fallback rather than causing a panic elsewhere.
    pub fn remove_custom_section(&mut self, id: CustomSectionId) {
        self.custom_sections.retain(|s| s.id != id);
    }

    /// Activate the named variant of a section, if it exists.
    pub fn set_active_variant_by_name(&mut self, section: SectionKind, name: &str) {
        use SectionKind::*;
        match section {
            Profile => self.profile.set_active_by_name(name),
            Work => self.work.set_active_by_name(name),
            Education => self.education.set_active_by_name(name),
            Skills => self.skills.set_active_by_name(name),
            Certificates => self.certificates.set_active_by_name(name),
            Organizations => self.volunteer.set_active_by_name(name),
            Custom(id) => {
                if let Some(s) = self.custom_section_mut(id) {
                    s.content.set_active_by_name(name);
                }
            }
        }
    }

    /// The current active-variant name for every section, built-in and
    /// custom alike, in the document's own order.
    pub fn current_selection(&self) -> Vec<(SectionKind, String)> {
        self.sections()
            .into_iter()
            .map(|s| (s, self.variant_name(s).clone()))
            .collect()
    }

    /// Save the current selection as a new preset.
    ///
    /// Not "capture" — in this product that word belongs to the Diary's
    /// quick-capture (roadmap D-7), and the two must not blur.
    pub fn add_preset(&mut self, name: impl Into<String>) {
        let selection = self.current_selection();
        // A preset records what is hidden as well as what is selected, so
        // saving "the current state" means the whole current state.
        let hidden = self.hidden_sections.clone();
        self.presets.push(Preset {
            name: name.into(),
            selection,
            hidden,
        });
    }

    /// Switch every section to the variants recorded in preset `index`.
    pub fn apply_preset(&mut self, index: usize) {
        let Some(preset) = self.presets.get(index).cloned() else {
            return;
        };
        for (section, variant_name) in preset.selection {
            self.set_active_variant_by_name(section, &variant_name);
        }
        // Visibility is part of the selection (O-13), so applying a preset
        // restores it wholesale — including *un*-hiding what this preset does
        // not hide, or switching presets would only ever accumulate hiding.
        self.hidden_sections = preset.hidden;
    }

    pub fn remove_preset(&mut self, index: usize) {
        if index < self.presets.len() {
            self.presets.remove(index);
        }
    }

    pub fn preset_name(&self, index: usize) -> Option<&String> {
        self.presets.get(index).map(|p| &p.name)
    }

    pub fn preset_name_mut(&mut self, index: usize) -> Option<&mut String> {
        self.presets.get_mut(index).map(|p| &mut p.name)
    }

    /// Total number of variants across all sections (for gallery metadata).
    pub fn total_variants(&self) -> usize {
        self.profile.variants.len()
            + self.work.variants.len()
            + self.education.variants.len()
            + self.skills.variants.len()
            + self.certificates.variants.len()
            + self.volunteer.variants.len()
            + self
                .custom_sections
                .iter()
                .map(|s| s.content.variants.len())
                .sum::<usize>()
    }

    /// The rendered document: each section's active variant, including every
    /// custom section's (D-9).
    /// The rendered document: every visible section at its active variant.
    ///
    /// A hidden section is composed as **empty**, not merely skipped by the
    /// renderer: hiding has to reach the PDF the same way renaming does
    /// (O-14's rule), and the template already omits a section with no
    /// entries. Profile is deliberately not hideable — a résumé without a name
    /// is not a shorter résumé, it is a broken one.
    pub fn compose(&self) -> Resume {
        let visible = |kind: SectionKind| !self.hidden_sections.contains(&kind);
        Resume {
            basics: self.profile.active().clone(),
            work: take_if(visible(SectionKind::Work), self.work.active()),
            education: take_if(visible(SectionKind::Education), self.education.active()),
            skills: take_if(visible(SectionKind::Skills), self.skills.active()),
            certificates: take_if(
                visible(SectionKind::Certificates),
                self.certificates.active(),
            ),
            volunteer: take_if(visible(SectionKind::Organizations), self.volunteer.active()),
            section_titles: Self::SECTIONS
                .iter()
                .map(|&kind| (kind, self.section_title(kind)))
                .collect(),
            // Hidden sections stay in the order — they compose as empty, and
            // the renderer skips an empty section anyway. Filtering here would
            // make un-hiding lose the position it had.
            section_order: self.sections(),
            // Emitted in the document's own order, not storage order, so the
            // renderer's `custom0, custom1, …` indices line up with the
            // `order` list `template.rs` writes beside them.
            custom_sections: self
                .sections()
                .into_iter()
                .filter_map(|kind| match kind {
                    SectionKind::Custom(id) if visible(kind) => self.custom_section(id),
                    _ => None,
                })
                .map(|s| ComposedCustomSection {
                    title: s.title.clone(),
                    entries: s.content.active().clone(),
                })
                .collect(),
        }
    }

    /// Whether `section` is currently left out of the rendered document.
    pub fn is_hidden(&self, section: SectionKind) -> bool {
        self.hidden_sections.contains(&section)
    }

    /// Show or hide `section`. Profile cannot be hidden — see [`Self::compose`].
    pub fn set_hidden(&mut self, section: SectionKind, hidden: bool) {
        if section == SectionKind::Profile {
            return;
        }
        match (hidden, self.hidden_sections.iter().position(|s| *s == section)) {
            (true, None) => self.hidden_sections.push(section),
            (false, Some(i)) => {
                self.hidden_sections.remove(i);
            }
            _ => {}
        }
    }

    /// How much printed text one variant of `section` carries, in characters.
    ///
    /// A proxy for "how much of the page this costs", and deliberately a
    /// crude one: it counts the words that reach the document, not the laid-out
    /// height, because height depends on the page geometry the user is in the
    /// middle of changing. Used only to compare two variants of the *same*
    /// section against each other, where the proxy holds — never to predict
    /// how many lines something occupies.
    pub fn variant_weight(&self, section: SectionKind, index: usize) -> usize {
        use SectionKind::*;
        let text_len = |strings: Vec<&String>| -> usize { strings.iter().map(|s| s.chars().count()).sum() };
        match section {
            Profile => self
                .profile
                .variants
                .get(index)
                .map(|v| v.data.summary.chars().count() + v.data.label.chars().count())
                .unwrap_or(0),
            Work => self
                .work
                .variants
                .get(index)
                .map(|v| {
                    v.data
                        .iter()
                        .map(|w| {
                            w.summary.chars().count()
                                + text_len(w.highlights.iter().collect())
                                + w.position.chars().count()
                        })
                        .sum()
                })
                .unwrap_or(0),
            Education => self
                .education
                .variants
                .get(index)
                .map(|v| v.data.iter().map(|e| e.institution.chars().count() + e.study_type.chars().count()).sum())
                .unwrap_or(0),
            Skills => self
                .skills
                .variants
                .get(index)
                .map(|v| {
                    v.data
                        .iter()
                        .map(|g| g.name.chars().count() + text_len(g.keywords.iter().collect()))
                        .sum()
                })
                .unwrap_or(0),
            Certificates => self
                .certificates
                .variants
                .get(index)
                .map(|v| v.data.iter().map(|c| c.name.chars().count() + c.issuer.chars().count()).sum())
                .unwrap_or(0),
            Organizations => self
                .volunteer
                .variants
                .get(index)
                .map(|v| {
                    v.data
                        .iter()
                        .map(|o| o.position.chars().count() + text_len(o.highlights.iter().collect()))
                        .sum()
                })
                .unwrap_or(0),
            Custom(id) => self
                .custom_section(id)
                .and_then(|s| s.content.variants.get(index))
                .map(|v| {
                    v.data
                        .iter()
                        .map(|e| e.title.chars().count() + text_len(e.highlights.iter().collect()))
                        .sum()
                })
                .unwrap_or(0),
        }
    }

    /// Sections that already have a shorter variant written, and how much
    /// shorter it is — the design row's "trim candidate" (US-08).
    ///
    /// "Candidate" means exactly one thing here: **you have already written a
    /// leaner cut of this section**, so switching costs you nothing you would
    /// have to write again. It is not a guess about which section is verbose,
    /// and not an AI suggestion (that is US-24, a different story) — it is a
    /// fact about the document, which is why it can be stated plainly.
    ///
    /// Hidden sections are skipped: they are not on the page to trim.
    pub fn trim_candidates(&self) -> Vec<TrimCandidate> {
        self.sections()
            .into_iter()
            .filter(|kind| !self.is_hidden(*kind))
            .filter_map(|kind| {
                let active = self.active_variant(kind);
                let current = self.variant_weight(kind, active);
                let names = self.variant_names(kind);
                let (index, weight) = names
                    .iter()
                    .enumerate()
                    .map(|(i, _)| (i, self.variant_weight(kind, i)))
                    .filter(|(i, w)| *i != active && *w < current)
                    .min_by_key(|(_, w)| *w)?;
                Some(TrimCandidate {
                    section: kind,
                    variant: names.get(index)?.clone(),
                    saved_chars: current - weight,
                })
            })
            .collect()
    }

    pub fn variant_names(&self, section: SectionKind) -> Vec<String> {
        use SectionKind::*;
        match section {
            Profile => self.profile.names(),
            Work => self.work.names(),
            Education => self.education.names(),
            Skills => self.skills.names(),
            Certificates => self.certificates.names(),
            Organizations => self.volunteer.names(),
            Custom(id) => self
                .custom_section(id)
                .map(|s| s.content.names())
                .unwrap_or_default(),
        }
    }

    pub fn active_variant(&self, section: SectionKind) -> usize {
        use SectionKind::*;
        match section {
            Profile => self.profile.active,
            Work => self.work.active,
            Education => self.education.active,
            Skills => self.skills.active,
            Certificates => self.certificates.active,
            Organizations => self.volunteer.active,
            Custom(id) => self
                .custom_section(id)
                .map(|s| s.content.active)
                .unwrap_or(0),
        }
    }

    pub fn set_active_variant(&mut self, section: SectionKind, index: usize) {
        use SectionKind::*;
        match section {
            Profile => self.profile.set_active(index),
            Work => self.work.set_active(index),
            Education => self.education.set_active(index),
            Skills => self.skills.set_active(index),
            Certificates => self.certificates.set_active(index),
            Organizations => self.volunteer.set_active(index),
            Custom(id) => {
                if let Some(s) = self.custom_section_mut(id) {
                    s.content.set_active(index);
                }
            }
        }
    }

    pub fn add_variant(&mut self, section: SectionKind) {
        use SectionKind::*;
        match section {
            Profile => self.profile.duplicate_active(),
            Work => self.work.duplicate_active(),
            Education => self.education.duplicate_active(),
            Skills => self.skills.duplicate_active(),
            Certificates => self.certificates.duplicate_active(),
            Organizations => self.volunteer.duplicate_active(),
            Custom(id) => {
                if let Some(s) = self.custom_section_mut(id) {
                    s.content.duplicate_active();
                }
            }
        }
    }

    pub fn remove_variant(&mut self, section: SectionKind, index: usize) {
        use SectionKind::*;
        match section {
            Profile => self.profile.remove(index),
            Work => self.work.remove(index),
            Education => self.education.remove(index),
            Skills => self.skills.remove(index),
            Certificates => self.certificates.remove(index),
            Organizations => self.volunteer.remove(index),
            Custom(id) => {
                if let Some(s) = self.custom_section_mut(id) {
                    s.content.remove(index);
                }
            }
        }
    }

    /// The active variant's name. A `Custom` id with no matching section
    /// (deleted underneath a stale reference) falls back to a shared empty
    /// string rather than panicking — [`Self::sections`] is what repairs the
    /// stale reference away; this just stays safe in the meantime.
    pub fn variant_name(&self, section: SectionKind) -> &String {
        use SectionKind::*;
        static EMPTY: String = String::new();
        match section {
            Profile => &self.profile.variants[self.profile.active].name,
            Work => &self.work.variants[self.work.active].name,
            Education => &self.education.variants[self.education.active].name,
            Skills => &self.skills.variants[self.skills.active].name,
            Certificates => &self.certificates.variants[self.certificates.active].name,
            Organizations => &self.volunteer.variants[self.volunteer.active].name,
            Custom(id) => self
                .custom_section(id)
                .map(|s| &s.content.variants[s.content.active].name)
                .unwrap_or(&EMPTY),
        }
    }

    /// `None` only for a `Custom` id with no matching section — see
    /// [`Self::variant_name`]. Every built-in section always has one.
    pub fn variant_name_mut(&mut self, section: SectionKind) -> Option<&mut String> {
        use SectionKind::*;
        Some(match section {
            Profile => self.profile.active_name_mut(),
            Work => self.work.active_name_mut(),
            Education => self.education.active_name_mut(),
            Skills => self.skills.active_name_mut(),
            Certificates => self.certificates.active_name_mut(),
            Organizations => self.volunteer.active_name_mut(),
            Custom(id) => {
                return self
                    .custom_section_mut(id)
                    .map(|s| s.content.active_name_mut())
            }
        })
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn default_layout_matches_the_old_hard_coded_preamble() {
        // resume/template.rs's old `PREAMBLE` constant, transcribed as the
        // default's expected values: `margin: (x: 1.6cm, top: 1.4cm, bottom:
        // 1.4cm)`, `size: 10pt` (= 100%), `leading: 0.62em`, paper "a4".
        let layout = LayoutSettings::default();
        assert_eq!(layout.page_size, PageSize::A4);
        assert_eq!(layout.page_size.typst_paper_name(), "a4");
        assert_eq!(layout.text_scale_pct, 100);
        assert_eq!(layout.leading_em, 0.62);
        assert_eq!(layout.margins.x_mm, 16.0);
        assert_eq!(layout.margins.top_mm, 14.0);
        assert_eq!(layout.margins.bottom_mm, 14.0);
        // sanitized() must not perturb an already-valid default.
        assert_eq!(layout.sanitized(), layout);
    }

    #[test]
    fn letter_uses_the_typst_us_letter_preset_name() {
        assert_eq!(PageSize::Letter.typst_paper_name(), "us-letter");
    }

    #[test]
    fn sanitized_clamps_out_of_range_values() {
        let wild = LayoutSettings {
            page_size: PageSize::A4,
            font: DocumentFont::default(),
            date_format: Default::default(),
            text_scale_pct: 0,
            leading_em: -3.0,
            margins: Margins {
                x_mm: -10.0,
                top_mm: 10_000.0,
                bottom_mm: f32::NAN.max(0.0), // still exercises the clamp path
            },
        };
        let safe = wild.sanitized();
        assert!(safe.text_scale_pct >= 50 && safe.text_scale_pct <= 200);
        assert!(safe.leading_em >= 0.3 && safe.leading_em <= 1.5);
        assert!(safe.margins.x_mm >= 3.0);
        assert!(safe.margins.top_mm >= 3.0 && safe.margins.top_mm <= 297.0 / 3.0);
        assert!(safe.margins.bottom_mm >= 3.0);
    }

    #[test]
    fn layout_round_trips_through_toml() {
        let layout = LayoutSettings {
            page_size: PageSize::Letter,
            font: DocumentFont::default(),
            date_format: Default::default(),
            text_scale_pct: 107,
            leading_em: 0.7,
            margins: Margins {
                x_mm: 20.0,
                top_mm: 18.0,
                bottom_mm: 18.0,
            },
        };
        let text = toml::to_string_pretty(&layout).expect("serializes");
        let back: LayoutSettings = toml::from_str(&text).expect("round-trips");
        assert_eq!(back, layout);
    }
}

#[cfg(test)]
mod custom_section_tests {
    use super::*;

    #[test]
    fn custom_section_round_trips_through_toml() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let id = doc.add_custom_section("Publications");
        doc.custom_section_mut(id)
            .unwrap()
            .content
            .active_mut()
            .push(CustomEntry {
                title: "A Paper".into(),
                subtitle: "Some Journal".into(),
                start_date: "2024".into(),
                end_date: Default::default(),
                url: "https://example.com".into(),
                highlights: vec!["Peer reviewed".into()],
            });
        // A second variant, so `Versioned` is exercised the same way as a
        // built-in section's.
        doc.add_variant(SectionKind::Custom(id));

        let text = toml::to_string_pretty(&doc).expect("serializes");
        let back: ResumeDoc = toml::from_str(&text).expect("round-trips");

        assert_eq!(back.custom_sections.len(), 1);
        let section = &back.custom_sections[0];
        assert_eq!(section.id, id);
        assert_eq!(section.title, "Publications");
        assert_eq!(section.content.variants.len(), 2);
        assert_eq!(back.next_custom_section_id, 1);
        // Index 0 is the placeholder every new section is seeded with; the
        // entry this test pushed follows it.
        assert_eq!(section.content.variants[0].data[0].title, "New entry");
        let entry = &section.content.variants[0].data[1];
        assert_eq!(entry.title, "A Paper");
        assert_eq!(entry.subtitle, "Some Journal");
        assert_eq!(entry.url, "https://example.com");
        assert_eq!(entry.highlights, vec!["Peer reviewed".to_string()]);
    }

    /// A document written before custom sections existed — no `custom_sections`
    /// table, no `next_custom_section_id` key at all — must still load, with
    /// an id counter that starts at 0 (correct: there is nothing yet to
    /// collide with) and `sections()` unchanged from the fixed six.
    #[test]
    fn old_doc_without_custom_sections_still_loads() {
        let doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let text = toml::to_string_pretty(&doc).expect("serializes");
        // `custom_sections` has `skip_serializing_if`, so a doc with none
        // never writes the table — exactly what makes an old vault file (with
        // no such table at all) parse as "no custom sections", not an error.
        assert!(!text.contains("custom_sections"));

        let back: ResumeDoc = toml::from_str(&text).expect("a pre-D-9 document must still load");
        assert!(back.custom_sections.is_empty());
        assert_eq!(back.next_custom_section_id, 0);
        assert_eq!(back.sections(), ResumeDoc::SECTIONS.to_vec());
    }

    /// A preset can name a custom section's variant exactly like a built-in
    /// one's, and that selection survives a save→load round trip.
    #[test]
    fn preset_naming_a_custom_sections_variant_round_trips() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let id = doc.add_custom_section("Awards");
        doc.add_variant(SectionKind::Custom(id)); // "Base copy", now active
        doc.variant_name_mut(SectionKind::Custom(id))
            .unwrap()
            .clear();
        doc.variant_name_mut(SectionKind::Custom(id))
            .unwrap()
            .push_str("Tailored");

        doc.add_preset("Includes Awards");
        // Switch away, then let the preset restore it.
        doc.set_active_variant(SectionKind::Custom(id), 0);
        assert_eq!(doc.variant_name(SectionKind::Custom(id)), "Base");

        let text = toml::to_string_pretty(&doc).expect("serializes");
        let mut back: ResumeDoc = toml::from_str(&text).expect("round-trips");

        assert_eq!(back.presets.len(), 1);
        assert!(
            back.presets[0]
                .selection
                .iter()
                .any(|(s, name)| *s == SectionKind::Custom(id) && name == "Tailored"),
            "preset selection lost its custom-section entry across a TOML round trip"
        );

        back.apply_preset(0);
        assert_eq!(back.variant_name(SectionKind::Custom(id)), "Tailored");
    }

    /// A `section_order` naming a custom section that has since been deleted
    /// must be repaired away, not left dangling — the same guarantee
    /// `section_order_defaults_and_repairs_itself` (`vault.rs`) already gives
    /// for the six built-ins.
    #[test]
    fn section_order_repairs_a_deleted_custom_section() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let id = doc.add_custom_section("Patents");
        doc.section_order = vec![SectionKind::Custom(id), SectionKind::Work];
        assert_eq!(doc.sections()[0], SectionKind::Custom(id));

        doc.remove_custom_section(id);
        let order = doc.sections();
        assert!(
            !order.contains(&SectionKind::Custom(id)),
            "a deleted custom section must not linger in `sections()`"
        );
        assert_eq!(order.len(), ResumeDoc::SECTIONS.len());
    }

    /// A preset pins selections and nothing else, so `set` must replace an
    /// existing pin rather than append a second one for the same section —
    /// two pins for one section would make the matrix's reading order decide
    /// what the document renders.
    #[test]
    fn pinning_a_section_twice_replaces_rather_than_duplicates() {
        let mut preset = Preset {
            name: "FAANG · concise".into(),
            selection: vec![(SectionKind::Work, "Detailed".into())],
            hidden: Vec::new(),
        };

        preset.set(SectionKind::Work, "Concise");
        preset.set(SectionKind::Skills, "Infra-heavy");

        assert_eq!(preset.selection.len(), 2);
        assert_eq!(preset.variant_for(SectionKind::Work), Some("Concise"));
        assert_eq!(preset.variant_for(SectionKind::Skills), Some("Infra-heavy"));
        // A section nobody pinned stays unpinned — the matrix renders that as
        // "not pinned", never as the design's `— hidden —` (O-13).
        assert_eq!(preset.variant_for(SectionKind::Education), None);
    }

    /// "Save current as new preset" must cover every section the document
    /// actually has, custom ones included — iterating the six built-ins would
    /// silently drop a custom section out of every preset saved that way.
    #[test]
    fn a_saved_preset_covers_custom_sections_too() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let id = doc.add_custom_section("Publications");

        let selection = doc.current_selection();
        let sections: Vec<SectionKind> = selection.iter().map(|(s, _)| *s).collect();

        assert!(sections.contains(&SectionKind::Custom(id)), "got {sections:?}");
        assert_eq!(selection.len(), doc.sections().len());
    }

    /// "Trim candidate" means one specific thing: a **shorter variant you
    /// already wrote**. It must never point at a longer one, never at the
    /// variant already active, and never at a hidden section — that last one
    /// is not on the page to trim.
    #[test]
    fn a_trim_candidate_is_only_ever_a_shorter_variant_you_already_have() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Detailed");
        doc.work.active_mut().push(Work {
            position: "Senior Engineer".into(),
            highlights: vec!["A fairly long bullet about a thing that happened".into(); 4],
            ..Default::default()
        });

        // No second variant yet: nothing to offer.
        assert!(doc.trim_candidates().is_empty());

        // A leaner cut of the same section.
        doc.add_variant(SectionKind::Work);
        let lean = doc.work.variants.len() - 1;
        doc.work.variants[lean].name = "Concise".into();
        doc.work.variants[lean].data = vec![Work {
            position: "Senior Engineer".into(),
            highlights: vec!["Short bullet".into()],
            ..Default::default()
        }];
        doc.work.set_active(0);

        let candidates = doc.trim_candidates();
        assert_eq!(candidates.len(), 1, "got {candidates:?}");
        assert_eq!(candidates[0].section, SectionKind::Work);
        assert_eq!(candidates[0].variant, "Concise");
        assert!(candidates[0].saved_chars > 0);

        // Standing on the lean variant, the fat one is not a candidate.
        doc.work.set_active(lean);
        assert!(doc.trim_candidates().is_empty());

        // Hidden sections are not on the page, so they are not trimmable.
        doc.work.set_active(0);
        assert_eq!(doc.trim_candidates().len(), 1);
        doc.set_hidden(SectionKind::Work, true);
        assert!(doc.trim_candidates().is_empty());
    }

    /// The preview works out its rasterization scale from the page's width in
    /// points, so that number has to be the real one — a wrong constant here
    /// would make every render softly wrong and nothing would fail.
    #[test]
    fn page_width_in_points_matches_the_paper() {
        // A4 is 210 mm; Letter is 8.5 in. Both to within a rounding step.
        assert!((PageSize::A4.width_pt() - 595.28).abs() < 0.1);
        assert!((PageSize::Letter.width_pt() - 612.0).abs() < 0.1);
        assert!(PageSize::Letter.width_pt() > PageSize::A4.width_pt());
    }

    /// A newly added custom section shows up in `sections()` even when
    /// `section_order` has never been touched (the common case) — it must
    /// not be silently invisible just because the order field is empty.
    #[test]
    fn a_fresh_custom_section_appears_with_no_stored_order() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let id = doc.add_custom_section("Languages");
        assert!(doc.section_order.is_empty());
        assert_eq!(doc.sections().last(), Some(&SectionKind::Custom(id)));
    }
}

#[cfg(test)]
mod applications_tests {
    use super::*;

    fn full_application() -> Application {
        Application {
            company: "Bramble Tech".into(),
            role: "Staff Engineer".into(),
            status_word: ApplicationStatus::Interviewing.word().into(),
            furthest: ApplicationStatus::Interviewing,
            created: "2026-06-01".into(),
            applied: Some("2026-06-02".into()),
            source_doc: Some("sofiia-senior-swe".into()),
            preset: "FAANG · concise".into(),
            url: "https://brambletech.example/careers/123".into(),
            notes: "Referred by Dana".into(),
            next_step: Some(NextStep {
                label: "Onsite".into(),
                date: "2026-08-20".into(),
                time: "14:00".into(),
            }),
            compensation: "$168k base · negotiating".into(),
            rejection_reason: None,
            snapshots: vec![Snapshot {
                version: 1,
                date: "2026-06-02".into(),
                preset: "FAANG · concise".into(),
                file: "bramble-tech-v1.pdf".into(),
            }],
        }
    }

    /// A fully populated application round-trips through TOML unchanged.
    #[test]
    fn application_round_trips_through_toml() {
        let apps = Applications {
            entries: vec![full_application()],
        };
        let text = toml::to_string_pretty(&apps).expect("serializes");
        let back: Applications = toml::from_str(&text).expect("round-trips");
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0], apps.entries[0]);
    }

    /// A hand-written minimal entry — company/role/status only — loads with
    /// everything else defaulted, and a missing `applications.toml` (the
    /// common case: no such file exists in any vault written before this
    /// feature) is read as an empty board, not an error.
    #[test]
    fn minimal_entry_loads_with_everything_else_defaulted() {
        let toml_text = "[[entries]]\ncompany = \"Acme\"\nrole = \"SWE\"\nstatus = \"applied\"\n";
        let apps: Applications = toml::from_str(toml_text).expect("a minimal entry must load");
        assert_eq!(apps.entries.len(), 1);
        let entry = &apps.entries[0];
        assert_eq!(entry.company, "Acme");
        assert_eq!(entry.role, "SWE");
        assert_eq!(entry.status(), ApplicationStatus::Applied);
        assert_eq!(entry.created, "");
        assert!(entry.applied.is_none());
        assert!(entry.source_doc.is_none());
        assert_eq!(entry.preset, "");
        assert!(entry.next_step.is_none());
        assert!(entry.rejection_reason.is_none());
        assert!(entry.snapshots.is_empty());
    }

    /// An unrecognised/typo'd status must not fail the whole file — it falls
    /// back to `Wishlist`, the board's own default column.
    #[test]
    fn unknown_status_falls_back_to_wishlist_rather_than_erroring() {
        let toml_text =
            "[[entries]]\ncompany = \"Acme\"\nrole = \"SWE\"\nstatus = \"ghosted\"\n";
        let apps: Applications =
            toml::from_str(toml_text).expect("a typo'd status must not fail the whole file");
        assert_eq!(apps.entries[0].status(), ApplicationStatus::Wishlist);
    }

    /// Empty/absent optional fields must not be written to disk at all —
    /// noise in a format whose whole point is being readable and diffable.
    #[test]
    fn empty_optional_fields_are_not_serialized() {
        let apps = Applications {
            entries: vec![Application {
                company: "Acme".into(),
                role: "SWE".into(),
                status_word: ApplicationStatus::Wishlist.word().into(),
                ..Default::default()
            }],
        };
        let text = toml::to_string_pretty(&apps).expect("serializes");
        for absent in [
            "created", "applied", "source_doc", "preset", "url", "notes", "next_step",
            "compensation", "rejection_reason", "snapshots",
        ] {
            assert!(
                !text.contains(absent),
                "empty field `{absent}` should not be written:\n{text}"
            );
        }
    }

    #[test]
    fn count_and_active_tally_by_status() {
        let apps = Applications {
            entries: vec![
                Application {
                    status_word: ApplicationStatus::Wishlist.word().into(),
                    ..Default::default()
                },
                Application {
                    status_word: ApplicationStatus::Applied.word().into(),
                    ..Default::default()
                },
                Application {
                    status_word: ApplicationStatus::Applied.word().into(),
                    ..Default::default()
                },
                Application {
                    status_word: ApplicationStatus::Rejected.word().into(),
                    ..Default::default()
                },
            ],
        };
        assert_eq!(apps.count(ApplicationStatus::Applied), 2);
        assert_eq!(apps.count(ApplicationStatus::Rejected), 1);
        assert_eq!(apps.active(), 3); // everything but the one Rejected
    }

    /// The funnel is cumulative over the **deepest stage ever reached**, not
    /// the column a card sits in now. This is the whole reason `furthest`
    /// exists: most interviews end in a rejection, so counting from `status`
    /// would erase every interview that did not end in an offer — and the
    /// review's headline metric (P-04, "4 interviews out of 11") is exactly a
    /// count of applications that *reached* interview.
    #[test]
    fn a_rejection_still_credits_the_interview_it_reached() {
        let mut rejected_after_onsite = Application {
            company: "Rejected Co".into(),
            preset: "FAANG · concise".into(),
            ..Default::default()
        };
        rejected_after_onsite.advance_to(ApplicationStatus::Applied);
        rejected_after_onsite.advance_to(ApplicationStatus::Interviewing);
        rejected_after_onsite.advance_to(ApplicationStatus::Rejected);
        // Terminal, not deep: the card is out, but it did interview.
        assert_eq!(rejected_after_onsite.status(), ApplicationStatus::Rejected);
        assert_eq!(
            rejected_after_onsite.furthest,
            ApplicationStatus::Interviewing
        );

        let mut offered = Application {
            company: "Offer Co".into(),
            preset: "FAANG · concise".into(),
            ..Default::default()
        };
        offered.advance_to(ApplicationStatus::Offer);

        let apps = Applications {
            entries: vec![
                offered,
                rejected_after_onsite,
                // Wishlist card on the same preset: never sent, excluded.
                Application {
                    company: "Someday Co".into(),
                    preset: "FAANG · concise".into(),
                    ..Default::default()
                },
                // No preset attributed: nothing to credit it to.
                Application {
                    company: "Cold Email Co".into(),
                    status_word: ApplicationStatus::Applied.word().into(),
                    furthest: ApplicationStatus::Applied,
                    ..Default::default()
                },
            ],
        };

        let conversion = apps.conversion();
        assert_eq!(conversion.len(), 1);
        let faang = &conversion[0];
        assert_eq!(faang.preset, "FAANG · concise");
        assert_eq!(faang.sent, 2);
        // Both reached interview — this is the assertion that would have been
        // wrong when the funnel counted from `status`.
        assert_eq!(faang.interviews, 2);
        assert_eq!(faang.offers, 1);
    }

    /// A card dragged back to an earlier column has still been through what it
    /// has been through. `furthest` records history, and history does not
    /// un-happen because a board was tidied up.
    #[test]
    fn advance_to_never_lowers_the_furthest_stage() {
        let mut app = Application::default();
        app.advance_to(ApplicationStatus::Interviewing);
        assert_eq!(app.furthest, ApplicationStatus::Interviewing);

        app.advance_to(ApplicationStatus::Applied);
        assert_eq!(app.status(), ApplicationStatus::Applied);
        assert_eq!(app.furthest, ApplicationStatus::Interviewing);

        app.advance_to(ApplicationStatus::Wishlist);
        assert_eq!(app.furthest, ApplicationStatus::Interviewing);
    }

    /// `furthest` arrived after the first `applications.toml` files existed,
    /// and after hand-editing is invited. An entry that says `status = "offer"`
    /// and nothing about `furthest` must not report a zeroed funnel — the
    /// load-time repair is what makes the file format forgiving.
    #[test]
    fn a_file_written_without_furthest_still_counts_its_offer() {
        let hand_written = "[[entries]]\ncompany = \"Meridian\"\nrole = \"Senior SWE\"\n\
                            status = \"offer\"\npreset = \"FAANG · concise\"\n";
        let mut apps: Applications = toml::from_str(hand_written).expect("loads");
        assert_eq!(apps.entries[0].furthest, ApplicationStatus::Wishlist);

        // What `vault::load_applications` does on the user's behalf.
        apps.normalize();
        assert_eq!(apps.entries[0].furthest, ApplicationStatus::Offer);

        let conversion = apps.conversion();
        assert_eq!(conversion[0].offers, 1);
        assert_eq!(conversion[0].interviews, 1);
        assert_eq!(conversion[0].sent, 1);
    }
}

/// `value.clone()` when `keep`, an empty collection otherwise — the shape
/// `ResumeDoc::compose` needs to drop a hidden section without the renderer
/// having to know about visibility at all.
fn take_if<T: Clone + Default>(keep: bool, value: &T) -> T {
    if keep {
        value.clone()
    } else {
        T::default()
    }
}

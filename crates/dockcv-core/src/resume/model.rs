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
    /// An id from a raw number, for building fixtures.
    ///
    /// Test-only on purpose: ids are handed out by `next_custom_section_id` and
    /// never reissued (D-9), and a public constructor is how two live sections
    /// come to share one. A fixture needs stable ids, and nothing else does.
    #[cfg(test)]
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

// ---------------------------------------------------------------------------
// Applications (roadmap D4, the product review US-04/US-05, P-03/P-04)
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
    /// The column an application lands in once it is over and there was no
    /// offer. Not "Rejected", which it used to be called: rejections,
    /// ghostings and withdrawals all end up here, and naming the column after
    /// one of the three made the other two look like something they are not.
    /// Which of them it was is [`Application::closed_as`].
    Closed,
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
            ApplicationStatus::Closed => "closed",
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
            "closed" => Some(ApplicationStatus::Closed),
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
    pub fn depth(self) -> Option<u8> {
        match self {
            ApplicationStatus::Wishlist => Some(0),
            ApplicationStatus::Applied => Some(1),
            ApplicationStatus::Interviewing => Some(2),
            ApplicationStatus::Offer => Some(3),
            ApplicationStatus::Closed => None,
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
    /// ISO date the card was created — the mockup's "saved 4d ago".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created: String,
    /// ISO date it was actually sent. `None` while it is still a wishlist entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<String>,
    /// The posting.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Free text: "$168k base · negotiating". Deliberately not a number —
    /// compensation is a negotiation state, not a figure to do arithmetic on.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub compensation: String,
    /// Free text about how it ended — `Some("role filled")` vs `None`, which
    /// the card renders as "no reason given".
    ///
    /// Read alongside [`Self::closed_as`], which is the typed half.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closure_note: Option<String>,
    /// How it ended, once it has. `None` while it is still live — which is
    /// not the same as `Ghosted`, and the difference is the user's to draw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_as: Option<Closure>,
    /// The CV this was sent with, if one has been attributed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_as: Option<SentCv>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_step: Option<NextStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<Snapshot>,
    /// Every stage this card has moved into, oldest first.
    ///
    /// [`Self::furthest`] answers *how deep did this ever get*; this answers
    /// *when*, which is the question a funnel over a date range cannot be
    /// asked without. Append-only, and written in exactly one place
    /// ([`Self::advance_to`]).
    ///
    /// The card's **first** stage is not in here. A card starts on the
    /// wishlist at [`Self::created`], so recording that would be one fact in
    /// two places; the first entry is the first time it *moved*. A card that
    /// has never moved therefore carries no history at all, which is correct
    /// and is not the same as a card written before this field existed —
    /// those are indistinguishable, and Insights has to say so rather than
    /// draw a month it cannot account for (US-14).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<StageChange>,
    /// The conversations that have happened, oldest first.
    ///
    /// Kept apart from [`Self::history`] on purpose: history is where the card
    /// *moved*, and three rounds of interview are three events inside one
    /// move. Folding them together would make "entered Interviewing" mean two
    /// different things depending on which entry you were reading.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rounds: Vec<InterviewRound>,
}

/// A conversation that actually happened — a screen, an onsite, a panel.
///
/// **Not a stage.** The board has one `Interviewing` column and keeps it: a
/// second and a third round are the same stage happening again, and a column
/// per round is a board that grows sideways for every person who gets deep
/// into one process. What varies between applications is *how many times*,
/// which is a count, and a count belongs in a list rather than in the shape
/// of the UI.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterviewRound {
    /// ISO date it happened.
    pub at: String,
    /// What it was: "Technical screen", "Onsite", "Final panel". Free text
    /// for the same reason `compensation` is — a process names its own steps,
    /// and an enum here would be this app telling companies how to hire.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// How an application ended.
///
/// Orthogonal to the column it sits in. The board answers *where is this now*;
/// this answers *how did it finish*, and the two are different questions — an
/// offer you turned down is not a rejection, and silence is not a no even
/// though it ends the same way.
///
/// `Ghosted` is set by the user, never inferred. The app can notice that
/// nothing has moved in eight weeks and say so; deciding that silence is final
/// is a judgement about a company, and inventing it would be inventing the
/// number people quote most (US-14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Closure {
    /// They said no.
    Rejected,
    /// No reply, ever.
    Ghosted,
    /// You pulled out before there was anything to turn down.
    Withdrew,
    /// You turned down an offer.
    Declined,
    /// You took it.
    Accepted,
}

impl Closure {
    pub const ALL: [Closure; 5] = [
        Closure::Rejected,
        Closure::Ghosted,
        Closure::Withdrew,
        Closure::Declined,
        Closure::Accepted,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Closure::Rejected => "Rejected",
            Closure::Ghosted => "Ghosted",
            Closure::Withdrew => "Withdrew",
            Closure::Declined => "Declined",
            Closure::Accepted => "Accepted",
        }
    }

    /// The column a finished application belongs in.
    ///
    /// The board says *where*, the closure says *how it ended*, and those are
    /// two spellings of one fact — so one of them has to derive the other or
    /// they will disagree. They did: a card dragged to Rejected read as
    /// rejected on the board and as still in progress in the journey diagram,
    /// because the board wrote the column and the diagram read the closure.
    pub fn column(self) -> ApplicationStatus {
        match self {
            // There was an offer, so the card belongs where offers live —
            // a search that ended in a yes should not be filed under no.
            Closure::Accepted | Closure::Declined => ApplicationStatus::Offer,
            Closure::Rejected | Closure::Ghosted | Closure::Withdrew => ApplicationStatus::Closed,
        }
    }
}

/// The CV an application was sent with.
///
/// One field rather than the two it was — a `source_doc: Option<String>` and a
/// `preset: String` — because it is one fact and the split made its absence
/// spellable four ways. Three of those four were unreachable, and every reader
/// had to decide for itself what a preset with no document meant.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentCv {
    /// The document's file stem — a label, not a live reference. A document
    /// can be renamed or deleted and the card must still tell the truth about
    /// what was sent (US-04).
    pub document: String,
    /// The preset within it. Empty when the document has none, which is a
    /// document sent whole rather than a missing value.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preset: String,
}

impl SentCv {
    /// How it reads on a card: `resume · Fintech Staff`, or just the document.
    pub fn label(&self) -> String {
        if self.preset.is_empty() {
            self.document.clone()
        } else {
            format!("{} · {}", self.document, self.preset)
        }
    }
}

/// One recorded move between stages.
///
/// `to` only. Where a move came *from* is the previous entry's `to`, or the
/// wishlist for the first one — storing it as well would be one fact in two
/// places, free to disagree, which is the reason `status_word` has no second
/// field beside it either.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageChange {
    /// ISO date the move was made. Supplied by the caller rather than read
    /// from a clock here: a model that tells the time cannot be tested.
    pub at: String,
    /// The stage moved into, as the word — the form that round-trips, for the
    /// same reason [`Application::status_word`] is stored that way.
    pub to: String,
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

    /// The deepest stage this application ever reached, which is not where it
    /// sits now: most interviews end in a rejection, and a funnel counted from
    /// the current column alone would erase every one of them (review P-04).
    ///
    /// Read from [`Self::history`] rather than stored beside it. It was a
    /// field, kept in step by `advance_to` — one fact in two places, and the
    /// stored copy could only ever be a slower, staler way of asking the list
    /// that already knows. `Rejected` has no depth, so a rejection never
    /// counts as deeper than the stage it interrupted.
    pub fn furthest(&self) -> ApplicationStatus {
        self.history
            .iter()
            .filter_map(|change| ApplicationStatus::from_word(&change.to))
            .chain(std::iter::once(self.status()))
            .filter(|stage| stage.depth().is_some())
            .max_by_key(|stage| stage.depth().unwrap_or(0))
            .unwrap_or(ApplicationStatus::Wishlist)
    }

    pub fn advance_to(&mut self, status: ApplicationStatus, on: &str) {
        // Dropping a card back into the column it is already in is not a
        // move. Recording it would invent a transition, and every count over
        // a date range would be a tally of how often the board was fidgeted
        // with rather than of what happened.
        let moved = self.status_word != status.word();

        // Whatever the file used to say, the user has now said otherwise —
        // this is the one place an unrecognised word is allowed to be lost.
        self.status_word = status.word().to_string();

        if moved {
            self.history.push(StageChange {
                at: on.to_string(),
                to: status.word().to_string(),
            });
        }

        // Keep the closure in step with the column, in the one place the
        // column is written. Dragging a card is the commonest way to say an
        // application is over, and it must mean the same thing as saying so
        // in the panel.
        self.closed_as = match (status, self.closed_as) {
            // Moved into the closed column and nothing said about why: it
            // stays unsaid. Guessing "rejected" put words in the user's mouth
            // — a job they withdrew from would have been filed as a company
            // turning them down, and the diagram would have counted it that
            // way. Unsaid is a state the board can ask about; a wrong answer
            // is not.
            (ApplicationStatus::Closed, some) => some,
            // An offer keeps only the endings that follow one.
            (ApplicationStatus::Offer, Some(c @ (Closure::Accepted | Closure::Declined))) => {
                Some(c)
            }
            // Anywhere else the application is live again — which is exactly
            // what dragging a ghosted card back to Applied is for.
            _ => None,
        };
    }

    /// Say how this ended, and file it where that ending belongs.
    ///
    /// The counterpart to [`Self::advance_to`]: that one is told a column and
    /// derives the closure, this one is told a closure and derives the column.
    /// Both go through `advance_to` so the move is recorded in the history
    /// either way.
    pub fn close_as(&mut self, closure: Closure, on: &str) {
        self.advance_to(closure.column(), on);
        self.closed_as = Some(closure);
    }

    /// Reopen — it replied after all.
    pub fn reopen(&mut self, to: ApplicationStatus, on: &str) {
        self.advance_to(to, on);
        self.closed_as = None;
    }

    /// The stage this card was in immediately before `index` in its history —
    /// the previous entry's destination, or the wishlist it started on.
    pub fn stage_before(&self, index: usize) -> ApplicationStatus {
        if index == 0 {
            return ApplicationStatus::Wishlist;
        }
        self.history
            .get(index - 1)
            .and_then(|c| ApplicationStatus::from_word(&c.to))
            .unwrap_or(ApplicationStatus::Wishlist)
    }
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
            .filter(|a| a.status() != ApplicationStatus::Closed)
            .count()
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
/// sections (D-9, the roadmap) — deliberately one new variant rather
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

/// How the Skills section is laid out.
///
/// Sections differ in what a layout choice even *means* — a skill group is a
/// label and a bag of words, a job is a dated entry with bullets — so this is
/// one enum for one section rather than a `SectionStyle` pretending to span
/// all of them. When Work grows its own choices they get their own type.
///
/// Nothing here derives a proficiency level: the model stores no such field,
/// and a bar chart of invented percentages is exactly the fabricated metric
/// US-14 forbids. Every style below is a different arrangement of words the
/// user actually typed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillsStyle {
    /// `Category: one, two, three` — one line per group.
    ///
    /// The default, and deliberately the shape every document had before this
    /// existed: a CV written last year renders byte-identically today.
    #[default]
    #[serde(alias = "inline")]
    Rows,
    /// Each keyword in its own pill, the category leading them.
    ///
    /// What a reader scanning for a technology finds fastest, and the reason
    /// this work started — it is the shape every modern builder offers and
    /// the one DockCV could not produce.
    Bubbles,
    /// The category in a fixed left column, keywords flowing beside it.
    ///
    /// Distinct from `Inline`, which wraps keywords under the category's own
    /// indent; here the categories line up as a column, which reads as a
    /// table when there are several.
    Grid,
    /// One flowing list, categories dropped.
    ///
    /// For a CV whose groups are an artefact of import rather than a
    /// distinction worth printing — LinkedIn exports have no categories at
    /// all, so this is the honest shape for that data.
    Compact,
}

impl SkillsStyle {
    pub const ALL: [SkillsStyle; 4] = [
        SkillsStyle::Rows,
        SkillsStyle::Bubbles,
        SkillsStyle::Grid,
        SkillsStyle::Compact,
    ];

    /// What the picker shows.
    pub fn label(self) -> &'static str {
        match self {
            SkillsStyle::Rows => "Rows",
            SkillsStyle::Bubbles => "Bubbles",
            SkillsStyle::Grid => "Grid",
            SkillsStyle::Compact => "Compact",
        }
    }

    /// The word the generated Typst branches on.
    pub fn keyword(self) -> &'static str {
        match self {
            SkillsStyle::Rows => "rows",
            SkillsStyle::Bubbles => "bubbles",
            SkillsStyle::Grid => "grid",
            SkillsStyle::Compact => "compact",
        }
    }
}

/// What goes between two keywords.
///
/// The reason this is a control at all: a Skills section is the densest text
/// on a CV — a real one runs to sixty-odd terms — and a comma disappears
/// between them, so the list reads as one long sentence.
///
/// Measured honestly: a rule is one character *wider* than a comma, so this
/// buys scannability rather than space. The space comes from
/// [`RowSpacing::Tight`] and from dropping the category mark; what this fixes
/// is that sixty comma-separated terms are unreadable at any density.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillSeparator {
    #[default]
    Comma,
    /// `a | b` — the densest of the four, and what the reference layouts use.
    Rule,
    /// `a · b`
    Middot,
    /// `a • b`
    Bullet,
}

impl SkillSeparator {
    pub const ALL: [SkillSeparator; 4] = [
        SkillSeparator::Comma,
        SkillSeparator::Rule,
        SkillSeparator::Middot,
        SkillSeparator::Bullet,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SkillSeparator::Comma => "a, b",
            SkillSeparator::Rule => "a | b",
            SkillSeparator::Middot => "a · b",
            SkillSeparator::Bullet => "a • b",
        }
    }

    /// The characters printed between two keywords, spacing included.
    pub fn printed(self) -> &'static str {
        match self {
            SkillSeparator::Comma => ", ",
            // Single spaces, not double. The first version padded these to
            // `  |  ` and made the section *wider* than commas — it bought
            // scannability and paid for it in the wrapping, which is the
            // opposite of the point. One space each side still separates.
            SkillSeparator::Rule => " | ",
            SkillSeparator::Middot => " · ",
            SkillSeparator::Bullet => " • ",
        }
    }
}

/// What follows a category name, before its keywords.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CategoryMark {
    #[default]
    Colon,
    /// `Category — a, b`, which reads as a heading rather than a key.
    Dash,
    /// `(Category) a, b`
    Bracket,
    /// Nothing at all — the weight of the category carries it.
    None,
}

impl CategoryMark {
    pub const ALL: [CategoryMark; 4] = [
        CategoryMark::Colon,
        CategoryMark::Dash,
        CategoryMark::Bracket,
        CategoryMark::None,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CategoryMark::Colon => "Name:",
            CategoryMark::Dash => "Name —",
            CategoryMark::Bracket => "(Name)",
            CategoryMark::None => "Name",
        }
    }

    /// `(before, after)` the category name.
    pub fn wraps(self) -> (&'static str, &'static str) {
        match self {
            CategoryMark::Colon => ("", ":"),
            CategoryMark::Dash => ("", " —"),
            CategoryMark::Bracket => ("(", ")"),
            CategoryMark::None => ("", ""),
        }
    }
}

/// How much air a Skills section leaves between its rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RowSpacing {
    /// The old spacing.
    #[default]
    Spacious,
    /// Half of it. Eight groups at spacious spacing cost most of a page.
    Tight,
}

impl RowSpacing {
    pub const ALL: [RowSpacing; 2] = [RowSpacing::Spacious, RowSpacing::Tight];

    pub fn label(self) -> &'static str {
        match self {
            RowSpacing::Spacious => "Spacious",
            RowSpacing::Tight => "Tight",
        }
    }

    /// Points between rows.
    pub fn gap_pt(self) -> f32 {
        match self {
            RowSpacing::Spacious => 2.0,
            RowSpacing::Tight => 0.5,
        }
    }
}

/// Everything about how the Skills section is set.
///
/// A struct rather than five fields on `LayoutSettings` because they belong
/// together and TOML says so: `[layout.skills]` with five keys reads as one
/// decision, `skills_separator = …` alongside `page_size` reads as debris.
/// The cost is a migration, since documents already carry `skills = "inline"`
/// — see the `Deserialize` impl, which accepts both shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SkillsLayout {
    pub style: SkillsStyle,
    pub separator: SkillSeparator,
    pub mark: CategoryMark,
    pub spacing: RowSpacing,
    /// Start each row with a bullet, so groups read as a list.
    pub bullets: bool,
}

impl<'de> Deserialize<'de> for SkillsLayout {
    /// Accepts the table this writes *and* the bare string that documents
    /// written before the options existed carry (`skills = "inline"`).
    ///
    /// Without this every such document would fail to load — not fall back,
    /// fail — because a string is not a table. A migration that loses the
    /// user's chosen style would be quieter and worse.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            JustTheStyle(SkillsStyle),
            Whole {
                #[serde(default)]
                style: SkillsStyle,
                #[serde(default)]
                separator: SkillSeparator,
                #[serde(default)]
                mark: CategoryMark,
                #[serde(default)]
                spacing: RowSpacing,
                #[serde(default)]
                bullets: bool,
            },
        }

        Ok(match Either::deserialize(deserializer)? {
            Either::JustTheStyle(style) => SkillsLayout {
                style,
                ..SkillsLayout::default()
            },
            Either::Whole {
                style,
                separator,
                mark,
                spacing,
                bullets,
            } => SkillsLayout {
                style,
                separator,
                mark,
                spacing,
                bullets,
            },
        })
    }
}

/// Where a dated entry puts its date and location.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetaPosition {
    /// Right-aligned on the title's own line — compact, and what every
    /// document did before this existed.
    #[default]
    Right,
    /// On its own line under the title. Costs a line per entry and buys a
    /// title that is never squeezed by a long date range.
    Below,
}

impl MetaPosition {
    pub const ALL: [MetaPosition; 2] = [MetaPosition::Right, MetaPosition::Below];

    pub fn label(self) -> &'static str {
        match self {
            MetaPosition::Right => "Right of title",
            MetaPosition::Below => "Below title",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            MetaPosition::Right => "right",
            MetaPosition::Below => "below",
        }
    }
}

/// Which of the date and the location comes first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetaOrder {
    #[default]
    DateFirst,
    LocationFirst,
}

impl MetaOrder {
    pub const ALL: [MetaOrder; 2] = [MetaOrder::DateFirst, MetaOrder::LocationFirst];

    pub fn label(self) -> &'static str {
        match self {
            MetaOrder::DateFirst => "Date, place",
            MetaOrder::LocationFirst => "Place, date",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            MetaOrder::DateFirst => "date-first",
            MetaOrder::LocationFirst => "location-first",
        }
    }
}

/// How a run of text is emphasised.
///
/// One type for the two places that need it — an entry's subtitle and its
/// date/location line — because "regular, bold or italic" is the same choice
/// twice and two enums saying it would be two things to keep in step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Emphasis {
    Regular,
    Bold,
    #[default]
    Italic,
}

impl Emphasis {
    pub const ALL: [Emphasis; 3] = [Emphasis::Regular, Emphasis::Bold, Emphasis::Italic];

    pub fn label(self) -> &'static str {
        match self {
            Emphasis::Regular => "Regular",
            Emphasis::Bold => "Bold",
            Emphasis::Italic => "Italic",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            Emphasis::Regular => "regular",
            Emphasis::Bold => "bold",
            Emphasis::Italic => "italic",
        }
    }
}

/// The glyph a bullet list uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BulletGlyph {
    #[default]
    Dot,
    Dash,
    /// No marker at all — the indent carries it. For a CV whose bullets are
    /// full sentences and read as paragraphs.
    None,
}

impl BulletGlyph {
    pub const ALL: [BulletGlyph; 3] = [BulletGlyph::Dot, BulletGlyph::Dash, BulletGlyph::None];

    pub fn label(self) -> &'static str {
        match self {
            BulletGlyph::Dot => "• Dot",
            BulletGlyph::Dash => "– Dash",
            BulletGlyph::None => "None",
        }
    }

    /// What Typst's `list(marker: …)` is given.
    pub fn marker(self) -> &'static str {
        match self {
            BulletGlyph::Dot => "•",
            BulletGlyph::Dash => "–",
            BulletGlyph::None => "",
        }
    }
}

/// How a dated entry — a job, a degree, a certificate — is set.
///
/// Separate from [`SkillsLayout`] because they are different shapes of data:
/// an entry is a title, a subtitle, two pieces of metadata and a list, and a
/// skill group is a label and a bag of words. A single `SectionStyle` spanning
/// both would have to pretend they answer the same questions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryLayout {
    #[serde(default)]
    pub meta_position: MetaPosition,
    #[serde(default)]
    pub meta_order: MetaOrder,
    #[serde(default)]
    pub subtitle: Emphasis,
    #[serde(default)]
    pub meta: Emphasis,
    #[serde(default)]
    pub bullet: BulletGlyph,
    /// Indent the summary and bullets under the entry's title, so the block
    /// reads as belonging to it rather than starting again at the margin.
    #[serde(default)]
    pub indent_body: bool,
}

/// Which edge the header sits against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeaderAlign {
    /// What every document did before this existed.
    #[default]
    Center,
    Left,
}

impl HeaderAlign {
    pub const ALL: [HeaderAlign; 2] = [HeaderAlign::Center, HeaderAlign::Left];

    pub fn label(self) -> &'static str {
        match self {
            HeaderAlign::Center => "Centred",
            HeaderAlign::Left => "Left",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            HeaderAlign::Center => "center",
            HeaderAlign::Left => "left",
        }
    }
}

/// How the contact details under the name are arranged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContactLayout {
    /// One flowing line, items joined by a separator. Cheapest in space and
    /// what the header has always done.
    #[default]
    Inline,
    /// One per line. Costs several lines at the top of the page and buys a
    /// header that never wraps mid-address.
    Stacked,
    /// Two columns — half the lines of `Stacked`, still one item per row.
    Columns,
}

impl ContactLayout {
    pub const ALL: [ContactLayout; 3] = [
        ContactLayout::Inline,
        ContactLayout::Stacked,
        ContactLayout::Columns,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ContactLayout::Inline => "One line",
            ContactLayout::Stacked => "One per line",
            ContactLayout::Columns => "Two columns",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            ContactLayout::Inline => "inline",
            ContactLayout::Stacked => "stacked",
            ContactLayout::Columns => "columns",
        }
    }

    /// Whether a separator between items is a real choice for this shape.
    ///
    /// It is not, for the two that put each item on its own row — and a
    /// control that changes nothing is a label pretending to be a control
    /// (E-43). The rail hides it rather than offering a dead one.
    pub fn uses_separator(self) -> bool {
        matches!(self, ContactLayout::Inline)
    }
}

/// How the block above the first section is set: the name, the title under it,
/// and the contact details.
///
/// No icon control here. The reference layouts draw a glyph before each
/// detail, which needs an icon font in the document — the vendored AltaCV
/// package carries FontAwesome for exactly that — and wiring one into this
/// template is its own piece of work rather than a fourth dropdown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderLayout {
    #[serde(default)]
    pub align: HeaderAlign,
    #[serde(default)]
    pub contacts: ContactLayout,
    /// Between contact details, when they share a line. Reuses the Skills
    /// separator because it is the same question — what goes between items in
    /// a run — and a second enum saying it would be a second thing to keep in
    /// step.
    #[serde(default)]
    pub separator: SkillSeparator,
}

/// What one section sets differently from the document's own layout.
///
/// Sparse on purpose: a row exists only while a section actually differs, so a
/// document nobody has customised carries no table at all and the
/// document-wide setting stays the single place to change everything. On a CV,
/// uniformity is the default and difference is the exception — a page assembled
/// from seven layouts reads as broken, not as designed.
///
/// This is the shape the rest of the per-section settings land in: `Option`
/// fields for the layouts that have a document-wide value to fall back to
/// (`HeadingLayout`, `EntryLayout`), plain fields for the ones that only ever
/// make sense for one section, like the flag below.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionOverrides {
    /// Print no heading above this section.
    ///
    /// The case that asked for it is Profile: a great many CVs open with the
    /// summary paragraph directly under the contact line, and the renderer
    /// always printed "PROFILE" over it. Per section rather than a seventh
    /// [`HeadingStyle`] because it is never a decision about the whole
    /// document — a CV with no section headings at all is not a CV.
    #[serde(default)]
    pub no_heading: bool,

    // One `Option` per *field* rather than one per struct. Overriding a whole
    // `HeadingLayout` would mean that choosing a style for one section quietly
    // pins its capitalisation and alignment too — and then changing the
    // document's capitalisation would visibly skip the section the user had
    // only ever restyled. Per field, "Skills is set in a rule" stays true and
    // stays *only* that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_style: Option<HeadingStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_case: Option<HeadingCase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_align: Option<HeaderAlign>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_position: Option<MetaPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_order: Option<MetaOrder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<Emphasis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Emphasis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet: Option<BulletGlyph>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent_body: Option<bool>,
}

impl SectionOverrides {
    /// Whether this row carries nothing. The signal to drop it rather than
    /// write a line of defaults to disk — see the struct's own comment.
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// Whether this section departs from the document's heading at all — the
    /// question the generated Typst asks before emitting anything for it.
    pub fn touches_heading(self) -> bool {
        self.heading_style.is_some() || self.heading_case.is_some() || self.heading_align.is_some()
    }

    /// The same question for a dated entry.
    pub fn touches_entries(self) -> bool {
        self.meta_position.is_some()
            || self.meta_order.is_some()
            || self.subtitle.is_some()
            || self.meta.is_some()
            || self.bullet.is_some()
            || self.indent_body.is_some()
    }

    /// This section's heading, resolved against the document's.
    pub fn headings(self, document: HeadingLayout) -> HeadingLayout {
        HeadingLayout {
            style: self.heading_style.unwrap_or(document.style),
            case: self.heading_case.unwrap_or(document.case),
            align: self.heading_align.unwrap_or(document.align),
        }
    }

    /// This section's dated entries, resolved against the document's.
    pub fn entries(self, document: EntryLayout) -> EntryLayout {
        EntryLayout {
            meta_position: self.meta_position.unwrap_or(document.meta_position),
            meta_order: self.meta_order.unwrap_or(document.meta_order),
            subtitle: self.subtitle.unwrap_or(document.subtitle),
            meta: self.meta.unwrap_or(document.meta),
            bullet: self.bullet.unwrap_or(document.bullet),
            indent_body: self.indent_body.unwrap_or(document.indent_body),
        }
    }
}

/// How the bar above each section is drawn.
///
/// The band is what every document has had until now, and it is a strong
/// choice: it reads as a divider, but it also spends a filled block of ink on
/// every section of a page that is mostly white. The alternatives are the
/// quieter ways the same job is done on a CV — a rule, a border, or nothing at
/// all — so the decision stops being the template's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeadingStyle {
    /// A filled band across the column. What every document did before this.
    #[default]
    Band,
    /// A hairline under the heading, the full width of the column.
    Rule,
    /// The heading, then a hairline carrying on to the right margin. Costs no
    /// line of its own, which on a full CV is a section's worth of space.
    RuleToMargin,
    /// A hairline under the words only, as long as they are.
    Underline,
    /// A thin border around the heading — the band's shape without its fill.
    Boxed,
    /// The words alone. The type does the separating.
    Plain,
}

impl HeadingStyle {
    pub const ALL: [HeadingStyle; 6] = [
        HeadingStyle::Band,
        HeadingStyle::Rule,
        HeadingStyle::RuleToMargin,
        HeadingStyle::Underline,
        HeadingStyle::Boxed,
        HeadingStyle::Plain,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HeadingStyle::Band => "Filled band",
            HeadingStyle::Rule => "Rule under",
            HeadingStyle::RuleToMargin => "Rule to margin",
            HeadingStyle::Underline => "Underline",
            HeadingStyle::Boxed => "Boxed",
            HeadingStyle::Plain => "Plain",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            HeadingStyle::Band => "band",
            HeadingStyle::Rule => "rule",
            HeadingStyle::RuleToMargin => "rule-to-margin",
            HeadingStyle::Underline => "underline",
            HeadingStyle::Boxed => "boxed",
            HeadingStyle::Plain => "plain",
        }
    }

    /// Whether the style puts the heading against an edge the alignment
    /// control can move it along. `RuleToMargin` cannot: its rule fills
    /// whatever the words leave, so the words are always at the left.
    pub fn can_align(self) -> bool {
        !matches!(self, HeadingStyle::RuleToMargin)
    }
}

/// Whether a section title is shouted or printed as the user typed it.
///
/// Two options, not three. "Title Case" would mean rewriting the user's own
/// words, and the rules for doing that are language-specific — this app is
/// used in Ukrainian, where they are not English's. Upper-casing is a
/// reversible display decision; re-capitalising is an edit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeadingCase {
    /// What every document did before this existed.
    #[default]
    Upper,
    AsTyped,
}

impl HeadingCase {
    pub const ALL: [HeadingCase; 2] = [HeadingCase::Upper, HeadingCase::AsTyped];

    pub fn label(self) -> &'static str {
        match self {
            HeadingCase::Upper => "UPPERCASE",
            HeadingCase::AsTyped => "As typed",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            HeadingCase::Upper => "upper",
            HeadingCase::AsTyped => "as-typed",
        }
    }
}

/// How the bar above each section is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadingLayout {
    #[serde(default)]
    pub style: HeadingStyle,
    #[serde(default)]
    pub case: HeadingCase,
    /// Which edge the words sit against. Reuses [`HeaderAlign`] because it is
    /// the same question the header already asks, and a second enum saying it
    /// would be a second thing to keep in step.
    #[serde(default)]
    pub align: HeaderAlign,
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

/// What the filename pattern's tokens stand for on one particular export.
///
/// Named fields rather than six positional `&str`s: every one of them is a
/// string, so a transposed pair would compile and quietly produce a filename
/// with the company where the role belongs.
///
/// A field left empty removes its token *and* the separator beside it, so a
/// pattern is safe to write with tokens that only some export paths can fill —
/// `{company}` resolves only when the export starts from an application card.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExportTokens<'a> {
    pub name: &'a str,
    pub role: &'a str,
    pub preset: &'a str,
    pub company: &'a str,
    pub variant: &'a str,
    pub date: &'a str,
}

/// How this document names the files it exports, and where it last sent one.
///
/// Document-level rather than app-level: two CVs in a vault are for different
/// jobs and belong in different folders under different names, and an app-wide
/// setting would make the second export of one of them propose the other's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSettings {
    #[serde(default = "ExportSettings::default_pattern")]
    pub filename_pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_destination: Option<std::path::PathBuf>,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            filename_pattern: Self::default_pattern(),
            last_destination: None,
        }
    }
}

impl ExportSettings {
    pub const DEFAULT_PATTERN: &'static str = "{name} - {role} - {preset}";

    pub fn default_pattern() -> String {
        Self::DEFAULT_PATTERN.to_string()
    }

    /// Whether the export settings match the default state.
    pub fn is_default(&self) -> bool {
        (self.filename_pattern.trim() == Self::DEFAULT_PATTERN
            || self.filename_pattern.trim().is_empty())
            && self.last_destination.is_none()
    }

    /// Preset filename patterns offered in the layout rail.
    pub const PRESETS: &'static [(&'static str, &'static str)] = &[
        ("Name · Role · Preset", "{name} - {role} - {preset}"),
        ("Name · Role · Company", "{name} - {role} - {company}"),
        ("Name · Role", "{name} - {role}"),
        ("Name · Preset", "{name} - {preset}"),
        ("Name · Date", "{name} - {date}"),
        ("Name only", "{name}"),
    ];

    /// Resolve tokens in the pattern and sanitize the resulting filename stem.
    pub fn resolve_filename(&self, tokens: &ExportTokens<'_>) -> String {
        let ExportTokens {
            name,
            role,
            preset,
            company,
            variant,
            date,
        } = *tokens;

        let pattern = if self.filename_pattern.trim().is_empty() {
            Self::DEFAULT_PATTERN
        } else {
            self.filename_pattern.as_str()
        };

        let mut result = pattern.to_string();

        let clean_company = company.trim();
        if clean_company.is_empty() {
            result = remove_token_with_separators(&result, "company");
        } else {
            result = result.replace("{company}", clean_company);
        }

        let clean_preset = preset.trim();
        if clean_preset.is_empty() {
            result = remove_token_with_separators(&result, "preset");
        } else {
            result = result.replace("{preset}", clean_preset);
        }

        let clean_role = role.trim();
        if clean_role.is_empty() {
            result = remove_token_with_separators(&result, "role");
            result = remove_token_with_separators(&result, "label");
        } else {
            result = result
                .replace("{role}", clean_role)
                .replace("{label}", clean_role);
        }

        let clean_variant = variant.trim();
        if clean_variant.is_empty() {
            result = remove_token_with_separators(&result, "variant");
        } else {
            result = result.replace("{variant}", clean_variant);
        }

        let clean_date = date.trim();
        if clean_date.is_empty() {
            result = remove_token_with_separators(&result, "date");
        } else {
            result = result.replace("{date}", clean_date);
        }

        let name_val = if name.trim().is_empty() {
            "CV"
        } else {
            name.trim()
        };
        result = result.replace("{name}", name_val);

        super::export_names::sanitize_filename_stem(&result)
    }
}

/// One file that left the building.
///
/// The answer to "which file did I actually send in July" for a CV that never
/// became an application card — and the only place a preset name is allowed to
/// sit beside a filename, because this is inside the vault and the file is not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRecord {
    /// Timestamp when exported (e.g. ISO date or formatted string).
    pub timestamp: String,
    /// Format identifier (e.g. "PDF", "DOCX", "Plain Text", etc.).
    pub format: String,
    /// The preset name at that moment.
    pub preset: String,
    /// Destination path written to on disk.
    pub path: std::path::PathBuf,
}

fn remove_token_with_separators(s: &str, token: &str) -> String {
    let t = format!("{{{token}}}");
    if !s.contains(&t) {
        return s.to_string();
    }
    let patterns = [
        format!(" - {t}"),
        format!("{t} - "),
        format!(" – {t}"),
        format!("{t} – "),
        format!(" · {t}"),
        format!("{t} · "),
        format!("_{t}"),
        format!("{t}_"),
        format!(" {t}"),
        format!("{t} "),
        t,
    ];
    let mut cur = s.to_string();
    for pat in &patterns {
        cur = cur.replace(pat, "");
    }
    cur
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
    /// property of the CV, not an app-wide preference (the product review
    /// §1, US-07). `#[serde(default)]` reproduces exactly the values
    /// `resume/template.rs`'s old hard-coded `PREAMBLE` used, so a document
    /// written before this field existed renders unchanged.
    #[serde(default)]
    pub layout: LayoutSettings,
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
    /// Resolve the filename stem (without extension) for one export.
    ///
    /// `today` is passed in rather than read from a clock here: this crate is
    /// the data model, it compiles to wasm, and a function whose result depends
    /// on the hour is not one a round-trip test can pin down. Callers format it
    /// as `YYYY-MM-DD`, which is the only form that sorts correctly in Finder.
    pub fn export_filename_stem(
        &self,
        preset_name: Option<&str>,
        company: Option<&str>,
        today: &str,
    ) -> String {
        let profile = self.profile.active();
        self.export.resolve_filename(&ExportTokens {
            name: &profile.name,
            role: &profile.label,
            preset: preset_name.unwrap_or_default(),
            company: company.unwrap_or_default(),
            variant: self.profile.active_name(),
            date: today,
        })
    }

    /// Append one export to this document's history.
    pub fn record_export(
        &mut self,
        timestamp: impl Into<String>,
        format: impl Into<String>,
        preset: impl Into<String>,
        path: std::path::PathBuf,
    ) {
        self.export_history.push(ExportRecord {
            timestamp: timestamp.into(),
            format: format.into(),
            preset: preset.into(),
            path,
        });
    }

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
            export: ExportSettings::default(),
            next_custom_section_id: 0,
            custom_sections: Vec::new(),
            hidden_sections: Vec::new(),
            section_overrides: Vec::new(),
            export_history: Vec::new(),
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
            // Rows for sections that are not in the document are noise the
            // renderer would have to skip, so they are dropped here rather
            // than there.
            section_overrides: self
                .section_overrides
                .iter()
                .filter(|(kind, o)| !o.is_empty() && visible(*kind))
                .copied()
                .collect(),
            // Hidden sections stay in the order — they compose as empty, and
            // the renderer skips an empty section anyway. Filtering here would
            // make un-hiding lose the position it had.
            section_order: self.sections(),
            // Emitted in the document's own order, which is what the renderer
            // falls back to when the document has not been reordered. Each
            // one carries its id, so `order` names it rather than counting to
            // it — see `ComposedCustomSection::id`.
            custom_sections: self
                .sections()
                .into_iter()
                .filter_map(|kind| match kind {
                    SectionKind::Custom(id) if visible(kind) => {
                        self.custom_section(id).map(|s| (id, s))
                    }
                    _ => None,
                })
                .map(|(id, s)| ComposedCustomSection {
                    id,
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
        match (
            hidden,
            self.hidden_sections.iter().position(|s| *s == section),
        ) {
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
        let text_len =
            |strings: Vec<&String>| -> usize { strings.iter().map(|s| s.chars().count()).sum() };
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
                .map(|v| {
                    v.data
                        .iter()
                        .map(|e| e.institution.chars().count() + e.study_type.chars().count())
                        .sum()
                })
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
                .map(|v| {
                    v.data
                        .iter()
                        .map(|c| c.name.chars().count() + c.issuer.chars().count())
                        .sum()
                })
                .unwrap_or(0),
            Organizations => self
                .volunteer
                .variants
                .get(index)
                .map(|v| {
                    v.data
                        .iter()
                        .map(|o| {
                            o.position.chars().count() + text_len(o.highlights.iter().collect())
                        })
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
            skills: Default::default(),
            entries: Default::default(),
            header: Default::default(),
            headings: Default::default(),
            sizes: TypeSizes {
                name_pt: 400.0,
                title_pt: -90.0,
                heading_pt: 0.0,
                entry_pt: 0.0,
            },
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
        let (lo, hi) = TypeSizes::DELTA_RANGE;
        assert!(safe.sizes.name_pt <= hi && safe.sizes.title_pt >= lo);
        // The clamp exists to keep the *rendered* size positive, so check the
        // thing that actually reaches Typst rather than only the offset.
        assert!(TypeSizes::resolve(safe.base_size_pt(), safe.sizes.title_pt) >= TypeSizes::MIN_PT);
    }

    #[test]
    fn layout_round_trips_through_toml() {
        let layout = LayoutSettings {
            page_size: PageSize::Letter,
            font: DocumentFont::default(),
            date_format: Default::default(),
            skills: Default::default(),
            entries: Default::default(),
            header: Default::default(),
            headings: Default::default(),
            sizes: TypeSizes {
                name_pt: 12.5,
                title_pt: 2.0,
                heading_pt: -1.5,
                entry_pt: 0.5,
            },
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

        assert!(
            sections.contains(&SectionKind::Custom(id)),
            "got {sections:?}"
        );
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
            history: vec![
                StageChange {
                    at: "2026-06-02".into(),
                    to: "applied".into(),
                },
                StageChange {
                    at: "2026-06-18".into(),
                    to: "interviewing".into(),
                },
            ],
            rounds: vec![InterviewRound {
                at: "2026-06-18".into(),
                label: "Technical screen".into(),
            }],
            closed_as: Some(Closure::Ghosted),
            created: "2026-06-01".into(),
            applied: Some("2026-06-02".into()),
            sent_as: Some(SentCv {
                document: "albert-senior-swe".into(),
                preset: "FAANG · concise".into(),
            }),
            url: "https://brambletech.example/careers/123".into(),
            notes: "Referred by Dana".into(),
            next_step: Some(NextStep {
                label: "Onsite".into(),
                date: "2026-08-20".into(),
                time: "14:00".into(),
            }),
            compensation: "$168k base · negotiating".into(),
            closure_note: None,
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
        assert!(entry.sent_as.is_none());
        assert!(entry.next_step.is_none());
        assert!(entry.closure_note.is_none());
        assert!(entry.snapshots.is_empty());
    }

    /// An unrecognised/typo'd status must not fail the whole file — it falls
    /// back to `Wishlist`, the board's own default column.
    #[test]
    fn unknown_status_falls_back_to_wishlist_rather_than_erroring() {
        let toml_text = "[[entries]]\ncompany = \"Acme\"\nrole = \"SWE\"\nstatus = \"ghosted\"\n";
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
            "created",
            "applied",
            "source_doc",
            "preset",
            "url",
            "notes",
            "next_step",
            "compensation",
            "rejection_reason",
            "snapshots",
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
                    status_word: ApplicationStatus::Closed.word().into(),
                    ..Default::default()
                },
            ],
        };
        assert_eq!(apps.count(ApplicationStatus::Applied), 2);
        assert_eq!(apps.count(ApplicationStatus::Closed), 1);
        assert_eq!(apps.active(), 3); // everything but the one Rejected
    }

    /// A card dragged back to an earlier column has still been through what
    /// it has been through, and `furthest()` reads that off the history —
    /// which does not un-happen because a board was tidied up.
    ///
    /// This is the whole reason the deepest stage is asked for at all: most
    /// interviews end in a rejection, so counting from the current column
    /// would erase every interview that did not end in an offer (P-04).
    #[test]
    fn a_rejection_does_not_erase_the_interview_that_came_before_it() {
        let mut app = Application::default();
        app.advance_to(ApplicationStatus::Applied, "2026-06-01");
        app.advance_to(ApplicationStatus::Interviewing, "2026-06-10");
        assert_eq!(app.furthest(), ApplicationStatus::Interviewing);

        // A rejection is where it *is*, not how deep it got.
        app.advance_to(ApplicationStatus::Closed, "2026-06-20");
        assert_eq!(app.furthest(), ApplicationStatus::Interviewing);

        // And neither does dragging it back by hand: the interview happened
        // whether or not the board still says so.
        app.advance_to(ApplicationStatus::Wishlist, "2026-06-21");
        assert_eq!(app.furthest(), ApplicationStatus::Interviewing);
    }

    /// A hand-written entry that only says `status = "offer"` still counts as
    /// an offer. `furthest` is read from the history and the current column,
    /// so a file with no history is not a file with no funnel.
    #[test]
    fn a_hand_written_entry_still_counts_its_offer() {
        let hand_written = "[[entries]]\ncompany = \"Meridian\"\nrole = \"Senior SWE\"\n\
                            status = \"offer\"\n\n[entries.sent_as]\n\
                            document = \"resume\"\npreset = \"FAANG · concise\"\n";
        let apps: Applications = toml::from_str(hand_written).expect("loads");
        assert_eq!(apps.entries[0].furthest(), ApplicationStatus::Offer);
    }

    fn tokens<'a>(name: &'a str, role: &'a str, preset: &'a str) -> ExportTokens<'a> {
        ExportTokens {
            name,
            role,
            preset,
            ..Default::default()
        }
    }

    #[test]
    fn export_filename_pattern_resolves_every_token() {
        let stem = ExportSettings::default().resolve_filename(&tokens(
            "Albert Einstein",
            "Principal Systems Architect",
            "Backend",
        ));
        assert_eq!(
            stem,
            "Albert Einstein - Principal Systems Architect - Backend"
        );

        // Every token the pattern advertises has to reach the name. A token the
        // menu offers and the resolver drops is a menu item that does nothing.
        let all = ExportSettings {
            filename_pattern: "{name} - {role} - {preset} - {company} - {variant} - {date}".into(),
            ..Default::default()
        };
        assert_eq!(
            all.resolve_filename(&ExportTokens {
                name: "Albert Einstein",
                role: "Staff SWE",
                preset: "Concise",
                company: "Acme",
                variant: "Short",
                date: "2026-09-01",
            }),
            "Albert Einstein - Staff SWE - Concise - Acme - Short - 2026-09-01"
        );

        // Every pattern the layout rail offers must survive a full token set.
        for (label, pattern) in ExportSettings::PRESETS {
            let settings = ExportSettings {
                filename_pattern: (*pattern).into(),
                ..Default::default()
            };
            let stem = settings.resolve_filename(&ExportTokens {
                name: "Ann Lee",
                role: "SRE",
                preset: "Concise",
                company: "Acme",
                variant: "Short",
                date: "2026-09-01",
            });
            for (token, value) in [
                ("{name}", "Ann Lee"),
                ("{role}", "SRE"),
                ("{preset}", "Concise"),
                ("{company}", "Acme"),
                ("{variant}", "Short"),
                ("{date}", "2026-09-01"),
            ] {
                if pattern.contains(token) {
                    assert!(
                        stem.contains(value),
                        "the {label:?} pattern dropped {token}: {stem:?}"
                    );
                }
            }
            assert!(
                !stem.contains('{'),
                "the {label:?} pattern left a token in the filename: {stem:?}"
            );
        }
    }

    #[test]
    fn export_filename_drops_missing_tokens_with_their_separators() {
        let settings = ExportSettings {
            filename_pattern: "{name} - {company} - {role} - {preset}".into(),
            ..Default::default()
        };
        // Company and role are empty here — neither may leave a stray dash.
        assert_eq!(
            settings.resolve_filename(&tokens("Albert Einstein", "", "Concise")),
            "Albert Einstein - Concise"
        );
        assert_eq!(
            settings.resolve_filename(&tokens("Albert Einstein", "", "")),
            "Albert Einstein"
        );
    }

    #[test]
    fn a_company_with_a_slash_in_it_does_not_reach_the_filesystem() {
        let settings = ExportSettings {
            filename_pattern: "{name} - {company}".into(),
            ..Default::default()
        };
        assert_eq!(
            settings.resolve_filename(&ExportTokens {
                name: "Albert Einstein",
                company: "Acme Corp / Tech",
                ..Default::default()
            }),
            "Albert Einstein - Acme Corp - Tech"
        );
    }

    #[test]
    fn export_filename_stem_feeds_every_token_the_rail_offers() {
        let resume = Resume {
            basics: Basics {
                name: "Albert Einstein".into(),
                label: "Principal Systems Architect".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut doc = ResumeDoc::from_resume(resume, "Base");

        // The rail offers "Name · Date"; the stem it produces has to carry one.
        doc.export.filename_pattern = "{name} - {date}".into();
        assert_eq!(
            doc.export_filename_stem(None, None, "2026-09-01"),
            "Albert Einstein - 2026-09-01"
        );

        // `{variant}` is the profile's active variant, which is what names the
        // document in the editor.
        doc.export.filename_pattern = "{name} - {variant}".into();
        assert_eq!(
            doc.export_filename_stem(None, None, "2026-09-01"),
            "Albert Einstein - Base"
        );

        // `{company}` resolves only from an application card, and drops its
        // separator everywhere else.
        doc.export.filename_pattern = "{name} - {company} - {preset}".into();
        assert_eq!(
            doc.export_filename_stem(Some("Concise"), None, "2026-09-01"),
            "Albert Einstein - Concise"
        );
        assert_eq!(
            doc.export_filename_stem(Some("Concise"), Some("Acme"), "2026-09-01"),
            "Albert Einstein - Acme - Concise"
        );
    }

    #[test]
    fn record_export_tracks_entries_and_serializes_to_toml() {
        let mut doc = ResumeDoc::default();
        assert!(doc.export_history.is_empty());

        doc.record_export(
            "2026-09-01 17:30:00",
            "PDF",
            "Concise",
            std::path::PathBuf::from(
                "/tmp/Albert Einstein - Principal Systems Architect - Concise.pdf",
            ),
        );
        assert_eq!(doc.export_history.len(), 1);
        assert_eq!(doc.export_history[0].format, "PDF");
        assert_eq!(doc.export_history[0].preset, "Concise");

        let serialized = toml::to_string(&doc).unwrap();
        assert!(serialized.contains("export_history"));
        assert!(serialized.contains("Concise"));

        let deserialized: ResumeDoc = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.export_history.len(), 1);
        assert_eq!(
            deserialized.export_history[0].path,
            doc.export_history[0].path
        );
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

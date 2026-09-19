//! Per-section variants and document presets.

use serde::{Deserialize, Serialize};

use super::model::CustomSectionId;

// ---------------------------------------------------------------------------
// Per-section versioning
// ---------------------------------------------------------------------------
//
// Each section is versioned independently: it carries several named variants
// (e.g. a generic Work history and one tailored for a specific employer), and
// exactly one is active. The rendered document is the composition of every
// section's active variant — see [`crate::resume::model::ResumeDoc::compose`].

/// A stable identifier for one variant of one section.
///
/// [`CustomSectionId`]'s argument, one level down and for the same reader:
/// `Preset::selection`. A `Vec` position re-points at a different variant the
/// moment an earlier one is deleted, and a *name* re-points at nothing at all
/// the moment it is edited — which is what a preset pinned until 0.4.0, so
/// renaming a variant left every preset that selected it quietly reading
/// whatever happened to be active instead. Measured, not supposed:
/// `a_renamed_variant_does_not_strand_the_presets_that_pin_it`.
///
/// Unique within its **section**, not within the document: `(SectionKind,
/// VariantId)` is the pair a preset pins, and per-section counters keep the
/// numbers small enough that the TOML still reads.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct VariantId(u32);

impl VariantId {
    /// The number behind the id, for the jobs that have to spell one out: the
    /// vault's migration of documents written before ids existed, and the
    /// tests that read one back. Nothing on screen should care.
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// An id from a raw number, for a fixture.
    ///
    /// Test-only, which is stricter than [`CustomSectionId::from_u32`] and for
    /// the same reason held harder: real ids come from [`Versioned::mint`],
    /// which cannot reissue one, and a constructor reachable from the app is
    /// how two variants of a section come to share an id — the exact failure
    /// this type was added to end.
    #[cfg(test)]
    pub(crate) const fn from_u32(id: u32) -> Self {
        Self(id)
    }
}

/// A single named variant of a section's content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Variant<T> {
    /// Stable for the life of the variant: a rename never touches it, and a
    /// deletion never hands it to somebody else.
    ///
    /// `#[serde(default)]` exists for documents written before ids did, and
    /// `VariantId(0)` is the only value that never belongs to a real variant —
    /// ids are minted from 1. [`crate::resume::parse_document_toml`] fills them
    /// in before an old document is deserialized, so a zero here means a
    /// `ResumeDoc` built straight out of TOML rather than through that boundary;
    /// [`Versioned::mint`] stays correct anyway rather than trusting that.
    #[serde(default)]
    pub id: VariantId,
    pub name: String,
    /// One line on what this cut of the section does — "Shorten setup; keep the
    /// reliability result first."
    ///
    /// The name says which variant, this says why you would pick it. That is
    /// the difference between a version screen that lists decisions and one
    /// that lists consequences, which is the whole point of the 0.4 direction:
    /// a person choosing between two cuts of their Work section should not have
    /// to read both to find out how they differ.
    ///
    /// Absent by default, and never invented — an empty description shows
    /// nothing rather than a sentence nobody wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub data: T,
}

/// A section's variants with one always active. Invariant: `variants` is
/// non-empty, `active` is always in range, and no two variants share an id.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Versioned<T> {
    // `active` and `next_id` (scalars) are declared before `variants` (a table
    // array) so TOML serialization emits them before the `[[...]]` sections.
    pub active: usize,
    /// The id to hand to the *next* variant. Monotonic, never rewound, so a
    /// deleted variant's id is never reissued — `next_custom_section_id`'s
    /// rule, kept here for `Preset::selection`'s benefit: a reissued id would
    /// silently re-point a preset at content it never selected, which is the
    /// bug ids were added to end.
    #[serde(default)]
    pub next_id: u32,
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
                id: VariantId(1),
                name: name.into(),
                description: None,
                data,
            }],
            active: 0,
            next_id: 2,
        }
    }

    /// Hand out the next id.
    ///
    /// Takes the counter *or* one past the highest id present, whichever is
    /// larger, rather than trusting the counter alone: a document hand-edited
    /// in a text editor, or built in a test straight from TOML, can arrive with
    /// a counter behind its own variants, and a reissued id is precisely the
    /// failure ids exist to prevent.
    fn mint(&mut self) -> VariantId {
        let floor = self.variants.iter().map(|v| v.id.0).max().unwrap_or(0) + 1;
        let id = VariantId(self.next_id.max(floor));
        self.next_id = id.0 + 1;
        id
    }

    /// Every variant's id, in the order the variants are stored — the index
    /// side of the pair, for callers that already work in indices.
    pub fn ids(&self) -> Vec<VariantId> {
        self.variants.iter().map(|v| v.id).collect()
    }

    /// Where `id` sits, or `None` when nothing in this section carries it —
    /// which is a preset pinning a variant that has since been deleted, and is
    /// reported rather than guessed at (see
    /// [`crate::resume::model::ResumeDoc::unresolved_pins`]).
    pub fn index_of_id(&self, id: VariantId) -> Option<usize> {
        self.variants.iter().position(|v| v.id == id)
    }

    pub fn active_id(&self) -> VariantId {
        self.variants[self.active].id
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

    /// Each variant's description, positionally beside [`Self::names`].
    pub fn descriptions(&self) -> Vec<Option<String>> {
        self.variants.iter().map(|v| v.description.clone()).collect()
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.variants.len() {
            self.active = index;
        }
    }

    /// Duplicate the active variant and switch to the copy.
    ///
    /// The copy is a new variant, so it takes a new id: presets that pinned the
    /// original keep pointing at the original, which is the only reading of
    /// "duplicate" that does not quietly edit other presets.
    pub fn duplicate_active(&mut self) {
        let mut copy = self.variants[self.active].clone();
        copy.id = self.mint();
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
/// Identifies a section for variant operations (switch/add/remove/rename).
///
/// `Custom(CustomSectionId)` is the *only* extension point for user-added
/// sections (D-9, the roadmap) — deliberately one new variant rather
/// than an open enum. `SectionKind` is `Copy + Hash`, is a `HashMap` key
/// (`views/preset_matrix.rs`), and is embedded in `FieldId` (also `Copy`), so
/// a `String` cannot live in it; the id is `Copy` for the same reason, and
/// the section's own user-editable title lives on [`crate::resume::model::CustomSection`], never
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
    /// The leaner variant that already exists — its id, because that is what
    /// switching to it needs.
    pub id: VariantId,
    /// Its name, because that is what the chip says. Carrying both is not one
    /// fact in two places: a candidate is recomputed from the document every
    /// time it is asked for and never stored.
    pub variant: String,
    /// Characters of printed text switching to it would remove.
    pub saved_chars: usize,
}

/// A named, document-wide reading: a chosen variant (by [`VariantId`]) for each
/// section, plus visibility, order, and printed headings.
/// Applying it restores all four dimensions in one click — e.g. a "GE Vernova"
/// preset can pick tailored Profile and Work variants, lead with Skills under
/// "Engineering", and leave Education on its shared variant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    /// The version this one was made from, by name.
    ///
    /// **A label, not a live reference** — the same rule `ExportRecord::preset`
    /// and `SentCv::preset` follow, and for the same reason: it records what
    /// happened. A version made from `Infra-heavy` was made from `Infra-heavy`
    /// even after that one is renamed or deleted, and repointing it later would
    /// be rewriting history to keep a link alive.
    ///
    /// This is *not* "the version tailoring starts from by default", which is a
    /// changing fact about the whole vault and is derived, never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
    /// What this version is for — "platform, reliability, distributed systems".
    ///
    /// The row on the front door is a choice between readings of one person,
    /// and the names alone ("Base", "Infra-heavy") do not say which to send.
    /// Absent by default; an empty one draws nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// One pin per section the document has — every one of them, which
    /// [`crate::resume::model::ResumeDoc::reconcile_presets`] is what keeps true. A preset silent
    /// about a section is not a reading of the document, it is an instruction
    /// whose result depends on what the user happened to be looking at last.
    ///
    /// Pinned by id, never by name: see [`VariantId`]. On disk this reads
    /// `[["Work", 2]]`, and the name that goes with id 2 is a few lines up in
    /// the same file, beside the variant that owns it.
    #[serde(default)]
    pub selection: Vec<(SectionKind, VariantId)>,
    /// Sections this preset leaves out of the document entirely.
    ///
    /// Visibility is part of what a preset *selects*, not a document-level
    /// flag (O-13, confirmed against the design row: the same Certificates
    /// section reads `— hidden —` under one preset and `Base · shown` under
    /// another). A preset that hides nothing writes no `hidden` line at all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<SectionKind>,
    /// The order this reading prints its sections in.
    ///
    /// Empty means the document's standard order, exactly as an empty
    /// [`crate::resume::model::ResumeDoc::section_order`] does. Keeping the
    /// sentinel makes pre-C8 presets compatible and keeps the common case out
    /// of the hand-editable TOML.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<SectionKind>,
    /// Heading overrides for this reading.
    ///
    /// Missing headings use the document's shipped defaults. These are values
    /// a preset may pin because there is no second copy to drift: applying the
    /// preset replaces the working copy's override table wholesale.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub titles: Vec<(SectionKind, String)>,
}

/// Pin `section` to `variant` in this preset, replacing any existing pin.
///
/// A preset holds selections and never content, so this is the only kind of
/// edit it has — and the Preset Matrix screen is where a user makes it.
impl Preset {
    pub fn set(&mut self, section: SectionKind, variant: VariantId) {
        match self.selection.iter_mut().find(|(s, _)| *s == section) {
            Some((_, existing)) => *existing = variant,
            None => self.selection.push((section, variant)),
        }
    }

    /// The variant this preset pins for `section`, if it pins one at all.
    ///
    /// `None` means the preset does not name the section — which after
    /// [`crate::resume::model::ResumeDoc::reconcile_presets`] only happens for a section that is not
    /// in the document either. A pin naming a variant that has been *deleted*
    /// is a different answer and a visible one: see
    /// [`crate::resume::model::ResumeDoc::unresolved_pins`].
    pub fn variant_for(&self, section: SectionKind) -> Option<VariantId> {
        self.selection
            .iter()
            .find(|(s, _)| *s == section)
            .map(|(_, v)| *v)
    }
}

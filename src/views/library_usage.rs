//! Where each library block is actually used, derived from the vault.
//!
//! The mockup's block card carries `used in 3 CVs`, and the review calls that
//! line out by name as one of the things worth keeping (L-03): *«сразу видно,
//! какие формулировки заслужили место, а какие мертвы»*. It was never built,
//! because nothing in the model records it — and the honest reason not to fake
//! it is US-14: never put a number about the user's own corpus on screen unless
//! it traces to something real.
//!
//! So it is **derived, not stored**. Every document in the vault is already
//! parsed and cached; a block is "used in" a document when that document
//! contains an entry matching it. Nothing is written, nothing can drift, and a
//! block edited in one place stops matching the moment it genuinely differs.
//!
//! ## The matching rule, and why it is exact
//!
//! A block matches on the fields that **identify** it, never on the prose that
//! describes it:
//!
//! | Section | Identity |
//! |---|---|
//! | Work | employer + position + start date |
//! | Education | institution + qualification |
//! | Skills | group name, or the keyword set when the group is unnamed |
//! | Certificates | name + issuer |
//! | Organizations | organization + position |
//!
//! Highlights and summaries are deliberately excluded. Tailoring a bullet for
//! one company is the entire point of this product; if the count dropped every
//! time a bullet was reworded, it would measure *editing*, not *reuse*, and it
//! would read as broken to the one user who is doing the right thing.
//!
//! Matching is case- and whitespace-insensitive, because "Acme Corp" typed
//! twice is one employer.
//!
//! ## What this deliberately is not
//!
//! It is **not** a link. A library block stays a copy pool (`CLAUDE.md`, data
//! model invariants), and this reports on copies after the fact rather than
//! binding them. The `Linked`/`Detached` status the design row draws is a
//! stored field with a migration and a push-to-all flow (US-03, roadmap D2) —
//! a separate decision about semantics, not something to slip in behind a
//! usage count. What this does give that work is its missing half: you cannot
//! show a blast radius without first knowing the radius.

use std::collections::BTreeMap;

use crate::resume::model::{
    Certificate, Education, Library, ResumeDoc, SectionKind, SkillGroup, Volunteer, Work,
};
use crate::vault::DocMeta;

/// The identity of one block, reduced to a comparable key.
///
/// A plain `String` rather than a struct per section: it is only ever compared
/// for equality and used as a map key, and one shape keeps the five section
/// arms from each needing their own container.
type Identity = String;

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Join identity parts with a separator no field can contain, so
/// `("ab", "c")` and `("a", "bc")` never collide.
fn key(parts: &[&str]) -> Identity {
    parts.iter().map(|p| norm(p)).collect::<Vec<_>>().join("\u{1f}")
}

fn work_key(w: &Work) -> Identity {
    key(&[&w.name, &w.position, &w.start_date.text])
}

fn education_key(e: &Education) -> Identity {
    key(&[&e.institution, &e.study_type])
}

fn skills_key(s: &SkillGroup) -> Identity {
    if s.name.trim().is_empty() {
        // An unnamed group has no name to be identified by — LinkedIn imports
        // produce exactly one of these per person. Its keywords are its
        // identity, sorted so the same set in a different order is the same
        // group.
        let mut keywords: Vec<String> = s.keywords.iter().map(|k| norm(k)).collect();
        keywords.sort();
        format!("\u{1e}{}", keywords.join("\u{1f}"))
    } else {
        key(&[&s.name])
    }
}

fn certificate_key(c: &Certificate) -> Identity {
    key(&[&c.name, &c.issuer])
}

fn volunteer_key(v: &Volunteer) -> Identity {
    key(&[&v.organization, &v.position])
}

/// Every identity a document contains, for one section.
///
/// Reads **every variant**, not just the active one: a block tailored into a
/// variant that is not currently selected is still in use by that document, and
/// counting only the active variant would make the number swing every time a
/// preset was applied.
fn identities_in(doc: &ResumeDoc, section: SectionKind) -> Vec<Identity> {
    match section {
        SectionKind::Work => doc
            .work
            .variants
            .iter()
            .flat_map(|v| v.data.iter().map(work_key))
            .collect(),
        SectionKind::Education => doc
            .education
            .variants
            .iter()
            .flat_map(|v| v.data.iter().map(education_key))
            .collect(),
        SectionKind::Skills => doc
            .skills
            .variants
            .iter()
            .flat_map(|v| v.data.iter().map(skills_key))
            .collect(),
        SectionKind::Certificates => doc
            .certificates
            .variants
            .iter()
            .flat_map(|v| v.data.iter().map(certificate_key))
            .collect(),
        SectionKind::Organizations => doc
            .volunteer
            .variants
            .iter()
            .flat_map(|v| v.data.iter().map(volunteer_key))
            .collect(),
        // No pool for these, so nothing to match.
        SectionKind::Profile | SectionKind::Custom(_) => Vec::new(),
    }
}

/// The identity of the block at `index` in `library`'s pool for `section`.
pub(super) fn block_identity(
    library: &Library,
    section: SectionKind,
    index: usize,
) -> Option<Identity> {
    match section {
        SectionKind::Work => library.work.get(index).map(work_key),
        SectionKind::Education => library.education.get(index).map(education_key),
        SectionKind::Skills => library.skills.get(index).map(skills_key),
        SectionKind::Certificates => library.certificates.get(index).map(certificate_key),
        SectionKind::Organizations => library.volunteer.get(index).map(volunteer_key),
        SectionKind::Profile | SectionKind::Custom(_) => None,
    }
}

/// Which documents use which blocks, for the whole vault.
///
/// Built once per render from the cache, then asked about individual blocks —
/// the alternative is re-walking every document per card, which is the shape
/// of per-frame work this codebase has already been bitten by once.
#[derive(Default)]
pub(super) struct UsageIndex {
    /// Identity → the display names of the documents containing it, in vault
    /// order, deduplicated.
    by_identity: BTreeMap<Identity, Vec<DocumentRef>>,
}

/// A document a block was found in — enough to name it and to open it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DocumentRef {
    /// The file stem, which is how the gallery and `Application::source_doc`
    /// both identify a document.
    pub stem: String,
    /// What to call it on screen: the person's name when the file has one,
    /// else the stem.
    pub label: String,
}

impl UsageIndex {
    /// Walk the vault once. `docs` is the cache's metadata (for names) paired
    /// with the parsed document; an unreadable document contributes nothing
    /// rather than being counted as empty.
    pub(super) fn build<'a>(docs: impl IntoIterator<Item = (&'a DocMeta, &'a ResumeDoc)>) -> Self {
        let mut by_identity: BTreeMap<Identity, Vec<DocumentRef>> = BTreeMap::new();
        for (meta, doc) in docs {
            let reference = DocumentRef {
                stem: meta.stem.clone(),
                label: if meta.name.trim().is_empty() {
                    meta.stem.clone()
                } else {
                    meta.name.clone()
                },
            };
            for section in [
                SectionKind::Work,
                SectionKind::Education,
                SectionKind::Skills,
                SectionKind::Certificates,
                SectionKind::Organizations,
            ] {
                for identity in identities_in(doc, section) {
                    let entry = by_identity.entry(identity).or_default();
                    // One document counts once for one block however many
                    // variants of it hold a copy — "used in 3 CVs" is about
                    // documents, and a document that tailored the same block
                    // into two variants has not used it twice.
                    if !entry.iter().any(|d| d.stem == reference.stem) {
                        entry.push(reference.clone());
                    }
                }
            }
        }
        Self { by_identity }
    }

    /// The documents using this block. Empty when it has never been placed.
    pub(super) fn documents_for(
        &self,
        library: &Library,
        section: SectionKind,
        index: usize,
    ) -> &[DocumentRef] {
        block_identity(library, section, index)
            .and_then(|id| self.by_identity.get(&id))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// How many documents use this block.
    pub(super) fn count_for(
        &self,
        library: &Library,
        section: SectionKind,
        index: usize,
    ) -> usize {
        self.documents_for(library, section, index).len()
    }

    /// Total placements across the library — the header's "reused N times".
    ///
    /// Sums per block × document, so a block in three CVs counts three times.
    /// That is what "reused" means: the header is measuring how much work the
    /// pool has saved, not how many blocks exist.
    pub(super) fn total_reuses(&self, library: &Library) -> usize {
        let mut total = 0;
        for (section, len) in [
            (SectionKind::Work, library.work.len()),
            (SectionKind::Education, library.education.len()),
            (SectionKind::Skills, library.skills.len()),
            (SectionKind::Certificates, library.certificates.len()),
            (SectionKind::Organizations, library.volunteer.len()),
        ] {
            for index in 0..len {
                total += self.count_for(library, section, index);
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Resume, ResumeDoc};

    fn meta(stem: &str, name: &str) -> DocMeta {
        DocMeta {
            path: std::path::PathBuf::from(format!("{stem}.toml")),
            stem: stem.into(),
            name: name.into(),
            label: String::new(),
            variants: 0,
            presets: 0,
            preset_names: Vec::new(),
            varied_sections: 0,
            unreadable: false,
            modified_secs: None,
        }
    }

    fn work(employer: &str, position: &str, start: &str, highlight: &str) -> Work {
        Work {
            name: employer.into(),
            position: position.into(),
            start_date: start.into(),
            highlights: vec![highlight.into()],
            ..Default::default()
        }
    }

    fn doc_with(work_entries: Vec<Work>) -> ResumeDoc {
        let resume = Resume {
            work: work_entries,
            ..Default::default()
        };
        ResumeDoc::from_resume(resume, "Base")
    }

    /// The number on the card is the number of documents holding a copy.
    #[test]
    fn a_block_is_counted_once_per_document_that_holds_it() {
        let block = work("Acme Corp", "Senior SWE", "2022-01", "Did the thing");
        let library = Library {
            work: vec![block.clone()],
            ..Default::default()
        };

        let a = doc_with(vec![block.clone()]);
        let b = doc_with(vec![block.clone()]);
        let c = doc_with(vec![work("Other Co", "SWE", "2019-01", "Something else")]);

        let index = UsageIndex::build(vec![
            (&meta("cv-a", "Sofiia"), &a),
            (&meta("cv-b", "Sofiia"), &b),
            (&meta("cv-c", "Sofiia"), &c),
        ]);

        assert_eq!(index.count_for(&library, SectionKind::Work, 0), 2);
        let stems: Vec<&str> = index
            .documents_for(&library, SectionKind::Work, 0)
            .iter()
            .map(|d| d.stem.as_str())
            .collect();
        assert_eq!(stems, vec!["cv-a", "cv-b"]);
    }

    /// The rule that makes the number worth trusting: rewording a bullet for
    /// one company must not drop the block out of the count. Tailoring is the
    /// product; a counter that punished it would measure editing, not reuse.
    #[test]
    fn tailoring_a_bullet_does_not_break_the_match() {
        let block = work("Acme Corp", "Senior SWE", "2022-01", "Cut p99 in half");
        let library = Library {
            work: vec![block.clone()],
            ..Default::default()
        };

        let mut tailored = block.clone();
        tailored.highlights = vec!["Halved p99 latency on the orders service".into()];
        tailored.summary = "Rewritten for this employer".into();
        let doc = doc_with(vec![tailored]);

        let index = UsageIndex::build(vec![(&meta("cv", "Sofiia"), &doc)]);
        assert_eq!(index.count_for(&library, SectionKind::Work, 0), 1);
    }

    /// …but a genuinely different job is a different block.
    #[test]
    fn a_different_role_at_the_same_employer_is_not_the_same_block() {
        let library = Library {
            work: vec![work("Acme Corp", "Senior SWE", "2022-01", "x")],
            ..Default::default()
        };
        let doc = doc_with(vec![work("Acme Corp", "Staff SWE", "2022-01", "x")]);

        let index = UsageIndex::build(vec![(&meta("cv", "Sofiia"), &doc)]);
        assert_eq!(index.count_for(&library, SectionKind::Work, 0), 0);
    }

    /// A block placed in a variant that is not currently selected is still in
    /// use — otherwise the count would swing every time a preset was applied.
    #[test]
    fn a_block_in_an_inactive_variant_still_counts() {
        let block = work("Acme Corp", "Senior SWE", "2022-01", "x");
        let library = Library {
            work: vec![block.clone()],
            ..Default::default()
        };

        // Base variant is empty; the second variant holds the block, and the
        // document is left on the first.
        let mut doc = doc_with(Vec::new());
        doc.add_variant(SectionKind::Work);
        doc.work.variants[1].data = vec![block];
        doc.set_active_variant(SectionKind::Work, 0);

        let index = UsageIndex::build(vec![(&meta("cv", "Sofiia"), &doc)]);
        assert_eq!(index.count_for(&library, SectionKind::Work, 0), 1);
    }

    /// Case and stray whitespace are not a difference of employer.
    #[test]
    fn matching_ignores_case_and_padding() {
        let library = Library {
            work: vec![work("Acme Corp", "Senior SWE", "2022-01", "x")],
            ..Default::default()
        };
        let doc = doc_with(vec![work("  acme corp ", "SENIOR SWE", "2022-01", "y")]);

        let index = UsageIndex::build(vec![(&meta("cv", "Sofiia"), &doc)]);
        assert_eq!(index.count_for(&library, SectionKind::Work, 0), 1);
    }

    /// An unnamed skill group — every LinkedIn import produces one — is
    /// identified by its keywords, in any order.
    #[test]
    fn an_unnamed_skill_group_is_identified_by_its_keywords() {
        let group = |keywords: &[&str]| SkillGroup {
            name: String::new(),
            keywords: keywords.iter().map(|k| k.to_string()).collect(),
        };
        let library = Library {
            skills: vec![group(&["Rust", "Typst", "GPUI"])],
            ..Default::default()
        };

        let doc = ResumeDoc::from_resume(
            Resume {
                skills: vec![group(&["gpui", "rust", "typst"])],
                ..Default::default()
            },
            "Base",
        );

        let index = UsageIndex::build(vec![(&meta("cv", "Sofiia"), &doc)]);
        assert_eq!(index.count_for(&library, SectionKind::Skills, 0), 1);
    }

    /// "reused N times" counts placements, not blocks: a block in three CVs
    /// has saved three pieces of work.
    #[test]
    fn the_header_total_counts_placements() {
        let one = work("Acme Corp", "Senior SWE", "2022-01", "x");
        let two = work("Liffey Labs", "SWE", "2019-01", "y");
        let library = Library {
            work: vec![one.clone(), two.clone()],
            ..Default::default()
        };

        let a = doc_with(vec![one.clone(), two.clone()]);
        let b = doc_with(vec![one.clone()]);
        let index = UsageIndex::build(vec![(&meta("a", "S"), &a), (&meta("b", "S"), &b)]);

        // `one` in two documents, `two` in one.
        assert_eq!(index.total_reuses(&library), 3);
    }

    /// A block nobody has placed reports zero rather than being absent — the
    /// card has to be able to say "not used yet" out loud.
    #[test]
    fn an_unused_block_reports_zero() {
        let library = Library {
            work: vec![work("Nowhere Ltd", "Intern", "2015-01", "x")],
            ..Default::default()
        };
        let index = UsageIndex::build(Vec::new());
        assert_eq!(index.count_for(&library, SectionKind::Work, 0), 0);
        assert!(index.documents_for(&library, SectionKind::Work, 0).is_empty());
    }
}

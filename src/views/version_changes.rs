//! What you can change about a reading, relative to the version it started from.
//!
//! The 0.4 direction turns the matrix from the primary surface into an audit
//! view, and puts a short list of named changes in its place. This is the model
//! behind that list, and the whole of the answer to "where do those names come
//! from": **they are the variants**. A change is one section reading a
//! different cut than the source version reads, or being left out altogether.
//! Nothing is generated and nothing is proposed by a model — the alternatives
//! are the ones already written, described in their author's own words.
//!
//! Pure, so the rules can be tested without a window.

use crate::resume::model::{ResumeDoc, SectionKind, VariantId};

/// One available change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Change {
    pub section: SectionKind,
    pub kind: ChangeKind,
    /// What the change is called — the variant's own name, or `Hide <section>`.
    pub name: String,
    /// One line on what it does, when the author wrote one.
    pub description: Option<String>,
    /// Whether the working copy currently has it.
    pub applied: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChangeKind {
    /// Read this cut of the section instead of the source's.
    Variant(VariantId),
    /// Leave the section out of the document.
    Hide,
}

/// Every change available against preset `source`, in the document's own
/// section order.
///
/// A section contributes one row per *other* variant it has, plus one for
/// hiding it. A section with a single variant that the source already shows and
/// does not hide contributes nothing, which is what keeps the list short: it is
/// the alternatives that exist, not a decision per section.
///
/// Profile is never hideable — a CV without a name is not a CV (O-13).
pub(super) fn available(doc: &ResumeDoc, source: usize) -> Vec<Change> {
    let Some(preset) = doc.presets.get(source) else {
        return Vec::new();
    };
    let mut changes = Vec::new();

    for section in doc.sections() {
        let from = preset.variant_for(section);
        let hidden_at_source = preset.hidden.contains(&section);
        let ids = doc.variant_ids(section);
        let names = doc.variant_names(section);
        let active = doc.active_variant_id(section);
        let hidden_now = doc.is_hidden(section);

        for (index, id) in ids.iter().enumerate() {
            // The cut the source already reads is not a change to it.
            if Some(*id) == from {
                continue;
            }
            changes.push(Change {
                section,
                kind: ChangeKind::Variant(*id),
                name: names.get(index).cloned().unwrap_or_default(),
                description: doc.variant_description(section, *id),
                // Hiding a section wins over which cut it would have read: a
                // section that is not in the document is not reading anything.
                applied: !hidden_now && active == Some(*id),
            });
        }

        if section != SectionKind::Profile && !hidden_at_source {
            changes.push(Change {
                section,
                kind: ChangeKind::Hide,
                name: format!("Leave out {}", doc.section_title(section)),
                description: None,
                applied: hidden_now,
            });
        }
    }

    changes
}

/// How many of them the working copy currently has.
pub(super) fn applied_count(changes: &[Change]) -> usize {
    changes.iter().filter(|change| change.applied).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Resume, ResumeDoc, SectionKind};

    /// A document whose Work section has two cuts, with a preset on the first.
    fn doc_with_two_work_cuts() -> ResumeDoc {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        doc.add_variant(SectionKind::Work); // "Base copy", now active
        doc.work.variants[1].name = "Lead with outcomes".into();
        doc.work.variants[1].description =
            Some("Shorten setup; keep the reliability result first.".into());
        doc.set_active_variant(SectionKind::Work, 0);
        doc.add_preset("Infra-heavy");
        doc
    }

    /// The alternatives are the variants, named and described by whoever wrote
    /// them. The cut the source already reads is not offered as a change to it.
    #[test]
    fn the_changes_are_the_other_cuts_that_exist() {
        let doc = doc_with_two_work_cuts();
        let changes = available(&doc, 0);

        let work: Vec<&Change> = changes
            .iter()
            .filter(|c| c.section == SectionKind::Work)
            .collect();
        assert_eq!(work.len(), 2, "the other cut, and leaving the section out");

        let other = work
            .iter()
            .find(|c| matches!(c.kind, ChangeKind::Variant(_)))
            .expect("the other cut is offered");
        assert_eq!(other.name, "Lead with outcomes");
        assert_eq!(
            other.description.as_deref(),
            Some("Shorten setup; keep the reliability result first.")
        );
        assert!(!other.applied, "the working copy still reads the source's cut");

        assert!(work.iter().any(|c| c.kind == ChangeKind::Hide));
        assert_eq!(applied_count(&changes), 0);
    }

    /// Switching the working copy to the other cut marks that change applied,
    /// and nothing else.
    #[test]
    fn applying_a_change_is_the_working_copy_reading_the_other_cut() {
        let mut doc = doc_with_two_work_cuts();
        let other = doc.variant_ids(SectionKind::Work)[1];
        doc.set_active_variant_by_id(SectionKind::Work, other);

        let changes = available(&doc, 0);
        assert_eq!(applied_count(&changes), 1);
        assert!(changes
            .iter()
            .any(|c| c.kind == ChangeKind::Variant(other) && c.applied));
    }

    /// Hiding a section is a change like any other, and it wins over which cut
    /// the section would otherwise read — a section that is out of the document
    /// is not reading anything.
    #[test]
    fn hiding_a_section_outranks_the_cut_it_would_have_read() {
        let mut doc = doc_with_two_work_cuts();
        let other = doc.variant_ids(SectionKind::Work)[1];
        doc.set_active_variant_by_id(SectionKind::Work, other);
        doc.hidden_sections.push(SectionKind::Work);

        let changes = available(&doc, 0);
        let work: Vec<&Change> = changes
            .iter()
            .filter(|c| c.section == SectionKind::Work)
            .collect();
        assert!(work.iter().any(|c| c.kind == ChangeKind::Hide && c.applied));
        assert!(
            work.iter()
                .all(|c| !matches!(c.kind, ChangeKind::Variant(_)) || !c.applied),
            "the cut is not 'applied' while the section is out of the document"
        );
    }

    /// Profile has no hide row, and a section with one cut the source already
    /// reads contributes only its hide row — which is what keeps the list the
    /// length of the decisions that actually exist.
    #[test]
    fn a_section_with_nothing_to_offer_contributes_nothing_but_its_hide_row() {
        let doc = doc_with_two_work_cuts();
        let changes = available(&doc, 0);

        assert!(
            !changes
                .iter()
                .any(|c| c.section == SectionKind::Profile),
            "Profile has one cut and cannot be hidden, so it offers nothing"
        );
        let education: Vec<&Change> = changes
            .iter()
            .filter(|c| c.section == SectionKind::Education)
            .collect();
        assert_eq!(education.len(), 1);
        assert_eq!(education[0].kind, ChangeKind::Hide);
    }

    #[test]
    fn a_source_that_does_not_exist_offers_nothing() {
        let doc = doc_with_two_work_cuts();
        assert!(available(&doc, 9).is_empty());
    }
}

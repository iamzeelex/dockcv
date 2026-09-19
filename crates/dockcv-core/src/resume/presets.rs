//! Making one preset out of another.
//!
//! A new file rather than more of `model.rs`, which is 4300 lines against a
//! ~800 limit and owes a split (#17). This is the subject that split will move,
//! so starting it here pre-pays a little of it instead of adding to the debt.

use crate::resume::model::{Preset, ResumeDoc, SectionKind, VariantId};

impl ResumeDoc {
    /// Add a preset that starts out reading exactly like preset `base`.
    ///
    /// The difference from [`ResumeDoc::add_preset`] is where the pins come
    /// from: that one saves the *working copy*, this one copies another
    /// preset's. Tailoring for a job starts from the reading that has been
    /// working, not from whatever the document happens to be open on.
    ///
    /// A copy, not a link. Two presets that read alike today must be free to
    /// diverge tomorrow without one of them quietly editing the other — the
    /// same answer the library gives for blocks, and the reason the matrix can
    /// show a column as total rather than as a cascade to resolve.
    ///
    /// Returns the new preset's index, or `None` when `base` names nothing.
    pub fn add_preset_from(&mut self, base: usize, name: impl Into<String>) -> Option<usize> {
        let source = self.presets.get(base)?;
        let preset = Preset {
            name: name.into(),
            based_on: Some(source.name.clone()),
            description: None,
            // Everything else is the source's reading, taken wholesale rather
            // than field by field. Listing the fields is how this silently
            // falls behind: C8 gave a preset an order and its own headings
            // while this said `selection` and `hidden`, and every version made
            // by tailoring would have quietly lost both.
            ..source.clone()
        };
        self.presets.push(preset);
        Some(self.presets.len() - 1)
    }

    /// What its author said this cut of the section is for.
    ///
    /// `None` both when the variant does not exist and when nobody wrote a
    /// description — the caller shows nothing either way, and inventing a
    /// sentence is the one thing a screen listing consequences must not do.
    pub fn variant_description(&self, section: SectionKind, id: VariantId) -> Option<String> {
        use SectionKind::*;
        let index = self.variant_ids(section).iter().position(|v| *v == id)?;
        let descriptions = match section {
            Profile => self.profile.descriptions(),
            Work => self.work.descriptions(),
            Education => self.education.descriptions(),
            Skills => self.skills.descriptions(),
            Certificates => self.certificates.descriptions(),
            Organizations => self.volunteer.descriptions(),
            Custom(id) => self
                .custom_section(id)
                .map(|s| s.content.descriptions())
                .unwrap_or_default(),
        };
        descriptions.get(index).cloned().flatten()
    }
}

#[cfg(test)]
mod tests {
    use crate::resume::model::{Resume, ResumeDoc, SectionKind};

    #[test]
    fn a_preset_made_from_another_reads_the_same_and_then_diverges() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        doc.add_variant(SectionKind::Work);
        doc.hidden_sections.push(SectionKind::Certificates);
        // A reading with something pinned on every axis, so copying one that
        // happens to be empty cannot pass for copying one.
        doc.section_order = vec![
            SectionKind::Skills,
            SectionKind::Profile,
            SectionKind::Work,
            SectionKind::Education,
            SectionKind::Certificates,
            SectionKind::Organizations,
        ];
        doc.set_section_title(SectionKind::Skills, "Engineering");
        doc.add_preset("Infra-heavy");
        assert!(!doc.presets[0].order.is_empty());
        assert!(!doc.presets[0].titles.is_empty());

        let copy = doc.add_preset_from(0, "Northwind").expect("base exists");
        assert_eq!(copy, 1);
        assert_eq!(doc.presets[1].name, "Northwind");
        // Every dimension of the reading, not the two this once listed by hand.
        assert_eq!(doc.presets[1].order, doc.presets[0].order);
        assert_eq!(doc.presets[1].titles, doc.presets[0].titles);
        assert_eq!(
            doc.presets[1].based_on.as_deref(),
            Some("Infra-heavy"),
            "a version remembers what it was made from"
        );
        assert_eq!(doc.presets[1].selection, doc.presets[0].selection);
        assert_eq!(doc.presets[1].hidden, doc.presets[0].hidden);

        // Both describe the working copy, so the first one in document order
        // takes the active mark — and editing one leaves the other alone.
        assert_eq!(doc.active_preset_index(), Some(0));
        // `add_variant` duplicated Base and switched to the copy, so the copy
        // is what both presets pinned. Point one of them back at the original.
        let original = doc.variant_ids(SectionKind::Work)[0];
        assert_ne!(doc.presets[1].variant_for(SectionKind::Work), Some(original));
        doc.presets[1].set(SectionKind::Work, original);
        assert_ne!(doc.presets[1].selection, doc.presets[0].selection);
    }

    #[test]
    fn a_base_that_does_not_exist_makes_no_preset() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        assert_eq!(doc.add_preset_from(3, "Northwind"), None);
        assert!(doc.presets.is_empty());
    }
}

//! Ephemeral editor navigation. None of this is document content: changing the
//! selected entry or settings category must never dirty the CV on disk.

use crate::resume::model::{ResumeDoc, SectionKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditorMode {
    Content,
    Layout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LayoutCategory {
    Typography,
    Page,
    Header,
    Headings,
    Entries,
    Skills,
    Export,
}

impl LayoutCategory {
    pub(super) const ALL: [Self; 7] = [
        Self::Typography,
        Self::Page,
        Self::Header,
        Self::Headings,
        Self::Entries,
        Self::Skills,
        Self::Export,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Typography => "Typography",
            Self::Page => "Page",
            Self::Header => "Header",
            Self::Headings => "Headings",
            Self::Entries => "Entries",
            Self::Skills => "Skills",
            Self::Export => "Export",
        }
    }
}

/// `None` means the profile identity or an empty repeatable section. Every
/// other selected entity is addressed by its active variant's current index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EditorSelection {
    pub(super) section: SectionKind,
    pub(super) item: Option<usize>,
}

impl EditorSelection {
    pub(super) fn initial() -> Self {
        Self {
            section: SectionKind::Profile,
            item: None,
        }
    }

    pub(super) fn for_section(doc: &ResumeDoc, section: SectionKind) -> Self {
        Self {
            section,
            item: (section != SectionKind::Profile && item_count(doc, section) > 0).then_some(0),
        }
    }

    pub(super) fn normalize(&mut self, doc: &ResumeDoc) {
        if !doc.sections().contains(&self.section) {
            *self = Self::initial();
            return;
        }
        let count = item_count(doc, self.section);
        self.item = match (self.section, self.item, count) {
            (SectionKind::Profile, None, _) | (_, None, 0) => None,
            (SectionKind::Profile, Some(index), n) if n > 0 => Some(index.min(n - 1)),
            (_, _, 0) => None,
            (_, Some(index), n) => Some(index.min(n - 1)),
            (_, None, _) => Some(0),
        };
    }
}

pub(super) fn item_count(doc: &ResumeDoc, section: SectionKind) -> usize {
    match section {
        SectionKind::Profile => doc.profile.active().profiles.len(),
        SectionKind::Work => doc.work.active().len(),
        SectionKind::Education => doc.education.active().len(),
        SectionKind::Skills => doc.skills.active().len(),
        SectionKind::Certificates => doc.certificates.active().len(),
        SectionKind::Organizations => doc.volunteer.active().len(),
        SectionKind::Custom(id) => doc
            .custom_section(id)
            .map(|section| section.content.active().len())
            .unwrap_or(0),
    }
}

/// What the editor's chrome calls a section.
///
/// `ResumeDoc::section_title` answers with the empty string for a custom
/// section nobody has named, which is the right answer for the printed page
/// and the wrong one for a row you have to click. The fallback used to live in
/// `render_custom_section`; it belongs here now that the navigator and the
/// inspector header both ask.
pub(super) fn section_label(doc: &ResumeDoc, section: SectionKind) -> String {
    let title = doc.section_title(section);
    if title.trim().is_empty() {
        "Untitled section".to_string()
    } else {
        title
    }
}

pub(super) fn item_label(doc: &ResumeDoc, section: SectionKind, index: usize) -> String {
    let (primary, fallback) = match section {
        SectionKind::Profile => (
            doc.profile
                .active()
                .profiles
                .get(index)
                .map(|v| v.network.as_str()),
            "Profile",
        ),
        SectionKind::Work => (
            doc.work.active().get(index).map(|v| v.position.as_str()),
            "Role",
        ),
        SectionKind::Education => (
            doc.education
                .active()
                .get(index)
                .map(|v| v.study_type.as_str()),
            "Education",
        ),
        SectionKind::Skills => (
            doc.skills.active().get(index).map(|v| v.name.as_str()),
            "Group",
        ),
        SectionKind::Certificates => (
            doc.certificates
                .active()
                .get(index)
                .map(|v| v.name.as_str()),
            "Certificate",
        ),
        SectionKind::Organizations => (
            doc.volunteer
                .active()
                .get(index)
                .map(|v| v.position.as_str()),
            "Organization",
        ),
        SectionKind::Custom(id) => (
            doc.custom_section(id)
                .and_then(|s| s.content.active().get(index))
                .map(|v| v.title.as_str()),
            "Entry",
        ),
    };
    primary
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{fallback} {}", index + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::edit::ListId;

    #[test]
    fn selection_clamps_after_delete_and_variant_change() {
        let mut doc = ResumeDoc::default();
        ListId::Work.add(&mut doc);
        ListId::Work.add(&mut doc);
        let mut selection = EditorSelection {
            section: SectionKind::Work,
            item: Some(1),
        };
        ListId::Work.remove(&mut doc, 1);
        selection.normalize(&doc);
        assert_eq!(selection.item, Some(0));
        ListId::Work.remove(&mut doc, 0);
        selection.normalize(&doc);
        assert_eq!(selection.item, None);
    }

    #[test]
    fn profile_identity_is_not_forced_to_first_network() {
        let mut doc = ResumeDoc::default();
        ListId::Profiles.add(&mut doc);
        let mut selection = EditorSelection::initial();
        selection.normalize(&doc);
        assert_eq!(selection.item, None);
    }

    /// Entering a section lands on something to edit — except Profile, whose
    /// first surface is the identity rather than a network row, and except a
    /// section with nothing in it yet.
    #[test]
    fn entering_a_section_selects_its_first_entry() {
        let mut doc = ResumeDoc::default();
        assert_eq!(
            EditorSelection::for_section(&doc, SectionKind::Work).item,
            None
        );
        ListId::Work.add(&mut doc);
        assert_eq!(
            EditorSelection::for_section(&doc, SectionKind::Work).item,
            Some(0)
        );
        ListId::Profiles.add(&mut doc);
        assert_eq!(
            EditorSelection::for_section(&doc, SectionKind::Profile).item,
            None
        );
    }

    /// The case the index model can actually get wrong: a variant holds its own
    /// list, so switching to a shorter one leaves the index pointing past the
    /// end. Selecting the third role and switching to a variant with one is the
    /// shape of it.
    #[test]
    fn switching_to_a_shorter_variant_clamps_the_index() {
        let mut doc = ResumeDoc::default();
        for _ in 0..3 {
            ListId::Work.add(&mut doc);
        }
        let mut selection = EditorSelection {
            section: SectionKind::Work,
            item: Some(2),
        };
        selection.normalize(&doc);
        assert_eq!(selection.item, Some(2));

        doc.add_variant(SectionKind::Work);
        let shorter = doc.work.variants.len() - 1;
        doc.set_active_variant(SectionKind::Work, shorter);
        while doc.work.active().len() > 1 {
            let last = doc.work.active().len() - 1;
            ListId::Work.remove(&mut doc, last);
        }
        selection.normalize(&doc);
        assert_eq!(selection.item, Some(0));
    }

    /// Deleting the section you were editing has to leave the editor somewhere
    /// real, not on an id the document no longer has.
    #[test]
    fn deleting_the_selected_custom_section_falls_back_to_profile() {
        let mut doc = ResumeDoc::default();
        let id = doc.add_custom_section("Talks");
        let mut selection = EditorSelection::for_section(&doc, SectionKind::Custom(id));
        assert_eq!(selection.section, SectionKind::Custom(id));

        doc.remove_custom_section(id);
        selection.normalize(&doc);
        assert_eq!(selection, EditorSelection::initial());
    }

    /// A row you have to click needs a name. `section_title` answers "" for an
    /// unnamed custom section, which is right on the page and unusable here.
    #[test]
    fn an_unnamed_section_and_an_unnamed_entry_still_read_as_something() {
        let mut doc = ResumeDoc::default();
        let id = doc.add_custom_section("   ");
        assert_eq!(
            section_label(&doc, SectionKind::Custom(id)),
            "Untitled section"
        );
        assert_eq!(section_label(&doc, SectionKind::Work), "Work Experience");

        ListId::Work.add(&mut doc);
        assert_eq!(item_label(&doc, SectionKind::Work, 0), "New role");
        doc.work.active_mut()[0].position.clear();
        assert_eq!(item_label(&doc, SectionKind::Work, 0), "Role 1");
    }

    /// Every category is offered, and no two of them say the same thing —
    /// splitting `Sections` into three was the point of the list.
    #[test]
    fn every_layout_category_is_listed_once_under_its_own_name() {
        let mut labels: Vec<&str> = LayoutCategory::ALL.iter().map(|c| c.label()).collect();
        let total = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total);
    }
}

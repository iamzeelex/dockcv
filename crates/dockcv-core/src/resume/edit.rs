//! Field and list addressing for the form editor.
//!
//! Every editable piece is a `String` somewhere in the model; [`FieldId`] names
//! one of them. Since each section is versioned, addressing resolves against
//! the **active variant** of the relevant section in a [`ResumeDoc`] — so
//! editing only ever changes the variant currently selected on that section's
//! timeline. [`ListId`] does the same for repeatable collections.

use crate::resume::model::{
    Certificate, CustomEntry, CustomSectionId, Education, NetworkProfile, ResumeDoc, SectionKind,
    SkillGroup, Volunteer, Work,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldId {
    // basics (Profile section)
    Name,
    Label,
    Summary,
    Email,
    Phone,
    Location,
    Url,
    ProfileNetwork(usize),
    ProfileUsername(usize),
    ProfileUrl(usize),
    // work[i]
    WorkName(usize),
    WorkPosition(usize),
    WorkLocation(usize),
    WorkStart(usize),
    WorkEnd(usize),
    WorkSummary(usize),
    WorkHighlight(usize, usize),
    // education[i]
    EduInstitution(usize),
    EduStudyType(usize),
    EduStart(usize),
    EduEnd(usize),
    EduUrl(usize),
    /// Coursework, thesis, honours. Added when `Education::highlights` was —
    /// without it the field existed in the model and on the page and could not
    /// be reached in the editor, so an imported line was visible in the PDF and
    /// impossible to find or delete.
    EduHighlight(usize, usize),
    // skills[i]
    SkillName(usize),
    SkillKeyword(usize, usize),
    // certificates[i]
    CertName(usize),
    CertIssuer(usize),
    CertDate(usize),
    CertUrl(usize),
    // volunteer[i]
    VolOrg(usize),
    VolPosition(usize),
    VolStart(usize),
    VolEnd(usize),
    VolHighlight(usize, usize),
    // custom_sections (D-9): the section's own title, plus its entries.
    CustomSectionTitle(CustomSectionId),
    CustomEntryTitle(CustomSectionId, usize),
    CustomEntrySubtitle(CustomSectionId, usize),
    CustomEntryStart(CustomSectionId, usize),
    CustomEntryEnd(CustomSectionId, usize),
    CustomEntryUrl(CustomSectionId, usize),
    CustomEntryHighlight(CustomSectionId, usize, usize),
    /// The name of the active variant of a section (edited from its timeline).
    VariantName(SectionKind),
    /// The name of a document preset (edited from the global timeline).
    PresetName(usize),
}

impl FieldId {
    /// Multi-line fields accept `Enter` as a newline; single-line fields ignore
    /// it.
    pub fn multiline(&self) -> bool {
        use FieldId::*;
        matches!(
            self,
            Summary
                | WorkSummary(_)
                | WorkHighlight(_, _)
                | EduHighlight(_, _)
                | VolHighlight(_, _)
                | CustomEntryHighlight(_, _, _)
        )
    }

    /// Which section's card a field lives in, for keyboard navigation between
    /// the fields of the currently focused section (`views/root.rs`'s
    /// `FocusNextField`/`FocusPrevField`). `None` for [`FieldId::PresetName`],
    /// which is document-wide and isn't rendered inside any section card, and
    /// for [`FieldId::VariantName`] (editor-comfort.md C-2): it lives inside a
    /// section card, but is only ever edited through the active chip's own
    /// pen (`root_section_variants.rs::start_variant_rename`), never through
    /// the section's linear Tab flow — leaving it `Some(section)` would make
    /// `FocusNextField` land on a `TextFieldState` no view mounts, since the
    /// generic per-field row for it is gone.
    pub fn section(&self) -> Option<SectionKind> {
        use FieldId::*;
        Some(match *self {
            Name | Label | Summary | Email | Phone | Location | Url | ProfileNetwork(_)
            | ProfileUsername(_) | ProfileUrl(_) => SectionKind::Profile,
            WorkName(_)
            | WorkPosition(_)
            | WorkLocation(_)
            | WorkStart(_)
            | WorkEnd(_)
            | WorkSummary(_)
            | WorkHighlight(_, _) => SectionKind::Work,
            EduInstitution(_)
            | EduStudyType(_)
            | EduStart(_)
            | EduEnd(_)
            | EduUrl(_)
            | EduHighlight(_, _) => {
                SectionKind::Education
            }
            SkillName(_) | SkillKeyword(_, _) => SectionKind::Skills,
            CertName(_) | CertIssuer(_) | CertDate(_) | CertUrl(_) => SectionKind::Certificates,
            VolOrg(_) | VolPosition(_) | VolStart(_) | VolEnd(_) | VolHighlight(_, _) => {
                SectionKind::Organizations
            }
            CustomSectionTitle(id)
            | CustomEntryTitle(id, _)
            | CustomEntrySubtitle(id, _)
            | CustomEntryStart(id, _)
            | CustomEntryEnd(id, _)
            | CustomEntryUrl(id, _)
            | CustomEntryHighlight(id, _, _) => SectionKind::Custom(id),
            VariantName(_) => return None,
            PresetName(_) => return None,
        })
    }

    /// Every field the document currently addresses, in form order.
    ///
    /// Lives here, beside [`FieldId::get`], so it cannot drift from the
    /// addressing it enumerates. The editor uses it to keep one live text-input
    /// state per field; a field that disappears from this list has its state
    /// dropped.
    pub fn addressable(doc: &ResumeDoc) -> Vec<FieldId> {
        use FieldId::*;
        let mut out = vec![Name, Label, Summary, Email, Phone, Location, Url];

        for i in 0..doc.profile.active().profiles.len() {
            out.extend([ProfileNetwork(i), ProfileUsername(i), ProfileUrl(i)]);
        }
        for (i, work) in doc.work.active().iter().enumerate() {
            out.extend([
                WorkPosition(i),
                WorkName(i),
                WorkLocation(i),
                WorkStart(i),
                WorkEnd(i),
                WorkSummary(i),
            ]);
            out.extend((0..work.highlights.len()).map(|j| WorkHighlight(i, j)));
        }
        for i in 0..doc.education.active().len() {
            out.extend([
                EduStudyType(i),
                EduInstitution(i),
                EduStart(i),
                EduEnd(i),
                EduUrl(i),
            ]);
            if let Some(e) = doc.education.active().get(i) {
                out.extend((0..e.highlights.len()).map(|j| EduHighlight(i, j)));
            }
        }
        for (i, group) in doc.skills.active().iter().enumerate() {
            out.push(SkillName(i));
            out.extend((0..group.keywords.len()).map(|j| SkillKeyword(i, j)));
        }
        for i in 0..doc.certificates.active().len() {
            out.extend([CertName(i), CertIssuer(i), CertDate(i), CertUrl(i)]);
        }
        for (i, entry) in doc.volunteer.active().iter().enumerate() {
            out.extend([VolPosition(i), VolOrg(i), VolStart(i), VolEnd(i)]);
            out.extend((0..entry.highlights.len()).map(|j| VolHighlight(i, j)));
        }
        for section in &doc.custom_sections {
            out.push(CustomSectionTitle(section.id));
            for (i, entry) in section.content.active().iter().enumerate() {
                out.extend([
                    CustomEntryTitle(section.id, i),
                    CustomEntrySubtitle(section.id, i),
                    CustomEntryStart(section.id, i),
                    CustomEntryEnd(section.id, i),
                    CustomEntryUrl(section.id, i),
                ]);
                out.extend(
                    (0..entry.highlights.len()).map(|j| CustomEntryHighlight(section.id, i, j)),
                );
            }
        }

        out.extend(ResumeDoc::SECTIONS.iter().map(|&s| VariantName(s)));
        out.extend(
            doc.custom_sections
                .iter()
                .map(|s| VariantName(SectionKind::Custom(s.id))),
        );
        out.extend((0..doc.presets.len()).map(PresetName));
        out
    }

    pub fn get<'a>(&self, doc: &'a ResumeDoc) -> Option<&'a String> {
        use FieldId::*;
        let basics = doc.profile.active();
        Some(match *self {
            Name => &basics.name,
            Label => &basics.label,
            Summary => &basics.summary,
            Email => &basics.email,
            Phone => &basics.phone,
            Location => &basics.location,
            Url => &basics.url,
            ProfileNetwork(i) => &basics.profiles.get(i)?.network,
            ProfileUsername(i) => &basics.profiles.get(i)?.username,
            ProfileUrl(i) => &basics.profiles.get(i)?.url,
            WorkName(i) => &doc.work.active().get(i)?.name,
            WorkPosition(i) => &doc.work.active().get(i)?.position,
            WorkLocation(i) => &doc.work.active().get(i)?.location,
            WorkStart(i) => &doc.work.active().get(i)?.start_date.text,
            WorkEnd(i) => &doc.work.active().get(i)?.end_date.text,
            WorkSummary(i) => &doc.work.active().get(i)?.summary,
            WorkHighlight(i, j) => doc.work.active().get(i)?.highlights.get(j)?,
            EduInstitution(i) => &doc.education.active().get(i)?.institution,
            EduStudyType(i) => &doc.education.active().get(i)?.study_type,
            EduStart(i) => &doc.education.active().get(i)?.start_date.text,
            EduEnd(i) => &doc.education.active().get(i)?.end_date.text,
            EduUrl(i) => &doc.education.active().get(i)?.url,
            EduHighlight(i, j) => doc.education.active().get(i)?.highlights.get(j)?,
            SkillName(i) => &doc.skills.active().get(i)?.name,
            SkillKeyword(i, j) => doc.skills.active().get(i)?.keywords.get(j)?,
            CertName(i) => &doc.certificates.active().get(i)?.name,
            CertIssuer(i) => &doc.certificates.active().get(i)?.issuer,
            CertDate(i) => &doc.certificates.active().get(i)?.date.text,
            CertUrl(i) => &doc.certificates.active().get(i)?.url,
            VolOrg(i) => &doc.volunteer.active().get(i)?.organization,
            VolPosition(i) => &doc.volunteer.active().get(i)?.position,
            VolStart(i) => &doc.volunteer.active().get(i)?.start_date.text,
            VolEnd(i) => &doc.volunteer.active().get(i)?.end_date.text,
            VolHighlight(i, j) => doc.volunteer.active().get(i)?.highlights.get(j)?,
            CustomSectionTitle(id) => &doc.custom_section(id)?.title,
            CustomEntryTitle(id, i) => &doc.custom_section(id)?.content.active().get(i)?.title,
            CustomEntrySubtitle(id, i) => {
                &doc.custom_section(id)?.content.active().get(i)?.subtitle
            }
            CustomEntryStart(id, i) => &doc.custom_section(id)?.content.active().get(i)?.start_date.text,
            CustomEntryEnd(id, i) => &doc.custom_section(id)?.content.active().get(i)?.end_date.text,
            CustomEntryUrl(id, i) => &doc.custom_section(id)?.content.active().get(i)?.url,
            CustomEntryHighlight(id, i, j) => doc
                .custom_section(id)?
                .content
                .active()
                .get(i)?
                .highlights
                .get(j)?,
            VariantName(section) => return Some(doc.variant_name(section)),
            PresetName(i) => return doc.preset_name(i),
        })
    }

    pub fn get_mut<'a>(&self, doc: &'a mut ResumeDoc) -> Option<&'a mut String> {
        use FieldId::*;
        Some(match *self {
            Name => &mut doc.profile.active_mut().name,
            Label => &mut doc.profile.active_mut().label,
            Summary => &mut doc.profile.active_mut().summary,
            Email => &mut doc.profile.active_mut().email,
            Phone => &mut doc.profile.active_mut().phone,
            Location => &mut doc.profile.active_mut().location,
            Url => &mut doc.profile.active_mut().url,
            ProfileNetwork(i) => &mut doc.profile.active_mut().profiles.get_mut(i)?.network,
            ProfileUsername(i) => &mut doc.profile.active_mut().profiles.get_mut(i)?.username,
            ProfileUrl(i) => &mut doc.profile.active_mut().profiles.get_mut(i)?.url,
            WorkName(i) => &mut doc.work.active_mut().get_mut(i)?.name,
            WorkPosition(i) => &mut doc.work.active_mut().get_mut(i)?.position,
            WorkLocation(i) => &mut doc.work.active_mut().get_mut(i)?.location,
            WorkStart(i) => &mut doc.work.active_mut().get_mut(i)?.start_date.text,
            WorkEnd(i) => &mut doc.work.active_mut().get_mut(i)?.end_date.text,
            WorkSummary(i) => &mut doc.work.active_mut().get_mut(i)?.summary,
            WorkHighlight(i, j) => doc.work.active_mut().get_mut(i)?.highlights.get_mut(j)?,
            EduInstitution(i) => &mut doc.education.active_mut().get_mut(i)?.institution,
            EduStudyType(i) => &mut doc.education.active_mut().get_mut(i)?.study_type,
            EduStart(i) => &mut doc.education.active_mut().get_mut(i)?.start_date.text,
            EduEnd(i) => &mut doc.education.active_mut().get_mut(i)?.end_date.text,
            EduUrl(i) => &mut doc.education.active_mut().get_mut(i)?.url,
            EduHighlight(i, j) => doc.education.active_mut().get_mut(i)?.highlights.get_mut(j)?,
            SkillName(i) => &mut doc.skills.active_mut().get_mut(i)?.name,
            SkillKeyword(i, j) => doc.skills.active_mut().get_mut(i)?.keywords.get_mut(j)?,
            CertName(i) => &mut doc.certificates.active_mut().get_mut(i)?.name,
            CertIssuer(i) => &mut doc.certificates.active_mut().get_mut(i)?.issuer,
            CertDate(i) => &mut doc.certificates.active_mut().get_mut(i)?.date.text,
            CertUrl(i) => &mut doc.certificates.active_mut().get_mut(i)?.url,
            VolOrg(i) => &mut doc.volunteer.active_mut().get_mut(i)?.organization,
            VolPosition(i) => &mut doc.volunteer.active_mut().get_mut(i)?.position,
            VolStart(i) => &mut doc.volunteer.active_mut().get_mut(i)?.start_date.text,
            VolEnd(i) => &mut doc.volunteer.active_mut().get_mut(i)?.end_date.text,
            VolHighlight(i, j) => doc
                .volunteer
                .active_mut()
                .get_mut(i)?
                .highlights
                .get_mut(j)?,
            CustomSectionTitle(id) => &mut doc.custom_section_mut(id)?.title,
            CustomEntryTitle(id, i) => {
                &mut doc
                    .custom_section_mut(id)?
                    .content
                    .active_mut()
                    .get_mut(i)?
                    .title
            }
            CustomEntrySubtitle(id, i) => {
                &mut doc
                    .custom_section_mut(id)?
                    .content
                    .active_mut()
                    .get_mut(i)?
                    .subtitle
            }
            CustomEntryStart(id, i) => {
                &mut doc
                    .custom_section_mut(id)?
                    .content
                    .active_mut()
                    .get_mut(i)?
                    .start_date
                    .text
            }
            CustomEntryEnd(id, i) => {
                &mut doc
                    .custom_section_mut(id)?
                    .content
                    .active_mut()
                    .get_mut(i)?
                    .end_date
                    .text
            }
            CustomEntryUrl(id, i) => {
                &mut doc
                    .custom_section_mut(id)?
                    .content
                    .active_mut()
                    .get_mut(i)?
                    .url
            }
            CustomEntryHighlight(id, i, j) => doc
                .custom_section_mut(id)?
                .content
                .active_mut()
                .get_mut(i)?
                .highlights
                .get_mut(j)?,
            VariantName(section) => return doc.variant_name_mut(section),
            PresetName(i) => return doc.preset_name_mut(i),
        })
    }
}

/// Addresses a repeatable collection within a section's **active variant**.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListId {
    Work,
    Education,
    Skills,
    Certificates,
    Volunteer,
    Profiles,
    WorkHighlights(usize),
    EduHighlights(usize),
    VolHighlights(usize),
    SkillKeywords(usize),
    /// The entries of a custom section's active variant (D-9). Not yet
    /// constructed anywhere — the sidebar's "+ Add" for a custom section's
    /// entries is out of scope for D-9's model half — but addressed and
    /// handled by `add`/`remove` like every other `ListId` so the editor
    /// UI has nothing left to wire beyond the buttons themselves.
    CustomEntries(CustomSectionId),
    CustomEntryHighlights(CustomSectionId, usize),
}

impl ListId {
    /// Append a new entry, pre-filled with a visible placeholder.
    pub fn add(&self, doc: &mut ResumeDoc) {
        match *self {
            Self::Work => doc.work.active_mut().push(Work {
                position: "New role".into(),
                ..Default::default()
            }),
            Self::Education => doc.education.active_mut().push(Education {
                study_type: "New qualification".into(),
                ..Default::default()
            }),
            Self::Skills => doc.skills.active_mut().push(SkillGroup {
                name: "New category".into(),
                keywords: vec!["New skill".into()],
            }),
            Self::Certificates => doc.certificates.active_mut().push(Certificate {
                name: "New certificate".into(),
                ..Default::default()
            }),
            Self::Volunteer => doc.volunteer.active_mut().push(Volunteer {
                position: "New role".into(),
                ..Default::default()
            }),
            Self::Profiles => doc.profile.active_mut().profiles.push(NetworkProfile {
                network: "Website".into(),
                ..Default::default()
            }),
            Self::WorkHighlights(i) => {
                if let Some(w) = doc.work.active_mut().get_mut(i) {
                    w.highlights.push("New highlight".into());
                }
            }
            Self::EduHighlights(i) => {
                if let Some(e) = doc.education.active_mut().get_mut(i) {
                    e.highlights.push("New highlight".into());
                }
            }
            Self::VolHighlights(i) => {
                if let Some(v) = doc.volunteer.active_mut().get_mut(i) {
                    v.highlights.push("New highlight".into());
                }
            }
            Self::SkillKeywords(i) => {
                if let Some(s) = doc.skills.active_mut().get_mut(i) {
                    s.keywords.push("New skill".into());
                }
            }
            Self::CustomEntries(id) => {
                if let Some(s) = doc.custom_section_mut(id) {
                    s.content.active_mut().push(CustomEntry {
                        title: "New entry".into(),
                        ..Default::default()
                    });
                }
            }
            Self::CustomEntryHighlights(id, i) => {
                if let Some(entry) = doc
                    .custom_section_mut(id)
                    .and_then(|s| s.content.active_mut().get_mut(i))
                {
                    entry.highlights.push("New highlight".into());
                }
            }
        }
    }

    /// Remove the entry at `index` (bounds-checked, no-op if out of range).
    pub fn remove(&self, doc: &mut ResumeDoc, index: usize) {
        match *self {
            Self::Work => remove_at(doc.work.active_mut(), index),
            Self::Education => remove_at(doc.education.active_mut(), index),
            Self::Skills => remove_at(doc.skills.active_mut(), index),
            Self::Certificates => remove_at(doc.certificates.active_mut(), index),
            Self::Volunteer => remove_at(doc.volunteer.active_mut(), index),
            Self::Profiles => remove_at(&mut doc.profile.active_mut().profiles, index),
            Self::WorkHighlights(i) => {
                if let Some(w) = doc.work.active_mut().get_mut(i) {
                    remove_at(&mut w.highlights, index);
                }
            }
            Self::EduHighlights(i) => {
                if let Some(e) = doc.education.active_mut().get_mut(i) {
                    remove_at(&mut e.highlights, index);
                }
            }
            Self::VolHighlights(i) => {
                if let Some(v) = doc.volunteer.active_mut().get_mut(i) {
                    remove_at(&mut v.highlights, index);
                }
            }
            Self::SkillKeywords(i) => {
                if let Some(s) = doc.skills.active_mut().get_mut(i) {
                    remove_at(&mut s.keywords, index);
                }
            }
            Self::CustomEntries(id) => {
                if let Some(s) = doc.custom_section_mut(id) {
                    remove_at(s.content.active_mut(), index);
                }
            }
            Self::CustomEntryHighlights(id, i) => {
                if let Some(entry) = doc
                    .custom_section_mut(id)
                    .and_then(|s| s.content.active_mut().get_mut(i))
                {
                    remove_at(&mut entry.highlights, index);
                }
            }
        }
    }
}

fn remove_at<T>(items: &mut Vec<T>, index: usize) {
    if index < items.len() {
        items.remove(index);
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldId, ListId};
    use crate::resume::model::{Resume, ResumeDoc, SectionKind};

    #[test]
    fn add_and_remove_route_to_active_variant() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");

        ListId::Work.add(&mut doc);
        assert_eq!(doc.work.active().len(), 1);
        assert_eq!(doc.work.active()[0].position, "New role");

        ListId::WorkHighlights(0).add(&mut doc);
        ListId::WorkHighlights(0).add(&mut doc);
        assert_eq!(doc.work.active()[0].highlights.len(), 2);
        ListId::WorkHighlights(0).remove(&mut doc, 0);
        assert_eq!(doc.work.active()[0].highlights.len(), 1);

        // A second variant starts as a copy but edits to it don't touch the first.
        doc.add_variant(SectionKind::Work);
        ListId::Work.add(&mut doc);
        assert_eq!(doc.work.active().len(), 2);
        doc.set_active_variant(SectionKind::Work, 0);
        assert_eq!(doc.work.active().len(), 1);
    }

    #[test]
    fn presets_capture_and_apply_selection() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");

        // A second Work variant ("Base copy"), now active.
        doc.add_variant(SectionKind::Work);
        assert_eq!(doc.work.active, 1);

        // Save the current selection as a preset (Work -> "Base copy").
        doc.add_preset("Tailored");

        // Manually switch Work back to the original.
        doc.set_active_variant(SectionKind::Work, 0);
        assert_eq!(doc.work.active, 0);

        // Applying the preset restores Work to the captured variant by name.
        doc.apply_preset(0);
        assert_eq!(doc.work.active, 1);
    }

    #[test]
    fn addressable_covers_every_entry_of_the_active_variant() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        ListId::Work.add(&mut doc);
        ListId::WorkHighlights(0).add(&mut doc);
        ListId::Skills.add(&mut doc);

        let fields = FieldId::addressable(&doc);

        // Every enumerated field must actually resolve; a stale index here would
        // hand the editor a text box bound to nothing.
        for field in &fields {
            assert!(
                field.get(&doc).is_some(),
                "{field:?} is enumerated but does not resolve"
            );
        }
        assert!(fields.contains(&FieldId::WorkHighlight(0, 0)));
        assert!(fields.contains(&FieldId::SkillKeyword(0, 0)));

        // Removing the entry retires its fields.
        ListId::WorkHighlights(0).remove(&mut doc, 0);
        assert!(!FieldId::addressable(&doc).contains(&FieldId::WorkHighlight(0, 0)));
    }

    #[test]
    fn custom_section_fields_are_addressable_and_editable() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let id = doc.add_custom_section("Publications");
        ListId::CustomEntries(id).add(&mut doc);
        ListId::CustomEntryHighlights(id, 0).add(&mut doc);

        let fields = FieldId::addressable(&doc);
        for field in &fields {
            assert!(
                field.get(&doc).is_some(),
                "{field:?} is enumerated but does not resolve"
            );
        }
        assert!(fields.contains(&FieldId::CustomSectionTitle(id)));
        assert!(fields.contains(&FieldId::CustomEntryTitle(id, 0)));
        assert!(fields.contains(&FieldId::CustomEntryHighlight(id, 0, 0)));
        assert!(fields.contains(&FieldId::VariantName(SectionKind::Custom(id))));
        assert_eq!(
            FieldId::CustomEntryTitle(id, 0).section(),
            Some(SectionKind::Custom(id))
        );
        assert!(FieldId::CustomEntryHighlight(id, 0, 0).multiline());

        // An edit through `get_mut` actually lands on the model.
        *FieldId::CustomEntryTitle(id, 0).get_mut(&mut doc).unwrap() = "Edited".into();
        assert_eq!(
            FieldId::CustomEntryTitle(id, 0).get(&doc).unwrap(),
            "Edited"
        );

        // A new section arrives with one seeded entry, so removing it twice is
        // what empties the section: once for the entry the test pushed, once for
        // the seed. Emptied, its entry fields retire — same as a built-in's.
        while !doc.custom_section(id).unwrap().content.active().is_empty() {
            ListId::CustomEntries(id).remove(&mut doc, 0);
        }
        assert!(!FieldId::addressable(&doc).contains(&FieldId::CustomEntryTitle(id, 0)));
        // The title survives an empty section — it is what the user renames.
        assert!(FieldId::addressable(&doc).contains(&FieldId::CustomSectionTitle(id)));
    }
    /// Every field in the model must be reachable from the editor. An
    /// `Education::highlights` entry was rendering on the page and had no field
    /// of its own, so an imported line could be read in the PDF and neither
    /// found nor deleted.
    #[test]
    fn an_education_highlight_can_be_read_edited_and_removed() {
        use crate::resume::model::Education;

        let mut doc = ResumeDoc::from_resume(
            Resume {
                education: vec![Education {
                    institution: "Bellows College".into(),
                    highlights: vec!["University of Florida".into()],
                    ..Default::default()
                }],
                ..Default::default()
            },
            "Base",
        );

        let id = FieldId::EduHighlight(0, 0);
        assert_eq!(id.get(&doc).map(String::as_str), Some("University of Florida"));
        assert!(
            FieldId::addressable(&doc).contains(&id),
            "the field must be enumerated, or the editor never draws it"
        );

        ListId::EduHighlights(0).add(&mut doc);
        assert_eq!(doc.education.active()[0].highlights.len(), 2);
        ListId::EduHighlights(0).remove(&mut doc, 0);
        assert_eq!(doc.education.active()[0].highlights, vec!["New highlight"]);
    }

    /// **Every addressable field must be drawn somewhere.**
    ///
    /// A preset kept the name it was born with — `Preset 2` — for life (G-14).
    #[test]
    fn a_preset_can_be_renamed_through_its_field() {
        use crate::resume::model::Preset;

        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        doc.presets.push(Preset {
            name: "Preset 2".into(),
            selection: Vec::new(),
            hidden: Vec::new(),
        });

        let id = FieldId::PresetName(0);
        assert!(FieldId::addressable(&doc).contains(&id));
        assert_eq!(id.get(&doc).map(String::as_str), Some("Preset 2"));

        *id.get_mut(&mut doc).expect("the preset exists") = "Backend, concise".into();
        assert_eq!(doc.presets[0].name, "Backend, concise");
    }

}

//! Operations that compose and manage a versioned resume document.

use super::model::*;

impl ResumeDoc {
    /// Wrap a flat resume as a document with one variant per section.
    pub fn from_resume(r: Resume, base_name: impl Into<String> + Clone) -> Self {
        let base_name = base_name.into();
        let customs = r.custom_sections;
        let mut doc = Self {
            profile: Versioned::single(base_name.clone(), r.basics),
            work: Versioned::single(base_name.clone(), r.work),
            education: Versioned::single(base_name.clone(), r.education),
            skills: Versioned::single(base_name.clone(), r.skills),
            certificates: Versioned::single(base_name.clone(), r.certificates),
            volunteer: Versioned::single(base_name.clone(), r.volunteer),
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
        };

        // A composed résumé's custom sections were dropped on the floor here,
        // which is why a document exported as Typst and read back had its
        // Publications and Talks silently gone. Ids are re-issued rather than
        // carried: the counter is this document's, and the incoming ids came
        // from whatever document wrote the file.
        for section in customs {
            let id = doc.add_custom_section(section.title);
            if let Some(added) = doc.custom_section_mut(id) {
                *added.content.active_mut() = section.entries;
            }
        }
        doc
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
        let id = CustomSectionId::from_u32(self.next_custom_section_id);
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
        // Every preset names every section (`reconcile_presets`), so a section
        // added now joins the presets that already exist rather than being
        // absent from them until somebody notices.
        self.reconcile_presets();
        id
    }

    /// Remove a custom section (every variant of it). A stale reference left
    /// behind in `section_order` is repaired away by [`Self::sections`]; one
    /// left in a `Preset` is repaired here, by [`Self::reconcile_presets`],
    /// rather than lingering as a pin to a section nothing can show.
    pub fn remove_custom_section(&mut self, id: CustomSectionId) {
        self.custom_sections.retain(|s| s.id != id);
        self.reconcile_presets();
    }

    /// Every variant id of a section, in stored order. The one dispatcher the
    /// id-based operations need; the rest are written in terms of it and
    /// [`Self::set_active_variant`].
    pub fn variant_ids(&self, section: SectionKind) -> Vec<VariantId> {
        use SectionKind::*;
        match section {
            Profile => self.profile.ids(),
            Work => self.work.ids(),
            Education => self.education.ids(),
            Skills => self.skills.ids(),
            Certificates => self.certificates.ids(),
            Organizations => self.volunteer.ids(),
            Custom(id) => self
                .custom_section(id)
                .map(|s| s.content.ids())
                .unwrap_or_default(),
        }
    }

    /// The active variant's id, or `None` for a `Custom` id with no matching
    /// section — [`Self::variant_name`]'s fallback, in the other direction.
    pub fn active_variant_id(&self, section: SectionKind) -> Option<VariantId> {
        self.variant_ids(section)
            .get(self.active_variant(section))
            .copied()
    }

    /// Activate a section's variant by id. Answers whether it resolved, so a
    /// caller that cares about a broken pin can say so instead of leaving the
    /// section wherever it happened to be — which is what the name-based
    /// version did, silently, for every preset that pinned a renamed variant.
    pub fn set_active_variant_by_id(&mut self, section: SectionKind, id: VariantId) -> bool {
        match self.variant_ids(section).iter().position(|v| *v == id) {
            Some(index) => {
                self.set_active_variant(section, index);
                true
            }
            None => false,
        }
    }

    /// The active variant's id for every section, built-in and custom alike, in
    /// the document's own order — what a preset saves.
    pub fn current_selection(&self) -> Vec<(SectionKind, VariantId)> {
        self.sections()
            .into_iter()
            .filter_map(|s| self.active_variant_id(s).map(|id| (s, id)))
            .collect()
    }

    /// Make every preset name every section the document has, and no others.
    ///
    /// Run on load and after any change to the *set* of sections. Two repairs,
    /// and both are about totality rather than about resolvability:
    ///
    /// - a section added after a preset was saved joins it, pinned to whatever
    ///   that section currently reads. Excel's Custom Views shipped the other
    ///   answer in 1993 — a view silently does not cover a sheet added later —
    ///   and it is the reason nobody trusts them;
    /// - a pin naming a section the document no longer has is dropped, the same
    ///   repair [`Self::sections`] already makes to `section_order`.
    ///
    /// A pin naming a *deleted variant* is left exactly where it is. That is
    /// not untidiness: it is the difference between "this preset has nothing to
    /// say about Skills" and "this preset selected a cut of Skills you threw
    /// away", and only the second one is worth telling somebody about.
    pub fn reconcile_presets(&mut self) {
        let current: Vec<(SectionKind, VariantId)> = self.current_selection();
        let known: Vec<SectionKind> = current.iter().map(|(s, _)| *s).collect();
        for preset in &mut self.presets {
            preset.selection.retain(|(s, _)| known.contains(s));
            for (section, active) in &current {
                if preset.variant_for(*section).is_none() {
                    preset.selection.push((*section, *active));
                }
            }
            preset.hidden.retain(|s| known.contains(s));
        }
    }

    /// The sections where preset `index` pins a variant that no longer exists.
    ///
    /// The one thing a preset can be wrong about after
    /// [`Self::reconcile_presets`], and the reason it is reported rather than
    /// repaired: repairing it would mean choosing a cut of that section on the
    /// user's behalf and never mentioning it.
    pub fn unresolved_pins(&self, index: usize) -> Vec<SectionKind> {
        let Some(preset) = self.presets.get(index) else {
            return Vec::new();
        };
        preset
            .selection
            .iter()
            .filter(|(section, id)| !self.variant_ids(*section).contains(id))
            .map(|(section, _)| *section)
            .collect()
    }

    /// Which presets pin this exact variant — what the delete confirmation has
    /// to name before it removes one, the way `library_link.rs` names the CVs
    /// an edited block would reach.
    pub fn presets_pinning(&self, section: SectionKind, id: VariantId) -> Vec<&str> {
        self.presets
            .iter()
            .filter(|p| p.variant_for(section) == Some(id))
            .map(|p| p.name.as_str())
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
            based_on: None,
            description: None,
            selection,
            hidden,
        });
    }

    /// Switch every section to the variants recorded in preset `index`.
    pub fn apply_preset(&mut self, index: usize) {
        let Some(preset) = self.presets.get(index).cloned() else {
            return;
        };
        for (section, variant) in preset.selection {
            // A pin that does not resolve falls back to the section's first
            // variant — which is what the delete confirmation has promised
            // since it was written, and what nothing implemented: the
            // name-based version simply did nothing, leaving the section on
            // whatever the previous reading had selected. Falling back is a
            // guess too, but it is the same guess every time, and
            // `unresolved_pins` is what says so on screen.
            if !self.set_active_variant_by_id(section, variant) {
                self.set_active_variant(section, 0);
            }
        }
        // Visibility is part of the selection (O-13), so applying a preset
        // restores it wholesale — including *un*-hiding what this preset does
        // not hide, or switching presets would only ever accumulate hiding.
        self.hidden_sections = preset.hidden;
    }

    /// Whether preset `index` is already what the document is showing.
    ///
    /// Applying a preset is an edit: it moves `active` on every section it
    /// names and rewrites `hidden_sections`, and all of that is stored. So
    /// arriving at a document *already* in that state must not be an edit —
    /// otherwise opening a card at the preset it is already in would checkpoint
    /// nothing onto the undo stack and write an identical file to disk.
    ///
    /// A preset naming no sections is not "active": it selects nothing, so
    /// there is nothing for the document to already agree with.
    pub fn is_preset_active(&self, index: usize) -> bool {
        let Some(preset) = self.presets.get(index) else {
            return false;
        };
        if preset.selection.is_empty() {
            return false;
        }
        // Order is not meaning — `apply_preset` assigns the list wholesale, but
        // a hand-edited vault can spell the same set differently.
        if preset.hidden.len() != self.hidden_sections.len()
            || !preset
                .hidden
                .iter()
                .all(|section| self.hidden_sections.contains(section))
        {
            return false;
        }
        preset
            .selection
            .iter()
            .all(|(section, id)| self.active_variant_id(*section) == Some(*id))
    }

    /// The first preset whose reading is exactly the document's working copy.
    ///
    /// There is deliberately no stored "current preset". Two presets may be
    /// identical, and storing either index would make the label depend on the
    /// last button clicked rather than on what the document actually says.
    /// Document order is the stable tie-break in that case.
    pub fn active_preset_index(&self) -> Option<usize> {
        (0..self.presets.len()).find(|index| self.is_preset_active(*index))
    }

    /// How many section cells differ between the working copy and a preset.
    ///
    /// Variant and visibility are one cell, not two: changing both on Skills
    /// is still one row the person has to inspect in the matrix. A broken pin
    /// differs from every live variant, which makes repairing it explicit.
    pub fn preset_distance(&self, index: usize) -> Option<usize> {
        let preset = self.presets.get(index)?;
        Some(
            self.sections()
                .into_iter()
                .filter(|section| {
                    preset.variant_for(*section) != self.active_variant_id(*section)
                        || preset.hidden.contains(section) != self.hidden_sections.contains(section)
                })
                .count(),
        )
    }

    /// The preset requiring the fewest section changes to reach from now.
    ///
    /// `min_by_key` keeps the first minimum, so identical distances obey the
    /// same document-order tie-break as [`Self::active_preset_index`].
    pub fn nearest_preset_index(&self) -> Option<usize> {
        (0..self.presets.len())
            .min_by_key(|index| self.preset_distance(*index).unwrap_or(usize::MAX))
    }

    /// Rewrite one preset to describe the document's working copy.
    ///
    /// Presets own no content; updating one means replacing only its variant
    /// pins and visibility. The UI checkpoints the whole document before this
    /// call, which makes the operation undoable without a second history type.
    pub fn update_preset(&mut self, index: usize) -> bool {
        let selection = self.current_selection();
        let hidden = self.hidden_sections.clone();
        let Some(preset) = self.presets.get_mut(index) else {
            return false;
        };
        preset.selection = selection;
        preset.hidden = hidden;
        true
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
                    id: *self.variant_ids(kind).get(index)?,
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

#[cfg(test)]
#[path = "document_variants_tests.rs"]
mod tests;

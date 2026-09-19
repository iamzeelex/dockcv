use crate::resume::model::*;

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

    let tailored = back
        .custom_section(id)
        .and_then(|s| s.content.variants.iter().find(|v| v.name == "Tailored"))
        .map(|v| v.id)
        .expect("the tailored variant survives the round trip");

    assert_eq!(back.presets.len(), 1);
    assert!(
        back.presets[0]
            .selection
            .iter()
            .any(|(s, pinned)| *s == SectionKind::Custom(id) && *pinned == tailored),
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
    let (detailed, concise, infra) = (
        VariantId::from_u32(1),
        VariantId::from_u32(2),
        VariantId::from_u32(7),
    );
    let mut preset = Preset {
        name: "FAANG · concise".into(),
        based_on: None,
        description: None,
        selection: vec![(SectionKind::Work, detailed)],
        hidden: Vec::new(),
        order: Vec::new(),
        titles: Vec::new(),
    };

    preset.set(SectionKind::Work, concise);
    preset.set(SectionKind::Skills, infra);

    assert_eq!(preset.selection.len(), 2);
    assert_eq!(preset.variant_for(SectionKind::Work), Some(concise));
    assert_eq!(preset.variant_for(SectionKind::Skills), Some(infra));
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

/// A display-name edit cannot change what a preset means. Before C0 the
/// pin itself was that name, so this exact edit stranded it.
#[test]
fn a_renamed_variant_does_not_strand_the_presets_that_pin_it() {
    let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
    doc.add_variant(SectionKind::Work);
    let pinned = doc.work.active_id();
    doc.work.active_name_mut().clone_from(&"Infra".into());
    doc.add_preset("Platform");

    doc.work.active_name_mut().clone_from(&"Infra-heavy".into());
    doc.set_active_variant(SectionKind::Work, 0);
    doc.apply_preset(0);

    assert_eq!(doc.work.active_id(), pinned);
    assert_eq!(doc.work.active_name(), "Infra-heavy");
    assert!(doc.is_preset_active(0));
}

/// Adding a section changes the set a preset has to account for. It joins
/// every existing preset at the current variant instead of inheriting
/// whichever state another reading happened to leave behind.
#[test]
fn a_new_section_joins_every_existing_preset() {
    let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
    doc.add_preset("Concise");
    let id = doc.add_custom_section("Publications");
    let active = doc.active_variant_id(SectionKind::Custom(id)).unwrap();

    assert_eq!(
        doc.presets[0].variant_for(SectionKind::Custom(id)),
        Some(active)
    );
    assert_eq!(doc.presets[0].selection.len(), doc.sections().len());
    assert_eq!(
        doc.sections_for_preset(0),
        Some(doc.sections()),
        "an empty preset order uses the repaired standard order"
    );
}

/// Deletion does not rewrite history into a different selection. The pin
/// stays visibly unresolved, and applying it has one deterministic fallback.
#[test]
fn deleting_a_pinned_variant_is_loud_and_falls_back_to_the_first() {
    let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
    doc.add_variant(SectionKind::Work);
    let deleted = doc.work.active_id();
    doc.work.active_name_mut().clone_from(&"Infra".into());
    doc.add_preset("Platform");

    doc.remove_variant(SectionKind::Work, 1);
    assert_eq!(
        doc.presets[0].variant_for(SectionKind::Work),
        Some(deleted),
        "the broken fact remains inspectable"
    );
    assert_eq!(doc.unresolved_pins(0), vec![SectionKind::Work]);

    doc.apply_preset(0);
    assert_eq!(doc.work.active, 0);
}

/// A deleted id is never handed to the next copy: doing so would quietly
/// make every dangling preset pin refer to unrelated new content.
#[test]
fn a_deleted_variant_id_is_never_reissued() {
    let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
    doc.add_variant(SectionKind::Work);
    let deleted = doc.work.active_id();
    doc.remove_variant(SectionKind::Work, 1);
    doc.add_variant(SectionKind::Work);

    assert_ne!(doc.work.active_id(), deleted);
    assert!(doc.work.active_id().as_u32() > deleted.as_u32());
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

#[test]
fn a_preset_restores_order_and_headings_as_one_reading() {
    let mut doc = ResumeDoc::default();
    let engineering_order = vec![
        SectionKind::Skills,
        SectionKind::Profile,
        SectionKind::Work,
        SectionKind::Education,
        SectionKind::Certificates,
        SectionKind::Organizations,
    ];
    doc.section_order = engineering_order.clone();
    doc.set_section_title(SectionKind::Skills, "Engineering");
    doc.add_preset("Engineering first");

    let experience_order = vec![
        SectionKind::Work,
        SectionKind::Profile,
        SectionKind::Education,
        SectionKind::Skills,
        SectionKind::Certificates,
        SectionKind::Organizations,
    ];
    doc.section_order = experience_order.clone();
    doc.set_section_title(SectionKind::Skills, "");
    doc.set_section_title(SectionKind::Work, "Experience");
    doc.add_preset("Experience first");

    assert_eq!(doc.presets[0].order, engineering_order);
    assert_eq!(
        doc.presets[0].titles,
        vec![(SectionKind::Skills, "Engineering".into())]
    );
    assert!(doc.is_preset_active(1));

    doc.apply_preset(0);
    assert_eq!(doc.sections(), engineering_order);
    assert_eq!(doc.section_title(SectionKind::Skills), "Engineering");
    assert!(doc.is_preset_active(0));
    assert!(!doc.is_preset_active(1));

    let composed = doc.compose();
    assert_eq!(composed.section_order, engineering_order);
    assert!(composed
        .section_titles
        .contains(&(SectionKind::Skills, "Engineering".into())));

    let text = toml::to_string_pretty(&doc).expect("preset serializes");
    let back: ResumeDoc = toml::from_str(&text).expect("preset round-trips");
    assert_eq!(back.presets[0].order, doc.presets[0].order);
    assert_eq!(back.presets[0].titles, doc.presets[0].titles);
}

#[test]
fn active_and_nearest_preset_include_order_and_headings() {
    let mut doc = ResumeDoc::default();
    doc.add_preset("Default reading");

    // Empty fields are the backwards-compatible form and stay out of TOML.
    let old_shape = toml::to_string_pretty(&doc).expect("serializes");
    assert!(!old_shape
        .lines()
        .any(|line| line.trim_start().starts_with("order =")));
    assert!(!old_shape
        .lines()
        .any(|line| line.trim_start().starts_with("titles =")));
    let old_back: ResumeDoc = toml::from_str(&old_shape).expect("pre-C8 preset opens");
    assert!(old_back.presets[0].order.is_empty());
    assert!(old_back.presets[0].titles.is_empty());

    doc.set_section_title(SectionKind::Work, "Engineering");
    assert!(!doc.is_preset_active(0));
    assert_eq!(doc.preset_distance(0), Some(1));
    assert_eq!(doc.nearest_preset_index(), Some(0));

    doc.apply_preset(0);
    assert!(doc.section_order.is_empty());
    assert!(doc.section_titles.is_empty());
    assert_eq!(doc.section_title(SectionKind::Work), "Work Experience");
    assert!(doc.is_preset_active(0));

    doc.set_section_title(SectionKind::Work, "Engineering");
    assert!(doc.update_preset(0));
    assert!(doc.is_preset_active(0));
    assert_eq!(
        doc.presets[0].titles,
        vec![(SectionKind::Work, "Engineering".into())]
    );

    // Swapping adjacent rows changes exactly those two rows in the matrix.
    doc.section_order = vec![
        SectionKind::Profile,
        SectionKind::Education,
        SectionKind::Work,
        SectionKind::Skills,
        SectionKind::Certificates,
        SectionKind::Organizations,
    ];
    assert!(!doc.is_preset_active(0));
    assert_eq!(doc.preset_distance(0), Some(2));
    assert!(doc.update_preset(0));
    assert_eq!(doc.presets[0].order, doc.section_order);
    assert!(doc.is_preset_active(0));
}

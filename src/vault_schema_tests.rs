//! Every stored field survives the disk, and every old file still opens.
//!
//! Split from `vault.rs` by C15. This is the file the storage rule in
//! `CLAUDE.md` is about — a schema change needs a forward migration and a
//! round-trip test, and these are the round-trip tests.

use crate::resume::{altacv, model::ResumeDoc};

/// O-13: visibility is part of what a preset selects. A document written
/// before it existed must load with everything visible, an empty list must
/// not reach the file, and — the part that matters — hiding has to reach
/// the *rendered* document, not just the sidebar.
#[test]
fn hiding_a_section_survives_a_round_trip_and_reaches_the_render() {
    use crate::resume::model::SectionKind;

    let mut doc =
        ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
    assert!(
        !doc.compose().certificates.is_empty(),
        "fixture must have certificates"
    );

    doc.set_hidden(SectionKind::Certificates, true);
    assert!(doc.is_hidden(SectionKind::Certificates));
    // The whole point: a hidden section leaves the composed document.
    assert!(doc.compose().certificates.is_empty());
    // …and the sections that were not hidden are untouched.
    assert!(!doc.compose().work.is_empty());

    let text = super::to_toml(&doc).expect("serializes");
    let back: ResumeDoc = toml::from_str(&text).expect("round-trips");
    assert!(back.is_hidden(SectionKind::Certificates));

    // Profile is not hideable — a résumé without a name is broken, not short.
    let mut doc = back;
    doc.set_hidden(SectionKind::Profile, true);
    assert!(!doc.is_hidden(SectionKind::Profile));

    // Showing it again empties the list, and an empty list is not written.
    doc.set_hidden(SectionKind::Certificates, false);
    let text = super::to_toml(&doc).expect("serializes");
    assert!(
        !text.contains("hidden_sections"),
        "an empty list must not be written"
    );
}

/// The first per-section override has to survive the disk, and — because
/// it is the first row of a table the rest of the per-section settings
/// will land in — an untouched document has to gain no key at all.
#[test]
fn a_sections_own_layout_survives_a_round_trip_and_is_absent_until_used() {
    use crate::resume::model::SectionKind;

    let mut doc =
        ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
    let text = super::to_toml(&doc).expect("serializes");
    assert!(
        !text.contains("section_overrides"),
        "a document nobody customised must carry no overrides table"
    );

    doc.set_heading_printed(SectionKind::Profile, false);
    assert!(!doc.prints_heading(SectionKind::Profile));
    assert!(
        doc.prints_heading(SectionKind::Work),
        "one section, not all of them"
    );

    let text = super::to_toml(&doc).expect("serializes");
    let back: ResumeDoc = toml::from_str(&text).expect("round-trips");
    assert!(!back.prints_heading(SectionKind::Profile));
    assert!(back.prints_heading(SectionKind::Work));

    // A per-field departure survives the disk too, and only the field that
    // departed is written — the rest stay absent so they keep following.
    let mut doc = back;
    doc.set_section_overrides(
        SectionKind::Skills,
        crate::resume::model::SectionOverrides {
            heading_style: Some(crate::resume::model::HeadingStyle::Boxed),
            ..doc.section_overrides(SectionKind::Skills)
        },
    );
    let text = super::to_toml(&doc).expect("serializes");
    assert!(text.contains("heading_style"), "{text}");
    assert!(
        !text.contains("heading_case"),
        "a field nobody set was written, so it has stopped following the document"
    );
    let back: ResumeDoc = toml::from_str(&text).expect("round-trips");
    assert_eq!(
        back.headings_for(SectionKind::Skills).style,
        crate::resume::model::HeadingStyle::Boxed
    );
    assert_eq!(
        back.headings_for(SectionKind::Skills).case,
        back.layout.headings.case,
        "the section stopped following the document on a field it never set"
    );

    // Following the document again removes the row rather than storing a
    // row of defaults — "follow the document" is the absence of an entry.
    let mut doc = back;
    doc.set_section_overrides(SectionKind::Skills, Default::default());
    doc.set_heading_printed(SectionKind::Profile, true);
    assert!(doc.section_overrides.is_empty());
    let text = super::to_toml(&doc).expect("serializes");
    assert!(
        !text.contains("section_overrides"),
        "an emptied table must not be written"
    );
}

/// Three fields arrived with the front door: what a version was made from,
/// what a version is for, and what a cut of a section does. All three are
/// stored, so all three have to survive the disk — and a document that uses
/// none of them has to gain no keys at all, because a version nobody
/// described is the ordinary case and an `Option` written out as empty is a
/// file that grew for nothing.
///
/// The absence half is also the forward-compatibility half: a file written
/// before these existed looks exactly like the one this test writes first.
#[test]
fn a_versions_lineage_and_both_descriptions_survive_a_round_trip() {
    let mut doc =
        ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
    doc.add_preset("Infra-heavy");

    let text = super::to_toml(&doc).expect("serializes");
    assert!(
        !text.contains("based_on"),
        "a version made from nothing must not record that it was"
    );
    assert!(
        !text.contains("description"),
        "nobody described anything, so nothing should be written: {text}"
    );

    // Which is also what a document saved before these fields existed looks
    // like, and it has to load.
    let mut doc: ResumeDoc = toml::from_str(&text).expect("round-trips");
    assert_eq!(doc.presets[0].based_on, None);
    assert_eq!(doc.presets[0].description, None);
    assert_eq!(doc.work.variants[0].description, None);

    doc.work.variants[0].description = Some("Lead with the reliability result.".into());
    let copy = doc.add_preset_from(0, "Northwind").expect("the base exists");
    doc.presets[copy].description = Some("platform, reliability, distributed".into());

    let text = super::to_toml(&doc).expect("serializes");
    let back: ResumeDoc = toml::from_str(&text).expect("round-trips");
    assert_eq!(
        back.presets[copy].based_on.as_deref(),
        Some("Infra-heavy"),
        "a version remembers what it was made from, by name"
    );
    assert_eq!(
        back.presets[copy].description.as_deref(),
        Some("platform, reliability, distributed")
    );
    assert_eq!(
        back.work.variants[0].description.as_deref(),
        Some("Lead with the reliability result.")
    );
    // And the one nobody touched still carries neither.
    assert_eq!(back.presets[0].based_on, None);
    assert_eq!(back.presets[0].description, None);
}

/// Stage history has to survive the disk, and a board written before it
/// existed has to load without gaining an empty one in every card.
#[test]
fn stage_history_round_trips_and_old_boards_gain_no_key() {
    use crate::resume::model::{Application, ApplicationStatus, Applications};

    let mut board = Applications {
        entries: vec![Application {
            company: "Acme".into(),
            created: "2026-06-01".into(),
            ..Default::default()
        }],
    };
    let text = toml::to_string_pretty(&board).expect("serializes");
    assert!(
        !text.contains("history"),
        "a card that has never moved must carry no history table"
    );

    board.entries[0].advance_to(ApplicationStatus::Applied, "2026-06-10");
    board.entries[0].advance_to(ApplicationStatus::Interviewing, "2026-07-02");
    let text = toml::to_string_pretty(&board).expect("serializes");
    let back: Applications = toml::from_str(&text).expect("round-trips");

    let history = &back.entries[0].history;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].at, "2026-06-10");
    assert_eq!(history[0].to, "applied");
    assert_eq!(history[1].to, "interviewing");
    // The word is what round-trips, exactly as `status` does — so a stage
    // this build does not know is not silently rewritten on the way back.
    assert_eq!(
        back.entries[0].stage_before(1),
        ApplicationStatus::Applied,
        "the stage before a move is the one the previous move landed on"
    );
    assert_eq!(back.entries[0].stage_before(0), ApplicationStatus::Wishlist);
}

/// Rounds and the closure have to survive the disk, and a board written
/// before either existed must gain no keys.
#[test]
fn rounds_and_closure_round_trip_and_old_boards_gain_no_keys() {
    use crate::resume::model::{Application, Applications, Closure, InterviewRound};

    let mut board = Applications {
        entries: vec![Application {
            company: "Acme".into(),
            ..Default::default()
        }],
    };
    let text = toml::to_string_pretty(&board).expect("serializes");
    assert!(!text.contains("rounds"), "{text}");
    assert!(!text.contains("closed_as"), "{text}");

    board.entries[0].rounds.push(InterviewRound {
        at: "2026-07-02".into(),
        label: "Technical screen".into(),
    });
    board.entries[0].rounds.push(InterviewRound {
        at: "2026-07-16".into(),
        // A round nobody named is still a round that happened.
        label: String::new(),
    });
    board.entries[0].closed_as = Some(Closure::Ghosted);

    let text = toml::to_string_pretty(&board).expect("serializes");
    let back: Applications = toml::from_str(&text).expect("round-trips");
    assert_eq!(back.entries[0].rounds.len(), 2);
    assert_eq!(back.entries[0].rounds[0].label, "Technical screen");
    assert!(back.entries[0].rounds[1].label.is_empty());
    assert_eq!(back.entries[0].closed_as, Some(Closure::Ghosted));
}

/// Renaming is a file move, so the two failure modes that matter are
/// clobbering an existing document and accepting a name the filesystem
/// cannot hold.
#[test]
fn renaming_a_document_never_overwrites_another() {
    let dir = std::env::temp_dir().join(format!("dockcv-rename-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp vault");

    let doc = ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
    let a = super::create_document(&dir, &doc, "first").expect("create a");
    let b = super::create_document(&dir, &doc, "second").expect("create b");

    // A name the user types is slugified, exactly like a new document's.
    let renamed = super::rename_document(&a, "FAANG concise").expect("rename");
    assert!(renamed.ends_with("faang-concise.toml"), "got {renamed:?}");
    assert!(!a.exists(), "the old file is gone");
    assert!(super::load(&renamed).is_ok(), "the document still parses");

    // Onto an occupied name: refused, and both files survive.
    let err = super::rename_document(&renamed, "second").expect_err("must refuse");
    assert!(err.contains("already exists"), "got {err}");
    assert!(renamed.exists() && b.exists());

    // Empty is refused rather than producing `.toml`.
    assert!(super::rename_document(&renamed, "   ").is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

/// The gallery's card reads these three facts to tell one CV from
/// another; a count alone cannot, since two CVs for the same person carry
/// the same name and the same job title.
#[test]
fn doc_meta_carries_what_distinguishes_two_cvs() {
    use crate::resume::model::SectionKind;
    let dir = std::env::temp_dir().join(format!("dockcv-meta-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp vault");

    let mut doc =
        ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
    doc.add_variant(SectionKind::Work);
    doc.add_variant(SectionKind::Skills);
    doc.add_preset("FAANG · concise");
    doc.add_preset("Infra-heavy");
    let path = super::create_document(&dir, &doc, "sean senior swe").expect("create");

    let meta = super::meta_from(&path, super::load(&path).ok().as_ref());
    assert_eq!(meta.stem, "sean-senior-swe");
    assert_eq!(
        meta.presets.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        vec!["FAANG · concise".to_string(), "Infra-heavy".to_string()],
        "the front door names the presets rather than counting them (P-01)"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Section order is data now. A document written before it existed must load,
/// and a stored order that has gone stale must be repaired rather than
/// silently dropping sections from the editor.
#[test]
fn section_order_defaults_and_repairs_itself() {
    use crate::resume::model::SectionKind;

    let mut doc = ResumeDoc::from_resume(crate::resume::model::Resume::default(), "Base");
    assert_eq!(
        doc.sections(),
        ResumeDoc::SECTIONS.to_vec(),
        "empty means default"
    );

    // A partial order (as an older or hand-edited file might hold) keeps the
    // listed sections first and appends the rest — nothing disappears.
    doc.section_order = vec![SectionKind::Skills, SectionKind::Work];
    let order = doc.sections();
    assert_eq!(&order[..2], &[SectionKind::Skills, SectionKind::Work]);
    assert_eq!(order.len(), ResumeDoc::SECTIONS.len());

    // Duplicates are dropped rather than rendering a section twice.
    doc.section_order = vec![SectionKind::Work, SectionKind::Work];
    assert_eq!(doc.sections().len(), ResumeDoc::SECTIONS.len());

    // Moving is clamped at both ends.
    doc.section_order = Vec::new();
    doc.move_section(SectionKind::Profile, -1);
    assert_eq!(
        doc.sections()[0],
        SectionKind::Profile,
        "cannot move past the top"
    );
    doc.move_section(SectionKind::Profile, 1);
    assert_eq!(doc.sections()[1], SectionKind::Profile);
}

/// A document with no custom sections must serialize exactly as it did
/// before D-9 — no new keys. Vaults are the user's real files under git.
#[test]
fn a_document_without_custom_sections_gains_no_new_keys() {
    let doc = ResumeDoc::from_resume(crate::resume::model::Resume::default(), "Base");
    let text = super::to_toml(&doc).expect("serializes");
    assert!(
        !text.contains("custom_sections"),
        "an untouched document should not gain a custom_sections table:\n{text}"
    );
    assert!(
        !text.contains("next_custom_section_id"),
        "an untouched document should not gain the id counter:\n{text}"
    );
}

/// A new custom section must actually change the rendered document.
///
/// The renderer skips empty sections, so an unseeded one produced identical
/// Typst source, the recompile was skipped as a no-op, and "+ Add" looked
/// like it had done nothing.
#[test]
fn a_new_custom_section_changes_the_generated_document() {
    use crate::resume::template;

    let mut doc = ResumeDoc::from_resume(crate::resume::model::Resume::default(), "Base");
    let before = template::generate_for(&doc);

    doc.add_custom_section("Publications");
    let after = template::generate_for(&doc);

    assert_ne!(
        before, after,
        "adding a section must change what gets rendered"
    );
    assert!(
        after.contains("Publications"),
        "the new section's title should reach the source"
    );
}

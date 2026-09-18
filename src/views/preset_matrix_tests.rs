use std::path::PathBuf;

use crate::resume::model::{Preset, ResumeDoc, SectionKind};

use super::preset_matrix::PresetMatrix;

/// Two presets that differ on Work and agree on Profile, pinned by id and read
/// back as names.
#[test]
fn test_preset_matrix_diff_computation() {
    let mut doc = ResumeDoc::default();
    doc.work.variants[0].name = "FAANG".into();
    let faang = doc.work.active_id();
    doc.add_variant(SectionKind::Work); // "FAANG copy", now active
    doc.work.variants[1].name = "Startup".into();
    let startup = doc.work.active_id();
    let base = doc.profile.active_id();

    doc.presets = vec![
        Preset {
            name: "Preset A".into(),
            selection: vec![(SectionKind::Profile, base), (SectionKind::Work, faang)],
            hidden: Vec::new(),
        },
        Preset {
            name: "Preset B".into(),
            selection: vec![(SectionKind::Profile, base), (SectionKind::Work, startup)],
            hidden: Vec::new(),
        },
    ];

    let mut matrix = PresetMatrix::new(PathBuf::from("/dummy/path"), doc);
    matrix.active_preset_idx = 0;
    matrix.compare_preset_idx = Some(1);

    let diff = matrix.compute_diff();
    let (prof_a, prof_b) = diff.get(&SectionKind::Profile).copied().unwrap();
    assert_eq!(prof_a, base);
    assert_eq!(prof_b, Some(base));

    let (work_a, work_b) = diff.get(&SectionKind::Work).copied().unwrap();
    assert_eq!(work_a, faang);
    assert_eq!(work_b, Some(startup));
    assert_eq!(
        matrix.variant_label(SectionKind::Work, work_a).as_deref(),
        Some("FAANG")
    );
}

/// A preset pinning a variant that has been deleted is a cell that says so.
///
/// The whole reason pins are ids: before C0 this cell showed whichever variant
/// the document happened to be on, a different fact about a different object.
#[test]
fn a_pin_to_a_deleted_variant_is_labelled_rather_than_guessed_at() {
    let mut doc = ResumeDoc::default();
    doc.add_variant(SectionKind::Work); // "Base copy", now active
    doc.work.variants[1].name = "Infra".into();
    let infra = doc.work.active_id();
    doc.add_preset("Infra-heavy");

    doc.remove_variant(SectionKind::Work, 1);
    assert_eq!(doc.work.variants.len(), 1);

    let matrix = PresetMatrix::new(PathBuf::from("/dummy/path"), doc);
    assert_eq!(matrix.variant_label(SectionKind::Work, infra), None);
    assert_eq!(
        matrix.doc.unresolved_pins(0),
        vec![SectionKind::Work],
        "the preset still names the cut that was deleted, and says which"
    );
}

/// The matrix opens on the preset that explains the working copy, not always
/// column zero, and changes its mark from ACTIVE to EDITED when no preset is an
/// exact match.
#[test]
fn the_working_copy_marks_the_exact_or_nearest_preset() {
    let mut doc = ResumeDoc::default();
    doc.add_preset("Base");
    doc.add_variant(SectionKind::Work);
    doc.add_preset("Work tailored");

    let exact = PresetMatrix::new(PathBuf::from("/dummy/path"), doc.clone());
    assert_eq!(exact.active_preset_idx, 1);
    assert_eq!(exact.working_copy_mark(1), Some("ACTIVE"));
    assert_eq!(exact.working_copy_mark(0), None);

    doc.add_variant(SectionKind::Skills);
    let edited = PresetMatrix::new(PathBuf::from("/dummy/path"), doc);
    assert_eq!(edited.active_preset_idx, 1);
    assert_eq!(edited.working_copy_mark(1), Some("EDITED"));
    assert_eq!(edited.working_copy_mark(0), None);
}

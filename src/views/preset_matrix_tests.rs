use std::path::PathBuf;

use crate::resume::model::{Preset, ResumeDoc, SectionKind};

use super::preset_matrix::{Choice, PresetMatrix};

/// A document with two Work variants and two presets that disagree about which
/// one to read. Returns the matrix and the two ids, named.
fn two_readings() -> (
    PresetMatrix,
    crate::resume::model::VariantId,
    crate::resume::model::VariantId,
) {
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
            based_on: None,
            description: None,
            selection: vec![(SectionKind::Profile, base), (SectionKind::Work, faang)],
            hidden: Vec::new(),
            order: Vec::new(),
            titles: Vec::new(),
        },
        Preset {
            name: "Preset B".into(),
            based_on: None,
            description: None,
            selection: vec![(SectionKind::Profile, base), (SectionKind::Work, startup)],
            hidden: Vec::new(),
            order: Vec::new(),
            titles: Vec::new(),
        },
    ];

    (
        PresetMatrix::new(PathBuf::from("/dummy/path"), doc),
        faang,
        startup,
    )
}

/// Every preset is a column, and the working copy is the first of them.
///
/// The screen used to pick two presets and call the rest invisible; a document
/// with three readings could not be seen at once at all.
#[test]
fn every_preset_is_a_column_behind_the_working_copy() {
    let (matrix, faang, startup) = two_readings();
    let columns = matrix.columns();

    assert_eq!(columns.len(), 3, "the working copy plus both presets");
    assert_eq!(columns[0].preset, None);
    assert_eq!(columns[0].name, "Now");
    assert_eq!(columns[1].name, "Preset A");
    assert_eq!(columns[2].name, "Preset B");

    assert_eq!(
        matrix.choice(&columns[1], SectionKind::Work),
        Choice::Pin(faang)
    );
    assert_eq!(
        matrix.choice(&columns[2], SectionKind::Work),
        Choice::Pin(startup)
    );
    // The document was left on the second variant, so that is what `Now` says —
    // it reads the document rather than a preset's claim about it.
    assert_eq!(
        matrix.choice(&columns[0], SectionKind::Work),
        Choice::Pin(startup)
    );
    assert_eq!(
        matrix.cell_text(SectionKind::Work, Choice::Pin(faang)),
        "FAANG"
    );
}

/// A row differs when any column departs from the working copy, and only then.
#[test]
fn a_row_differs_when_a_column_departs_from_the_working_copy() {
    let (mut matrix, _faang, _startup) = two_readings();

    assert!(
        matrix.row_differs(SectionKind::Work),
        "Preset A reads FAANG while the document reads Startup"
    );
    assert!(
        !matrix.row_differs(SectionKind::Profile),
        "every column pins the one Profile variant there is"
    );

    // `differences only` drops exactly the agreeing rows, and never the
    // disagreeing one.
    matrix.differences_only = true;
    let rows = matrix.rows();
    assert!(rows.contains(&SectionKind::Work));
    assert!(!rows.contains(&SectionKind::Profile));

    matrix.differences_only = false;
    assert_eq!(matrix.rows(), matrix.doc.sections());
}

/// Hiding a section in one preset is a difference like any other, and reads as
/// itself rather than as a missing variant (O-13).
#[test]
fn hiding_a_section_in_one_preset_is_a_difference() {
    let (mut matrix, _faang, _startup) = two_readings();
    matrix.doc.presets[1].hidden.push(SectionKind::Certificates);

    let columns = matrix.columns();
    assert_eq!(
        matrix.choice(&columns[2], SectionKind::Certificates),
        Choice::Hidden
    );
    assert!(matrix.row_differs(SectionKind::Certificates));
    assert_eq!(
        matrix.cell_text(SectionKind::Certificates, Choice::Hidden),
        "— hidden —"
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
        matrix.cell_text(SectionKind::Work, Choice::Pin(infra)),
        "— deleted variant —"
    );
    assert_eq!(
        matrix.doc.unresolved_pins(0),
        vec![SectionKind::Work],
        "the preset still names the cut that was deleted, and says which"
    );
}

/// The column marks come from the document, so the grid and the editor's
/// toolbar cannot disagree about which reading is in front of you.
#[test]
fn the_working_copy_marks_the_exact_or_nearest_preset() {
    let mut doc = ResumeDoc::default();
    doc.add_preset("Base");
    doc.add_variant(SectionKind::Work);
    doc.add_preset("Work tailored");

    let exact = PresetMatrix::new(PathBuf::from("/dummy/path"), doc.clone());
    assert_eq!(exact.working_copy_mark(1), Some("ACTIVE"));
    assert_eq!(exact.working_copy_mark(0), None);

    doc.add_variant(SectionKind::Skills);
    let edited = PresetMatrix::new(PathBuf::from("/dummy/path"), doc);
    assert_eq!(edited.working_copy_mark(1), Some("EDITED"));
    assert_eq!(edited.working_copy_mark(0), None);
}

/// Past three presets the agreeing rows are noise, so the toggle starts on.
/// At two or three they are the context that makes the differences legible.
#[test]
fn differences_only_defaults_by_how_many_presets_there_are() {
    let mut doc = ResumeDoc::default();
    for n in 0..3 {
        doc.add_preset(format!("Preset {n}"));
    }
    let three = PresetMatrix::new(PathBuf::from("/dummy/path"), doc.clone());
    assert!(!three.differences_only);

    doc.add_preset("Preset 4");
    let four = PresetMatrix::new(PathBuf::from("/dummy/path"), doc);
    assert!(four.differences_only);
}

/// Nothing is guessed. A column the compiler has not measured yet prints no
/// page line at all, which is the difference between "not known" and a number
/// that might be wrong.
#[test]
fn a_column_says_nothing_about_pages_until_it_is_measured() {
    let (mut matrix, _faang, _startup) = two_readings();
    assert_eq!(matrix.pages_line(Some(0)), None);
    assert!(!matrix.overflows(Some(0)));

    matrix.pages.insert(Some(0), geometry(1, 0.0, Some(14.0)));
    assert_eq!(matrix.pages_line(Some(0)).as_deref(), Some("1 page"));
    assert!(!matrix.overflows(Some(0)));
}

/// Overflow is said in lines, because that is the unit somebody trims in, and
/// it is rounded up: half a line over is still a line that does not fit.
#[test]
fn a_reading_that_does_not_fit_says_how_far_over_it_runs() {
    let (mut matrix, _faang, _startup) = two_readings();

    matrix.pages.insert(Some(0), geometry(2, 70.0, Some(14.0)));
    assert_eq!(
        matrix.pages_line(Some(0)).as_deref(),
        Some("2 pages · 5 lines over")
    );
    assert!(matrix.overflows(Some(0)));

    matrix.pages.insert(Some(1), geometry(2, 8.0, Some(14.0)));
    assert_eq!(
        matrix.pages_line(Some(1)).as_deref(),
        Some("2 pages · 1 line over")
    );

    // Measured as overflowing, but with too little text to average a line
    // height from: the page count is still true, so it is still said.
    matrix.pages.insert(None, geometry(2, 70.0, None));
    assert_eq!(matrix.pages_line(None).as_deref(), Some("2 pages"));
}

/// Counts, never a rate, and nothing at all for a reading nothing went out
/// under — an empty column header beats `sent 0 · 0 interviews`.
#[test]
fn the_record_line_counts_and_never_computes_a_rate() {
    use crate::resume::outcomes::PresetRecord;

    let (mut matrix, _faang, _startup) = two_readings();
    assert_eq!(matrix.record_line(Some(0)), None);

    matrix.records.insert(
        0,
        PresetRecord {
            sent: 11,
            interviewed: 4,
        },
    );
    assert_eq!(
        matrix.record_line(Some(0)).as_deref(),
        Some("sent 11 · 4 interviews")
    );

    matrix.records.insert(
        1,
        PresetRecord {
            sent: 3,
            interviewed: 1,
        },
    );
    assert_eq!(
        matrix.record_line(Some(1)).as_deref(),
        Some("sent 3 · 1 interview")
    );

    matrix.records.insert(
        0,
        PresetRecord {
            sent: 2,
            interviewed: 0,
        },
    );
    assert_eq!(matrix.record_line(Some(0)).as_deref(), Some("sent 2"));

    // The working copy is not a named reading, so nothing was sent under it.
    assert_eq!(matrix.record_line(None), None);
}

/// A `PageGeometry` as the compiler would report one.
fn geometry(
    pages: usize,
    overflow_pt: f64,
    line_advance_pt: Option<f64>,
) -> crate::typst_engine::PageGeometry {
    crate::typst_engine::PageGeometry {
        page_count: pages,
        page_height_pt: 842.0,
        column_pt: 700.0,
        last_page_used_pt: 100.0,
        last_page_content_top_pt: 40.0,
        overflow_pt,
        line_advance_pt,
    }
}
/// Order and headings are reading-level differences: the grid reports them
/// without turning either into another per-cell editor.
#[test]
fn order_and_heading_differences_are_visible_in_their_rows() {
    let mut doc = ResumeDoc {
        section_order: vec![
            SectionKind::Skills,
            SectionKind::Profile,
            SectionKind::Work,
            SectionKind::Education,
            SectionKind::Certificates,
            SectionKind::Organizations,
        ],
        ..Default::default()
    };
    doc.set_section_title(SectionKind::Skills, "Engineering");
    doc.add_preset("Engineering first");

    doc.section_order = vec![
        SectionKind::Work,
        SectionKind::Profile,
        SectionKind::Education,
        SectionKind::Skills,
        SectionKind::Certificates,
        SectionKind::Organizations,
    ];
    doc.set_section_title(SectionKind::Skills, "");
    doc.set_section_title(SectionKind::Work, "Experience");
    doc.add_preset("Experience first");

    let matrix = PresetMatrix::new(PathBuf::from("/dummy/path"), doc);
    let columns = matrix.columns();

    assert_eq!(
        matrix.section_heading(&columns[1], SectionKind::Skills),
        Some("Engineering".into())
    );
    assert_eq!(
        matrix.section_heading(&columns[2], SectionKind::Work),
        Some("Experience".into())
    );
    assert_eq!(
        matrix.section_number(&columns[1], SectionKind::Skills),
        Some(1)
    );
    assert_eq!(
        matrix.section_number(&columns[2], SectionKind::Skills),
        Some(4)
    );
    assert!(matrix.heading_differs(SectionKind::Work));
    assert!(matrix.heading_differs(SectionKind::Skills));
    assert!(matrix.order_differs(SectionKind::Skills));
    assert!(matrix.row_differs(SectionKind::Work));
    assert!(matrix.row_differs(SectionKind::Skills));
    assert!(matrix.cell_differs(&columns[1], SectionKind::Skills));
    assert!(!matrix.cell_differs(&columns[2], SectionKind::Work));
}

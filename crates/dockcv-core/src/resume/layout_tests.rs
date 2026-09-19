use crate::resume::model::*;

#[test]
fn default_layout_matches_the_old_hard_coded_preamble() {
    // resume/template.rs's old `PREAMBLE` constant, transcribed as the
    // default's expected values: `margin: (x: 1.6cm, top: 1.4cm, bottom:
    // 1.4cm)`, `size: 10pt` (= 100%), `leading: 0.62em`, paper "a4".
    let layout = LayoutSettings::default();
    assert_eq!(layout.page_size, PageSize::A4);
    assert_eq!(layout.page_size.typst_paper_name(), "a4");
    assert_eq!(layout.text_scale_pct, 100);
    assert_eq!(layout.leading_em, 0.62);
    assert_eq!(layout.margins.x_mm, 16.0);
    assert_eq!(layout.margins.top_mm, 14.0);
    assert_eq!(layout.margins.bottom_mm, 14.0);
    // sanitized() must not perturb an already-valid default.
    assert_eq!(layout.sanitized(), layout);
}

#[test]
fn letter_uses_the_typst_us_letter_preset_name() {
    assert_eq!(PageSize::Letter.typst_paper_name(), "us-letter");
}

#[test]
fn sanitized_clamps_out_of_range_values() {
    let wild = LayoutSettings {
        page_size: PageSize::A4,
        font: DocumentFont::default(),
        date_format: Default::default(),
        skills: Default::default(),
        entries: Default::default(),
        header: Default::default(),
        headings: Default::default(),
        sizes: TypeSizes {
            name_pt: 400.0,
            title_pt: -90.0,
            heading_pt: 0.0,
            entry_pt: 0.0,
        },
        text_scale_pct: 0,
        leading_em: -3.0,
        margins: Margins {
            x_mm: -10.0,
            top_mm: 10_000.0,
            bottom_mm: f32::NAN.max(0.0), // still exercises the clamp path
        },
    };
    let safe = wild.sanitized();
    assert!(safe.text_scale_pct >= 50 && safe.text_scale_pct <= 200);
    assert!(safe.leading_em >= 0.3 && safe.leading_em <= 1.5);
    assert!(safe.margins.x_mm >= 3.0);
    assert!(safe.margins.top_mm >= 3.0 && safe.margins.top_mm <= 297.0 / 3.0);
    assert!(safe.margins.bottom_mm >= 3.0);
    let (lo, hi) = TypeSizes::DELTA_RANGE;
    assert!(safe.sizes.name_pt <= hi && safe.sizes.title_pt >= lo);
    // The clamp exists to keep the *rendered* size positive, so check the
    // thing that actually reaches Typst rather than only the offset.
    assert!(TypeSizes::resolve(safe.base_size_pt(), safe.sizes.title_pt) >= TypeSizes::MIN_PT);
}

#[test]
fn layout_round_trips_through_toml() {
    let layout = LayoutSettings {
        page_size: PageSize::Letter,
        font: DocumentFont::default(),
        date_format: Default::default(),
        skills: Default::default(),
        entries: Default::default(),
        header: Default::default(),
        headings: Default::default(),
        sizes: TypeSizes {
            name_pt: 12.5,
            title_pt: 2.0,
            heading_pt: -1.5,
            entry_pt: 0.5,
        },
        text_scale_pct: 107,
        leading_em: 0.7,
        margins: Margins {
            x_mm: 20.0,
            top_mm: 18.0,
            bottom_mm: 18.0,
        },
    };
    let text = toml::to_string_pretty(&layout).expect("serializes");
    let back: LayoutSettings = toml::from_str(&text).expect("round-trips");
    assert_eq!(back, layout);
}

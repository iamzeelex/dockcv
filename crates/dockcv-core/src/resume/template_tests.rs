//! The generator end to end: page presets, sanitizing, links, custom sections.
//!
//! Split out of `template.rs` by C15, along with the date and empty-document
//! suites that used to sit beside it.

use crate::resume::model::{Margins, PageSize, TypeSizes};
use crate::resume::template::*;
use crate::resume::template_page::*;

#[test]
fn default_layout_renders_the_old_hard_coded_values() {
    let source = generate_with_layout(&Resume::default(), &LayoutSettings::default());
    assert!(source.contains(r#"paper: "a4""#));
    assert!(source.contains("margin: (x: 16mm, top: 14mm, bottom: 14mm)"));
    assert!(source.contains("size: 10pt"));
    assert!(source.contains("leading: 0.62em"));
    // generate() with no layout in hand must match the explicit default.
    assert_eq!(source, generate(&Resume::default()));
}

#[test]
fn letter_uses_the_us_letter_paper_preset() {
    let layout = LayoutSettings {
        page_size: PageSize::Letter,
        font: Default::default(),
        date_format: Default::default(),
        ..LayoutSettings::default()
    };
    let source = generate_with_layout(&Resume::default(), &layout);
    assert!(source.contains(r#"paper: "us-letter""#));
}

#[test]
fn text_scale_and_leading_render_into_the_preamble() {
    let layout = LayoutSettings {
        text_scale_pct: 107,
        leading_em: 0.7,
        margins: Margins {
            x_mm: 20.0,
            top_mm: 18.0,
            bottom_mm: 18.0,
        },
        ..LayoutSettings::default()
    };
    let source = generate_with_layout(&Resume::default(), &layout);
    assert!(source.contains("size: 10.7pt"), "{source}");
    assert!(source.contains("leading: 0.7em"), "{source}");
    assert!(source.contains("margin: (x: 20mm, top: 18mm, bottom: 18mm)"));
}

#[test]
fn invalid_layout_is_sanitized_before_it_reaches_typst_source() {
    // A hand-edited or corrupted vault file could hold any of these; the
    // generated source must still be renderable, not a Typst error.
    let layout = LayoutSettings {
        page_size: PageSize::A4,
        font: Default::default(),
        date_format: Default::default(),
        skills: Default::default(),
        entries: Default::default(),
        header: Default::default(),
        headings: Default::default(),
        show_link_marks: true,
        sizes: TypeSizes {
            name_pt: 900.0,
            title_pt: -900.0,
            heading_pt: 0.0,
            entry_pt: 0.0,
        },
        text_scale_pct: 0,
        leading_em: -1.0,
        margins: Margins {
            x_mm: -5.0,
            top_mm: 900.0,
            bottom_mm: 900.0,
        },
    };
    let source = generate_with_layout(&Resume::default(), &layout);
    assert!(!source.contains("size: 0pt"));
    assert!(!source.contains("leading: -1em"));
    assert!(!source.contains("margin: (x: -5mm"));
}

/// The page-size switch has to survive the compiler, not just the string.
///
/// Asserting that `"us-letter"` appears in the generated source proves the
/// preset name is spelled right; it does not prove Typst accepts it or that
/// the page actually changed. A4 is 297mm tall and Letter 279.4mm, so a real
/// compile must report measurably different page heights.
/// The bug this guards: a bullet carrying the CV's own email compiled to
/// `label <example.comcritical> does not exist`, because Typst reads `@x` as a
/// reference. `C#` fails the same way through code mode. Asserting on the
/// generated source would not have caught it — only the compiler knows.
#[test]
fn prose_that_looks_like_typst_syntax_still_compiles() {
    use crate::resume::model::Work;
    use crate::typst_engine::TypstEngine;

    let resume = Resume {
        work: vec![Work {
            position: "Engineer".into(),
            highlights: vec![
                "Reachable at albert@example.com for critical incidents".into(),
                "Ported the C# service and its #tags to Rust".into(),
                "Cut latency [p99] by 40%".into(),
            ],
            ..Default::default()
        }],
        ..Default::default()
    };

    let engine = TypstEngine::new(generate(&resume));
    let attempt = engine.compile_with_diagnostics(1.0);
    assert!(
        attempt.result.is_ok(),
        "an email in a bullet must not break the document: {:?}",
        attempt.diagnostics
    );
}

/// The bug this guards: every call site in the app rendered with
/// `LayoutSettings::default()`, so the font picker, page size, margins,
/// text scale and date format all reached the model, were saved, and never
/// arrived in the preview. The engine's own tests passed throughout —
/// they called `generate_with_layout` directly, which is not the path the
/// app took.
#[test]
fn the_documents_own_layout_reaches_the_source_the_app_renders() {
    use crate::resume::model::{DocumentFont, PageSize};

    let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
    doc.layout.font = DocumentFont::ALL
        .iter()
        .copied()
        .find(|f| *f != DocumentFont::default())
        .expect("more than one font is offered");
    doc.layout.page_size = PageSize::Letter;

    let source = generate_for(&doc);
    assert!(
        source.contains(doc.layout.font.family()),
        "the chosen font never reached the source"
    );
    assert!(
        source.contains(r#"paper: "us-letter""#),
        "page size was dropped"
    );
    assert_ne!(
        source,
        generate(&doc.compose()),
        "generate_for must differ from the default-layout path"
    );
}

/// A skill group with no name printed a bare colon in front of its list.
/// LinkedIn exports have no categories at all, so this is the ordinary
/// shape, not an edge case.
#[test]
fn an_unnamed_skill_group_prints_no_label_and_no_colon() {
    use crate::resume::model::SkillGroup;

    let named = Resume {
        skills: vec![SkillGroup {
            name: "Languages".into(),
            keywords: vec!["Rust".into()],
        }],
        ..Default::default()
    };
    let unnamed = Resume {
        skills: vec![SkillGroup {
            name: String::new(),
            keywords: vec!["Rust".into()],
        }],
        ..Default::default()
    };

    assert!(generate(&named).contains(r#"name: "Languages""#));
    // The empty name *is* emitted — the decision belongs to the renderer,
    // not the codegen, and the guard is one branch in `RENDERER`. A source
    // test cannot see a glyph, so what is asserted here is that the branch
    // exists and that both shapes compile; the missing colon is visible on
    // the page.
    let source = generate(&unnamed);
    let at = source.rfind("skills: (").expect("a skills array");
    assert!(source[at..].contains(r#"name: """#));

    for resume in [&named, &unnamed] {
        let engine = crate::typst_engine::TypstEngine::new(generate(resume));
        assert!(
            engine.compile_with_diagnostics(1.0).result.is_ok(),
            "skills must render either way"
        );
    }

    // The actual bug (E-36) was a *printed* `:` in front of a list with no
    // category, and neither the model nor the generated source can show
    // that. The page can: with no name, the mark must make no difference
    // at all, so rendering with a colon and with no mark has to produce
    // the same pixels. This used to be a substring test against the
    // renderer's source, which said nothing about the output and broke the
    // moment the branch was rewritten with the same behaviour.
    let pixels_with = |mark: crate::resume::model::CategoryMark| {
        let layout = LayoutSettings {
            skills: crate::resume::model::SkillsLayout {
                mark,
                ..Default::default()
            },
            ..LayoutSettings::default()
        };
        let engine =
            crate::typst_engine::TypstEngine::new(generate_with_layout(&unnamed, &layout));
        engine.compile_to_pixels(1.0).expect("compiles").0.rgba
    };
    assert_eq!(
        pixels_with(crate::resume::model::CategoryMark::Colon),
        pixels_with(crate::resume::model::CategoryMark::None),
        "an unnamed group printed its mark — the bare colon is back"
    );

    // And with a name, the mark must change the page, or the control is
    // decoration.
    let named_pixels = |mark: crate::resume::model::CategoryMark| {
        let layout = LayoutSettings {
            skills: crate::resume::model::SkillsLayout {
                mark,
                ..Default::default()
            },
            ..LayoutSettings::default()
        };
        let engine =
            crate::typst_engine::TypstEngine::new(generate_with_layout(&named, &layout));
        engine.compile_to_pixels(1.0).expect("compiles").0.rgba
    };
    assert_ne!(
        named_pixels(crate::resume::model::CategoryMark::Colon),
        named_pixels(crate::resume::model::CategoryMark::Dash),
        "the category mark never reached the page"
    );
}

/// A certificate's link was emitted into the dictionary and dropped by the
/// renderer — stored, saved, editable, invisible. Asserting on the model
/// would have passed; this asserts on the generated source, which is where
/// it went missing.
#[test]
fn a_certificates_link_reaches_the_page() {
    use crate::resume::model::Certificate;

    let resume = Resume {
        certificates: vec![Certificate {
            name: "Reservoir Engineering".into(),
            issuer: "SPE".into(),
            url: "https://spe.org/cert/42".into(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let source = generate(&resume);
    assert!(
        source.contains("https://spe.org/cert/42"),
        "the url is not emitted"
    );
    assert!(
        RENDERER.contains(r#"let u = c.at("url", default: "")"#),
        "the renderer no longer reads a certificate's url"
    );
    let engine = crate::typst_engine::TypstEngine::new(source);
    assert!(engine.compile_with_diagnostics(1.0).result.is_ok());
}

/// An entry's URL reaches the generated dict as two values: the text the
/// page prints, exactly as typed, and the absolute target a viewer follows.
///
/// That the target then becomes a working PDF annotation, and that the
/// indicator mark is typeset in a face that has the glyph, are asserted
/// where those artefacts are made — `typst_engine::font_tests`. Asserting
/// them here, against the text of `RENDERER`, is what let G1 ship with
/// every link dead and this test green.
#[test]
fn entry_links_reach_the_page_as_text_and_target() {
    use crate::resume::model::{Education, Resume, Volunteer, Work};

    let resume = Resume {
        work: vec![Work {
            name: "Acme Corp".into(),
            position: "Senior Engineer".into(),
            url: "acme.example.com".into(),
            ..Default::default()
        }],
        education: vec![Education {
            institution: "MIT".into(),
            study_type: "B.S.".into(),
            url: "https://mit.edu".into(),
            ..Default::default()
        }],
        volunteer: vec![Volunteer {
            organization: "Red Cross".into(),
            // A URL field can hold anything someone typed into it.
            url: "ask me".into(),
            position: "Volunteer".into(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let source = generate(&resume);
    for (url, href) in [
        ("acme.example.com", Some("https://acme.example.com")),
        ("https://mit.edu", Some("https://mit.edu")),
        ("ask me", None),
    ] {
        assert!(
            source.contains(&format!(r#"url: "{url}""#)),
            "{url:?} must print as the user typed it, got:\n{source}"
        );
        match href {
            Some(href) => assert!(
                source.contains(&format!(r#"href: "{href}""#)),
                "{url:?} must carry the target {href:?}, got:\n{source}"
            ),
            // Nothing followable, so nothing to follow: the text prints and
            // the entry is not dressed up as a link.
            None => assert!(
                !source.contains(r#"href: "ask"#),
                "free text became a link target, got:\n{source}"
            ),
        }
    }

    let engine = crate::typst_engine::TypstEngine::new(source);
    let report = engine.compile_with_diagnostics(1.0);
    assert!(
        report.result.is_ok(),
        "document with links must compile: {:?}",
        report.diagnostics
    );
}

#[test]
fn ats_safe_profile_turns_the_link_mark_off_and_compiles() {
    use crate::resume::model::{Education, Resume};
    use crate::resume::ATS_SAFE_PROFILE;

    let resume = Resume {
        education: vec![Education {
            institution: "MIT".into(),
            study_type: "B.S.".into(),
            url: "https://mit.edu".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut doc = ResumeDoc::from_resume(resume, "Base");

    assert!(generate_for(&doc).contains("#let show-link-marks = true"));

    doc.layout_profile = Some(ATS_SAFE_PROFILE.to_string());
    let source = generate_for(&doc);
    assert!(source.contains("#let show-link-marks = false"));
    crate::typst_engine::TypstEngine::new(source)
        .compile_to_pdf()
        .expect("ATS-safe profile compiles");
}

#[test]
fn letter_and_a4_compile_to_genuinely_different_pages() {
    use crate::resume::altacv;
    use crate::typst_engine::TypstEngine;

    let resume = altacv::import(altacv::ALTACV_SAMPLE).expect("the sample parses");

    let height_of = |page_size| {
        let layout = LayoutSettings {
            page_size,
            font: Default::default(),
            date_format: Default::default(),
            ..LayoutSettings::default()
        };
        let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
        let (_, geometry) = engine
            .compile_to_pixels(1.0)
            .unwrap_or_else(|e| panic!("{page_size:?} must compile: {e}"));
        geometry.page_height_pt
    };

    let a4 = height_of(PageSize::A4);
    let letter = height_of(PageSize::Letter);

    assert!(
        a4 > letter,
        "A4 ({a4}pt) should be taller than Letter ({letter}pt)"
    );
    // 297mm - 279.4mm = 17.6mm ≈ 49.9pt. Generous tolerance: the point is that
    // the setting reached the compiler, not that we re-derive the constant.
    assert!(
        (a4 - letter - 49.9).abs() < 2.0,
        "the gap should be ~49.9pt, got {:.1}pt",
        a4 - letter
    );
}

#[test]
fn fmt_measure_strips_trailing_zeros() {
    assert_eq!(fmt_measure(16.0), "16");
    assert_eq!(fmt_measure(0.62), "0.62");
    assert_eq!(fmt_measure(10.7), "10.7");
    assert_eq!(fmt_measure(0.0), "0");
}

/// A document with no custom sections (every résumé written before D-9)
/// must not gain a `customSections` entry in its `#let cv = (..)` dict —
/// `RENDERER` itself references the key name unconditionally (it's fixed
/// Typst source, present regardless of data), so the check is on the
/// *data* the dict-builder emits, not on the bare substring.
/// Renaming must reach the exported document, not just the panel — and an
/// untouched document must generate exactly what it did before.
#[test]
fn a_renamed_section_reaches_the_generated_source() {
    use crate::resume::model::{ResumeDoc, SectionKind};

    let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
    doc.work.active_mut().push(Default::default());

    let before = generate(&doc.compose());
    // The *renderer* mentions `sectionTitles` (it reads the key); what must
    // be absent is the emitted data table, which carries the leading indent.
    assert!(
        !before.contains("  sectionTitles: ("),
        "an untouched document must not gain a titles table"
    );

    doc.set_section_title(SectionKind::Work, "Engineering");
    let after = generate(&doc.compose());
    assert!(
        after.contains("Engineering"),
        "the new heading must reach the source"
    );
    assert_ne!(before, after);

    // Blanking it clears the override rather than printing an empty heading.
    doc.set_section_title(SectionKind::Work, "   ");
    assert_eq!(
        generate(&doc.compose()),
        before,
        "a blank name returns to the default"
    );
}

#[test]
fn no_custom_sections_means_no_custom_sections_key_in_the_dict() {
    let source = generate(&Resume::default());
    assert!(!source.contains("customSections: ("));
}

/// A custom section (D-9) reaches the generated Typst source with its
/// title and entry fields, and — the real proof, not just string
/// matching — the document still compiles.
#[test]
fn custom_section_renders_and_compiles() {
    use crate::resume::model::CustomEntry;
    use crate::typst_engine::TypstEngine;

    // Built through the document rather than by hand: an id comes from
    // `ResumeDoc`'s counter, and this is the path the app actually walks.
    let mut resume = Resume::default();
    resume.basics.name = "Test Person".into();
    let mut doc = ResumeDoc::from_resume(resume, "Base");
    let id = doc.add_custom_section("Publications");
    let content = doc.custom_section_mut(id).unwrap().content.active_mut();
    content.clear();
    content.push(CustomEntry {
        title: "A Paper on Something".into(),
        subtitle: "Journal of Examples".into(),
        start_date: "2024".into(),
        end_date: Default::default(),
        url: "https://example.com/paper".into(),
        highlights: vec!["Peer reviewed".into()],
    });

    let source = generate(&doc.compose());
    assert!(source.contains("customSections"));
    assert!(source.contains("A Paper on Something"));
    assert!(source.contains("Publications"));

    let engine = TypstEngine::new(source);
    let (_, geometry) = engine
        .compile_to_pixels(1.0)
        .expect("a document with a custom section must compile");
    assert_eq!(geometry.page_count, 1);
}

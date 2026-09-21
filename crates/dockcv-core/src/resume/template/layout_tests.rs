//! The layout-control matrix: header, type scale, entries, skills.
//!
//! Split out of `template.rs` by C15. Every one of these asks the same pair of
//! questions of a different setting — that changing it changes the page, and
//! that leaving it alone changes nothing — which is what keeps a default from
//! quietly becoming a decision.

use crate::resume::template::*;

/// The header controls, held to the same two rules as the entry ones:
/// each has to change the page, and the default has to leave a document
/// exactly as it was.
#[test]
fn every_header_control_changes_the_page_and_the_default_changes_nothing() {
    use crate::resume::model::{
        Basics, ContactLayout, HeaderAlign, HeaderLayout, SkillSeparator,
    };
    use crate::typst_engine::TypstEngine;

    let resume = Resume {
        basics: Basics {
            name: "Albert Einstein".into(),
            label: "Staff Engineer".into(),
            location: "Barcelona, Spain".into(),
            email: "s@example.com".into(),
            phone: "+34 000 000 000".into(),
            url: "https://example.com".into(),
            ..Default::default()
        },
        ..Default::default()
    };

    let pixels = |header: HeaderLayout| {
        let layout = LayoutSettings {
            header,
            ..LayoutSettings::default()
        };
        let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
        engine.compile_to_pixels(1.0).expect("compiles").0.rgba
    };

    let base = pixels(HeaderLayout::default());
    let engine = TypstEngine::new(generate(&resume));
    assert_eq!(
        engine.compile_to_pixels(1.0).expect("compiles").0.rgba,
        base,
        "the default header is not what `generate` produces"
    );

    let variants: [(&str, HeaderLayout); 4] = [
        (
            "left aligned",
            HeaderLayout {
                align: HeaderAlign::Left,
                ..Default::default()
            },
        ),
        (
            "stacked contacts",
            HeaderLayout {
                contacts: ContactLayout::Stacked,
                ..Default::default()
            },
        ),
        (
            "two columns",
            HeaderLayout {
                contacts: ContactLayout::Columns,
                ..Default::default()
            },
        ),
        (
            "bullet separator",
            HeaderLayout {
                separator: SkillSeparator::Bullet,
                ..Default::default()
            },
        ),
    ];
    for (what, header) in variants {
        assert_ne!(
            pixels(header),
            base,
            "{what} rendered identically to the default — the control never reached the page"
        );
    }

    // The separator is only a choice when the details share a line, which
    // is why the rail hides it otherwise. If that ever stops being true
    // here, the rail is hiding a live control.
    assert_eq!(
        pixels(HeaderLayout {
            contacts: ContactLayout::Stacked,
            separator: SkillSeparator::Bullet,
            ..Default::default()
        }),
        pixels(HeaderLayout {
            contacts: ContactLayout::Stacked,
            ..Default::default()
        }),
        "the separator changed a stacked header — the rail hides a control that does something"
    );
}

/// The per-element sizes, held to the two rules the other layout groups
/// are: each control has to reach the page, and a document nobody has
/// touched has to render exactly as it did before the controls existed.
///
/// The second half is checked on the *numbers that reach Typst* rather
/// than on pixels, because the equality it guards is arithmetic: at the
/// default base of 10pt the six offsets have to resolve to the six
/// literals the renderer used to spell out. (That the substitution is
/// otherwise a no-op was confirmed once by rendering the sample against
/// both trees — identical bytes at 100%.)
#[test]
fn every_type_size_changes_the_page_and_the_default_resolves_to_the_old_literals() {
    use crate::resume::model::{Basics, TypeSizes, Work};
    use crate::typst_engine::TypstEngine;

    let source = generate(&Resume::default());
    for expected in [
        "#let size-name = 20pt",
        "#let size-title = 12pt",
        "#let size-heading = 9pt",
        "#let size-entry = 10pt",
        "#let size-meta = 9pt",
        "#let size-pill = 8.5pt",
    ] {
        assert!(
            source.contains(expected),
            "a default document no longer sets `{expected}` — every CV \
             written before this control existed has been re-typeset"
        );
    }

    let resume = Resume {
        basics: Basics {
            name: "Albert Einstein".into(),
            label: "Staff Engineer".into(),
            ..Default::default()
        },
        work: vec![Work {
            name: "Acme".into(),
            position: "Staff Engineer".into(),
            start_date: "2022-01".into(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let pixels = |sizes: TypeSizes| {
        let layout = LayoutSettings {
            sizes,
            ..LayoutSettings::default()
        };
        let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
        engine.compile_to_pixels(1.0).expect("compiles").0.rgba
    };

    let base = pixels(TypeSizes::default());
    let variants: [(&str, TypeSizes); 4] = [
        (
            "name",
            TypeSizes {
                name_pt: 14.0,
                ..Default::default()
            },
        ),
        (
            "professional title",
            TypeSizes {
                title_pt: 5.0,
                ..Default::default()
            },
        ),
        (
            "section headings",
            TypeSizes {
                heading_pt: 3.0,
                ..Default::default()
            },
        ),
        (
            "entry title",
            TypeSizes {
                entry_pt: 3.0,
                ..Default::default()
            },
        ),
    ];
    for (what, sizes) in variants {
        assert_ne!(
            pixels(sizes),
            base,
            "the {what} size rendered identically to the default — the \
             control never reached the page"
        );
    }
}

/// Text scale scales the *document*, not only its paragraphs.
///
/// This is the behaviour the offsets bought and the one thing about them
/// that is not backwards-compatible: before, the name was a flat 20pt at
/// every scale, so turning the body down to 85% widened the size contrast
/// instead of shrinking the page. Rendered documents at a non-default
/// scale therefore changed once, deliberately.
#[test]
fn every_size_follows_the_base_rather_than_standing_still() {
    let smaller = generate_with_layout(
        &Resume::default(),
        &LayoutSettings {
            text_scale_pct: 90,
            ..LayoutSettings::default()
        },
    );
    // 9pt base: 9+10, 9+2, 9−1, 9+0, 9−1, 9−1.5.
    for expected in [
        "#let size-name = 19pt",
        "#let size-title = 11pt",
        "#let size-heading = 8pt",
        "#let size-entry = 9pt",
        "#let size-pill = 7.5pt",
    ] {
        assert!(
            smaller.contains(expected),
            "at 90% the document does not set `{expected}` — some element \
             is pinned to an absolute size again"
        );
    }
}

/// Every entry control has to reach the page, and the default has to
/// leave a document rendering exactly as it did before the controls
/// existed — the second half is what stops a layout feature from quietly
/// re-typesetting everyone's CV.
///
/// Compared on pixels rather than on the generated source: E-32 was a
/// whole layout rail that reached the model, was saved, and never arrived
/// on the page while every source-level test stayed green.
#[test]
fn every_entry_control_changes_the_page_and_the_default_changes_nothing() {
    use crate::resume::model::{
        BulletGlyph, Emphasis, EntryLayout, MetaOrder, MetaPosition, Work,
    };
    use crate::typst_engine::TypstEngine;

    let resume = Resume {
        work: vec![Work {
            name: "Acme".into(),
            position: "Staff Engineer".into(),
            location: "Barcelona, Spain".into(),
            // Dates matter to this fixture: `meta` drops empty parts, so
            // an undated entry has one thing to print and reversing the
            // order of one thing proves nothing.
            start_date: "2022-01".into(),
            end_date: "2024-06".into(),
            summary: "Recovered reliable state from unreliable measurement.".into(),
            highlights: vec![
                "Cut p99 latency in half.".into(),
                "Rewrote the ingest.".into(),
            ],
            ..Default::default()
        }],
        ..Default::default()
    };

    let pixels = |entries: EntryLayout| {
        let layout = LayoutSettings {
            entries,
            ..LayoutSettings::default()
        };
        let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
        engine.compile_to_pixels(1.0).expect("compiles").0.rgba
    };

    let base = pixels(EntryLayout::default());

    // The default must be the old rendering, byte for byte.
    let engine = TypstEngine::new(generate(&resume));
    assert_eq!(
        engine.compile_to_pixels(1.0).expect("compiles").0.rgba,
        base,
        "the default entry layout is not what `generate` produces"
    );

    let variants: [(&str, EntryLayout); 6] = [
        (
            "meta below",
            EntryLayout {
                meta_position: MetaPosition::Below,
                ..Default::default()
            },
        ),
        (
            "place first",
            EntryLayout {
                meta_order: MetaOrder::LocationFirst,
                ..Default::default()
            },
        ),
        (
            "subtitle bold",
            EntryLayout {
                subtitle: Emphasis::Bold,
                ..Default::default()
            },
        ),
        (
            "meta bold",
            EntryLayout {
                meta: Emphasis::Bold,
                ..Default::default()
            },
        ),
        (
            "dash bullets",
            EntryLayout {
                bullet: BulletGlyph::Dash,
                ..Default::default()
            },
        ),
        (
            "indented body",
            EntryLayout {
                indent_body: true,
                ..Default::default()
            },
        ),
    ];

    for (what, entries) in variants {
        assert_ne!(
            pixels(entries),
            base,
            "{what} rendered identically to the default — the control never reached the page"
        );
    }
}

/// The point of the options: a Skills section with sixty terms is the
/// thing that pushes a CV onto a second page, and the dense settings have
/// to actually *be* denser on the laid-out page — not merely different.
///
/// Measured in points of used page height, from the same geometry the
/// overflow chip reads, because "the source changed" would pass for
/// settings the compiler ignored (E-32).
#[test]
fn the_dense_settings_take_less_page_than_the_roomy_ones() {
    use crate::resume::model::{CategoryMark, RowSpacing, SkillSeparator, SkillsLayout};
    use crate::typst_engine::TypstEngine;

    let mut resume = Resume::default();
    resume.skills = (0..8)
        .map(|i| crate::resume::model::SkillGroup {
            name: format!("Category number {i}"),
            keywords: (0..9).map(|k| format!("Technology {i}-{k}")).collect(),
        })
        .collect();

    let used = |skills: SkillsLayout| {
        let layout = LayoutSettings {
            skills,
            ..LayoutSettings::default()
        };
        let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
        let (_, geometry) = engine.compile_to_pixels(1.0).expect("compiles");
        geometry.page_count as f64 * 1000.0 + geometry.last_page_used_pt
    };

    let roomy = used(SkillsLayout::default());
    let dense = used(SkillsLayout {
        separator: SkillSeparator::Rule,
        mark: CategoryMark::Dash,
        spacing: RowSpacing::Tight,
        ..SkillsLayout::default()
    });

    assert!(
        dense < roomy,
        "the dense settings used {dense} against {roomy} — they cost space instead of saving it"
    );
}

/// A group with no category name is the ordinary shape of a LinkedIn
/// export, and every style has to survive it — a `bubbles` run that
/// emitted an empty pill, or a `grid` with a blank first column, would be
/// the bare-colon bug (E-36) wearing a new hat.
#[test]
fn every_skills_style_survives_a_group_with_no_category() {
    use crate::resume::model::{SkillsLayout, SkillsStyle};
    use crate::typst_engine::TypstEngine;

    let mut resume = Resume::default();
    resume.skills = vec![crate::resume::model::SkillGroup {
        name: String::new(),
        keywords: vec!["Rust".into(), "Kafka".into()],
    }];

    for style in SkillsStyle::ALL {
        let layout = LayoutSettings {
            skills: SkillsLayout {
                style,
                ..SkillsLayout::default()
            },
            entries: Default::default(),
            header: Default::default(),
            ..LayoutSettings::default()
        };
        let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
        assert!(
            engine.compile_to_pdf().is_ok(),
            "{} failed on an unnamed group",
            style.label()
        );
    }
}

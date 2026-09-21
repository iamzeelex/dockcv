//! What a heading does to the page, and what a section may say about itself.
//!
//! Split out of `template.rs` by C15. These are integration tests — most
//! compile a real document and measure it — so they are grouped by the
//! decision they exercise rather than by which function they call.

use crate::resume::model::SectionKind;
use crate::resume::template::*;

// A helper the split moved to a sibling. Named rather than
// glob-imported, so the list says what this reaches across for.
/// A skills style is only real if it reaches the page. Every one of them
/// **compiles**, and each produces a different number of laid-out items —
/// asserting on the generated source would pass for a style the compiler
/// then ignored, which is the mistake E-32 records (the whole layout rail
/// was inert while every source-level test was green).
#[test]
fn every_skills_style_compiles_and_lays_out_differently() {
    use crate::resume::model::{SkillsLayout, SkillsStyle};
    use crate::typst_engine::TypstEngine;

    let mut resume = Resume::default();
    resume.skills = vec![
        crate::resume::model::SkillGroup {
            name: "Languages".into(),
            keywords: vec!["Rust".into(), "Python".into(), "TypeScript".into()],
        },
        crate::resume::model::SkillGroup {
            name: "Infrastructure".into(),
            keywords: vec!["Kubernetes".into(), "Kafka".into()],
        },
    ];

    let mut heights = Vec::new();
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
        let pdf = engine
            .compile_to_pdf()
            .unwrap_or_else(|e| panic!("{} did not compile: {e}", style.label()));
        assert!(
            pdf.starts_with(b"%PDF"),
            "{} produced no PDF",
            style.label()
        );
        heights.push((style, pdf.len()));
    }

    // Not all four need differ from each other — `grid` and `inline` are
    // close by design — but the two that rearrange the words must differ
    // from the default, or nothing was actually applied.
    let of = |s: SkillsStyle| heights.iter().find(|(k, _)| *k == s).unwrap().1;
    assert_ne!(
        of(SkillsStyle::Rows),
        of(SkillsStyle::Bubbles),
        "bubbles rendered identically to inline — the style never reached the page"
    );
    assert_ne!(
        of(SkillsStyle::Rows),
        of(SkillsStyle::Compact),
        "compact rendered identically to inline"
    );
}

/// The first per-section override: a section that prints no heading.
///
/// Held to three rules. It has to take the heading off the page; it has
/// to leave the section's own content there (the whole point is a summary
/// with no "PROFILE" over it, not a missing summary); and a document that
/// never used it has to render exactly as it did.
#[test]
fn a_section_can_print_no_heading_without_losing_its_content() {
    use crate::resume::model::{Basics, Work};
    use crate::typst_engine::TypstEngine;

    let resume = Resume {
        basics: Basics {
            name: "Albert Einstein".into(),
            summary: "Backend engineer with eight years of experience.".into(),
            ..Default::default()
        },
        work: vec![Work {
            name: "Acme".into(),
            position: "Staff Engineer".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut doc = ResumeDoc::from_resume(resume.clone(), "Base");

    let ink = |doc: &ResumeDoc| {
        TypstEngine::new(generate(&doc.compose()))
            .compile_to_pixels(1.0)
            .expect("compiles")
            .0
            .rgba
            .chunks_exact(4)
            .filter(|px| px[0] < 200 || px[1] < 200 || px[2] < 200)
            .count()
    };

    let with_heading = ink(&doc);
    doc.set_heading_printed(SectionKind::Profile, false);
    let without = ink(&doc);
    assert!(
        without < with_heading,
        "hiding the Profile heading drew as much ink as printing it \
         ({without} vs {with_heading})"
    );

    // The summary itself is still there — a rule that would otherwise be
    // satisfied by dropping the section altogether. Compared against the
    // same document with nothing *in* the section, because Profile is the
    // one section `set_hidden` refuses to touch (a CV without a name is
    // not a shorter CV), so there is no "hidden" version to compare with.
    let mut gutted = doc.clone();
    gutted.profile.active_mut().summary.clear();
    assert!(
        ink(&gutted) < without,
        "the section lost its content, not just its heading"
    );

    // Off again, and the document is byte for byte what it was: the row
    // is removed rather than stored as a row of defaults.
    doc.set_heading_printed(SectionKind::Profile, true);
    assert!(
        doc.section_overrides.is_empty(),
        "turning the override off left a row behind: {:?}",
        doc.section_overrides
    );
    assert_eq!(
        generate(&doc.compose()),
        generate(&ResumeDoc::from_resume(resume, "Base").compose()),
        "a document that used and un-used the flag is not what it started as"
    );
}

/// Every section can drop its heading, not just Profile — including a
/// custom one, whose key is its id and not its position.
#[test]
fn the_no_heading_flag_reaches_the_source_for_every_kind_of_section() {
    let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
    let custom = doc.add_custom_section("Publications");

    assert!(
        generate(&doc.compose()).contains("#let no-heading = ()"),
        "an untouched document should carry an empty list"
    );

    for (section, key) in [
        (SectionKind::Profile, "profile"),
        (SectionKind::Skills, "skills"),
        (SectionKind::Organizations, "organizations"),
        (SectionKind::Custom(custom), "custom0"),
    ] {
        doc.set_heading_printed(section, false);
        let source = generate(&doc.compose());
        assert!(
            source.contains(&format!("\"{key}\"")),
            "{section:?} did not reach the source as `{key}`"
        );
        doc.set_heading_printed(section, true);
    }
}

/// Every heading style has to compile, put ink on the page, and differ
/// from every other one — six branches of Typst is six chances for a
/// variant to silently fall through to the same drawing.
#[test]
fn every_heading_style_compiles_and_draws_something_different() {
    use crate::resume::model::{HeadingLayout, HeadingStyle, Work};
    use crate::typst_engine::TypstEngine;
    use std::collections::HashMap;

    let resume = Resume {
        work: vec![Work {
            name: "Acme".into(),
            position: "Staff Engineer".into(),
            start_date: "2022-01".into(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut seen: HashMap<Vec<u8>, &'static str> = HashMap::new();
    for style in HeadingStyle::ALL {
        let layout = LayoutSettings {
            headings: HeadingLayout {
                style,
                ..Default::default()
            },
            ..LayoutSettings::default()
        };
        let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
        let pixels = engine
            .compile_to_pixels(1.0)
            .unwrap_or_else(|e| panic!("{} did not compile: {e}", style.label()))
            .0
            .rgba;
        // `Plain` is the one style with nothing but type, so it is the
        // floor every other style has to draw more than.
        if let Some(other) = seen.insert(pixels, style.label()) {
            panic!("{} rendered identically to {other}", style.label());
        }
    }
}

/// The heading controls, on the same two rules as the rest of the rail:
/// each changes the page, and the default leaves a document alone.
#[test]
fn every_heading_control_changes_the_page_and_the_default_changes_nothing() {
    use crate::resume::model::{HeaderAlign, HeadingCase, HeadingLayout, HeadingStyle, Work};
    use crate::typst_engine::TypstEngine;

    let resume = Resume {
        work: vec![Work {
            name: "Acme".into(),
            position: "Staff Engineer".into(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let pixels = |headings: HeadingLayout| {
        let layout = LayoutSettings {
            headings,
            ..LayoutSettings::default()
        };
        let engine = TypstEngine::new(generate_with_layout(&resume, &layout));
        engine.compile_to_pixels(1.0).expect("compiles").0.rgba
    };

    let base = pixels(HeadingLayout::default());
    let engine = TypstEngine::new(generate(&resume));
    assert_eq!(
        engine.compile_to_pixels(1.0).expect("compiles").0.rgba,
        base,
        "the default heading is not what `generate` produces"
    );

    let variants: [(&str, HeadingLayout); 3] = [
        (
            "plain style",
            HeadingLayout {
                style: HeadingStyle::Plain,
                ..Default::default()
            },
        ),
        (
            "as-typed case",
            HeadingLayout {
                case: HeadingCase::AsTyped,
                ..Default::default()
            },
        ),
        (
            "left aligned",
            HeadingLayout {
                align: HeaderAlign::Left,
                ..Default::default()
            },
        ),
    ];
    for (what, headings) in variants {
        assert_ne!(
            pixels(headings),
            base,
            "{what} rendered identically to the default — the control never reached the page"
        );
    }

    // Alignment is hidden for the one style whose words cannot move, so
    // it had better be true that they cannot. If this ever fails the rail
    // is hiding a control that does something.
    assert!(!HeadingStyle::RuleToMargin.can_align());
    assert_eq!(
        pixels(HeadingLayout {
            style: HeadingStyle::RuleToMargin,
            align: HeaderAlign::Center,
            ..Default::default()
        }),
        pixels(HeadingLayout {
            style: HeadingStyle::RuleToMargin,
            align: HeaderAlign::Left,
            ..Default::default()
        }),
        "alignment moved a rule-to-margin heading — the rail hides a live control"
    );
}

/// Letter-spacing belongs to the capitals, so dropping the capitals has
/// to drop it too — otherwise "Work Experience" comes out gappy and the
/// case control looks broken rather than chosen.
///
/// Isolated on the page rather than in the source: the title is typed in
/// capitals already, so `upper()` is a no-op and the **only** thing left
/// between the two renders is the tracking. An earlier version of this
/// test matched the renderer's source text and broke the moment the branch
/// was rewritten with identical behaviour (E-36).
#[test]
fn tracking_follows_the_capitals() {
    use crate::resume::model::{HeadingCase, Work};
    use crate::typst_engine::TypstEngine;

    let mut doc = ResumeDoc::from_resume(
        Resume {
            work: vec![Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        "Base",
    );
    doc.set_section_title(SectionKind::Work, "WORK");

    let pixels = |case: HeadingCase| {
        let mut doc = doc.clone();
        doc.layout.headings.case = case;
        // `generate_for`, not `generate`: the latter renders under the
        // *default* layout, so a test that sets one and calls it proves
        // nothing about the setting.
        TypstEngine::new(generate_for(&doc))
            .compile_to_pixels(1.0)
            .expect("compiles")
            .0
            .rgba
    };

    assert!(
        pixels(HeadingCase::Upper) != pixels(HeadingCase::AsTyped),
        "a heading already typed in capitals rendered the same either way — \
         the letter-spacing is no longer tied to the case"
    );
}

/// A per-section departure has to reach the page, and reach **only** that
/// section — the whole risk of this feature is a setting that leaks.
#[test]
fn a_sections_own_heading_reaches_the_page_and_leaves_the_others_alone() {
    use crate::resume::model::{Education, HeadingStyle, SectionOverrides, Work};
    use crate::typst_engine::TypstEngine;

    let doc = ResumeDoc::from_resume(
        Resume {
            work: vec![Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                ..Default::default()
            }],
            education: vec![Education {
                institution: "Tallaght".into(),
                study_type: "M.Sc.".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        "Base",
    );

    let pixels = |d: &ResumeDoc| {
        TypstEngine::new(generate_for(d))
            .compile_to_pixels(1.0)
            .expect("compiles")
            .0
            .rgba
    };
    let base = pixels(&doc);

    // One section restyled.
    let mut one = doc.clone();
    one.set_section_overrides(
        SectionKind::Work,
        SectionOverrides {
            heading_style: Some(HeadingStyle::Plain),
            ..Default::default()
        },
    );
    assert!(pixels(&one) != base, "the override never reached the page");

    // The same style applied to the whole document must differ from the
    // one-section version — otherwise "only that section" is unproven.
    let mut all = doc.clone();
    all.layout.headings.style = HeadingStyle::Plain;
    assert!(
        pixels(&one) != pixels(&all),
        "restyling one section rendered the same as restyling every section"
    );

    // And the section that was left alone is genuinely untouched: turning
    // the *document* to the same style leaves Work where the override
    // already put it, so only Education can account for the difference.
    let mut both = one.clone();
    both.layout.headings.style = HeadingStyle::Plain;
    assert_eq!(
        pixels(&both),
        pixels(&all),
        "an override and the document agreeing did not converge"
    );
}

/// Overrides are per **field**, not per struct.
///
/// A section that departs on style must still follow the document on
/// capitalisation — otherwise restyling one section silently freezes two
/// other decisions, and the next document-wide change skips it for
/// reasons the user never chose.
#[test]
fn overriding_one_field_does_not_pin_the_others() {
    use crate::resume::model::{HeadingCase, HeadingStyle, SectionOverrides, Work};
    use crate::typst_engine::TypstEngine;

    let mut doc = ResumeDoc::from_resume(
        Resume {
            work: vec![Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        "Base",
    );
    doc.set_section_overrides(
        SectionKind::Work,
        SectionOverrides {
            heading_style: Some(HeadingStyle::Plain),
            ..Default::default()
        },
    );

    // The model's own answer…
    assert_eq!(
        doc.headings_for(SectionKind::Work).style,
        HeadingStyle::Plain
    );
    assert_eq!(doc.headings_for(SectionKind::Work).case, HeadingCase::Upper);
    doc.layout.headings.case = HeadingCase::AsTyped;
    assert_eq!(
        doc.headings_for(SectionKind::Work).case,
        HeadingCase::AsTyped,
        "the section stopped following the document on a field it never set"
    );
    assert_eq!(
        doc.headings_for(SectionKind::Work).style,
        HeadingStyle::Plain,
        "the field it did set was lost"
    );

    // …and the page's, since a resolution that never reaches Typst is not
    // a resolution (E-32).
    let pixels = |d: &ResumeDoc| {
        TypstEngine::new(generate_for(d))
            .compile_to_pixels(1.0)
            .expect("compiles")
            .0
            .rgba
    };
    let following = pixels(&doc);
    let mut pinned = doc.clone();
    pinned.layout.headings.case = HeadingCase::Upper;
    assert!(
        following != pixels(&pinned),
        "the document's capitalisation did not move the overridden section"
    );
}

/// An untouched document emits an empty table and is unchanged.
#[test]
fn no_overrides_means_an_empty_table_and_the_document_it_always_was() {
    use crate::resume::model::{HeadingStyle, SectionOverrides};

    let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
    let before = generate_for(&doc);
    assert!(before.contains("#let section-layout = (:)"));

    doc.set_section_overrides(
        SectionKind::Skills,
        SectionOverrides {
            heading_style: Some(HeadingStyle::Boxed),
            ..Default::default()
        },
    );
    let with = generate_for(&doc);
    assert!(with.contains("\"skills\": ("), "{with}");
    assert!(
        with.contains("heading: (style: \"boxed\""),
        "the resolved heading is not in the source"
    );
    assert!(
        !with.contains("    entry: ("),
        "a section that only restyled its heading pinned its entries too"
    );

    // Back to following the document, byte for byte.
    doc.set_section_overrides(SectionKind::Skills, SectionOverrides::default());
    assert_eq!(generate_for(&doc), before);
}

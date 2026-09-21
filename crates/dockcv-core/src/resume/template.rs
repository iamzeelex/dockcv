//! Codegen: turn a [`Resume`] into a self-contained Typst document.
//!
//! The generated source is `page_setup(layout)` (the `#set page`/`#set text`/
//! `#set par` lines, generated per document from its
//! [`LayoutSettings`](crate::resume::model::LayoutSettings) rather than
//! hard-coded — C1, US-07) + `RENDERER` (the rest of our renderer, written in
//! Typst, layout-independent) + `#let cv = (..)` (the model serialized back
//! to a Typst dictionary) + `#render-cv(cv)`. Everything lives in one
//! in-memory file so it compiles with the bundled fonts and no
//! package/network access — we do **not** download the AltaCV package to
//! render. (C7 vendors its MIT source into the repo instead; that changes which
//! renderer this dictionary is handed to, not the fact that nothing is fetched.)
//!
//! Plain fields become quoted Typst strings (so no markup escaping is needed);
//! free-text fields (`summary`, `highlights`) are emitted as `[..]` content
//! blocks, keeping the author's emphasis markup — see `neutralize` for the
//! syntax that is escaped instead, and why.

use crate::resume::model::{
    DocumentLanguage, LayoutSettings, ProfileCatalog, Resume, ResumeDoc,
};

use super::template_dict::resume_to_dict_into;
use super::template_page::{
    document_metadata_into, no_heading_into, page_setup_into, section_layout_into,
};

/// The renderer body, written in Typst: helper functions plus `render-cv`.
///
/// In a `.typ` file, not a 493-line string literal in the middle of a Rust
/// one. It is Typst source — it should be read, edited and highlighted as
/// Typst — and `include_str!` puts the same bytes in the same place at compile
/// time, so nothing about the build or the output changes. It was more than a
/// sixth of this file.
///
/// Page/text setup is *not* here: it is generated per-document by
/// [`page_setup_into`] from the document's own `LayoutSettings` (C1, US-07).
/// This is still one self-contained block — no packages, no network, bundled
/// fonts only (US-10).
const RENDERER: &str = include_str!("renderer.typ");

/// Build the full Typst document for a resume, using the default layout
/// (A4, the original margins/text-scale/leading `PREAMBLE` used to hard-code).
///
/// Kept alongside [`generate_with_layout`] as the no-layout-opinion entry
/// point: the AltaCV importer and the PDF-import preview both compile a bare
/// [`Resume`] with no [`crate::resume::model::ResumeDoc`] (and so no stored
/// layout) in hand.
pub fn generate(resume: &Resume) -> String {
    generate_with_layout_and_language(
        resume,
        &LayoutSettings::default(),
        DocumentLanguage::English,
    )
}

/// Build the Typst document for a **document**, with the layout it carries.
///
/// This is what the app calls. It exists because the obvious call —
/// `generate(&doc.compose())` — silently renders with
/// `LayoutSettings::default()`, and six call sites did exactly that: the font
/// picker, the page size, the margins, the text scale and the date format all
/// reached the model, were saved to the vault, and then never arrived in the
/// preview. Taking the whole `ResumeDoc` makes the layout impossible to drop.
pub fn generate_for(doc: &ResumeDoc) -> String {
    generate_for_with_profiles(doc, &ProfileCatalog::default())
}

/// Build the Typst document with vault-wide profiles available.
///
/// The plain [`generate_for`] entry point still resolves built-ins and is the
/// right boundary for WASM and isolated files. The desktop passes its vault
/// catalog here so a custom profile remains a reference rather than a copy in
/// every document.
pub fn generate_for_with_profiles(doc: &ResumeDoc, profiles: &ProfileCatalog) -> String {
    let layout = profiles.resolve(doc.layout_profile.as_deref(), doc.layout);
    // Both axes a reading carries reach the page here, and this is the one
    // place they can: the profile decides the layout, the reading decides the
    // language, and a caller that resolved only one of them would print a
    // German CV in the document's own layout or an ATS-safe one in English.
    generate_with_layout_and_language(&doc.compose(), &layout, doc.language())
}

/// Build the full Typst document for a resume with an explicit layout. This
/// is what a document editor should call — `resume/model.rs::ResumeDoc`
/// carries `layout` precisely so callers have real settings to pass here
/// instead of the default.
///
/// `layout` is sanitized before it ever reaches Typst source: a corrupted or
/// hand-edited vault file can supply a zero text scale or a negative margin,
/// and this is where that gets caught, not as a Typst compile error.
pub fn generate_with_layout(resume: &Resume, layout: &LayoutSettings) -> String {
    generate_with_layout_and_language(resume, layout, DocumentLanguage::English)
}

/// Build Typst source with an explicit document language.
///
/// The language reaches one authoritative Typst `text` rule, which controls
/// both the PDF catalog's `/Lang` tag and Typst's hyphenation dictionary. It
/// also selects the words DockCV emits for parsed dates.
pub fn generate_with_layout_and_language(
    resume: &Resume,
    layout: &LayoutSettings,
    language: DocumentLanguage,
) -> String {
    let mut out = String::with_capacity(8192);
    document_metadata_into(&mut out, resume);
    page_setup_into(&mut out, &layout.sanitized(), language);
    no_heading_into(&mut out, resume);
    section_layout_into(&mut out, resume, &layout.sanitized());
    out.push('\n');
    out.push_str(RENDERER);
    out.push_str("\n#let cv = ");
    resume_to_dict_into(&mut out, resume, layout.date_format, language);
    out.push_str("\n#render-cv(cv)\n");
    out
}

#[cfg(test)]
mod section_order_tests {
    use super::*;
    use crate::resume::model::{Resume, ResumeDoc, SectionKind};

    /// The bug this guards, found by the user in the running app: sections
    /// reordered in the sidebar did not move in the rendered PDF. Order was a
    /// saved document field that reached `ResumeDoc::sections()` and then died
    /// at the `compose()` boundary — `Resume` had nowhere to carry it and the
    /// Typst renderer emitted a hard-coded sequence. A test asserting the
    /// *sidebar* order would have passed the whole time; this asserts the
    /// generated source, which is what actually prints.
    #[test]
    fn reordering_sections_reaches_the_generated_source() {
        let mut doc = ResumeDoc::from_resume(
            crate::resume::altacv::import(crate::resume::altacv::ALTACV_SAMPLE).unwrap(),
            "Base",
        );

        // Untouched: no `order` line at all, so existing documents generate
        // byte-identical source to before this feature.
        let before = generate(&doc.compose());
        assert!(
            !before.contains("order: ("),
            "default order must not be emitted"
        );

        // Move Education up one slot, so it sits above Work.
        doc.move_section(SectionKind::Education, -1);
        assert_eq!(
            doc.sections()[1],
            SectionKind::Education,
            "fixture: Education should now sit directly after Profile"
        );

        let after = generate(&doc.compose());
        assert!(
            after.contains("order: ("),
            "a reordered document must emit its order"
        );
        let order_line = after
            .lines()
            .find(|l| l.trim_start().starts_with("order: ("))
            .expect("order line");
        let education_at = order_line.find("education").expect("education in order");
        let work_at = order_line.find("\"work\"").expect("work in order");
        assert!(
            education_at < work_at,
            "education must precede work in the emitted order: {order_line}"
        );
    }

    #[test]
    fn each_preset_heading_and_order_reaches_the_pdf_source() {
        use crate::typst_engine::TypstEngine;

        let mut doc = ResumeDoc::from_resume(
            crate::resume::altacv::import(crate::resume::altacv::ALTACV_SAMPLE).unwrap(),
            "Base",
        );
        doc.section_order = vec![
            SectionKind::Skills,
            SectionKind::Profile,
            SectionKind::Work,
            SectionKind::Education,
            SectionKind::Certificates,
            SectionKind::Organizations,
        ];
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

        for (index, heading, first) in [
            (0, "Engineering", "\"skills\""),
            (1, "Experience", "\"work\""),
        ] {
            doc.apply_preset(index);
            let source = generate_for(&doc);
            let order = source
                .lines()
                .find(|line| line.trim_start().starts_with("order: ("))
                .expect("preset order reaches generated source");
            assert!(
                order.trim_start().starts_with(&format!("order: ({first}")),
                "wrong first section for preset {index}: {order}"
            );
            assert!(source.contains(heading), "missing heading {heading}");
            TypstEngine::new(source)
                .compile_to_pdf()
                .expect("each preset compiles to PDF");
        }
    }

    /// Hiding one custom section must not move another one.
    ///
    /// The renderer used to address custom sections by their **position** in
    /// the emitted array, while the `order` list beside it is built by walking
    /// every section including the hidden ones. The two walks agree right up
    /// until a hidden custom section sits above a visible one — then the keys
    /// are off by one, and the survivor is drawn at the hidden section's
    /// place instead of its own. Nothing errored; the CV was just wrong.
    ///
    /// The invariant, stated so it cannot drift: hiding a section has to
    /// render exactly like that section having nothing in it.
    #[test]
    fn hiding_a_custom_section_leaves_the_others_where_they_are() {
        use crate::resume::model::CustomEntry;
        use crate::typst_engine::TypstEngine;

        // The document needs real content between the two custom sections:
        // on an empty CV every other section renders nothing, so "first" and
        // "last" collapse to the same place and a positional bug is invisible.
        let seeded = || Resume {
            basics: crate::resume::model::Basics {
                name: "Albert Einstein".into(),
                ..Default::default()
            },
            work: vec![crate::resume::model::Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                ..Default::default()
            }],
            ..Resume::default()
        };

        let build = || {
            let mut doc = ResumeDoc::from_resume(seeded(), "Base");
            let upper = doc.add_custom_section("Talks");
            let lower = doc.add_custom_section("Publications");
            for (id, title) in [(upper, "A talk"), (lower, "A paper")] {
                let content = doc.custom_section_mut(id).unwrap().content.active_mut();
                content.clear();
                content.push(CustomEntry {
                    title: title.into(),
                    ..Default::default()
                });
            }
            // Off the default order, so the `order` list is actually emitted
            // — that is the code path the keys are read on.
            for _ in 0..doc.sections().len() {
                doc.move_section(SectionKind::Custom(upper), -1);
            }
            (doc, upper, lower)
        };

        let pixels = |doc: &ResumeDoc| {
            TypstEngine::new(generate(&doc.compose()))
                .compile_to_pixels(1.0)
                .expect("compiles")
                .0
                .rgba
        };

        let (mut hidden_doc, upper, lower) = build();
        hidden_doc.set_hidden(SectionKind::Custom(upper), true);

        let (mut emptied_doc, upper_b, _) = build();
        emptied_doc
            .custom_section_mut(upper_b)
            .unwrap()
            .content
            .active_mut()
            .clear();

        assert!(
            pixels(&hidden_doc) == pixels(&emptied_doc),
            "hiding the section above moved the one below it — the renderer is \
             counting custom sections instead of naming them"
        );

        // And the survivor is genuinely on the page, so the equality above
        // cannot be satisfied by both documents drawing nothing.
        let (mut both_gone, upper_c, lower_c) = build();
        both_gone.set_hidden(SectionKind::Custom(upper_c), true);
        both_gone.set_hidden(SectionKind::Custom(lower_c), true);
        assert!(
            pixels(&hidden_doc) != pixels(&both_gone),
            "the surviving section drew nothing, so the comparison above proves nothing"
        );
        let _ = lower;
    }

    /// A custom section keeps its place in the order, and its renderer key
    /// matches its position in the emitted `customSections` array.
    #[test]
    fn a_custom_section_keeps_its_position_in_the_order() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let id = doc.add_custom_section("Publications");
        doc.custom_section_mut(id)
            .unwrap()
            .content
            .active_mut()
            .push(crate::resume::model::CustomEntry {
                title: "A paper".into(),
                ..Default::default()
            });

        // Pull it to the very top.
        for _ in 0..doc.sections().len() {
            doc.move_section(SectionKind::Custom(id), -1);
        }
        assert_eq!(doc.sections()[0], SectionKind::Custom(id));

        let source = generate(&doc.compose());
        let order_line = source
            .lines()
            .find(|l| l.trim_start().starts_with("order: ("))
            .expect("order line");
        assert!(
            order_line.trim_start().starts_with("order: (\"custom0\""),
            "the custom section leads the order: {order_line}"
        );
    }
}

#[cfg(test)]
mod empty_document_tests {
    use super::*;
    use crate::resume::model::Resume;
    use crate::typst_engine::TypstEngine;

    /// A brand-new blank CV must compile. It did not: an all-empty profile
    /// emitted `basics: (\n)`, which Typst reads as an empty **array** rather
    /// than an empty dictionary (`(:)`), so the renderer's
    /// `cv.basics.at("name", …)` indexed an array with a string and the whole
    /// preview failed with "expected integer, found string".
    ///
    /// This is the first screen after "Skip — start blank", and nothing in the
    /// suite had ever compiled a document with no content in it.
    #[test]
    fn a_blank_document_still_compiles() {
        let source = generate(&Resume::default());
        assert!(
            source.contains("basics: (:)"),
            "an empty profile must emit an empty dictionary, not an empty array"
        );

        let mut engine = TypstEngine::new(String::new());
        engine.set_source(source);
        engine
            .compile_to_pixels(1.0)
            .expect("a blank CV must compile");
    }
}

#[cfg(test)]
mod date_format_tests {
    use super::*;
    use crate::resume::template_dict::neutralize;
    use crate::resume::model::{
        DateFormat, DocumentLanguage, LayoutSettings, Resume, SectionKind, Work,
    };

    /// The document's date format has to reach the *generated source*, not
    /// just sit in the model — the same boundary that swallowed section order
    /// (E-12). And what the user typed must survive in the file while only
    /// its rendering changes.
    #[test]
    fn the_date_format_reaches_the_generated_source() {
        let resume = Resume {
            work: vec![Work {
                position: "Senior Engineer".into(),
                name: "Acme".into(),
                start_date: "2022-01".into(),
                end_date: "2024-06-15".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let with = |format: DateFormat| {
            generate_with_layout(
                &resume,
                &LayoutSettings {
                    date_format: format,
                    ..Default::default()
                },
            )
        };

        let iso = with(DateFormat::Iso);
        assert!(iso.contains("2022-01"), "iso keeps the stored shape");
        assert!(iso.contains("2024-06-15"));

        let uk = with(DateFormat::DayMonShortYear);
        assert!(
            uk.contains("Jan 2022"),
            "month-only degrades, no invented day"
        );
        assert!(uk.contains("15 Jun 2024"));
        assert!(!uk.contains("2024-06-15"), "the ISO form must be gone");

        let us = with(DateFormat::MonthDayOrdinalYear);
        assert!(us.contains("January 2022"));
        assert!(us.contains("June 15th, 2024"));
    }

    /// Text that is not a date reaches the page untouched whatever the format
    /// is set to — the promise `resume::dates` is built around.
    #[test]
    fn unparseable_dates_print_as_written_under_every_format() {
        let resume = Resume {
            work: vec![Work {
                position: "Engineer".into(),
                start_date: "Summer 2021".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        for format in DateFormat::ALL {
            let source = generate_with_layout(
                &resume,
                &LayoutSettings {
                    date_format: format,
                    ..Default::default()
                },
            );
            assert!(
                source.contains("Summer 2021"),
                "{format:?} dropped text it could not parse"
            );
        }
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn one_document_exports_english_and_german_readings_with_real_pdf_languages() {
        use crate::typst_engine::TypstEngine;

        let mut doc = ResumeDoc::from_resume(
            crate::resume::altacv::import(crate::resume::altacv::ALTACV_SAMPLE).unwrap(),
            "Base",
        );
        doc.layout.date_format = DateFormat::DayOrdinalMonthYear;
        doc.work.active_mut()[0].start_date = "2020-03".into();
        doc.work.active_mut()[0].end_date = "".into();

        doc.set_section_title(SectionKind::Work, "Experience");
        doc.add_preset("English");

        doc.set_language(DocumentLanguage::German);
        doc.set_section_title(SectionKind::Work, "Berufserfahrung");
        doc.add_preset("Deutsch");

        let shared_layout = doc.layout;
        for (index, lang, heading, month, present) in [
            (0, "en", "Experience", "March 2020", "Present"),
            (1, "de", "Berufserfahrung", "März 2020", "Heute"),
        ] {
            doc.apply_preset(index);
            assert_eq!(doc.layout, shared_layout, "language never forks layout");

            let source = generate_for(&doc);
            assert!(source.contains(&format!("lang: \"{lang}\"")));
            assert!(source.contains(heading));
            assert!(source.contains(month));
            assert!(source.contains(&format!("present-label = \"{present}\"")));

            let pdf = TypstEngine::new(source)
                .compile_to_pdf()
                .expect("each language compiles to PDF");
            let pdf = lopdf::Document::load_mem(&pdf).expect("produced PDF opens");
            let catalog = pdf.catalog().expect("produced PDF has a catalog");
            let tagged = catalog
                .get(b"Lang")
                .expect("PDF catalog has /Lang")
                .as_str()
                .expect("/Lang is a string");
            assert_eq!(tagged, lang.as_bytes());
        }
    }

    #[test]
    fn neutralize_escapes_backslashes_and_special_markup() {
        assert_eq!(
            neutralize(r"C:\#1 @user [test] $100"),
            r"C:\\\#1 \@user \[test\] \$100"
        );
    }

    /// A preset name in a recruiter's document properties is an embarrassment
    /// we would have built ourselves, so this checks the produced **file**, not
    /// the source we hoped would produce it. The file-to-preset mapping lives in
    /// export history, inside the vault, where it is ours.
    #[cfg(feature = "pdf")]
    #[test]
    fn no_exported_pdf_carries_a_preset_or_variant_name() {
        use crate::resume::model::{Basics, Preset, SectionKind};
        use crate::typst_engine::TypstEngine;

        let resume = Resume {
            basics: Basics {
                name: "Albert Einstein".into(),
                label: "Principal Systems Architect".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut doc = ResumeDoc::from_resume(resume, "Base");
        // Variant and preset names a person would actually type, and would
        // certainly not want sent: the point is that neither can reach the file.
        doc.profile.variants[0].name = "FAANG rewrite".into();
        doc.presets = vec![
            Preset {
                name: "FAANG · concise".into(),
                based_on: None,
                description: None,
                profile: None,
                selection: vec![(SectionKind::Profile, doc.profile.active_id())],
                hidden: vec![SectionKind::Organizations],
                order: vec![],
                titles: vec![],
                lang: None,
            },
            Preset {
                name: "Startup · long".into(),
                based_on: None,
                description: None,
                profile: None,
                selection: vec![],
                hidden: vec![],
                order: vec![],
                titles: vec![],
                lang: None,
            },
        ];

        let source = generate_for(&doc);
        let pdf = TypstEngine::new(source)
            .compile_to_pdf()
            .expect("the document compiles");
        let bytes = String::from_utf8_lossy(&pdf);

        for secret in [
            "FAANG", "concise", "Startup", "rewrite", "preset", "variant",
        ] {
            assert!(
                !bytes.contains(secret),
                "{secret:?} reached the exported PDF"
            );
        }

        // And the two fields we *do* set are exactly the two we meant to set.
        assert!(bytes.contains("Albert Einstein - Principal Systems Architect"));
        assert!(bytes.contains("/Author"));
        assert!(bytes.contains("/Title"));
        assert!(
            !bytes.contains("/Keywords") && !bytes.contains("/Subject"),
            "a metadata field nobody decided on is being written"
        );
    }

    /// The source is where the leak would start, so it is worth pinning too —
    /// and it is the only half a build without the PDF writer can check.
    #[test]
    fn document_metadata_names_the_person_and_nothing_else() {
        use crate::resume::model::Basics;

        let with_both = Resume {
            basics: Basics {
                name: "Albert Einstein".into(),
                label: "Principal Systems Architect".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut out = String::new();
        document_metadata_into(&mut out, &with_both);
        assert_eq!(
            out.trim(),
            r#"#set document(title: "Albert Einstein - Principal Systems Architect", author: "Albert Einstein")"#
        );

        // A quote in a name is a Typst string literal ending early, which is a
        // compile error rather than a wrong title — so it gets escaped.
        let quoted = Resume {
            basics: Basics {
                name: r#"Ann "Nan" O'Neil"#.into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut out = String::new();
        document_metadata_into(&mut out, &quoted);
        assert!(out.contains(r#"\"Nan\""#), "{out}");

        // An empty document still gets a title, and no author at all rather
        // than an empty one.
        let mut out = String::new();
        document_metadata_into(&mut out, &Resume::default());
        assert_eq!(out.trim(), r#"#set document(title: "Resume")"#);
        assert!(!out.contains("author"));
    }
}

#[cfg(test)]
#[path = "template_heading_tests.rs"]
mod heading_tests;

#[cfg(test)]
#[path = "template_layout_tests.rs"]
mod layout_tests;

#[cfg(test)]
#[path = "template_tests.rs"]
mod generator_tests;

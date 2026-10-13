use typst::foundations::Bytes;

use super::*;

/// A short document, well within one A4 page at the default size.
const ONE_PAGE_SOURCE: &str = "= Hello\nJust one short paragraph of text.";

/// A tiny page size with enough repeated paragraphs to force pagination.
/// Deliberately not derived from any real résumé template — this test is
/// about `TypstEngine` in general, not about `resume::template`.
fn overflowing_source() -> String {
    let mut source = String::from("#set page(width: 200pt, height: 150pt, margin: 8pt)\n");
    for i in 0..40 {
        source.push_str(&format!(
            "Paragraph number {i} of filler text to push layout past a single small page.\n\n"
        ));
    }
    source
}

/// A document broken the way a résumé author could actually break one —
/// a stray `]` typed into free-text markup (a summary or bullet) closes
/// the surrounding content block early. Real Typst wording for this is
/// "unexpected closing bracket"; `humanize` is what stands between that
/// and the person editing the document.
const BROKEN_SOURCE: &str = "A summary with a stray ] bracket that breaks the markup.";

/// `measure` answers the same page geometry a full compile does, without
/// rasterizing — the Preset Matrix reads every column through it, so the
/// two paths agreeing is the whole contract.
#[test]
fn measuring_agrees_with_compiling_and_needs_no_raster() {
    let engine = TypstEngine::new(ONE_PAGE_SOURCE);
    let measured = engine.measure().expect("a valid document measures");
    assert_eq!(measured.page_count, 1);
    assert_eq!(measured.overflow_pt, 0.0);

    let engine = TypstEngine::new(overflowing_source());
    let measured = engine.measure().expect("an overflowing document measures");
    assert!(measured.page_count > 1, "it paginated");
    assert!(
        measured.overflow_pt > 0.0,
        "and says how much is past one page"
    );
}

/// A document that will not compile measures to an error rather than to
/// zero pages. A column header showing `0 pages` would be the matrix
/// stating something untrue about a document it simply could not read.
#[test]
fn a_broken_document_refuses_to_measure() {
    let engine = TypstEngine::new(BROKEN_SOURCE);
    assert!(engine.measure().is_err());
}

#[test]
fn broken_document_yields_a_human_readable_error() {
    let engine = TypstEngine::new(BROKEN_SOURCE);
    let attempt = engine.compile_with_diagnostics(1.0);

    assert!(
        attempt.result.is_err(),
        "expected this document to fail to compile"
    );
    let errors: Vec<&Diagnostic> = attempt
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        !errors.is_empty(),
        "a failed compile must report at least one error"
    );

    for error in &errors {
        // Never a raw span/location dump: no "3:5" style position, no
        // mention of the word "span".
        assert!(
            !error.message.contains(':') || error.message.matches(':').count() <= 1,
            "message reads like a location dump: {:?}",
            error.message
        );
        assert!(!error.message.to_ascii_lowercase().contains("span"));
        assert!(!error.message.is_empty());
    }
}

#[test]
fn warning_does_not_present_as_an_error() {
    // Triggers a genuine Typst warning (unknown font family) with no
    // compile error at all.
    let src = "#set text(font: \"Totally Nonexistent Font XYZ\")\nHello, world.";
    let engine = TypstEngine::new(src);
    let attempt = engine.compile_with_diagnostics(1.0);

    assert!(
        attempt.result.is_ok(),
        "a missing font is a warning, not a failure"
    );
    assert!(
        attempt
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning),
        "expected at least one warning diagnostic"
    );
    assert!(
        !attempt
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error),
        "a successful compile must not report any Severity::Error diagnostic"
    );
}

#[test]
fn successful_compile_reports_no_diagnostics() {
    let engine = TypstEngine::new(ONE_PAGE_SOURCE);
    let attempt = engine.compile_with_diagnostics(1.0);

    assert!(attempt.result.is_ok());
    assert!(
        attempt.diagnostics.is_empty(),
        "a clean compile should not manufacture diagnostics: {:?}",
        attempt.diagnostics
    );
}

#[test]
fn one_page_document_reports_one_page() {
    let engine = TypstEngine::new(ONE_PAGE_SOURCE);
    let (_pixels, geometry) = engine.compile_to_pixels(1.0).expect("compiles");

    assert_eq!(geometry.page_count, 1);
    assert_eq!(geometry.overflow_pt, 0.0);
    // Content plainly does not reach the bottom of a full A4 page.
    assert!(geometry.last_page_used_pt < geometry.page_height_pt);
}

#[test]
fn overflowing_document_reports_more_pages_and_positive_overflow() {
    let engine = TypstEngine::new(overflowing_source());
    let (_pixels, geometry) = engine.compile_to_pixels(1.0).expect("compiles");

    assert!(
        geometry.page_count > 1,
        "expected pagination, got {} page(s)",
        geometry.page_count
    );
    assert!(
        geometry.overflow_pt > 0.0,
        "expected positive overflow, got {}pt",
        geometry.overflow_pt
    );
    // Sanity: reported overflow cannot exceed the content stacked across
    // every page but the first.
    assert!(geometry.overflow_pt < geometry.page_height_pt * geometry.page_count as f64);
}

#[test]
fn overflow_measurement_is_stable_for_the_same_input() {
    let source = overflowing_source();
    let a = TypstEngine::new(source.clone())
        .compile_to_pixels(1.0)
        .expect("compiles")
        .1;
    let b = TypstEngine::new(source)
        .compile_to_pixels(1.0)
        .expect("compiles")
        .1;

    assert_eq!(a, b, "measuring the same source twice should agree exactly");
}

mod line_advance_tests {
    use super::*;
    use crate::resume::model::{Resume, ResumeDoc, Work};
    use crate::resume::template;

    /// The measurement that turns points into the design's "+N lines".
    /// A real document must yield a plausible advance, and the number has to
    /// track the document's own text scale — if it did not, it would be a
    /// constant wearing a measurement's clothes.
    #[test]
    fn the_line_advance_is_measured_from_the_page_and_follows_text_scale() {
        let mut resume = Resume::default();
        resume.basics.name = "Albert Einstein".into();
        resume.work = (0..6)
            .map(|i| Work {
                position: format!("Engineer {i}"),
                name: "Acme".into(),
                highlights: vec!["Did a thing that took a whole line of text".into(); 3],
                ..Default::default()
            })
            .collect();

        let mut doc = ResumeDoc::from_resume(resume, "Base");
        let mut engine = TypstEngine::new(String::new());

        engine.set_source(template::generate_with_layout(&doc.compose(), &doc.layout));
        let (_, small) = engine.compile_to_pixels(1.0).expect("compiles");
        let base = small.line_advance_pt.expect("a page of text is measurable");
        // 10pt body text: anything outside this band means we are measuring
        // something other than consecutive body lines.
        assert!(
            (8.0..24.0).contains(&base),
            "implausible line advance {base}pt"
        );

        doc.layout.text_scale_pct = 150;
        engine.set_source(template::generate_with_layout(&doc.compose(), &doc.layout));
        let (_, large) = engine.compile_to_pixels(1.0).expect("compiles");
        let scaled = large.line_advance_pt.expect("still measurable");
        assert!(
            scaled > base * 1.1,
            "advance must grow with text scale: {base} -> {scaled}"
        );
    }

    /// Overflow is **content**, never paper. A page-2 that holds three short
    /// lines must report roughly three lines' worth — not those plus the top
    /// margin, which is what measuring from the frame's top edge did. The
    /// margin is not something a user can trim, so counting it made the number
    /// overstate the problem by a fixed amount on every document.
    #[test]
    fn the_top_margin_is_not_counted_as_overflow() {
        let mut resume = Resume::default();
        resume.basics.name = "Albert Einstein".into();
        // Just enough to push a little content onto a second page.
        resume.work = (0..26)
            .map(|i| Work {
                position: format!("Engineer {i}"),
                name: "Acme".into(),
                highlights: vec!["A bullet long enough to take a full line".into(); 2],
                ..Default::default()
            })
            .collect();

        let doc = ResumeDoc::from_resume(resume, "Base");
        let mut engine = TypstEngine::new(String::new());
        engine.set_source(template::generate_with_layout(&doc.compose(), &doc.layout));
        let (_, geometry) = engine.compile_to_pixels(1.0).expect("compiles");

        assert!(geometry.page_count > 1, "fixture must overflow");
        assert!(
            geometry.last_page_content_top_pt > 0.0,
            "there is a top margin above the spilled content"
        );
        // The old formula was `last_page_used_pt`, margin included. Overflow
        // must be strictly less than that by at least the margin.
        assert!(
            geometry.overflow_pt < geometry.last_page_used_pt,
            "overflow {} must exclude the {}pt of margin above it",
            geometry.overflow_pt,
            geometry.last_page_content_top_pt
        );
        let content_height = geometry.last_page_used_pt - geometry.last_page_content_top_pt;
        assert!((geometry.overflow_pt - content_height).abs() < 0.5);
    }

    /// An empty document has nothing to average, and must say so rather than
    /// returning a made-up number the overflow chip would then quote.
    #[test]
    fn a_document_with_no_text_reports_no_measurement() {
        let doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let mut engine = TypstEngine::new(String::new());
        engine.set_source(template::generate_with_layout(&doc.compose(), &doc.layout));
        let (_, geometry) = engine.compile_to_pixels(1.0).expect("compiles");
        assert_eq!(geometry.line_advance_pt, None);
    }
}

mod page_break_tests {
    use super::*;
    use crate::resume::model::{Resume, ResumeDoc, Work};
    use crate::resume::template;

    /// A two-page CV must *look* like two pages.
    ///
    /// `render_merged` was filling the whole raster white, so the gap between
    /// pages was white on white: the break existed and was invisible, and a
    /// long CV read as one sheet with a blank stretch in the middle. The pages
    /// are opaque because the template sets `#set page(fill: white)`; the gap
    /// is transparent so the preview's canvas shows through it.
    #[test]
    fn the_gap_between_pages_is_transparent_and_the_pages_are_not() {
        let mut resume = Resume::default();
        resume.basics.name = "Albert Einstein".into();
        // Enough content to certainly spill onto a second page.
        resume.work = (0..40)
            .map(|i| Work {
                position: format!("Senior Engineer {i}"),
                name: "Acme Corp".into(),
                highlights: vec!["A bullet point long enough to occupy a line".into(); 3],
                ..Default::default()
            })
            .collect();

        let doc = ResumeDoc::from_resume(resume, "Base");
        let mut engine = TypstEngine::new(String::new());
        engine.set_source(template::generate_with_layout(&doc.compose(), &doc.layout));

        let (pixels, geometry) = engine.compile_to_pixels(1.0).expect("compiles");
        assert!(geometry.page_count > 1, "fixture must overflow one page");

        // Walk down the middle column: an opaque page, then a transparent
        // band, then an opaque page again.
        let x = pixels.width / 2;
        let alpha_at = |y: u32| pixels.rgba[((y * pixels.width + x) * 4 + 3) as usize];
        let transparent_rows = (0..pixels.height).filter(|y| alpha_at(*y) == 0).count();
        let opaque_rows = (0..pixels.height).filter(|y| alpha_at(*y) == 255).count();

        assert!(
            transparent_rows > 0,
            "the inter-page gap must be transparent, or the break is invisible"
        );
        assert!(
            opaque_rows > transparent_rows,
            "pages must stay opaque: {opaque_rows} opaque vs {transparent_rows} transparent rows"
        );
    }
}

mod font_tests {
    use super::*;
    use crate::resume::model::{DocumentFont, LayoutSettings, Resume};
    use crate::resume::template;

    /// Every family the picker offers must be one the compiler can actually
    /// resolve. A missing face does not fail the compile — Typst quietly falls
    /// back — so a picker entry with no font behind it would look like it
    /// worked and change nothing.
    #[test]
    fn every_offered_font_is_registered_and_changes_the_render() {
        let engine = TypstEngine::new(String::new());
        let families: Vec<String> = engine
            .book
            .families()
            .map(|(n, _)| n.to_lowercase())
            .collect();

        for font in DocumentFont::ALL {
            assert!(
                families.iter().any(|f| f == &font.family().to_lowercase()),
                "`{}` is offered by the picker but not registered with the compiler",
                font.family()
            );
        }

        // …and choosing one has to reach the page. Two families with very
        // different metrics must not rasterize to the same bytes.
        let mut resume = Resume::default();
        resume.basics.name = "Seán Ó Murchú".into();
        resume.basics.summary = "Backend engineer with eight years of experience.".into();

        let render = |font: DocumentFont| {
            let layout = LayoutSettings {
                font,
                ..Default::default()
            };
            let mut engine = TypstEngine::new(String::new());
            engine.set_source(template::generate_with_layout(&resume, &layout));
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba
        };

        assert_ne!(
            render(DocumentFont::LibertinusSerif),
            render(DocumentFont::Geist),
            "switching serif to sans must change what is drawn"
        );
    }

    /// The default must stay the family every existing document was already
    /// rendered in, or adding this feature would silently reflow every CV in
    /// every vault.
    #[test]
    fn the_default_font_is_the_one_documents_already_used() {
        assert_eq!(DocumentFont::default(), DocumentFont::LibertinusSerif);
    }

    /// Which offered families can actually set a CV in Cyrillic.
    #[test]
    fn which_document_fonts_can_set_a_cv_in_cyrillic() {
        // Ukrainian needs the four letters Russian does not have; a face that
        // stops at the Russian alphabet is still wrong for this persona.
        const SAMPLE: &str = "Софія Медведенко ЄІЇҐ";

        let engine = TypstEngine::new(String::new());
        let covers = |font: DocumentFont| -> bool {
            let family = font.family().to_lowercase();
            engine
                .fonts
                .iter()
                .filter(|f| f.info().family.to_lowercase() == family)
                .any(|f| {
                    SAMPLE
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .all(|c| f.info().coverage.contains(c as u32))
                })
        };

        for font in DocumentFont::ALL {
            let expected = !matches!(font, DocumentFont::Newsreader);
            assert_eq!(
                covers(font),
                expected,
                "Cyrillic coverage of `{}` changed — update this test and L-11 \
                 in the decisions ledger, do not just flip the expectation",
                font.label()
            );
        }

        // The point of recording it: there *is* a serif that works, so the
        // limitation is a picker problem and not a missing-font problem.
        assert!(covers(DocumentFont::PtSerif));
        assert!(covers(DocumentFont::LibertinusSerif));
    }

    #[test]
    fn fonts_for_the_browser_cover_what_a_cv_can_contain() {
        const SUBSETS: &[(&str, &str)] = &[
            (
                "assets/fonts/web/LibertinusSerif-Regular.subset.otf",
                "assets/fonts/web/LibertinusSerif-Regular.otf",
            ),
            (
                "assets/fonts/web/LibertinusSerif-Bold.subset.otf",
                "assets/fonts/web/LibertinusSerif-Bold.otf",
            ),
            (
                "assets/fonts/web/LibertinusSerif-Italic.subset.otf",
                "assets/fonts/web/LibertinusSerif-Italic.otf",
            ),
            (
                "assets/fonts/web/Geist-Regular.subset.ttf",
                "assets/fonts/Geist-Regular.ttf",
            ),
            (
                "assets/fonts/web/Geist-Bold.subset.ttf",
                "assets/fonts/Geist-Bold.ttf",
            ),
        ];

        const REPERTOIRE: &str = "AZaz09 éòäûüñçßÅØ ĀŁŐ \
                                  Софія Медведенко ЄІЇҐ ЁЪЫЭ \
                                  –—…•·§©→≈×±≤≥ €₴ № ↗";

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");

        let load = |relative: &str| {
            let bytes = std::fs::read(root.join(relative)).unwrap_or_else(|e| {
                panic!("{relative} is missing ({e}) — run scripts/subset-fonts.sh")
            });
            Font::new(Bytes::new(bytes), 0)
                .unwrap_or_else(|| panic!("{relative} did not parse as a font"))
        };

        for (subset, original) in SUBSETS {
            let (small, full) = (load(subset), load(original));

            let lost: Vec<char> = REPERTOIRE
                .chars()
                .filter(|c| !c.is_whitespace())
                .filter(|c| full.info().coverage.contains(*c as u32))
                .filter(|c| !small.info().coverage.contains(*c as u32))
                .collect();

            assert!(
                lost.is_empty(),
                "{subset} lost {lost:?} that {original} had — widen RANGES in \
                 scripts/subset-fonts.sh and re-run it, or the browser will \
                 silently set them in another face"
            );
        }
    }

    /// G1: The followable mark `↗` (U+2197) is drawn in Geist, because Newsreader
    /// and PT Serif lack it and Typst falls back silently.
    /// Geist must cover U+2197 in both desktop and web subsets.
    #[test]
    fn link_mark_glyph_is_present_in_bundled_font() {
        let engine = TypstEngine::new(String::new());
        let geist_faces: Vec<&Font> = engine
            .fonts
            .iter()
            .filter(|f| f.info().family.to_lowercase().contains("geist"))
            .collect();
        assert!(
            !geist_faces.is_empty(),
            "Geist font must be loaded in TypstEngine"
        );
        for face in geist_faces {
            assert!(
                face.info().coverage.contains('↗' as u32),
                "Geist face {:?} lacks U+2197 (↗)",
                face.info().family
            );
        }
    }

    /// Every `/URI` action in a compiled PDF, in page order.
    #[cfg(feature = "pdf")]
    fn pdf_link_targets(pdf: &[u8]) -> Vec<String> {
        const MARKER: &[u8] = b"/URI(";
        let mut targets = Vec::new();
        let mut at = 0;
        while let Some(found) = pdf[at..]
            .windows(MARKER.len())
            .position(|w| w == MARKER)
            .map(|i| at + i + MARKER.len())
        {
            let mut target = String::new();
            let mut i = found;
            while i < pdf.len() && pdf[i] != b')' {
                if pdf[i] == b'\\' && i + 1 < pdf.len() {
                    i += 1;
                }
                target.push(pdf[i] as char);
                i += 1;
            }
            targets.push(target);
            at = i;
        }
        targets
    }

    fn typeset_runs(frame: &typst::layout::Frame, out: &mut Vec<(String, String, bool)>) {
        for (_, item) in frame.items() {
            match item {
                typst::layout::FrameItem::Group(g) => typeset_runs(&g.frame, out),
                typst::layout::FrameItem::Text(t) => out.push((
                    t.text.to_string(),
                    t.font.info().family.clone(),
                    t.glyphs.iter().any(|g| g.id == 0),
                )),
                _ => {}
            }
        }
    }

    #[test]
    fn the_link_mark_is_typeset_in_a_face_that_has_it() {
        use crate::resume::model::{Resume, Work};
        use crate::resume::template;

        let resume = Resume {
            work: vec![Work {
                name: "Patent Office".into(),
                position: "Examiner".into(),
                url: "ige.ch".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let engine = TypstEngine::new(template::generate(&resume));
        let document = typst::compile::<PagedDocument>(&engine)
            .output
            .expect("document with a link compiles");

        let mut runs = Vec::new();
        for page in document.pages() {
            typeset_runs(&page.frame, &mut runs);
        }

        let (_, family, has_notdef) = runs
            .iter()
            .find(|(text, _, _)| text.contains('\u{2197}'))
            .expect("the entry has a URL, so the page must carry the mark");
        assert!(
            family.to_lowercase().contains("geist"),
            "the mark fell back to {family:?} instead of the bundled face"
        );
        assert!(!has_notdef, "the mark was typeset as .notdef in {family:?}");
    }

    #[test]
    #[cfg(feature = "pdf")]
    fn every_link_a_compiled_pdf_offers_is_one_a_viewer_can_follow() {
        use crate::resume::model::{Basics, Certificate, NetworkProfile, Resume, Volunteer, Work};
        use crate::resume::template;

        let resume = Resume {
            basics: Basics {
                name: "Albert Einstein".into(),
                email: "albert@example.com".into(),
                phone: "+41 44 632 11 11".into(),
                url: "einstein.example.com".into(),
                profiles: vec![NetworkProfile {
                    network: "GitHub".into(),
                    username: "aeinstein".into(),
                    url: "github.com/aeinstein".into(),
                }],
                ..Default::default()
            },
            work: vec![Work {
                name: "Patent Office".into(),
                position: "Examiner".into(),
                url: "ige.ch".into(),
                ..Default::default()
            }],
            volunteer: vec![Volunteer {
                organization: "Institute".into(),
                position: "Fellow".into(),
                url: "ask me about this one".into(),
                ..Default::default()
            }],
            certificates: vec![Certificate {
                name: "Paper".into(),
                issuer: "Annalen der Physik".into(),
                url: "doi.org/10.1002/andp.19053221004".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let pdf = TypstEngine::new(template::generate(&resume))
            .compile_to_pdf()
            .expect("compile to pdf");
        let targets = pdf_link_targets(&pdf);

        assert!(
            !targets.is_empty(),
            "the document has links; the PDF has none"
        );
        for target in &targets {
            assert!(
                target.starts_with("https://")
                    || target.starts_with("http://")
                    || target.starts_with("mailto:")
                    || target.starts_with("tel:"),
                "{target:?} is a relative reference, and no viewer resolves one \
                 against a CV — all targets: {targets:?}"
            );
            assert!(
                !target.contains(' '),
                "{target:?} has a space in it and is not a URI"
            );
        }

        for expected in [
            "mailto:albert@example.com",
            "tel:+41446321111",
            "https://einstein.example.com",
            "https://github.com/aeinstein",
            "https://ige.ch",
            "https://doi.org/10.1002/andp.19053221004",
        ] {
            assert!(
                targets.iter().any(|t| t == expected),
                "no link to {expected}; got {targets:?}"
            );
        }

        assert!(
            !targets.iter().any(|t| t.contains("ask me")),
            "free text was dressed up as a link: {targets:?}"
        );
    }

    #[test]
    fn page_geometry_is_always_a_real_measurement() {
        use crate::resume::model::{
            CategoryMark, HeadingLayout, HeadingStyle, LayoutSettings, Resume, RowSpacing,
            SkillGroup, SkillSeparator, SkillsLayout, SkillsStyle,
        };
        use crate::resume::template;

        let resume = Resume {
            skills: (0..8)
                .map(|i| SkillGroup {
                    name: format!("A rather long category name {i}"),
                    keywords: (0..9).map(|k| format!("Technology {i}-{k}")).collect(),
                })
                .collect(),
            ..Resume::default()
        };

        for style in SkillsStyle::ALL {
            for spacing in RowSpacing::ALL {
                for heading in HeadingStyle::ALL {
                    let layout = LayoutSettings {
                        skills: SkillsLayout {
                            style,
                            separator: SkillSeparator::Rule,
                            mark: CategoryMark::Dash,
                            spacing,
                            bullets: true,
                        },
                        headings: HeadingLayout {
                            style: heading,
                            ..Default::default()
                        },
                        ..LayoutSettings::default()
                    };
                    let engine = TypstEngine::new(template::generate_with_layout(&resume, &layout));
                    let (_, geometry) = engine.compile_to_pixels(1.0).expect("compiles");

                    for (what, value) in [
                        ("last_page_used_pt", geometry.last_page_used_pt),
                        ("page_height_pt", geometry.page_height_pt),
                        ("overflow_pt", geometry.overflow_pt),
                        (
                            "last_page_content_top_pt",
                            geometry.last_page_content_top_pt,
                        ),
                    ] {
                        assert!(
                            value.is_finite(),
                            "{what} was {value} for {} / {} / {}",
                            style.label(),
                            spacing.label(),
                            heading.label()
                        );
                    }
                }
            }
        }
    }
}

mod vendored_package_tests {
    use super::*;

    #[test]
    fn the_vendored_package_compiles_with_no_external_dependencies() {
        let engine = TypstEngine::new(
            "#import \"altacv/lib.typ\": alta\n\
             #alta((basics: (name: \"Test\", email: \"a@b.co\", url: \"https://b.co\")))\n",
        );
        let attempt = engine.compile_with_diagnostics(1.0);
        assert!(
            attempt.result.is_ok(),
            "vendored AltaCV did not compile: {:#?}",
            attempt.diagnostics
        );
    }

    #[test]
    fn the_contact_icons_actually_put_ink_on_the_page() {
        let ink_of = |source: &str| {
            let (pixels, _) = TypstEngine::new(source)
                .compile_to_pixels(1.0)
                .expect("the sample compiles");
            pixels
                .rgba
                .chunks_exact(4)
                .filter(|px| px[0] < 200 || px[1] < 200 || px[2] < 200)
                .count()
        };

        let bare = ink_of("#import \"altacv/lib.typ\": alta\n#alta((basics: (name: \"Test\")))\n");
        let with_icons = ink_of(
            "#import \"altacv/lib.typ\": alta\n\
             #alta((basics: (name: \"Test\", email: \"a@b.co\", phone: \"1\")))\n",
        );
        assert!(
            with_icons > bare,
            "the contact bar drew no more ink than an empty one: {bare} vs {with_icons}"
        );
    }
}

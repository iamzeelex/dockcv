//! The conformance harness: the same CV, every layout, read every way.
//!
//! This is the measurement the rest of Track B is tuned against. It compiles
//! the fixture through each layout scenario, reads every PDF back with the
//! three in-process personalities and whatever external extractors are
//! installed, and asks of each pinned field whether it came back.
//!
//! Two properties, and the second is the one that keeps the file honest:
//!
//! 1. **Nothing outside [`KNOWN_GAPS`] may fail.** A template change that
//!    costs a parser a field fails the build.
//! 2. **Nothing inside [`KNOWN_GAPS`] may pass.** When a gap closes, its line
//!    has to go, so the list cannot quietly become a list of things that were
//!    once wrong.
//!
//! The full table lands in `target/ats-conformance.md`, external engines
//! included — derived, rebuildable, and outside the vault and the repository
//! alike.

use std::path::{Path, PathBuf};

use dockcv_core::resume::altacv;
use dockcv_core::resume::model::{
    ContactLayout, DocumentLanguage, HeaderLayout, HeadingCase, HeadingLayout, HeadingStyle,
    LayoutSettings, Resume, SectionKind, SkillsLayout, SkillsStyle,
};
use dockcv_core::resume::template;
use dockcv_core::typst_engine::TypstEngine;

use dockcv_core::resume::ats;
use dockcv_core::resume::edit::FieldId;
use dockcv_core::resume::{builtin_profile, ATS_SAFE_PROFILE};

use super::adversarial;
use super::docx;
use super::external;
use super::fields::{self, Pinned};
use super::readers;

/// A field one reading of one scenario does not recover, and the task that
/// owns the fix. Sorted the way the report prints them.
///
/// Every line here is a defect DockCV ships today, measured rather than
/// guessed. Deleting a line is how a fix is declared.
///
/// **It is empty, and that is a result rather than an oversight.** It held
/// nine lines — one per scenario, all the same field — recorded against a
/// reading called `sorted` that was `pdf_extract::extract_text_from_mem`, the
/// crate's demo sink. Measured against the readers that actually exist, it was
/// alone: `pdftotext` in all three modes, pdfminer.six, Apache PDFBox, the
/// file's own content order, its structure tree and DockCV's own importer all
/// put that bullet on its own line, in all nine readings. The page was
/// uniformly spaced where it was accused of being tight — 12.78pt from the
/// entry summary to the first bullet, and 12.78pt from that bullet to the next.
///
/// So the gap was the straw man's, and the straw man is gone: the column is
/// the importer now, which is the reader a DockCV file actually meets when
/// somebody re-imports it, and the market is measured directly by the five
/// engines `scripts/ats-tools.sh` installs.
///
/// The list stays because the next real gap goes in it, and because the test
/// below fails when a line in it starts passing — a list of things that were
/// once wrong is worse than no list.
const KNOWN_GAPS: &[Gap] = &[];

#[derive(Debug, PartialEq, Eq)]
struct Gap {
    scenario: &'static str,
    engine: &'static str,
    what: &'static str,
    /// The task that closes it, so a reader of this list knows whether it is
    /// waiting on work or on a decision.
    owner: &'static str,
}

/// One way a CV actually leaves this app: a layout, a language, and the
/// headings that reading prints.
///
/// A struct rather than a tuple because it stopped being one thing. C5 gave a
/// reading a language and C8 gave it headings of its own, and both reach the
/// page — so a harness that varied only the layout was measuring a third of
/// what ships.
struct Scenario {
    name: &'static str,
    layout: LayoutSettings,
    language: DocumentLanguage,
    /// What this reading calls its sections, when it does not use the
    /// defaults — `Preset::titles` arriving at the page.
    titles: &'static [(SectionKind, &'static str)],
}

impl Scenario {
    fn new(name: &'static str, layout: LayoutSettings) -> Self {
        Self {
            name,
            layout,
            language: DocumentLanguage::English,
            titles: &[],
        }
    }

    fn in_language(mut self, language: DocumentLanguage) -> Self {
        self.language = language;
        self
    }

    fn under(mut self, titles: &'static [(SectionKind, &'static str)]) -> Self {
        self.titles = titles;
        self
    }

    /// The fixture as this reading prints it.
    fn resume(&self) -> Resume {
        let mut resume = fixture();
        if !self.titles.is_empty() {
            resume.section_titles = self
                .titles
                .iter()
                .map(|(kind, title)| (*kind, (*title).to_string()))
                .collect();
        }
        resume
    }

    fn slug(&self) -> String {
        self.name.replace(' ', "-")
    }
}

/// A German CV names its sections in German. Both halves travel together —
/// a `lang: "de"` page under English headings is not a document anybody sends.
const GERMAN_HEADINGS: &[(SectionKind, &str)] = &[
    (SectionKind::Profile, "Profil"),
    (SectionKind::Work, "Berufserfahrung"),
    (SectionKind::Education, "Ausbildung"),
    (SectionKind::Skills, "Kenntnisse"),
    (SectionKind::Certificates, "Zertifikate"),
    (SectionKind::Organizations, "Ehrenamt"),
];

/// The layouts a CV actually gets sent in, chosen for the parser risk each one
/// carries rather than for coverage of the settings.
fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario::new("default", LayoutSettings::default()),
        Scenario::new(
            "headings as typed",
            LayoutSettings {
                headings: HeadingLayout {
                    case: HeadingCase::AsTyped,
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        Scenario::new(
            "heading rule to margin",
            LayoutSettings {
                headings: HeadingLayout {
                    style: HeadingStyle::RuleToMargin,
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        Scenario::new(
            "heading band",
            LayoutSettings {
                headings: HeadingLayout {
                    style: HeadingStyle::Band,
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        Scenario::new(
            "contacts in two columns",
            LayoutSettings {
                header: HeaderLayout {
                    contacts: ContactLayout::Columns,
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        Scenario::new(
            "skills as pills",
            LayoutSettings {
                skills: SkillsLayout {
                    style: SkillsStyle::Bubbles,
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        Scenario::new(
            "ATS-safe",
            builtin_profile(ATS_SAFE_PROFILE).expect("ATS-safe is a shipped profile"),
        ),
        // C8: a reading may print its own headings, and a heading is what
        // every parser segments a CV on. Renaming one is the single change
        // most able to cost a whole section, so it is measured.
        Scenario::new("renamed headings", LayoutSettings::default()).under(&[
            (SectionKind::Profile, "Summary"),
            (SectionKind::Work, "Professional Experience"),
            (SectionKind::Skills, "Core Competencies"),
        ]),
        // C5: a whole reading in another language — the `/Lang` tag, the
        // localized month names, and the German headings that go with them.
        Scenario::new("German", LayoutSettings::default())
            .in_language(DocumentLanguage::German)
            .under(GERMAN_HEADINGS),
    ]
}

fn fixture() -> Resume {
    altacv::import(altacv::ALTACV_SAMPLE).expect("the AltaCV fixture parses")
}

fn compile(resume: &Resume, layout: &LayoutSettings, language: DocumentLanguage) -> Vec<u8> {
    TypstEngine::new(template::generate_with_layout_and_language(
        resume, layout, language,
    ))
    .compile_to_pdf()
    .expect("the fixture compiles to PDF")
}

fn target_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target")
}

/// One scenario's readings: every engine that answered, in report order.
fn readings(pdf: &[u8], path: &Path) -> Vec<(String, String)> {
    let mut out = vec![
        (
            "content order".to_string(),
            readers::content_order(pdf).unwrap_or_default(),
        ),
        (
            "our importer".to_string(),
            crate::import::engines::pdf::read_as_the_importer_does(path).unwrap_or_default(),
        ),
        (
            "structure tree".to_string(),
            readers::structure_text(pdf).unwrap_or_default(),
        ),
    ];
    for found in external::read_all(path) {
        out.push((found.engine.to_string(), found.text));
    }
    out
}

/// `✓` / `✗` per engine, one row per pinned field.
fn table(engines: &[String], rows: &[(Pinned, Vec<bool>)]) -> String {
    let mut out = String::new();
    out.push_str("| field |");
    for engine in engines {
        out.push_str(&format!(" {engine} |"));
    }
    out.push_str("\n|---|");
    out.push_str(&"---|".repeat(engines.len()));
    out.push('\n');
    for (pinned, results) in rows {
        out.push_str(&format!("| {} |", pinned.what));
        for ok in results {
            out.push_str(if *ok { " ✓ |" } else { " ✗ |" });
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_file_reads_the_same_way_whoever_reads_it() {
    let resume = fixture();
    let pinned = fields::pin(&resume);
    assert!(
        pinned.len() > 30,
        "the fixture should pin a whole CV, got {} fields",
        pinned.len()
    );

    let mut report = String::from(
        "# ATS conformance\n\nGenerated by `cargo test the_file_reads_the_same_way`. \
         Every row is a field of the fixture CV; every column is a way of reading the PDF \
         back. `✗` is a field that reading does not recover.\n\n",
    );
    let mut failures: Vec<String> = Vec::new();
    let mut closed: Vec<String> = Vec::new();

    for case in scenarios() {
        let scenario = case.name;
        // Per scenario, because a reading that renames its headings or writes
        // them in German pins different strings — the whole point of measuring
        // those two.
        let resume = case.resume();
        let pinned = fields::pin(&resume);
        let pdf = compile(&resume, &case.layout, case.language);
        let path = std::env::temp_dir().join(format!("dockcv-ats-{}.pdf", case.slug()));
        std::fs::write(&path, &pdf).expect("write the PDF the external engines read");

        let readings = readings(&pdf, &path);
        let engines: Vec<String> = readings.iter().map(|(e, _)| e.clone()).collect();

        let rows: Vec<(Pinned, Vec<bool>)> = pinned
            .iter()
            .map(|p| {
                let results = readings
                    .iter()
                    .map(|(_, text)| p.recovered(text))
                    .collect::<Vec<_>>();
                (p.clone(), results)
            })
            .collect();

        for (pinned, results) in &rows {
            for (engine, ok) in engines.iter().zip(results) {
                let known = KNOWN_GAPS.iter().find(|g| {
                    g.scenario == scenario && g.engine == engine && g.what == pinned.what
                });
                match (ok, known) {
                    (false, None) => {
                        failures.push(format!("{scenario} · {engine} · {}", pinned.what))
                    }
                    (true, Some(gap)) => closed.push(format!(
                        "{} · {} · {} (owner: {})",
                        gap.scenario, gap.engine, gap.what, gap.owner
                    )),
                    _ => {}
                }
            }
        }

        report.push_str(&format!("\n## {scenario}\n\n"));
        report.push_str(&table(&engines, &rows));
    }

    let path = target_dir().join("ats-conformance.md");
    let _ = std::fs::write(&path, &report);

    assert!(
        closed.is_empty(),
        "recorded in KNOWN_GAPS and now passing — delete their lines:\n{}",
        closed.join("\n")
    );
    assert!(
        failures.is_empty(),
        "{} reading(s) lose a field that is not a known gap. \
         The full table is in target/ats-conformance.md.\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_section_the_page_prints_is_a_heading_in_the_structure_tree() {
    let resume = fixture();
    let pdf = compile(&resume, &LayoutSettings::default(), DocumentLanguage::English);
    let tree = readers::structure(&pdf).expect("the PDF we just wrote has a structure tree");

    let tagged_headings: Vec<String> = tree
        .iter()
        .filter(|t| {
            t.tag.len() == 2 && t.tag.starts_with('H') && t.tag.as_bytes()[1].is_ascii_digit()
        })
        .map(|t| fields::normalize(&t.text))
        .collect();
    assert!(
        !tagged_headings.is_empty(),
        "the exported PDF carries no heading tags at all — the section bar has stopped \
         being a `heading` element, and a tag-aware parser is back to guessing"
    );

    for pinned in fields::pin(&resume)
        .iter()
        .filter(|p| p.what.starts_with("heading"))
    {
        let needle = fields::normalize(&pinned.needle);
        assert!(
            tagged_headings.iter().any(|h| h == &needle),
            "“{}” is printed as a section bar but is not an /H tag in the structure tree; \
             tagged headings are {:?}",
            pinned.needle,
            tagged_headings
        );
    }
}

#[test]
fn every_layout_we_offer_exports_a_file_pdf_ua1_accepts() {
    for case in scenarios() {
        let engine = TypstEngine::new(template::generate_with_layout_and_language(
            &case.resume(),
            &case.layout,
            case.language,
        ));
        if let Err(why) = engine.compile_to_pdf_ua1() {
            let scenario = case.name;
            panic!(
                "“{scenario}” would export a file PDF/UA-1 refuses, and its rules are \
                 most of what a parser needs too:\n{why}"
            );
        }
    }
}

/// The Word file, read by three readers that are not each other.
///
/// `export_docx` takes a `Resume` and no layout, so there is one file here
/// rather than six: the scenarios above are decisions about a page, and this
/// format has none.
fn word_readings(path: &std::path::Path, bytes: &[u8]) -> Vec<(String, String)> {
    let mut out = vec![(
        "paragraphs".to_string(),
        docx::flat_text(bytes).unwrap_or_default(),
    )];
    for found in external::read_all_docx(path) {
        out.push((found.engine.to_string(), found.text));
    }
    out
}

#[test]
fn the_word_file_reads_the_same_way_whoever_reads_it() {
    let resume = fixture();
    let bytes = dockcv_core::resume::export::docx::export_docx(&resume)
        .expect("the fixture exports to .docx");
    let path = std::env::temp_dir().join("dockcv-ats.docx");
    std::fs::write(&path, &bytes).expect("write the file the external readers open");

    let mut failures = Vec::new();
    for (engine, text) in word_readings(&path, &bytes) {
        for pinned in fields::pin(&resume) {
            if !pinned.recovered(&text) {
                failures.push(format!("{engine} · {}", pinned.what));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} field(s) a reader of our .docx does not recover:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_word_file_says_what_its_headings_and_its_lists_are() {
    let resume = fixture();
    let bytes = dockcv_core::resume::export::docx::export_docx(&resume)
        .expect("the fixture exports to .docx");
    let paragraphs = docx::paragraphs(&bytes).expect("we can read back what we just wrote");

    for pinned in fields::pin(&resume)
        .iter()
        .filter(|p| p.what.starts_with("heading"))
    {
        let needle = fields::normalize(&pinned.needle);
        let found = paragraphs
            .iter()
            .find(|p| fields::normalize(&p.text) == needle);
        let Some(found) = found else {
            panic!("“{}” is not a paragraph of the .docx at all", pinned.needle);
        };
        assert!(
            found.is_heading(),
            "“{}” is printed as a section but is neither a heading style nor an \
             outline level, so a reader looking for headings finds none. Style was \
             {:?}, outline level {:?}",
            pinned.needle,
            found.style,
            found.outline
        );
    }

    for job in &resume.work {
        for bullet in &job.highlights {
            let needle = fields::normalize(bullet);
            let found = paragraphs
                .iter()
                .find(|p| fields::normalize(&p.text) == needle);
            let Some(found) = found else {
                panic!("a bullet of {:?} is not a paragraph of the .docx", job.name);
            };
            assert!(
                found.list,
                "a bullet of {:?} carries no numbering property, so it is a paragraph \
                 that happens to be short rather than an item in a list",
                job.name
            );
        }
    }
}

/// Every attack that lands is one the lint saw coming.
///
/// This is the tie between the two halves of the track, and it is the property
/// that keeps either half from rotting. The adversarial documents in
/// `ats::adversarial` are written to break the pipeline; each one that still
/// takes a field must have a lint finding pointing at *that field*, so the
/// author is told before they send it rather than after nobody replies.
///
/// A failure here means one of three things, and all three are worth stopping
/// for: a new defect in the export, a rule the lint is missing, or an attack
/// that has been fixed and whose expectation should now be that it lands on
/// nothing.
#[test]
fn every_attack_that_lands_is_one_the_lint_saw_coming() {
    let mut unpredicted: Vec<String> = Vec::new();

    for adversary in adversarial::all() {
        let pinned = fields::pin(&adversary.resume);
        let warned: Vec<FieldId> = ats::lint(&adversary.resume, DocumentLanguage::English)
            .into_iter()
            .filter_map(|f| f.at)
            .collect();

        let pdf = TypstEngine::new(template::generate(&adversary.resume))
            .compile_to_pdf()
            .unwrap_or_else(|why| panic!("“{}” does not compile at all: {why}", adversary.name));
        let path = std::env::temp_dir().join(format!(
            "dockcv-adv-{}.pdf",
            adversary.name.replace(' ', "-")
        ));
        std::fs::write(&path, &pdf).expect("write");

        let bytes = dockcv_core::resume::export::docx::export_docx(&adversary.resume)
            .unwrap_or_else(|why| panic!("“{}” does not export to .docx: {why}", adversary.name));
        let word_path = std::env::temp_dir().join(format!(
            "dockcv-adv-{}.docx",
            adversary.name.replace(' ', "-")
        ));
        std::fs::write(&word_path, &bytes).expect("write");

        let readings = readings(&pdf, &path)
            .into_iter()
            .map(|(engine, text)| (format!("PDF {engine}"), text))
            .chain(
                word_readings(&word_path, &bytes)
                    .into_iter()
                    .map(|(engine, text)| (format!("DOCX {engine}"), text)),
            );

        for (engine, text) in readings {
            for lost in pinned.iter().filter(|p| !p.recovered(&text)) {
                let predicted = lost.at.is_some_and(|at| warned.contains(&at));
                if !predicted {
                    unpredicted.push(format!(
                        "{} · {engine} · {} — and the lint says nothing about it.\n                             That document exists to test: {}",
                        adversary.name, lost.what, adversary.attacks
                    ));
                }
            }
        }
    }

    assert!(
        unpredicted.is_empty(),
        "{} field(s) an adversary took without warning:\n{}",
        unpredicted.len(),
        unpredicted.join("\n")
    );
}

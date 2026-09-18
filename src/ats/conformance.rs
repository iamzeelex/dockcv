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
    ContactLayout, HeaderLayout, HeadingCase, HeadingLayout, HeadingStyle, LayoutSettings, Resume,
    SkillsLayout, SkillsStyle,
};
use dockcv_core::resume::template;
use dockcv_core::typst_engine::TypstEngine;

use super::external;
use super::fields::{self, Pinned};
use super::readers;

/// A field one reading of one scenario does not recover, and the task that
/// owns the fix. Sorted the way the report prints them.
///
/// Every line here is a defect DockCV ships today, measured rather than
/// guessed. Deleting a line is how a fix is declared.
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

/// The layouts a CV actually gets sent in, chosen for the parser risk each one
/// carries rather than for coverage of the settings.
fn scenarios() -> Vec<(&'static str, LayoutSettings)> {
    vec![
        ("default", LayoutSettings::default()),
        (
            "headings as typed",
            LayoutSettings {
                headings: HeadingLayout {
                    case: HeadingCase::AsTyped,
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        (
            "heading rule to margin",
            LayoutSettings {
                headings: HeadingLayout {
                    style: HeadingStyle::RuleToMargin,
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        (
            "heading band",
            LayoutSettings {
                headings: HeadingLayout {
                    style: HeadingStyle::Band,
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        (
            "contacts in two columns",
            LayoutSettings {
                header: HeaderLayout {
                    contacts: ContactLayout::Columns,
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        (
            "skills as pills",
            LayoutSettings {
                skills: SkillsLayout {
                    style: SkillsStyle::Bubbles,
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
    ]
}

fn fixture() -> Resume {
    altacv::import(altacv::ALTACV_SAMPLE).expect("the AltaCV fixture parses")
}

fn compile(resume: &Resume, layout: &LayoutSettings) -> Vec<u8> {
    TypstEngine::new(template::generate_with_layout(resume, layout))
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
            "sorted".to_string(),
            readers::sorted(pdf).unwrap_or_default(),
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

    for (scenario, layout) in scenarios() {
        let pdf = compile(&resume, &layout);
        let path =
            std::env::temp_dir().join(format!("dockcv-ats-{}.pdf", scenario.replace(' ', "-")));
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
    let pdf = compile(&resume, &LayoutSettings::default());
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

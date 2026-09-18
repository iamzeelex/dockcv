//! CVs written to break the pipeline.
//!
//! The conformance harness reads one tidy document eight ways. That proves the
//! path works; it does not prove the path is *safe*, because the fixture was
//! written by someone who knew what the code did. Everything here was written
//! the other way round — one attack per document, each aimed at a place where
//! a page and its text layer are known to come apart:
//!
//! - **ligatures**, where `office` is one glyph and comes back as one character;
//! - **combining marks**, where `José` is five characters or six depending on
//!   who typed it, and only one of the two matches a search;
//! - **hyphenation**, where a justified line breaks a word and the text layer
//!   keeps the break;
//! - **scripts** we claim to support and scripts we do not;
//! - **the shapes a CV actually contains**: `C++`, `A/B`, `40%`, `p99`, a URL
//!   with a query string, a name with an apostrophe;
//! - **the second page**, where a PDF's fonts and a parser's assumptions both
//!   get a chance to go wrong.
//!
//! A document here is not a regression test for a bug that happened. It is a
//! question nobody had asked.

use dockcv_core::resume::model::{Basics, Certificate, Education, Resume, SkillGroup, Work};

/// One attack, and what it is aimed at.
pub struct Adversary {
    pub name: &'static str,
    /// What this document is trying to break, in one line, for the report.
    pub attacks: &'static str,
    pub resume: Resume,
}

fn job(position: &str, employer: &str, highlights: &[&str]) -> Work {
    Work {
        name: employer.into(),
        position: position.into(),
        start_date: dockcv_core::resume::dates::ResumeDate::new("2019-06"),
        end_date: dockcv_core::resume::dates::ResumeDate::new("2022-01"),
        highlights: highlights.iter().map(|h| (*h).to_string()).collect(),
        ..Default::default()
    }
}

fn person(name: &str, label: &str) -> Basics {
    Basics {
        name: name.into(),
        label: label.into(),
        email: "person@example.com".into(),
        phone: "+353 1 555 0100".into(),
        location: "Dublin".into(),
        ..Default::default()
    }
}

pub fn all() -> Vec<Adversary> {
    vec![
        Adversary {
            name: "ligatures",
            attacks: "fi, ffi and fl are one glyph in a serif face, and a text layer \
                      that keeps the ligature codepoint turns `office` into a word no \
                      keyword search finds",
            resume: Resume {
                basics: person("Fiona Griffiths", "Office Workflow Architect"),
                work: vec![job(
                    "Efficiency Officer",
                    "Affluent Fields",
                    &[
                        "Rebuilt the office workflow to make fulfilment sufficiently efficient.",
                        "Staffed a difficult affiliate office in Sheffield.",
                    ],
                )],
                skills: vec![SkillGroup {
                    name: "Tools".into(),
                    keywords: vec!["Workflow".into(), "Office".into(), "Firefly".into()],
                }],
                ..Default::default()
            },
        },
        Adversary {
            name: "combining marks",
            attacks: "the same name typed two ways: `José` as one character and as `e` \
                      plus a combining acute. A parser matching the other form finds \
                      nobody",
            resume: Resume {
                // Deliberately decomposed (NFD): this is what a macOS filename
                // and several keyboards produce.
                basics: person(
                    "Jose\u{0301} O\u{0301} Murchu\u{0301}",
                    "Se\u{0301}nior Engineer",
                ),
                work: vec![job(
                    "Inge\u{0301}nieur",
                    "Ame\u{0301}lie SA",
                    &["Shipped the re\u{0301}sume\u{0301} pipeline for Que\u{0301}bec."],
                )],
                ..Default::default()
            },
        },
        Adversary {
            name: "hyphenation",
            attacks: "a justified column breaking a long word at the line end, and the \
                      text layer keeping the hyphen",
            resume: Resume {
                basics: person("Wilhelmina Rasmussen", "Internationalization Lead"),
                work: vec![job(
                    "Infrastructure Engineer",
                    "Telecommunications GmbH",
                    &[
                        "Led the internationalization of a telecommunications infrastructure \
                         platform, standardizing interoperability across counterparties and \
                         institutionalizing the responsibilities of every subcontractor.",
                        "Reorganized the organizational responsibilities of the \
                         infrastructure team around uninterruptible deployments.",
                    ],
                )],
                ..Default::default()
            },
        },
        Adversary {
            name: "cyrillic",
            attacks: "a Ukrainian CV end to end — the script this product says it \
                      supports, in a face that has to cover it",
            resume: Resume {
                basics: Basics {
                    name: "Олена Ковальчук".into(),
                    label: "Провідна інженерка".into(),
                    email: "olena@example.com".into(),
                    phone: "+380 44 555 0100".into(),
                    location: "Київ".into(),
                    summary: "Інженерка з восьмирічним досвідом у розподілених системах.".into(),
                    ..Default::default()
                },
                work: vec![job(
                    "Провідна інженерка",
                    "Приватбанк",
                    &["Скоротила затримку p99 удвічі на платіжному шляху."],
                )],
                skills: vec![SkillGroup {
                    name: "Мови".into(),
                    keywords: vec!["Rust".into(), "Пайтон".into()],
                }],
                ..Default::default()
            },
        },
        Adversary {
            name: "cjk",
            attacks: "Japanese, which has no spaces to break lines at and needs a face \
                      the bundle may not have",
            resume: Resume {
                basics: Basics {
                    name: "山田太郎".into(),
                    label: "主任エンジニア".into(),
                    email: "yamada@example.com".into(),
                    location: "東京".into(),
                    ..Default::default()
                },
                work: vec![job(
                    "主任エンジニア",
                    "株式会社アクメ",
                    &["決済システムのレイテンシを半減させました。"],
                )],
                ..Default::default()
            },
        },
        Adversary {
            name: "the shapes a CV contains",
            attacks: "`C++`, `C#`, `.NET`, `A/B`, `40%`, `p99`, `3×`, an apostrophe, \
                      curly quotes, a URL with a query string — every one of them a \
                      place a naive tokeniser gives up",
            resume: Resume {
                basics: Basics {
                    name: "Siobhán O'Brien-Núñez".into(),
                    label: "C++ & .NET Architect".into(),
                    email: "siobhan+cv@example.com".into(),
                    phone: "+1 (415) 555‑0134".into(),
                    url: "example.com/cv?ref=linkedin&utm_source=cv".into(),
                    ..Default::default()
                },
                work: vec![job(
                    "Staff Engineer",
                    "Smith & Wesson Ltd.",
                    &[
                        "Cut p99 latency by 40% and raised throughput 3× on the A/B path.",
                        "Migrated a C++/C# estate to .NET 8 — “without a maintenance window”.",
                        "Saved $1.2M/year by rewriting the ETL (≈18 000 lines).",
                    ],
                )],
                skills: vec![SkillGroup {
                    name: "Languages".into(),
                    keywords: vec!["C++".into(), "C#".into(), "F#".into(), ".NET".into()],
                }],
                ..Default::default()
            },
        },
        Adversary {
            name: "two pages",
            attacks: "the page boundary: a PDF's second page is where fonts, reading \
                      order and a parser's patience have all failed before",
            resume: Resume {
                basics: person("Bartholomew Winterbourne", "Principal Engineer"),
                work: (0..8)
                    .map(|i| {
                        job(
                            &format!("Engineer Grade {i}"),
                            &format!("Company Number {i}"),
                            &[
                                "Rebuilt a subsystem nobody had touched in four years.",
                                "Halved the time an on-call week costs a team of nine.",
                            ],
                        )
                    })
                    .collect(),
                education: vec![Education {
                    institution: "Trinity College Dublin".into(),
                    study_type: "M.Sc. in Computer Science".into(),
                    ..Default::default()
                }],
                certificates: vec![Certificate {
                    name: "Certified Kubernetes Administrator".into(),
                    issuer: "CNCF".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        },
        Adversary {
            name: "whitespace nobody sees",
            attacks: "a non-breaking space inside a phone number, a tab in a field, a \
                      double space, a zero-width joiner — invisible on the page and \
                      fatal to an exact match",
            resume: Resume {
                basics: Basics {
                    name: "Anna\u{00a0}Maria Nowak".into(),
                    label: "Data\tEngineer".into(),
                    email: "anna@example.com".into(),
                    phone: "+48\u{00a0}22\u{2009}555\u{2009}0100".into(),
                    location: "Warszawa".into(),
                    ..Default::default()
                },
                work: vec![job(
                    "Data Engineer",
                    "Orlen  S.A.",
                    &["Built  the  warehouse  everyone  now  reports  from."],
                )],
                ..Default::default()
            },
        },
    ]
}

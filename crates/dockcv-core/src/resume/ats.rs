//! What a parser will read differently from a person.
//!
//! Six rules, each of them a **fact about this document** rather than an
//! opinion about the writing. They are deterministic, they name the field they
//! are about, and none of them returns a number: a score out of a hundred is
//! the thing this whole track exists to be an answer to, and a confident number
//! invented over six boolean checks would be the same snake oil in a smaller
//! bottle.
//!
//! What is *not* here is as deliberate as what is. Whether the text layer
//! survives at all, whether a section heading reaches an extractor unbroken,
//! whether a bullet is a list item — those were once meant to be rules, and
//! they turned out to be properties of the *template*, true or false for
//! everybody at once. They are asserted in `src/ats/conformance.rs`, where a
//! regression fails the build, rather than reported to a person who cannot act
//! on them. A lint is for what the author chose.
//!
//! Each rule carries the [`FieldId`] of the field it is about, so a caller can
//! put the cursor there rather than describing where to look.

use super::dates::{CivilDate, ResumeDate};
use super::edit::FieldId;
use super::export_text::strip_typst_markup;
use super::export_walk::{is_section_empty, ordered_sections, resolve_section_title};
use super::model::{Resume, SectionKind};

/// One fact about the document, and where it lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: Rule,
    /// The section the finding belongs under, for grouping.
    pub section: SectionKind,
    /// The field to put the cursor in. `None` when the finding is about the
    /// document rather than about one field — a missing section heading has no
    /// field to focus.
    pub at: Option<FieldId>,
}

/// The six. Each carries the text it is about, so the caller decides the
/// wording and this module never writes a sentence for a screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// A section is printed under a name that is not one of the handful every
    /// parser's vocabulary holds. The section is still read; what is lost is
    /// its *kind*, which is how a parser decides that the lines under it are
    /// jobs.
    HeadingNoParserKnows {
        title: String,
        expected: &'static str,
    },
    /// A date nothing can read as a date: not empty, not "Present", and not
    /// parseable. It prints exactly as typed, which is right, and a parser
    /// computing tenure from it gets nothing.
    DateNoParserCanRead { text: String },
    /// The end is before the start. Whatever a parser makes of it, it is not
    /// what the person meant.
    DatesRunBackwards { start: String, end: String },
    /// Neither an email address nor a telephone number. A parser that cannot
    /// key a candidate on one of the two has nowhere to file the application.
    NoWayToReachThePerson,
    /// Markup that never renders anywhere: not on the page, not in any export.
    ///
    /// Measured twice, and wrong the first time. `*bold*` and `_italic_` are
    /// live — they set as emphasis and come back as their words, so there is
    /// nothing to report. A *function* call is not: `template.rs::neutralize`
    /// escapes `#` before Typst sees it, deliberately, because `C#` and `$1.2M`
    /// are things people write and a live `#` takes the whole document down
    /// with it. So `#strong[p99]` prints as `#strong[p99]` on the page, and
    /// `strip_typst_markup` leaves it alone in the text, Markdown and Word
    /// exports too.
    ///
    /// Which makes this the mildest rule here and still worth having: nothing
    /// is lost, but the author typed something meaning bold and has not been
    /// told that a person and a parser will both read it out as source.
    MarkupThatNeverRenders { text: String, in_the_text: String },
    /// A bullet typed with its own marker, inside a list that already draws
    /// one. The reader gets the glyph twice, or gets it where it expects the
    /// first word.
    ListMarkerTypedIntoTheText { text: String },
    /// Characters no bundled face can set. The page prints holes where they
    /// should be and the text layer disagrees with itself between extractors —
    /// measured on a Japanese CV, whose PDF embeds Libertinus and a maths face
    /// and not one glyph of kanji.
    CharactersNoFaceCanSet { text: String, missing: Vec<char> },
    /// Whitespace that is not a space: a non-breaking space inside a telephone
    /// number, a thin space between digit groups, a tab in a field. Invisible
    /// to the author, and measured to cost the field — `pdftotext -raw` returns
    /// a phone number written with thin spaces as one unbroken run of digits,
    /// which is not the number anybody searched for.
    WhitespaceNobodySees { text: String, characters: Vec<char> },
}

/// Whitespace a reader cannot see and a parser cannot ignore: the no-break
/// space, the three thin ones, the zero-width joiner and its friends, and a
/// tab. A space typed as one of these is a space to the eye and a different
/// character to every exact match ever written.
const INVISIBLE: &[char] = &[
    '\u{0009}', // tab
    '\u{00a0}', // no-break space
    '\u{2007}', // figure space
    '\u{2009}', // thin space
    '\u{200a}', // hair space
    '\u{200b}', // zero-width space
    '\u{200c}', // zero-width non-joiner
    '\u{200d}', // zero-width joiner
    '\u{202f}', // narrow no-break space
    '\u{feff}', // byte-order mark, which arrives by paste
];

/// The names a section can be printed under and still be recognised.
///
/// Deliberately **short**, and deliberately not the importer's taxonomy. The
/// two answer opposite questions. `import::classifier` asks "did somebody mean
/// a section here?" over other people's CVs, so it is generous and matches
/// within a Levenshtein distance of two — which would happily accept `Skilz`.
/// This asks "will a stranger's parser know this heading?", where being
/// generous means telling a person their invention is safe when it is not.
///
/// English is the canon because that is what the parsers were trained and
/// written on. The Cyrillic names are here because a CV written in Ukrainian or
/// Russian is a CV this product expects, and flagging every heading in it would
/// be a lint that taught people to ignore it. Adding a language is one line.
const KNOWN_HEADINGS: &[(SectionKind, &[&str])] = &[
    (
        SectionKind::Profile,
        &[
            "profile",
            "summary",
            "professional summary",
            "about",
            "about me",
            "objective",
            "career objective",
            "про себе",
            "о себе",
            "резюме",
        ],
    ),
    (
        SectionKind::Work,
        &[
            "work experience",
            "experience",
            "professional experience",
            "employment",
            "employment history",
            "work history",
            "career history",
            "досвід роботи",
            "опыт работы",
        ],
    ),
    (
        SectionKind::Education,
        &[
            "education",
            "education and training",
            "academic background",
            "освіта",
            "образование",
        ],
    ),
    (
        SectionKind::Skills,
        &[
            "skills",
            "technical skills",
            "core competencies",
            "competencies",
            "навички",
            "навыки",
        ],
    ),
    (
        SectionKind::Certificates,
        &[
            "certifications",
            "certificates",
            "licenses and certifications",
            "licences and certifications",
            "сертифікати",
            "сертификаты",
        ],
    ),
    (
        SectionKind::Organizations,
        &[
            "volunteering",
            "volunteer experience",
            "volunteer",
            "organizations",
            "organisations",
            "community involvement",
            "волонтерство",
            "громадська діяльність",
        ],
    ),
];

/// Every printed string in the document, with the field it came from.
///
/// One walk rather than a rule each: two of the six are about *characters*
/// rather than about structure, and a second traversal of the same fields is
/// how the two drift apart.
fn printed_fields(resume: &Resume) -> Vec<(SectionKind, FieldId, String)> {
    let mut out: Vec<(SectionKind, FieldId, String)> = vec![
        (
            SectionKind::Profile,
            FieldId::Name,
            resume.basics.name.clone(),
        ),
        (
            SectionKind::Profile,
            FieldId::Label,
            resume.basics.label.clone(),
        ),
        (
            SectionKind::Profile,
            FieldId::Summary,
            resume.basics.summary.clone(),
        ),
        (
            SectionKind::Profile,
            FieldId::Email,
            resume.basics.email.clone(),
        ),
        (
            SectionKind::Profile,
            FieldId::Phone,
            resume.basics.phone.clone(),
        ),
        (
            SectionKind::Profile,
            FieldId::Location,
            resume.basics.location.clone(),
        ),
    ];
    for (i, job) in resume.work.iter().enumerate() {
        out.push((
            SectionKind::Work,
            FieldId::WorkPosition(i),
            job.position.clone(),
        ));
        out.push((SectionKind::Work, FieldId::WorkName(i), job.name.clone()));
        out.push((
            SectionKind::Work,
            FieldId::WorkLocation(i),
            job.location.clone(),
        ));
        out.push((
            SectionKind::Work,
            FieldId::WorkSummary(i),
            job.summary.clone(),
        ));
        for (j, h) in job.highlights.iter().enumerate() {
            out.push((SectionKind::Work, FieldId::WorkHighlight(i, j), h.clone()));
        }
    }
    for (i, school) in resume.education.iter().enumerate() {
        out.push((
            SectionKind::Education,
            FieldId::EduInstitution(i),
            school.institution.clone(),
        ));
        out.push((
            SectionKind::Education,
            FieldId::EduStudyType(i),
            school.study_type.clone(),
        ));
    }
    for (i, group) in resume.skills.iter().enumerate() {
        out.push((
            SectionKind::Skills,
            FieldId::SkillName(i),
            group.name.clone(),
        ));
        for (j, kw) in group.keywords.iter().enumerate() {
            out.push((SectionKind::Skills, FieldId::SkillKeyword(i, j), kw.clone()));
        }
    }
    for (i, cert) in resume.certificates.iter().enumerate() {
        out.push((
            SectionKind::Certificates,
            FieldId::CertName(i),
            cert.name.clone(),
        ));
        out.push((
            SectionKind::Certificates,
            FieldId::CertIssuer(i),
            cert.issuer.clone(),
        ));
    }
    for (i, role) in resume.volunteer.iter().enumerate() {
        out.push((
            SectionKind::Organizations,
            FieldId::VolPosition(i),
            role.position.clone(),
        ));
        out.push((
            SectionKind::Organizations,
            FieldId::VolOrg(i),
            role.organization.clone(),
        ));
    }
    out.retain(|(_, _, text)| !text.trim().is_empty());
    out
}

/// Every finding in this document, in the order a reader meets them.
///
/// The composed résumé, not the document: what a parser sees is the active
/// variant of each section, and a sentence sitting in a variant nobody sends is
/// not a defect in what was sent.
pub fn lint(resume: &Resume) -> Vec<Finding> {
    let mut out = Vec::new();

    if resume.basics.email.trim().is_empty() && resume.basics.phone.trim().is_empty() {
        out.push(Finding {
            rule: Rule::NoWayToReachThePerson,
            section: SectionKind::Profile,
            at: Some(FieldId::Email),
        });
    }

    check_markup(
        &resume.basics.summary,
        SectionKind::Profile,
        FieldId::Summary,
        &mut out,
    );

    for kind in ordered_sections(resume) {
        if is_section_empty(resume, kind) || matches!(kind, SectionKind::Custom(_)) {
            // A custom section is a name the person invented on purpose, and
            // saying "no parser knows this" about a section called
            // `Publications` is advice to stop having one.
            continue;
        }
        let Some(known) = KNOWN_HEADINGS.iter().find(|(k, _)| *k == kind) else {
            continue;
        };
        let title = resolve_section_title(resume, kind);
        if title.trim().is_empty() {
            continue;
        }
        if !known.1.contains(&normalize_heading(&title).as_str()) {
            out.push(Finding {
                rule: Rule::HeadingNoParserKnows {
                    title,
                    expected: known.1[0],
                },
                section: kind,
                at: None,
            });
        }
    }

    for (section, at, text) in printed_fields(resume) {
        let missing = crate::typst_engine::characters_no_bundled_face_can_set(&text);
        if !missing.is_empty() {
            out.push(Finding {
                rule: Rule::CharactersNoFaceCanSet {
                    text: text.clone(),
                    missing,
                },
                section,
                at: Some(at),
            });
        }
        let invisible: Vec<char> = text
            .chars()
            .filter(|c| INVISIBLE.contains(c))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if !invisible.is_empty() {
            out.push(Finding {
                rule: Rule::WhitespaceNobodySees {
                    text,
                    characters: invisible,
                },
                section,
                at: Some(at),
            });
        }
    }

    for (i, job) in resume.work.iter().enumerate() {
        check_dates(
            &job.start_date,
            &job.end_date,
            SectionKind::Work,
            FieldId::WorkStart(i),
            FieldId::WorkEnd(i),
            &mut out,
        );
        check_markup(
            &job.summary,
            SectionKind::Work,
            FieldId::WorkSummary(i),
            &mut out,
        );
        for (j, highlight) in job.highlights.iter().enumerate() {
            check_markup(
                highlight,
                SectionKind::Work,
                FieldId::WorkHighlight(i, j),
                &mut out,
            );
            check_marker(
                highlight,
                SectionKind::Work,
                FieldId::WorkHighlight(i, j),
                &mut out,
            );
        }
    }

    for (i, school) in resume.education.iter().enumerate() {
        check_dates(
            &school.start_date,
            &school.end_date,
            SectionKind::Education,
            FieldId::EduStart(i),
            FieldId::EduEnd(i),
            &mut out,
        );
        for (j, highlight) in school.highlights.iter().enumerate() {
            check_marker(
                highlight,
                SectionKind::Education,
                FieldId::EduHighlight(i, j),
                &mut out,
            );
        }
    }

    for (i, cert) in resume.certificates.iter().enumerate() {
        check_date(
            &cert.date,
            SectionKind::Certificates,
            FieldId::CertDate(i),
            &mut out,
        );
    }

    for (i, role) in resume.volunteer.iter().enumerate() {
        check_dates(
            &role.start_date,
            &role.end_date,
            SectionKind::Organizations,
            FieldId::VolStart(i),
            FieldId::VolEnd(i),
            &mut out,
        );
    }

    out
}

/// Case, punctuation and spacing folded away — `Work Experience`,
/// `WORK EXPERIENCE` and `Work  experience:` are one heading.
fn normalize_heading(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut space = false;
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            if space {
                out.push(' ');
                space = false;
            }
            out.extend(ch.to_lowercase());
        } else if !out.is_empty() {
            space = true;
        }
    }
    out
}

fn check_date(date: &ResumeDate, section: SectionKind, at: FieldId, out: &mut Vec<Finding>) {
    if date.is_empty() || date.names_the_present() || date.parse().is_some() {
        return;
    }
    out.push(Finding {
        rule: Rule::DateNoParserCanRead {
            text: date.text.clone(),
        },
        section,
        at: Some(at),
    });
}

fn check_dates(
    start: &ResumeDate,
    end: &ResumeDate,
    section: SectionKind,
    start_at: FieldId,
    end_at: FieldId,
    out: &mut Vec<Finding>,
) {
    check_date(start, section, start_at, out);
    check_date(end, section, end_at, out);

    if let (Some(from), Some(to)) = (start.parse(), end.parse()) {
        if as_ordinal(&to) < as_ordinal(&from) {
            out.push(Finding {
                rule: Rule::DatesRunBackwards {
                    start: start.text.clone(),
                    end: end.text.clone(),
                },
                section,
                at: Some(end_at),
            });
        }
    }
}

/// A date as one comparable number, missing parts reading as the earliest they
/// could be — `2019` starts in January, so it does not sort after `2019-03`.
fn as_ordinal(date: &CivilDate) -> (i32, u32, u32) {
    (date.year, date.month.unwrap_or(1), date.day.unwrap_or(1))
}

fn check_markup(text: &str, section: SectionKind, at: FieldId, out: &mut Vec<Finding>) {
    if text.trim().is_empty() {
        return;
    }
    let stripped = strip_typst_markup(text);
    if !holds_a_typst_call(&stripped) {
        return;
    }
    out.push(Finding {
        rule: Rule::MarkupThatNeverRenders {
            text: text.to_string(),
            in_the_text: stripped,
        },
        section,
        at: Some(at),
    });
}

/// Does this text still call a Typst function after the exporters have had
/// their turn — `#strong[…]`, `#text(…)`, `#h(1em)`?
///
/// An escaped `\#` is a hash the author wanted, and `#1` is a number: neither
/// is a call. What makes one is a name directly after the hash and a bracket
/// or parenthesis directly after the name.
fn holds_a_typst_call(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        if *ch != '#' || (i > 0 && chars[i - 1] == '\\') {
            continue;
        }
        let mut j = i + 1;
        if !chars.get(j).is_some_and(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        while chars
            .get(j)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.')
        {
            j += 1;
        }
        if matches!(chars.get(j), Some('[') | Some('(')) {
            return true;
        }
    }
    false
}

fn check_marker(text: &str, section: SectionKind, at: FieldId, out: &mut Vec<Finding>) {
    let trimmed = text.trim_start();
    let marked = ["• ", "- ", "– ", "— ", "* ", "· "]
        .iter()
        .any(|m| trimmed.starts_with(m));
    if !marked {
        return;
    }
    out.push(Finding {
        rule: Rule::ListMarkerTypedIntoTheText {
            text: text.to_string(),
        },
        section,
        at: Some(at),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Basics, Certificate, Education, SkillGroup, Work};

    /// A CV with nothing wrong with it. Every test below starts here and breaks
    /// exactly one thing, so a rule that fires on the wrong document is as
    /// visible as one that does not fire on the right one.
    fn clean() -> Resume {
        Resume {
            basics: Basics {
                name: "Seán Ó Murchú".into(),
                email: "sean@example.com".into(),
                summary: "Backend engineer with eight years of experience.".into(),
                ..Default::default()
            },
            work: vec![Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                start_date: ResumeDate::new("2019-06"),
                end_date: ResumeDate::new("2022-01"),
                highlights: vec!["Halved p99 latency on the checkout path.".into()],
                ..Default::default()
            }],
            education: vec![Education {
                institution: "Trinity".into(),
                study_type: "B.Sc.".into(),
                ..Default::default()
            }],
            skills: vec![SkillGroup {
                name: "Languages".into(),
                keywords: vec!["Rust".into()],
            }],
            certificates: vec![Certificate {
                name: "CKA".into(),
                issuer: "CNCF".into(),
                date: ResumeDate::new("2023-09"),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn rules(resume: &Resume) -> Vec<Rule> {
        lint(resume).into_iter().map(|f| f.rule).collect()
    }

    #[test]
    fn a_document_with_nothing_wrong_with_it_reports_nothing() {
        assert_eq!(lint(&clean()), Vec::new());
    }

    #[test]
    fn a_heading_a_parser_does_not_know() {
        let mut resume = clean();
        resume
            .section_titles
            .push((SectionKind::Work, "Where I've Been".into()));
        let found = lint(&resume);
        assert!(matches!(
            found.as_slice(),
            [Finding {
                rule: Rule::HeadingNoParserKnows { title, .. },
                section: SectionKind::Work,
                ..
            }] if title == "Where I've Been"
        ));

        // And the ones people actually use are left alone, in any casing.
        for title in ["EXPERIENCE", "Employment History", "Досвід роботи"] {
            let mut resume = clean();
            resume
                .section_titles
                .push((SectionKind::Work, title.into()));
            assert_eq!(lint(&resume), Vec::new(), "{title} should be recognised");
        }
    }

    #[test]
    fn a_section_nobody_else_named_is_not_second_guessed() {
        // A custom section is a name the author chose on purpose. Telling them
        // no parser knows `Publications` is advice to stop having one.
        let mut resume = clean();
        let id = resume.custom_sections.len();
        let _ = id;
        assert_eq!(lint(&resume), Vec::new());
        resume
            .section_titles
            .push((SectionKind::Skills, "Kit".into()));
        assert_eq!(rules(&resume).len(), 1);
    }

    #[test]
    fn a_date_nothing_can_read() {
        let mut resume = clean();
        resume.work[0].start_date = ResumeDate::new("Summer 2021");
        assert!(matches!(
            rules(&resume).as_slice(),
            [Rule::DateNoParserCanRead { text }] if text == "Summer 2021"
        ));

        // "Present" is not a date and is not a defect: every parser reads it,
        // and it is the one word a CV writes in an end-date field.
        let mut resume = clean();
        resume.work[0].end_date = ResumeDate::new("Present");
        assert_eq!(lint(&resume), Vec::new());

        // Nor is an empty end date, which is how this model says the same thing.
        let mut resume = clean();
        resume.work[0].end_date = ResumeDate::default();
        assert_eq!(lint(&resume), Vec::new());
    }

    #[test]
    fn dates_that_run_backwards() {
        let mut resume = clean();
        resume.work[0].start_date = ResumeDate::new("2022-01");
        resume.work[0].end_date = ResumeDate::new("2019-06");
        assert!(matches!(
            rules(&resume).as_slice(),
            [Rule::DatesRunBackwards { .. }]
        ));

        // A year against a month inside it is not backwards: 2019 begins in
        // January, so `2019 – 2019-06` is an ordinary range.
        let mut resume = clean();
        resume.work[0].start_date = ResumeDate::new("2019");
        resume.work[0].end_date = ResumeDate::new("2019-06");
        assert_eq!(lint(&resume), Vec::new());
    }

    #[test]
    fn no_way_to_reach_the_person() {
        let mut resume = clean();
        resume.basics.email = String::new();
        assert!(matches!(
            rules(&resume).as_slice(),
            [Rule::NoWayToReachThePerson]
        ));

        // Either one is enough to file an application against.
        let mut resume = clean();
        resume.basics.email = String::new();
        resume.basics.phone = "+353 1 555 0100".into();
        assert_eq!(lint(&resume), Vec::new());
    }

    #[test]
    fn markup_that_never_renders() {
        let mut resume = clean();
        resume.work[0].highlights[0] = "Halved #strong[p99] latency.".into();
        let found = lint(&resume);
        assert!(
            matches!(
                found.as_slice(),
                [Finding {
                    rule: Rule::MarkupThatNeverRenders { in_the_text, .. },
                    at: Some(FieldId::WorkHighlight(0, 0)),
                    ..
                }] if in_the_text.contains("#strong[p99]")
            ),
            "{found:?}"
        );

        // The emphasis the app documents is not a defect: these come out as
        // their words, so the page and the text say the same thing. Checked
        // against `strip_typst_markup` rather than assumed — this rule was
        // written the wrong way round first, and that is what caught it.
        for prose in [
            "Halved *p99* latency.",
            "Halved _p99_ latency.",
            "Cut p99 latency by 40% — and kept it there.",
            "Shipped C# tooling for the #1 account.",
        ] {
            let mut resume = clean();
            resume.work[0].highlights[0] = prose.into();
            assert_eq!(lint(&resume), Vec::new(), "{prose:?} should be left alone");
        }
    }

    #[test]
    fn a_bullet_that_brought_its_own_marker() {
        for typed in ["• Halved p99 latency.", "- Halved p99 latency."] {
            let mut resume = clean();
            resume.work[0].highlights[0] = typed.into();
            assert!(
                rules(&resume)
                    .iter()
                    .any(|r| matches!(r, Rule::ListMarkerTypedIntoTheText { .. })),
                "{typed:?} should be reported"
            );
        }

        // A dash inside the sentence is a dash, not a marker.
        let mut resume = clean();
        resume.work[0].highlights[0] = "Halved p99 latency - and kept it there.".into();
        assert_eq!(lint(&resume), Vec::new());
    }

    #[test]
    fn every_finding_names_a_field_or_a_section_worth_naming() {
        let mut resume = clean();
        resume.work[0].start_date = ResumeDate::new("Summer 2021");
        resume.work[0].highlights[0] = "• Halved #strong[p99] latency.".into();
        resume.basics.email = String::new();
        let found = lint(&resume);
        assert!(found.len() >= 4, "expected several findings, got {found:?}");
        for finding in &found {
            match &finding.rule {
                // The only rule with nothing to focus: a heading is drawn from
                // the section's title, which the editor addresses differently.
                Rule::HeadingNoParserKnows { .. } => {}
                other => assert!(
                    finding.at.is_some(),
                    "{other:?} should name the field it is about"
                ),
            }
        }
    }
    #[test]
    fn the_fixture_every_other_test_is_built_on_has_nothing_wrong_with_it() {
        // The tie between the two halves of this track: the document the
        // conformance harness compiles, exports and reads eight ways is also a
        // document this lint has nothing to say about. If a rule starts firing
        // here it is either a real defect in the fixture or a rule that cries
        // wolf, and both are worth stopping for.
        let resume = crate::resume::altacv::import(crate::resume::altacv::ALTACV_SAMPLE)
            .expect("the AltaCV fixture parses");
        assert_eq!(lint(&resume), Vec::new());
    }
    #[test]
    fn characters_no_bundled_face_can_set() {
        // A Japanese CV compiles, and the PDF it produces embeds Libertinus, a
        // maths face, and not one glyph of kanji. The page has holes in it and
        // the extractors disagree with each other about what is left.
        let mut resume = clean();
        resume.basics.name = "山田太郎".into();
        let found = lint(&resume);
        assert!(
            matches!(
                found.as_slice(),
                [Finding {
                    rule: Rule::CharactersNoFaceCanSet { missing, .. },
                    at: Some(FieldId::Name),
                    ..
                }] if missing.contains(&'山')
            ),
            "{found:?}"
        );

        // Cyrillic is covered by four of the five document faces and must not
        // be reported — a lint that fires on Ukrainian is a lint nobody reads.
        let mut resume = clean();
        resume.basics.name = "Олена Ковальчук".into();
        resume.work[0].highlights[0] = "Скоротила затримку p99 удвічі.".into();
        assert_eq!(lint(&resume), Vec::new());

        // Nor do the shapes a CV is actually full of.
        let mut resume = clean();
        resume.basics.label = "C++ & .NET Architect — 40% ↑ 3×".into();
        resume.work[0].highlights[0] = "Saved $1.2M/year (≈18 000 lines).".into();
        assert_eq!(lint(&resume), Vec::new());
    }

    #[test]
    fn whitespace_nobody_sees() {
        let mut resume = clean();
        resume.basics.phone = "+48\u{00a0}22\u{2009}555\u{2009}0100".into();
        let found = lint(&resume);
        assert!(
            matches!(
                found.as_slice(),
                [Finding {
                    rule: Rule::WhitespaceNobodySees { characters, .. },
                    at: Some(FieldId::Phone),
                    ..
                }] if characters.contains(&'\u{00a0}') && characters.contains(&'\u{2009}')
            ),
            "{found:?}"
        );

        // An ordinary space, however many of them, is not this.
        let mut resume = clean();
        resume.basics.phone = "+48  22 555 0100".into();
        assert_eq!(lint(&resume), Vec::new());
    }
}

//! What the importer **noticed**, in place of how sure it felt.
//!
//! The review step used to carry a `HashMap<String, Confidence>` — three levels
//! against ad-hoc string keys — and it failed in every way that shape can:
//!
//! * **A level is not a reason.** `Medium` rendered as *"Partly guessed — worth
//!   a look"*, which tells a user something is wrong and gives them nothing to
//!   check it against. `import_flow.rs`'s own comment admitted as much.
//! * **It never varied.** `work` was set to `Medium` whenever the list was
//!   non-empty, so the flag was lit on every import that found a job at all. A
//!   marker that is always on is one the user learns to scroll past.
//! * **The keys drifted silently.** The screen asked for five (`profile.name`,
//!   `work`, `education`, `skills`, `certificates`); the classifier wrote two.
//!   Education, Skills and Certificates could not be flagged from any PDF, DOCX
//!   or text import — the three sections a text classifier is most likely to
//!   mangle.
//!
//! A note is the opposite of a level. It names one thing the parser saw, it
//! carries the numbers behind it, and **it only exists when it is true** — so a
//! clean import shows nothing and a flag means something.
//!
//! ### Where notes come from
//!
//! Most are a pure function of the document that came out: [`observe`] reads a
//! [`ResumeDoc`] and reports what is odd about it. That is deliberate — it means
//! every engine gets the same review for free, which is exactly what the old
//! per-engine key-writing failed to do.
//!
//! The one thing a result cannot tell you is whether something was *supposed* to
//! be there. An empty Education section is a defect on a CV that had an
//! EDUCATION heading and ordinary on a CV that never went to university, and the
//! document alone cannot tell those apart. So [`Note::Empty`] is pushed by the
//! engine, which has the evidence — a heading it classified, a CSV that was in
//! the archive, a JSON array that was not empty.

use crate::resume::model::ResumeDoc;

/// Which part of the document a note is about.
///
/// An enum, not a string key. The old map let the writer and the reader drift
/// apart without anything failing to compile, and they did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Profile,
    Work,
    Education,
    Skills,
    Certificates,
}

/// Something the parser noticed that a person should look at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// The source said this section was there and nothing came out of it.
    ///
    /// Only an engine can raise this — see the module note.
    Empty,
    /// The person's name never arrived.
    NoName,
    /// Neither an email nor a phone number was found.
    NoContact,
    /// Entries came out, some of them with no dates on them.
    MissingDates { found: usize, without: usize },
    /// Entries came out, some with no employer or institution against them.
    MissingOrg { found: usize, without: usize },
    /// Jobs came out and not one bullet came with them — the bullets are
    /// somewhere, and it is not here.
    NoHighlights { found: usize },
    /// Every skill landed in one unnamed group: the categories were in the
    /// document and did not survive.
    UngroupedSkills { keywords: usize },
    /// A "keyword" long enough to be a sentence, which means a line was split
    /// on the wrong thing.
    KeywordIsASentence { longest: usize },
}

impl Note {
    /// The sentence the review step prints.
    ///
    /// Every one names its evidence. "Partly guessed" is what this replaces.
    pub fn message(&self, part: Part) -> String {
        match self {
            Note::Empty => format!(
                "The document has a {} section and nothing came out of it",
                part.noun()
            ),
            Note::NoName => "No name found — check the top of the document".to_string(),
            Note::NoContact => "No email or phone number found".to_string(),
            Note::MissingDates { found, without } => format!(
                "{without} of {found} {} came out without dates",
                part.entries(*found)
            ),
            Note::MissingOrg { found, without } => format!(
                "{without} of {found} {} came out without {}",
                part.entries(*found),
                part.org()
            ),
            Note::NoHighlights { found } => format!(
                "{found} {} and no bullet points at all — they may have gone elsewhere",
                part.entries(*found)
            ),
            Note::UngroupedSkills { keywords } => format!(
                "All {keywords} skills landed in one unnamed group — any categories were lost"
            ),
            Note::KeywordIsASentence { longest } => format!(
                "One skill is {longest} characters long, so a line was probably split wrongly"
            ),
        }
    }
}

impl Part {
    fn noun(self) -> &'static str {
        match self {
            Part::Profile => "profile",
            Part::Work => "work",
            Part::Education => "education",
            Part::Skills => "skills",
            Part::Certificates => "certificates",
        }
    }

    fn entries(self, count: usize) -> &'static str {
        let one = count == 1;
        match self {
            Part::Work => {
                if one {
                    "job"
                } else {
                    "jobs"
                }
            }
            Part::Education => {
                if one {
                    "entry"
                } else {
                    "entries"
                }
            }
            Part::Certificates => {
                if one {
                    "certificate"
                } else {
                    "certificates"
                }
            }
            _ => {
                if one {
                    "entry"
                } else {
                    "entries"
                }
            }
        }
    }

    fn org(self) -> &'static str {
        match self {
            Part::Education => "an institution",
            Part::Certificates => "an issuer",
            _ => "an employer",
        }
    }
}

/// A "keyword" past this length is a sentence that got split on the wrong
/// character. Chosen well above the longest real one — `Kubernetes (CKA)`,
/// `Distributed Systems`, `Stakeholder management` — and well below a bullet.
const KEYWORD_IS_A_SENTENCE: usize = 60;

/// Read the document that came out and report what is odd about it.
///
/// Pure: same document, same notes. Everything here is derivable from the
/// result, which is what makes it apply to every engine equally rather than to
/// whichever one remembered to write a key.
pub fn observe(doc: &ResumeDoc) -> Vec<(Part, Note)> {
    let mut notes = Vec::new();

    let profile = doc.profile.active();
    if profile.name.trim().is_empty() {
        notes.push((Part::Profile, Note::NoName));
    }
    if profile.email.trim().is_empty() && profile.phone.trim().is_empty() {
        notes.push((Part::Profile, Note::NoContact));
    }

    let work = doc.work.active();
    if !work.is_empty() {
        let undated = work.iter().filter(|w| w.start_date.text.is_empty()).count();
        if undated > 0 {
            notes.push((
                Part::Work,
                Note::MissingDates {
                    found: work.len(),
                    without: undated,
                },
            ));
        }
        let nameless = work.iter().filter(|w| w.name.trim().is_empty()).count();
        if nameless > 0 {
            notes.push((
                Part::Work,
                Note::MissingOrg {
                    found: work.len(),
                    without: nameless,
                },
            ));
        }
        if work.iter().all(|w| w.highlights.is_empty()) {
            notes.push((Part::Work, Note::NoHighlights { found: work.len() }));
        }
    }

    let education = doc.education.active();
    if !education.is_empty() {
        let undated = education
            .iter()
            .filter(|e| e.start_date.text.is_empty() && e.end_date.text.is_empty())
            .count();
        if undated > 0 {
            notes.push((
                Part::Education,
                Note::MissingDates {
                    found: education.len(),
                    without: undated,
                },
            ));
        }
        let nameless = education
            .iter()
            .filter(|e| e.institution.trim().is_empty())
            .count();
        if nameless > 0 {
            notes.push((
                Part::Education,
                Note::MissingOrg {
                    found: education.len(),
                    without: nameless,
                },
            ));
        }
    }

    let skills = doc.skills.active();
    let keywords: usize = skills.iter().map(|g| g.keywords.len()).sum();
    // One unnamed group is what a flat list looks like *and* what a categorised
    // list looks like once its categories are lost. Below a handful there was
    // probably nothing to categorise, so the note would be noise.
    if skills.len() == 1 && skills[0].name.trim().is_empty() && keywords >= 8 {
        notes.push((Part::Skills, Note::UngroupedSkills { keywords }));
    }
    if let Some(longest) = skills
        .iter()
        .flat_map(|g| g.keywords.iter())
        .map(|k| k.chars().count())
        .max()
        .filter(|len| *len > KEYWORD_IS_A_SENTENCE)
    {
        notes.push((Part::Skills, Note::KeywordIsASentence { longest }));
    }

    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Education, Resume, SkillGroup, Work};

    fn doc_of(resume: Resume) -> ResumeDoc {
        ResumeDoc::from_resume(resume, "Base")
    }

    /// The regression this module exists for. A clean import used to light the
    /// Work flag anyway, because `Medium` was set whenever the list was
    /// non-empty. A document with nothing wrong with it says nothing.
    #[test]
    fn a_clean_document_produces_no_notes_at_all() {
        let resume = Resume {
            basics: crate::resume::model::Basics {
                name: "Albert Einstein".into(),
                email: "s@example.com".into(),
                ..Default::default()
            },
            work: vec![Work {
                name: "Acme".into(),
                position: "Staff Engineer".into(),
                start_date: "2021-01".to_string().into(),
                highlights: vec!["Cut p99 in half".into()],
                ..Default::default()
            }],
            education: vec![Education {
                institution: "TU Berlin".into(),
                start_date: "2014".to_string().into(),
                ..Default::default()
            }],
            skills: vec![SkillGroup {
                name: "Backend".into(),
                keywords: vec!["Rust".into(), "Kafka".into()],
            }],
            ..Default::default()
        };
        assert_eq!(observe(&doc_of(resume)), Vec::new());
    }

    /// And a note carries the numbers behind it, so it can be checked.
    #[test]
    fn an_undated_job_is_counted_not_merely_suspected() {
        let resume = Resume {
            work: vec![
                Work {
                    name: "Acme".into(),
                    start_date: "2021-01".to_string().into(),
                    highlights: vec!["a".into()],
                    ..Default::default()
                },
                Work {
                    name: "Globex".into(),
                    highlights: vec!["b".into()],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let notes = observe(&doc_of(resume));
        assert!(notes.contains(&(
            Part::Work,
            Note::MissingDates {
                found: 2,
                without: 1
            }
        )));
        assert_eq!(
            Note::MissingDates {
                found: 2,
                without: 1
            }
            .message(Part::Work),
            "1 of 2 jobs came out without dates"
        );
    }

    /// Jobs with no bullets anywhere means the bullets went somewhere else —
    /// the most common way a PDF import goes wrong without looking wrong.
    #[test]
    fn jobs_with_no_bullets_at_all_are_worth_saying_out_loud() {
        let resume = Resume {
            work: vec![Work {
                name: "Acme".into(),
                start_date: "2021".to_string().into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let notes = observe(&doc_of(resume));
        assert!(notes.contains(&(Part::Work, Note::NoHighlights { found: 1 })));
    }

    /// A flat list of two is a flat list. A flat list of twenty is a
    /// categorised list whose categories did not survive.
    #[test]
    fn only_a_long_flat_skill_list_reads_as_lost_categories() {
        let short = Resume {
            skills: vec![SkillGroup {
                name: String::new(),
                keywords: vec!["Rust".into(), "Kafka".into()],
            }],
            ..Default::default()
        };
        assert!(!observe(&doc_of(short))
            .iter()
            .any(|(p, _)| *p == Part::Skills));

        let long = Resume {
            skills: vec![SkillGroup {
                name: String::new(),
                keywords: (0..12).map(|i| format!("skill{i}")).collect(),
            }],
            ..Default::default()
        };
        assert!(observe(&doc_of(long))
            .contains(&(Part::Skills, Note::UngroupedSkills { keywords: 12 })));
    }

    /// A skill the length of a sentence is a line that was split on the wrong
    /// character — the failure that turns one bullet into nine "skills".
    #[test]
    fn a_skill_the_length_of_a_sentence_says_the_split_went_wrong() {
        let sentence =
            "Designed and shipped the ingest service that now carries every profile".to_string();
        let longest = sentence.chars().count();
        let resume = Resume {
            skills: vec![SkillGroup {
                name: "Backend".into(),
                keywords: vec![sentence],
            }],
            ..Default::default()
        };
        assert!(observe(&doc_of(resume))
            .contains(&(Part::Skills, Note::KeywordIsASentence { longest })));
    }

    /// Every message names its evidence — that is the whole point of replacing
    /// a level with a note.
    #[test]
    fn every_message_says_something_specific() {
        let cases = [
            (Part::Education, Note::Empty),
            (Part::Profile, Note::NoName),
            (Part::Profile, Note::NoContact),
            (
                Part::Education,
                Note::MissingOrg {
                    found: 3,
                    without: 2,
                },
            ),
            (Part::Work, Note::NoHighlights { found: 4 }),
            (Part::Skills, Note::UngroupedSkills { keywords: 20 }),
            (Part::Skills, Note::KeywordIsASentence { longest: 84 }),
        ];
        for (part, note) in cases {
            let message = note.message(part);
            assert!(!message.is_empty());
            assert!(
                !message.contains("guessed") && !message.contains("reliably"),
                "a note must name what it saw, not how it felt: {message}"
            );
        }
        assert_eq!(
            Note::MissingOrg {
                found: 3,
                without: 2
            }
            .message(Part::Education),
            "2 of 3 entries came out without an institution"
        );
    }
}

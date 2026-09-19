//! What has to survive, and what surviving means.
//!
//! A parser does not recover "most of a CV". It recovers a phone number or it
//! does not, and a phone number broken across two runs of text is one it will
//! not find. So the unit of measurement here is a **field**, pinned by the
//! document it came from, and a reading either contains it or does not.
//!
//! Two kinds of expectation, because section headings are not data:
//!
//! - [`Expect::Contains`] — the field's own text appears, unbroken. Data.
//! - [`Expect::OwnLine`] — it starts a line of its own, after any list marker.
//!   Section headings are what every parser segments a CV on, so one fused to
//!   the paragraph under it (`EXPERIENCEBackend engineer with…`) costs the
//!   whole section rather than a line. A bullet is the same argument one level
//!   down: an achievement glued to the sentence above it stops being an item
//!   in a list and becomes the tail of a paragraph.
//!
//! No score is computed anywhere, here or above. A field is recovered or it is
//! not, and the report names the ones that are not.

use dockcv_core::resume::edit::FieldId;
use dockcv_core::resume::model::{Resume, SectionKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    Contains,
    OwnLine,
}

/// One thing a parser must get back, and the name the report gives it.
#[derive(Debug, Clone)]
pub struct Pinned {
    pub what: String,
    pub needle: String,
    pub expect: Expect,
    /// The field this came from, when it came from one. It is what lets a lost
    /// field be matched against the lint finding that predicted it — and, when
    /// the ATS screen is built, what lets a reading that lost something put the
    /// cursor where it went missing.
    pub at: Option<FieldId>,
}

impl Pinned {
    fn contains(what: impl Into<String>, needle: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            needle: needle.into(),
            expect: Expect::Contains,
            at: None,
        }
    }

    fn at(mut self, field: FieldId) -> Self {
        self.at = Some(field);
        self
    }

    /// Is this field in this reading?
    pub fn recovered(&self, reading: &str) -> bool {
        let needle = normalize(&self.needle);
        if needle.is_empty() {
            return true;
        }
        match self.expect {
            Expect::Contains => normalize(reading).contains(&needle),
            // Two things at once, because either alone is not the property:
            // the text has to be **intact**, and it has to **begin a line**.
            //
            // Beginning a line is not the same as being one. A bullet worth
            // writing runs past the measure and wraps, and an extractor wraps
            // it where the page did — so the line that starts it is a *prefix*
            // of it rather than the whole of it. Demanding the whole of it was
            // a defect in this check and not in any document: it passed only
            // because the first fixture's bullets were short enough to fit.
            Expect::OwnLine => {
                if !normalize(reading).contains(&needle) {
                    return false;
                }
                reading.lines().map(normalize).any(|line| {
                    // The marker is the list's, not the item's: every extractor
                    // decides differently whether to keep it, drop it or put it
                    // on a line of its own, and none of that is the defect
                    // being looked for.
                    let line = line.trim_start_matches(['•', '-', '–', '*', '·', '‣', ' ']);
                    if line == needle || line.starts_with(&format!("{needle} ")) {
                        return true;
                    }
                    // A wrapped opening line. Long enough that a stray word
                    // cannot pass for the start of a sentence.
                    needle.starts_with(line) && line.chars().count() >= 12
                })
            }
        }
    }
}

/// Fold away every difference that is not a difference to a parser.
///
/// Extractors disagree about whitespace by design — one writes a space per
/// glyph gap, another a newline per text run — and none of that changes
/// whether an email address came back. What it must *not* fold away is
/// letter-spacing: `e d u c a t i o n` stays unequal to `education`, because
/// that is exactly the defect this measures.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars() {
        // Non-breaking, thin and hair spaces are spaces to a reader and
        // separate characters to a parser.
        let ch = match ch {
            '\u{00a0}' | '\u{2009}' | '\u{200a}' | '\u{202f}' => ' ',
            // The three dashes a date range gets written with.
            '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
            other => other,
        };
        if ch.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
    }
    out
}

/// The default heading a section prints under, matching `template.rs`.
fn default_title(kind: &SectionKind) -> Option<&'static str> {
    Some(match kind {
        SectionKind::Profile => "Profile",
        SectionKind::Work => "Work Experience",
        SectionKind::Education => "Education",
        SectionKind::Skills => "Skills",
        SectionKind::Certificates => "Certifications",
        SectionKind::Organizations => "Organizations",
        SectionKind::Custom(_) => return None,
    })
}

fn title_of(resume: &Resume, kind: SectionKind) -> Option<String> {
    resume
        .section_titles
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, title)| title.clone())
        .or_else(|| default_title(&kind).map(str::to_string))
}

/// Everything this document says, as a parser would have to get it back.
pub fn pin(resume: &Resume) -> Vec<Pinned> {
    let mut out = Vec::new();
    let b = &resume.basics;

    out.push(Pinned::contains("name", &b.name).at(FieldId::Name));
    out.push(Pinned::contains("role", &b.label).at(FieldId::Label));
    out.push(Pinned::contains("email", &b.email).at(FieldId::Email));
    out.push(Pinned::contains("phone", &b.phone).at(FieldId::Phone));
    out.push(Pinned::contains("location", &b.location).at(FieldId::Location));
    if !b.summary.trim().is_empty() {
        // The first clause only: a summary is re-wrapped by every extractor at
        // a different width, and a line break inside it is not a defect.
        out.push(
            Pinned::contains(
                "summary opens",
                b.summary.split(',').next().unwrap_or_default().trim(),
            )
            .at(FieldId::Summary),
        );
    }

    for kind in [
        SectionKind::Profile,
        SectionKind::Work,
        SectionKind::Education,
        SectionKind::Skills,
        SectionKind::Certificates,
        SectionKind::Organizations,
    ] {
        let printed = match kind {
            SectionKind::Profile => !b.summary.trim().is_empty(),
            SectionKind::Work => !resume.work.is_empty(),
            SectionKind::Education => !resume.education.is_empty(),
            SectionKind::Skills => !resume.skills.is_empty(),
            SectionKind::Certificates => !resume.certificates.is_empty(),
            SectionKind::Organizations => !resume.volunteer.is_empty(),
            SectionKind::Custom(_) => false,
        };
        if !printed {
            continue;
        }
        if let Some(title) = title_of(resume, kind) {
            out.push(Pinned {
                what: format!("heading “{title}”"),
                needle: title,
                expect: Expect::OwnLine,
                at: None,
            });
        }
    }

    for (i, job) in resume.work.iter().enumerate() {
        out.push(
            Pinned::contains(format!("work {i} position"), &job.position)
                .at(FieldId::WorkPosition(i)),
        );
        out.push(
            Pinned::contains(format!("work {i} employer"), &job.name).at(FieldId::WorkName(i)),
        );
        for (j, bullet) in job.highlights.iter().enumerate() {
            out.push(Pinned {
                what: format!("work {i} bullet {j}"),
                needle: bullet.clone(),
                expect: Expect::OwnLine,
                at: Some(FieldId::WorkHighlight(i, j)),
            });
        }
    }

    for (i, school) in resume.education.iter().enumerate() {
        out.push(
            Pinned::contains(format!("education {i} study"), &school.study_type)
                .at(FieldId::EduStudyType(i)),
        );
        out.push(
            Pinned::contains(format!("education {i} institution"), &school.institution)
                .at(FieldId::EduInstitution(i)),
        );
    }

    for (i, group) in resume.skills.iter().enumerate() {
        for (j, keyword) in group.keywords.iter().enumerate() {
            out.push(
                Pinned::contains(format!("skill “{keyword}”"), keyword)
                    .at(FieldId::SkillKeyword(i, j)),
            );
        }
    }

    for (i, cert) in resume.certificates.iter().enumerate() {
        out.push(Pinned::contains(format!("certificate {i}"), &cert.name).at(FieldId::CertName(i)));
        out.push(
            Pinned::contains(format!("certificate {i} issuer"), &cert.issuer)
                .at(FieldId::CertIssuer(i)),
        );
    }

    out.retain(|p| !p.needle.trim().is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_spacing_is_not_whitespace_to_fold_away() {
        let heading = Pinned {
            what: "heading".into(),
            needle: "Education".into(),
            expect: Expect::OwnLine,
            at: None,
        };
        assert!(heading.recovered("EDUCATION\nM.Sc. in Computer Science"));
        // The defect the harness exists to catch, and the reason `normalize`
        // may not simply strip spaces.
        assert!(!heading.recovered("E D U C AT I O N\nM.Sc. in Computer Science"));
    }

    #[test]
    fn a_heading_fused_to_its_paragraph_is_not_its_own_line() {
        let heading = Pinned {
            what: "heading".into(),
            needle: "Work Experience".into(),
            expect: Expect::OwnLine,
            at: None,
        };
        assert!(heading.recovered("WORK EXPERIENCE\nSenior Software Engineer"));
        assert!(!heading.recovered("WORK EXPERIENCESenior Software Engineer"));
    }

    #[test]
    fn a_dash_is_a_dash_however_it_was_typeset() {
        let dates = Pinned::contains("dates", "2019-06 - 2022-01");
        assert!(dates.recovered("2019-06 \u{2013} 2022-01"));
    }
}

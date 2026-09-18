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
}

impl Pinned {
    fn contains(what: impl Into<String>, needle: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            needle: needle.into(),
            expect: Expect::Contains,
        }
    }

    /// Is this field in this reading?
    pub fn recovered(&self, reading: &str) -> bool {
        let needle = normalize(&self.needle);
        if needle.is_empty() {
            return true;
        }
        match self.expect {
            Expect::Contains => normalize(reading).contains(&needle),
            Expect::OwnLine => reading.lines().map(normalize).any(|line| {
                // The marker is the list's, not the item's: every extractor
                // decides differently whether to keep it, drop it or put it on
                // a line of its own, and none of that is the defect being
                // looked for.
                let line = line.trim_start_matches(['•', '-', '–', '*', '·', '‣', ' ']);
                line == needle || line.starts_with(&format!("{needle} "))
            }),
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

    out.push(Pinned::contains("name", &b.name));
    out.push(Pinned::contains("role", &b.label));
    out.push(Pinned::contains("email", &b.email));
    out.push(Pinned::contains("phone", &b.phone));
    out.push(Pinned::contains("location", &b.location));
    if !b.summary.trim().is_empty() {
        // The first clause only: a summary is re-wrapped by every extractor at
        // a different width, and a line break inside it is not a defect.
        out.push(Pinned::contains(
            "summary opens",
            b.summary.split(',').next().unwrap_or_default().trim(),
        ));
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
            });
        }
    }

    for (i, job) in resume.work.iter().enumerate() {
        out.push(Pinned::contains(
            format!("work {i} position"),
            &job.position,
        ));
        out.push(Pinned::contains(format!("work {i} employer"), &job.name));
        for (j, bullet) in job.highlights.iter().enumerate() {
            out.push(Pinned {
                what: format!("work {i} bullet {j}"),
                needle: bullet.clone(),
                expect: Expect::OwnLine,
            });
        }
    }

    for (i, school) in resume.education.iter().enumerate() {
        out.push(Pinned::contains(
            format!("education {i} study"),
            &school.study_type,
        ));
        out.push(Pinned::contains(
            format!("education {i} institution"),
            &school.institution,
        ));
    }

    for group in &resume.skills {
        for keyword in &group.keywords {
            out.push(Pinned::contains(format!("skill “{keyword}”"), keyword));
        }
    }

    for (i, cert) in resume.certificates.iter().enumerate() {
        out.push(Pinned::contains(format!("certificate {i}"), &cert.name));
        out.push(Pinned::contains(
            format!("certificate {i} issuer"),
            &cert.issuer,
        ));
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

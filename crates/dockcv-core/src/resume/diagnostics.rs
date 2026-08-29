//! Turn a compile attempt's diagnostics into sentences that name a résumé
//! *section*, when that is honestly derivable — the other half of
//! `typst_engine::Diagnostic`'s humanized message (US-07, review P-06).
//!
//! `typst_engine` stays generic on purpose: it knows Typst, not résumés.
//! Section attribution needs to know the shape `resume::template` emits (the
//! top-level `basics`/`work`/`education`/… keys of its `#let cv = (..)`
//! dictionary), which is résumé-model knowledge, so it lives here instead.
//!
//! **What this can and cannot promise.** `template::generate`'s codegen
//! keeps no per-section span map — every diagnostic's only address is a
//! byte offset into the flat generated source. What follows is a heuristic
//! over that generated text: it walks the same literal section-marker lines
//! `resume::template`'s dict-builder always writes (`"  work: ("`,
//! `"  skills: ("`, …) and reports which one precedes a given offset. It is
//! *not* a real span→section map — see this module's doc on
//! [`attribute_section`] for exactly how it can be wrong, and
//! the Typst-controls spec §7/§9 for the hook (per-section markers or
//! a line-range table emitted alongside the source) that would replace it
//! with a real one.
//!
//! One more honest limit worth naming: only `basics.summary`,
//! `work[].summary`, `work[].highlights[]` and `volunteer[].highlights[]` are
//! ever emitted as raw Typst markup a user could break — every other field
//! (names, dates, skills, certificates, education) is quoted into a string
//! literal by `resume::template::quote` and cannot itself cause a syntax
//! error. So in practice, attribution only ever fires for Profile, Work and
//! Organizations; Education/Skills/Certificates diagnostics would only ever
//! come from a bug in the template's own fixed renderer body, which this
//! module correctly reports as unattributable (`section: None`) rather than
//! guessing.

use crate::resume::model::SectionKind;
use crate::typst_engine::{Diagnostic, Severity};

/// One diagnostic, translated into a sentence ready to display as-is, with
/// severity preserved and a section attached when the evidence supports it.
#[derive(Debug, Clone, PartialEq)]
pub struct CompileMessage {
    pub severity: Severity,
    /// `None` covers two honest cases: the diagnostic's span fell before the
    /// first section marker (inside the fixed renderer preamble/body — a
    /// template bug, not the user's data), or Typst could not resolve the
    /// span to an offset at all (a detached span, e.g. the missing-font
    /// warning).
    pub section: Option<SectionKind>,
    /// The complete sentence — already prefixed ("Couldn't compile — …" for
    /// an error) and punctuated. Show this verbatim.
    pub text: String,
}

/// The literal top-level keys `resume::template`'s dict-builder always
/// writes, one per résumé section, in the exact order it writes them —
/// mirroring `resume_to_dict`'s own `basics`/`work`/`education`/`skills`/
/// `certificates`/`volunteer` sequence.
const SECTION_MARKERS: &[(&str, SectionKind)] = &[
    ("  basics: (", SectionKind::Profile),
    ("  work: (", SectionKind::Work),
    ("  education: (", SectionKind::Education),
    ("  skills: (", SectionKind::Skills),
    ("  certificates: (", SectionKind::Certificates),
    ("  volunteer: (", SectionKind::Organizations),
];

/// Find which section's data block a byte offset into the generated source
/// falls inside, by scanning source lines and remembering the last
/// section-marker line seen at or before `offset`.
///
/// This is a heuristic over generated text, not a real span map (see the
/// module doc). It holds for every document the current codegen can
/// produce, because the six marker strings above only ever appear as
/// whole, standalone lines written by `resume_to_dict` itself — a marker
/// could only be mistaken for user data if a summary or bullet consisted of
/// a line that is *character-for-character* one of these six strings
/// (down to the leading two spaces and trailing open-paren), which is not
/// something a person types by accident. Even in that pathological case,
/// this degrades to attributing the diagnostic to whichever section's
/// marker most recently preceded it — never to a wrong, distant section.
pub fn attribute_section(source: &str, offset: usize) -> Option<SectionKind> {
    let mut current = None;
    let mut pos = 0usize;
    for line in source.split_inclusive('\n') {
        if pos > offset {
            break;
        }
        let trimmed = line.strip_suffix('\n').unwrap_or(line);
        if let Some(&(_, section)) = SECTION_MARKERS
            .iter()
            .find(|(marker, _)| *marker == trimmed)
        {
            current = Some(section);
        }
        pos += line.len();
    }
    current
}

/// The section title shown in a compile message — the same strings
/// `views/root.rs::section_title` shows in the sidebar. Duplicated rather
/// than imported: this module belongs to the résumé-model layer and must
/// not reach up into `views` for a one-line, effectively-fixed match, the
/// same call `root.rs::section_title`'s own doc comment already makes for
/// its duplication against `root_sidebar.rs`.
fn section_title(section: SectionKind) -> &'static str {
    use SectionKind::*;
    match section {
        Profile => "Profile",
        Work => "Work Experience",
        Education => "Education",
        Skills => "Skills",
        Certificates => "Certifications",
        Organizations => "Organizations",
        // `SECTION_MARKERS` never produces `Custom` — the generated source's
        // custom-section block has no per-section marker line yet — so this
        // arm exists only to keep the match exhaustive.
        Custom(_) => "Custom Section",
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Compose the full, ready-to-show sentence for one engine diagnostic,
/// attributing it to a section when `source` — the exact Typst source that
/// diagnostic came from — supports that.
pub fn describe(diagnostic: &Diagnostic, source: &str) -> CompileMessage {
    let section = diagnostic
        .source_offset
        .and_then(|offset| attribute_section(source, offset));

    let text = match (diagnostic.severity, section) {
        (Severity::Error, Some(section)) => format!(
            "Couldn't compile — the {} section: {}.",
            section_title(section),
            diagnostic.message
        ),
        (Severity::Error, None) => format!("Couldn't compile: {}.", diagnostic.message),
        (Severity::Warning, Some(section)) => format!(
            "{}: {}.",
            section_title(section),
            capitalize(&diagnostic.message)
        ),
        (Severity::Warning, None) => format!("{}.", capitalize(&diagnostic.message)),
    };

    CompileMessage {
        severity: diagnostic.severity,
        section,
        text,
    }
}

/// Translate every diagnostic from one compile attempt, in order.
pub fn describe_all(diagnostics: &[Diagnostic], source: &str) -> Vec<CompileMessage> {
    diagnostics.iter().map(|d| describe(d, source)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Resume, Work};
    use crate::resume::template;
    use crate::typst_engine::TypstEngine;

    #[test]
    fn attributes_a_broken_highlight_to_the_work_section() {
        let mut resume = Resume::default();
        resume.basics.name = "Test Person".into();
        resume.work.push(Work {
            name: "Acme".into(),
            position: "Engineer".into(),
            highlights: vec!["Shipped a bracket by mistake".into()],
            ..Default::default()
        });

        // The break is injected into the *generated source*, not into the
        // model: `template::neutralize` now escapes a stray `]` on the way out,
        // which is correct for a résumé and would make this test vacuous. What
        // is under test is the span→section attribution, so the source is what
        // has to be broken.
        let source =
            template::generate(&resume).replace("Shipped a bracket", "Shipped a stray ] bracket");
        let engine = TypstEngine::new(source.clone());
        let attempt = engine.compile_with_diagnostics(1.0);
        assert!(
            attempt.result.is_err(),
            "the stray bracket should break compilation"
        );

        let messages = describe_all(&attempt.diagnostics, &source);
        let attributed = messages
            .iter()
            .find(|m| m.severity == Severity::Error && m.section == Some(SectionKind::Work));
        assert!(
            attributed.is_some(),
            "expected an error attributed to Work, got: {messages:?}"
        );
        let text = &attributed.unwrap().text;
        assert!(text.starts_with("Couldn't compile — the Work Experience section:"));
        assert!(text.contains("Work Experience"));
    }

    #[test]
    fn attribute_section_is_none_before_the_first_marker() {
        // A bare offset of 0 falls inside the fixed renderer, before any
        // section marker — must not be guessed at as a section.
        let source = template::generate(&Resume::default());
        assert_eq!(attribute_section(&source, 0), None);
    }

    /// Every marker must exist in what the generator actually emits.
    ///
    /// `SECTION_MARKERS` matches the generated source by exact text, including
    /// indentation, and nothing in the type system ties the two together. The
    /// end-to-end tests only exercise Work and Education, so renaming any of the
    /// other four keys in `template.rs` would silently stop attributing them —
    /// the banner would just quietly say less. Pin all six.
    #[test]
    fn every_marker_matches_what_the_generator_emits() {
        let mut resume = Resume::default();
        // A profile with content, so `basics` opens a real dictionary: an
        // all-empty one now emits `(:)` (see `template.rs`'s empty-document
        // test), and section attribution is only meaningful for a section
        // that exists.
        resume.basics.name = "Sofiia Medvedenko".into();
        resume.work.push(Work::default());
        resume.education.push(Default::default());
        resume.skills.push(Default::default());
        resume.certificates.push(Default::default());
        resume.volunteer.push(Default::default());
        let source = template::generate(&resume);

        for (marker, section) in SECTION_MARKERS {
            let at = source.find(marker).unwrap_or_else(|| {
                panic!(
                    "`{marker}` ({section:?}) is not in the generated source — \
                     template.rs changed and section attribution silently broke"
                )
            });
            assert_eq!(
                attribute_section(&source, at + marker.len()),
                Some(*section),
                "`{marker}` should attribute to {section:?}"
            );
        }
    }

    #[test]
    fn attribute_section_finds_each_marker_in_order() {
        let mut resume = Resume::default();
        // A profile with content, so `basics` opens a real dictionary: an
        // all-empty one now emits `(:)` (see `template.rs`'s empty-document
        // test), and section attribution is only meaningful for a section
        // that exists.
        resume.basics.name = "Sofiia Medvedenko".into();
        resume.work.push(Work::default());
        resume.education.push(Default::default());
        let source = template::generate(&resume);

        let work_at = source.find("  work: (").expect("work marker present");
        let edu_at = source
            .find("  education: (")
            .expect("education marker present");

        assert_eq!(
            attribute_section(&source, work_at + 2),
            Some(SectionKind::Work)
        );
        assert_eq!(
            attribute_section(&source, edu_at + 2),
            Some(SectionKind::Education)
        );
    }
}

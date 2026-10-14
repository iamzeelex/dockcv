//! cv-ats-linter — deterministic ATS compliance linter (Kraton VMC).

wit_bindgen::generate!({
    path: "wit",
    world: "ats-linter-vmc",
});

use exports::cv::ats_linter::linter::{
    DateRange, Finding, FindingRule, Guest, HeadingMismatch, LintError, LintRequest, LintResult,
    MarkupFinding, MissingChars, SectionKind, WhitespaceChars,
};

use dockcv_core::resume::ats::{self, Rule};
use dockcv_core::resume::model::{DocumentLanguage, SectionKind as CoreSectionKind};
use dockcv_core::resume::parse_any;

struct Linter;

impl Guest for Linter {
    fn lint(req: LintRequest) -> Result<LintResult, LintError> {
        let doc = parse_any(&req.document_content)
            .map_err(|err| LintError::ParseFailed(err))?;

        let resume = doc.compose();
        let lang = DocumentLanguage::from_code(
            doc.presets.first().and_then(|p| p.lang.as_deref()),
        );
        let findings_raw = ats::lint(&resume, lang);
        let mut findings = Vec::with_capacity(findings_raw.len());

        for f in findings_raw {
            let (rule, message) = match f.rule {
                Rule::HeadingNoParserKnows { title, expected } => {
                    let msg = format!("Unrecognized section heading '{title}' (expected standard name like '{expected}')");
                    (
                        FindingRule::HeadingNoParserKnows(HeadingMismatch {
                            title,
                            expected: expected.to_string(),
                        }),
                        msg,
                    )
                }
                Rule::DateNoParserCanRead { text } => {
                    let msg = format!("Unparseable date '{text}'");
                    (FindingRule::DateNoParserCanRead(text), msg)
                }
                Rule::DatesRunBackwards { start, end } => {
                    let msg = format!("End date '{end}' is earlier than start date '{start}'");
                    (FindingRule::DatesRunBackwards(DateRange { start, end }), msg)
                }
                Rule::NoWayToReachThePerson => {
                    let msg = "No phone number or email address found in the document".to_string();
                    (FindingRule::NoWayToReachPerson, msg)
                }
                Rule::MarkupThatNeverRenders { text, in_the_text } => {
                    let msg = format!("Markup that never renders '{text}' inside '{in_the_text}'");
                    (
                        FindingRule::MarkupThatNeverRenders(MarkupFinding { text, in_the_text }),
                        msg,
                    )
                }
                Rule::ListMarkerTypedIntoTheText { text } => {
                    let msg = format!("Bullet list marker manually typed into text: '{text}'");
                    (FindingRule::ListMarkerTypedIntoText(text), msg)
                }
                Rule::CharactersNoFaceCanSet { text, missing } => {
                    let msg = format!("Characters missing in font faces: {missing:?} in '{text}'");
                    (
                        FindingRule::CharactersNoFaceCanSet(MissingChars { text, missing }),
                        msg,
                    )
                }
                Rule::WhitespaceNobodySees { text, characters } => {
                    let msg = format!("Invisible unicode whitespace characters found in '{text}'");
                    (
                        FindingRule::WhitespaceNobodySees(WhitespaceChars { text, characters }),
                        msg,
                    )
                }
            };

            let section = match f.section {
                CoreSectionKind::Profile => SectionKind::Profile,
                CoreSectionKind::Work => SectionKind::Work,
                CoreSectionKind::Education => SectionKind::Education,
                CoreSectionKind::Skills => SectionKind::Skills,
                CoreSectionKind::Certificates => SectionKind::Certificates,
                CoreSectionKind::Organizations => SectionKind::Organizations,
                CoreSectionKind::Custom(_) => SectionKind::Custom,
            };

            let field_path = f.at.map(|id| format!("{id:?}"));

            findings.push(Finding {
                rule,
                section,
                field_path,
                message,
            });
        }

        let is_clean = findings.is_empty();
        let finding_count = findings.len() as u32;

        Ok(LintResult {
            findings,
            is_clean,
            finding_count,
        })
    }
}

export!(Linter);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_clean_resume() {
        let json = r#"{
            "basics": {
                "name": "Ada Lovelace",
                "email": "ada@example.com",
                "phone": "+44 20 7946 0912",
                "summary": "First computer programmer."
            },
            "work": [
                {
                    "name": "Babbage Analytical Engines",
                    "position": "Lead Mathematician",
                    "startDate": "1842-01",
                    "endDate": "1843-12",
                    "highlights": ["Published the first computer algorithm."]
                }
            ]
        }"#;

        let result = Linter::lint(LintRequest {
            document_content: json.to_string(),
        })
        .expect("lint succeeds");

        assert!(result.is_clean, "clean resume must have 0 findings: {:?}", result.findings);
        assert_eq!(result.finding_count, 0);
    }

    #[test]
    fn lint_missing_contact() {
        let json = r#"{
            "basics": {
                "name": "Ghost Candidate"
            },
            "work": []
        }"#;

        let result = Linter::lint(LintRequest {
            document_content: json.to_string(),
        })
        .expect("lint succeeds");

        assert!(!result.is_clean);
        assert!(result.findings.iter().any(|f| matches!(f.rule, FindingRule::NoWayToReachPerson)));
    }

    #[test]
    fn lint_backwards_dates() {
        let json = r#"{
            "basics": {
                "name": "Time Traveler",
                "email": "traveler@example.com"
            },
            "work": [
                {
                    "name": "Tardis Corp",
                    "position": "Pilot",
                    "startDate": "2024-05",
                    "endDate": "2021-01",
                    "highlights": ["Invented backwards causality."]
                }
            ]
        }"#;

        let result = Linter::lint(LintRequest {
            document_content: json.to_string(),
        })
        .expect("lint succeeds");

        assert!(!result.is_clean);
        assert!(result.findings.iter().any(|f| matches!(f.rule, FindingRule::DatesRunBackwards(_))));
    }

    #[test]
    fn lint_invalid_format_returns_error() {
        let bad_input = "this is definitely not a resume";
        let result = Linter::lint(LintRequest {
            document_content: bad_input.to_string(),
        });

        assert!(result.is_err(), "malformed document should return error");
    }
}

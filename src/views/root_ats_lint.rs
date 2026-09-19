//! Deterministic ATS findings, placed beside the fields that can answer them.
//!
//! B2 deliberately returns facts and addresses rather than screen copy. This
//! file is the UI half: one lint pass per editor frame, a mark on the section
//! card, a concise explanation, and an action that either focuses the exact
//! [`FieldId`] or restores a parser-known heading. There is no score and no
//! separate checker screen.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement, Window};

use dockcv_ui_components::{Button, ButtonExt, Icon, IconName, Sizable, Tag};

use crate::resume::ats::{self, Finding, Rule};
use crate::resume::edit::FieldId;
use crate::resume::model::{ResumeDoc, SectionKind};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::root::Root;

/// Lint the reading that would actually leave through an export.
///
/// `None` is the working copy; `Some` first applies that preset to a clone, so
/// switching the export-sheet preset changes the count before anything is
/// written and without changing the editor behind the sheet.
pub(super) fn findings_for_view(doc: &ResumeDoc, preset: Option<usize>) -> Vec<Finding> {
    let mut reading = doc.clone();
    if let Some(index) = preset {
        reading.apply_preset(index);
    }
    ats::lint(&reading.compose())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtsRemedy {
    FocusField(FieldId),
    StandardHeading,
    OpenSection,
}

struct FindingCopy {
    title: String,
    detail: String,
    action: &'static str,
}

impl Root {
    /// Refresh once per frame; section cards then filter this shared result
    /// instead of each composing and walking the whole document again.
    ///
    /// Per frame rather than on a change signal because `lint` is string work
    /// over an already-composed resume — no compile, no fonts, no disk. If a
    /// rule ever needs more than that, this is the line that has to become a
    /// cache keyed on the document's revision, not the render path it sits in.
    pub(super) fn refresh_ats_findings(&mut self) {
        self.ats_findings = findings_for_view(&self.doc, None);
    }

    pub(super) fn render_ats_chip(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> Option<AnyElement> {
        let count = self
            .ats_findings
            .iter()
            .filter(|finding| finding.section == section)
            .count();
        if count == 0 {
            return None;
        }
        let theme = *cx.theme();
        Some(
            Tag::custom(
                theme.warning.opacity(0.14),
                theme.warning,
                theme.warning.opacity(0.28),
            )
            .px(px(7.0))
            .py(px(2.0))
            .rounded(theme.radius_sm())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(Icon::new(IconName::TriangleAlert).small())
                    .child(format!("{count} ATS")),
            )
            .into_any_element(),
        )
    }

    /// The expanded section's findings. Every row carries a remedy; adding a
    /// new B2 rule makes the exhaustive copy match below fail to compile until
    /// its wording exists, while `remedy_for` still guarantees a safe fallback
    /// if a future finding has no field address.
    pub(super) fn render_ats_findings(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> Option<AnyElement> {
        let findings: Vec<Finding> = self
            .ats_findings
            .iter()
            .filter(|finding| finding.section == section)
            .cloned()
            .collect();
        if findings.is_empty() {
            return None;
        }
        let theme = *cx.theme();
        let count = findings.len();
        let mut rows = div().flex().flex_col().gap(px(8.0));

        for (index, finding) in findings.into_iter().enumerate() {
            let copy = copy_for(&finding.rule);
            let action_finding = finding.clone();
            rows = rows.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.0))
                    .pt(if index == 0 { px(0.0) } else { px(8.0) })
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(theme.warning.opacity(0.2))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_style(TextStyle::label())
                                    .text_color(theme.text)
                                    .child(copy.title),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_muted)
                                    .child(copy.detail),
                            ),
                    )
                    .child(
                        Button::new(format!("ats-remedy-{section:?}-{index}"))
                            .quiet()
                            .small()
                            .label(copy.action)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.apply_ats_remedy(&action_finding, window, cx);
                            })),
                    ),
            );
        }

        Some(
            div()
                .mb(px(14.0))
                .p(px(10.0))
                .rounded(theme.radius_md())
                .bg(theme.warning.opacity(0.07))
                .border_1()
                .border_color(theme.warning.opacity(0.25))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            Icon::new(IconName::TriangleAlert)
                                .small()
                                .text_color(theme.warning),
                        )
                        .child(
                            div()
                                .text_style(TextStyle::eyebrow())
                                .text_color(theme.warning)
                                .child(format!(
                                    "{count} ATS {}",
                                    if count == 1 { "finding" } else { "findings" }
                                )),
                        ),
                )
                .child(rows)
                .into_any_element(),
        )
    }

    fn apply_ats_remedy(&mut self, finding: &Finding, window: &mut Window, cx: &mut Context<Self>) {
        self.focused_section = finding.section;
        self.expanded.insert(finding.section);

        match remedy_for(finding) {
            AtsRemedy::FocusField(field) => {
                if let Some(binding) = self.fields.get(&field) {
                    binding.state.read(cx).focus_handle(cx).focus(window, cx);
                }
            }
            AtsRemedy::StandardHeading => {
                let standard = ResumeDoc::default_section_title(finding.section);
                if self.doc.section_title(finding.section) != standard {
                    self.checkpoint();
                    self.doc.set_section_title(finding.section, standard);
                    self.fields_stale = true;
                    self.schedule_save(cx);
                    self.schedule_recompile(window, cx);
                }
            }
            AtsRemedy::OpenSection => {}
        }
        cx.notify();
    }

    /// The export sheet's compact pre-flight status. Findings remain facts,
    /// not a gate: export is never disabled, and the button takes the user to
    /// the marked sections rather than pretending a score can decide for them.
    pub(super) fn render_export_ats_status(
        &self,
        cx: &mut Context<Self>,
        preset: Option<usize>,
    ) -> AnyElement {
        let theme = *cx.theme();
        let count = findings_for_view(&self.doc, preset).len();
        let clear = count == 0;

        div()
            .rounded(theme.radius_md())
            .border_1()
            .border_color(if clear {
                theme.border
            } else {
                theme.warning.opacity(0.35)
            })
            .bg(if clear {
                theme.surface
            } else {
                theme.warning.opacity(0.07)
            })
            .px_3()
            .py_2()
            .flex()
            .items_center()
            .gap(px(9.0))
            .child(
                Icon::new(if clear {
                    IconName::Check
                } else {
                    IconName::TriangleAlert
                })
                .small()
                .text_color(if clear { theme.accent } else { theme.warning }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .text_style(TextStyle::label())
                            .text_color(theme.text)
                            .child(if clear {
                                "ATS checks clear".to_string()
                            } else {
                                format!(
                                    "{count} ATS {} before export",
                                    if count == 1 { "finding" } else { "findings" }
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_muted)
                            .child(if clear {
                                "No deterministic content findings in this reading."
                            } else {
                                "Review the amber marks beside the affected sections."
                            }),
                    ),
            )
            .children((!clear).then(|| {
                Button::new("export-review-ats")
                    .quiet()
                    .small()
                    .label("Review findings")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.review_export_findings(preset, window, cx);
                    }))
            }))
            .into_any_element()
    }

    fn review_export_findings(
        &mut self,
        preset: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let findings = findings_for_view(&self.doc, preset);
        let Some(first) = findings.first() else {
            return;
        };
        if let Some(index) = preset {
            if self.active_preset() != Some(index) {
                self.apply_preset(index, window, cx);
            }
        }
        self.export_sheet = None;
        self.focused_section = first.section;
        self.expanded.insert(first.section);
        cx.notify();
    }
}

fn remedy_for(finding: &Finding) -> AtsRemedy {
    if matches!(finding.rule, Rule::HeadingNoParserKnows { .. }) {
        AtsRemedy::StandardHeading
    } else if let Some(field) = finding.at {
        AtsRemedy::FocusField(field)
    } else {
        AtsRemedy::OpenSection
    }
}

fn copy_for(rule: &Rule) -> FindingCopy {
    match rule {
        Rule::HeadingNoParserKnows { title, expected } => FindingCopy {
            title: "Parser may miss this heading".into(),
            detail: format!(
                "{} is outside the common vocabulary; use a familiar {expected} heading.",
                quote(title)
            ),
            action: "Use standard",
        },
        Rule::DateNoParserCanRead { text } => FindingCopy {
            title: "This date will be read as text".into(),
            detail: format!("{} is neither a calendar date nor Present.", quote(text)),
            action: "Edit date",
        },
        Rule::DatesRunBackwards { start, end } => FindingCopy {
            title: "These dates run backwards".into(),
            detail: format!("{} ends before {} starts.", quote(end), quote(start)),
            action: "Fix dates",
        },
        Rule::NoWayToReachThePerson => FindingCopy {
            title: "No email or phone".into(),
            detail: "A parser has no contact key for this CV.".into(),
            action: "Add contact",
        },
        Rule::MarkupThatNeverRenders { in_the_text, .. } => FindingCopy {
            title: "Typst syntax prints as source".into(),
            detail: format!("{} appears literally in every export.", quote(in_the_text)),
            action: "Edit text",
        },
        Rule::ListMarkerTypedIntoTheText { text } => FindingCopy {
            title: "This bullet has two markers".into(),
            detail: format!("{} already sits inside a list.", quote(text)),
            action: "Edit bullet",
        },
        Rule::CharactersNoFaceCanSet { text, missing } => {
            let glyphs: String = missing.iter().take(6).collect();
            FindingCopy {
                title: "The bundled fonts cannot draw this text".into(),
                detail: format!("{} contains unsupported characters: {glyphs}.", quote(text)),
                action: "Edit text",
            }
        }
        Rule::WhitespaceNobodySees { text, characters } => FindingCopy {
            title: "Invisible whitespace changes this field".into(),
            detail: format!(
                "{} contains {} non-standard whitespace {}.",
                quote(text),
                characters.len(),
                if characters.len() == 1 {
                    "character"
                } else {
                    "characters"
                }
            ),
            action: "Edit field",
        },
    }
}

fn quote(text: &str) -> String {
    const LIMIT: usize = 52;
    let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = flattened.chars();
    let short: String = chars.by_ref().take(LIMIT).collect();
    if chars.next().is_some() {
        format!("“{short}…”")
    } else {
        format!("“{short}”")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::dates::ResumeDate;
    use crate::resume::model::{Basics, Resume, Work};

    #[test]
    fn a_preset_is_linted_as_the_reading_it_exports() {
        let mut doc = ResumeDoc::from_resume(
            Resume {
                basics: Basics {
                    email: "person@example.com".into(),
                    ..Default::default()
                },
                work: vec![Work {
                    start_date: ResumeDate::new("2020"),
                    end_date: ResumeDate::new("2021"),
                    ..Default::default()
                }],
                ..Default::default()
            },
            "Base",
        );
        doc.add_variant(SectionKind::Work);
        doc.work.active_mut()[0].start_date = ResumeDate::new("Summer 2020");
        doc.add_preset("Broken date");
        doc.set_active_variant(SectionKind::Work, 0);

        assert!(findings_for_view(&doc, None).is_empty());
        assert!(findings_for_view(&doc, Some(0))
            .iter()
            .any(|finding| matches!(finding.rule, Rule::DateNoParserCanRead { .. })));
    }

    #[test]
    fn every_current_rule_has_copy_and_an_actionable_remedy() {
        let findings = [
            Finding {
                rule: Rule::HeadingNoParserKnows {
                    title: "Where I worked".into(),
                    expected: "work experience",
                },
                section: SectionKind::Work,
                at: None,
            },
            Finding {
                rule: Rule::DateNoParserCanRead {
                    text: "Summer 2020".into(),
                },
                section: SectionKind::Work,
                at: Some(FieldId::WorkStart(0)),
            },
            Finding {
                rule: Rule::DatesRunBackwards {
                    start: "2022".into(),
                    end: "2020".into(),
                },
                section: SectionKind::Work,
                at: Some(FieldId::WorkEnd(0)),
            },
            Finding {
                rule: Rule::NoWayToReachThePerson,
                section: SectionKind::Profile,
                at: Some(FieldId::Email),
            },
            Finding {
                rule: Rule::MarkupThatNeverRenders {
                    text: "#strong[p99]".into(),
                    in_the_text: "#strong[p99]".into(),
                },
                section: SectionKind::Profile,
                at: Some(FieldId::Summary),
            },
            Finding {
                rule: Rule::ListMarkerTypedIntoTheText {
                    text: "• shipped".into(),
                },
                section: SectionKind::Work,
                at: Some(FieldId::WorkHighlight(0, 0)),
            },
            Finding {
                rule: Rule::CharactersNoFaceCanSet {
                    text: "山".into(),
                    missing: vec!['山'],
                },
                section: SectionKind::Profile,
                at: Some(FieldId::Name),
            },
            Finding {
                rule: Rule::WhitespaceNobodySees {
                    text: "+1\u{a0}555".into(),
                    characters: vec!['\u{a0}'],
                },
                section: SectionKind::Profile,
                at: Some(FieldId::Phone),
            },
        ];

        for finding in findings {
            let copy = copy_for(&finding.rule);
            assert!(!copy.title.is_empty());
            assert!(!copy.detail.is_empty());
            assert!(!copy.action.is_empty());
            assert_ne!(remedy_for(&finding), AtsRemedy::OpenSection);
        }
    }

    #[test]
    fn long_user_text_is_flattened_and_bounded() {
        let quoted = quote("one\ntwo three four five six seven eight nine ten eleven twelve");
        assert!(!quoted.contains('\n'));
        assert!(quoted.ends_with("…”"));
    }
}

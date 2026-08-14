//! First-run import flow & "+ New CV" chooser ("Starting from what you have").
//!
//! Design mockup: `.design/DockCV-Refresh.html` (lines 1296–1368).
//!
//! Implements:
//! 1. `Step 1 · bring a document`: a drop zone, a file browser trigger, and
//!    "Skip — start blank →". The mockup's format switcher is **not**
//!    reproduced: `import::import_file` picks the engine from the file's own
//!    extension and never consulted the tab, so the choice was fiction —
//!    picking DOCX and dropping a PDF worked, and the reverse worked too. The
//!    formats are stated instead of offered.
//! 2. `Parsing`: animated loading state.
//! 3. `Step 2 · review the split`: section confidence list ("Looks good" vs
//!    "Needs review"), 1-click "Undo import" back to Step 1, and
//!    "Looks good — continue" to enter the editor.

use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    div, px, ClickEvent, Context, FontWeight, IntoElement, PathPromptOptions,
    SharedString,
};

use dockcv_ui_components::{lucide, Button, ButtonExt, SANS, SERIF};

use crate::import::model::{Confidence, ImportedDoc};
use crate::resume::altacv;
use crate::resume::model::ResumeDoc;
use crate::theme::ActiveTheme;

#[allow(dead_code)]
fn first_path(
    paths: Result<Result<Option<Vec<PathBuf>>, impl std::fmt::Debug>, impl std::fmt::Debug>,
) -> Option<PathBuf> {
    let opt_vec = paths.ok()?.ok()?;
    let vec = opt_vec?;
    vec.into_iter().next()
}

/// The formats the importer reads, named for the drop zone.
///
/// A statement of fact, not a mode: the engine is chosen from the file's own
/// extension by `import::import_file`, so nothing here changes behaviour and
/// there is nothing for the user to get wrong.
const ACCEPTED_FORMATS: [&str; 5] = ["PDF", "DOCX", "LinkedIn .zip", "JSON Resume", "TXT / MD"];

/// Where the LinkedIn archive comes from.
const LINKEDIN_HINT: &str = "LinkedIn → Settings → Data privacy → Get a copy of your data";

/// The active step in the import wizard.
#[derive(Clone, Default)]
pub enum ImportStep {
    /// Step 1: drag-and-drop, or browse for a file.
    #[default]
    Step1Drop,
    /// Parsing file / fetching URL with animation.
    Parsing { filename: String },
    /// Step 2: Review extracted sections and confidence flags.
    Step2Review { imported: Box<ImportedDoc> },
}

/// How many extracted entries a flagged section shows before it summarises the
/// rest. Enough to recognise a mis-parse; short of turning the card into the
/// document.
const PREVIEW_LINES: usize = 4;

/// Standardized section item data for Step 2 review.
struct SectionReviewItem {
    name: String,
    detail: String,
    needs_review: bool,
    review_reason: Option<&'static str>,
    /// What the parser actually produced, one line per entry.
    ///
    /// A flag with no evidence is not reviewable — "Partly guessed" tells the
    /// user something is wrong and gives them nothing to check it against. The
    /// flagged section shows its entries so the judgement can be made here,
    /// where undo is still one click away.
    preview: Vec<String>,
}

impl SectionReviewItem {
    /// Build the review list from **what the parser actually reported**, and
    /// from nothing else.
    ///
    /// An earlier version inferred flags from list lengths — more than one
    /// education entry became "Dates look reversed — check order", a single
    /// skill group became "Couldn't tell categories apart". Both are ordinary,
    /// correct shapes for a CV, and neither says anything about dates or
    /// categories. Telling a user their data is suspect when it is not is the
    /// same failure as inventing a metric: the screen asserts something it does
    /// not know. A section the classifier said nothing about is simply not
    /// flagged.
    fn from_imported(imported: &ImportedDoc) -> Vec<Self> {
        let profile = imported.doc.profile.active();
        let work = imported.doc.work.active();
        let edu = imported.doc.education.active();
        let skills = imported.doc.skills.active();
        let certs = imported.doc.certificates.active();

        // The classifier's own vocabulary, kept vague on purpose: it knows how
        // sure it was, not *what* went wrong, and a specific-sounding reason it
        // cannot back up is worse than an honest general one.
        let reason = |key: &str| match imported.confidence.get(key) {
            Some(Confidence::Low) => Some("Couldn't read this reliably — check it"),
            Some(Confidence::Medium) => Some("Partly guessed — worth a look"),
            _ => None,
        };

        let work_highlights: usize = work.iter().map(|w| w.highlights.len()).sum();

        // Join the parts an entry actually has — an empty field would otherwise
        // show up as a stray separator, which reads as data the parser lost.
        let line = |parts: [&str; 2]| {
            parts
                .iter()
                .filter(|p| !p.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" — ")
        };

        [
            (
                "Profile",
                format!("{}, {}, {}", profile.name, profile.label, profile.email),
                reason("profile.name"),
                vec![
                    line([&profile.name, &profile.label]),
                    line([&profile.email, &profile.location]),
                ],
            ),
            (
                "Work Experience",
                format!("{} roles, {work_highlights} highlights", work.len()),
                reason("work"),
                work.iter().map(|w| line([&w.position, &w.name])).collect(),
            ),
            (
                "Education",
                format!("{} entries", edu.len()),
                reason("education"),
                edu.iter()
                    .map(|e| line([&e.study_type, &e.institution]))
                    .collect(),
            ),
            (
                "Skills",
                format!("{} category groups", skills.len()),
                reason("skills"),
                skills.iter().map(|s| s.name.clone()).collect(),
            ),
            (
                "Certificates",
                format!("{} entries", certs.len()),
                reason("certificates"),
                certs.iter().map(|c| line([&c.name, &c.issuer])).collect(),
            ),
        ]
        .into_iter()
        .map(|(name, detail, review_reason, preview)| Self {
            name: name.to_string(),
            detail,
            needs_review: review_reason.is_some(),
            review_reason,
            preview: preview.into_iter().filter(|l| !l.is_empty()).collect(),
        })
        // Sections the document had and the model has no built-in shape for —
        // Projects, Languages, Interests. They were imported and **not listed**,
        // so "5 sections found" was five however many the CV really had, and a
        // user had no way to see that a whole section had made it in.
        .chain(imported.doc.custom_sections.iter().map(|section| {
            let entries = section.content.active();
            Self {
                name: section.title.clone(),
                detail: format!(
                    "{} {}",
                    entries.len(),
                    if entries.len() == 1 { "entry" } else { "entries" }
                ),
                needs_review: false,
                review_reason: None,
                preview: entries
                    .iter()
                    .map(|e| line([&e.title, &e.subtitle]))
                    .filter(|l| !l.is_empty())
                    .collect(),
            }
        }))
        .collect()
    }
}

/// Prompt for a file on disk (PDF, DOCX, JSON, TXT).
#[allow(dead_code)]
pub fn prompt_for_import_file<V: 'static>(
    cx: &mut Context<V>,
    on_result: impl Fn(&mut V, Result<ImportedDoc, String>, &mut Context<V>) + 'static + Copy,
) {
    let prompt = PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some("Select a CV — PDF, DOCX, LinkedIn .zip, JSON Resume, TXT".into()),
    };
    let receiver = cx.prompt_for_paths(prompt);
    let executor = cx.background_executor().clone();

    cx.spawn(async move |this, cx| {
        let Some(file_path) = first_path(receiver.await) else {
            return;
        };
        let result = executor
            .spawn(async move { crate::import::import_file(&file_path) })
            .await;
        let _ = this.update(cx, move |this, cx| {
            on_result(this, result, cx);
        });
    })
    .detach();
}

/// Build sample imported doc if none provided.
#[allow(dead_code)]
pub fn sample_imported_doc() -> ImportedDoc {
    let resume = altacv::import(altacv::ALTACV_SAMPLE).unwrap_or_default();
    let doc = ResumeDoc::from_resume(resume, "Base");
    let mut imported = ImportedDoc::new("PDF", doc);
    imported.set_confidence("education", Confidence::Low);
    imported.set_confidence("skills", Confidence::Low);
    imported
}

pub fn render_step1_bring_document<V: 'static>(
    cx: &mut Context<V>,
    on_browse: impl Fn(&mut V, &mut Context<V>) + 'static + Copy,
    on_skip_blank: impl Fn(&mut V, &mut Context<V>) + 'static + Copy,
) -> impl IntoElement {
    let theme = cx.theme().clone();

    div()
        .w(px(560.0))
        .h(px(600.0))
        .rounded(px(11.0))
        .overflow_hidden()
        .border_1()
        .border_color(theme.border)
        .bg(theme.elevated)
        .shadow_lg()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .p(px(40.0))
        // Brand logo
        .child(
            div()
                .flex()
                .items_baseline()
                .mb(px(8.0))
                .child(
                    div()
                        .text_size(px(21.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text)
                        .child("Dock"),
                )
                .child(
                    div()
                        .text_size(px(21.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.accent)
                        .child("CV"),
                ),
        )
        // Title
        .child(
            div()
                .font_family(SERIF)
                .text_size(px(24.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .text_center()
                .mb(px(8.0))
                .child("Bring what you already have"),
        )
        // Subtitle
        .child(
            div()
                .font_family(SANS)
                .text_size(px(13.5))
                .text_color(theme.text_muted)
                .text_center()
                .max_w(px(380.0))
                .line_height(px(20.0))
                .mb(px(28.0))
                .child("We'll split it into sections and blocks you can edit right away."),
        )
        // What we accept — a statement, not a choice.
        //
        // These were three tabs, and the choice was fiction: `import_file`
        // dispatches on the file's own extension and never consulted the tab,
        // so picking DOCX and dropping a PDF worked, and picking PDF and
        // dropping a DOCX worked too. A control that changes nothing teaches
        // the user something untrue about the product.
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .mb(px(20.0))
                .children(ACCEPTED_FORMATS.iter().map(|format| {
                    div()
                        .font_family(crate::theme::MONO)
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .px(px(9.0))
                        .py(px(4.0))
                        .rounded(px(6.0))
                        .bg(theme.hover)
                        .text_color(theme.text_muted)
                        .child(*format)
                })),
        )
        // Drag and Drop Zone
        .child(
            div()
                .id("dropzone-area")
                .w_full()
                .max_w(px(420.0))
                .h(px(150.0))
                .border_1()
                .border_dashed()
                .border_color(theme.border_strong)
                .rounded(px(11.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .cursor_pointer()
                .hover(|s| s.bg(theme.hover).border_color(theme.accent))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    on_browse(this, cx);
                }))
                .child(
                    div()
                        .w(px(34.0))
                        .h(px(34.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(theme.text_subtle)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.text_subtle)
                        .text_size(px(16.0))
                        .child("↑"),
                )
                .child(
                    div()
                        .font_family(SANS)
                        .text_size(px(13.5))
                        .text_color(theme.text)
                        .child("Drop your CV here"),
                )
                .child(
                    div()
                        .font_family(SANS)
                        .text_size(px(12.0))
                        .text_color(theme.text_subtle)
                        .child("or click to browse"),
                )
                .child(
                    // The LinkedIn archive is three clicks deep in a settings
                    // screen most people have never opened, and naming the
                    // format without saying where to get it is a dead end.
                    div()
                        .mt(px(10.0))
                        .font_family(crate::theme::MONO)
                        .text_size(px(10.5))
                        .text_color(theme.text_subtle)
                        .child(LINKEDIN_HINT),
                ),
        )
        // Skip link
        .child(
            div()
                .id("skip-start-blank")
                .mt(px(26.0))
                .font_family(SANS)
                .text_size(px(13.0))
                .text_color(theme.text_subtle)
                .cursor_pointer()
                .hover(|s| s.text_color(theme.text))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    on_skip_blank(this, cx);
                }))
                .child("Skip — start blank →"),
        )
}

pub fn render_parsing_step<V: 'static>(
    cx: &mut Context<V>,
    filename: &str,
) -> impl IntoElement {
    let theme = cx.theme().clone();

    div()
        .w(px(560.0))
        .h(px(600.0))
        .rounded(px(11.0))
        .overflow_hidden()
        .border_1()
        .border_color(theme.border)
        .bg(theme.elevated)
        .shadow_lg()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .p(px(40.0))
        .child(
            div()
                .text_size(px(24.0))
                .mb(px(12.0))
                .text_color(theme.accent)
                .child("⟳"),
        )
        .child(
            div()
                .font_family(SERIF)
                .text_size(px(20.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .mb(px(6.0))
                .child("Parsing document..."),
        )
        .child(
            div()
                .font_family(SANS)
                .text_size(px(13.0))
                .text_color(theme.text_muted)
                .child(filename.to_string()),
        )
}

/// The lines the classifier could not place in any section.
///
/// This is the requirement US-01 is actually about — "everything not understood
/// is flagged, not silently dropped" — and until now nothing rendered
/// `ImportedDoc::unparsed` at all: the importer computed it and threw it away,
/// so a CV could lose paragraphs on the way in with no trace. Shown verbatim,
/// because a summary of what was lost is not evidence of what was lost.
fn render_unparsed<V: 'static>(cx: &mut Context<V>, imported: &ImportedDoc) -> Option<impl IntoElement> {
    if imported.unparsed.is_empty() {
        return None;
    }
    let theme = cx.theme().clone();
    let count = imported.unparsed.len();
    // Long tails are common in a scanned PDF; show enough to judge by and say
    // how many are behind it rather than pretending the list is complete.
    const SHOWN: usize = 12;
    let shown: Vec<String> = imported.unparsed.iter().take(SHOWN).cloned().collect();
    let hidden = count.saturating_sub(shown.len());

    Some(
        div()
            .mt(px(6.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(theme.surface)
            .border_1()
            .border_color(theme.warning)
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.warning)
                    .child(format!(
                        "{count} line{} didn't fit any section",
                        if count == 1 { "" } else { "s" }
                    )),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(theme.text_muted)
                    .child(
                        "They are not in the imported CV. Copy anything worth keeping before you \
                         continue, or undo the import and bring the file in another format.",
                    ),
            )
            .children(shown.into_iter().map(|line| {
                div()
                    .text_size(px(11.5))
                    .text_color(theme.text_subtle)
                    .child(line)
            }))
            .children((hidden > 0).then(|| {
                div()
                    .text_size(px(11.5))
                    .text_color(theme.text_subtle)
                    .child(format!("…and {hidden} more"))
            })),
    )
}

pub fn render_step2_review_split<V: 'static>(
    cx: &mut Context<V>,
    imported: &ImportedDoc,
    on_undo: impl Fn(&mut V, &mut Context<V>) + 'static,
    on_continue: impl Fn(&mut V, &mut Context<V>) + 'static,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let items = SectionReviewItem::from_imported(imported);
    let flagged_count = items.iter().filter(|i| i.needs_review).count();
    let total_count = items.len();

    div()
        .w(px(560.0))
        // Grows with the window instead of a fixed 600px. The list of
        // sections was scrolling *inside* a short card that was itself inside
        // the gallery's scroll area — two nested scrollbars for one list, on a
        // screen with room to spare.
        .flex_1()
        .min_h(px(420.0))
        .max_h(px(880.0))
        .rounded(px(11.0))
        .overflow_hidden()
        .border_1()
        .border_color(theme.border)
        .bg(theme.elevated)
        .shadow_lg()
        .flex()
        .flex_col()
        // Top Bar
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(22.0))
                .py(px(16.0))
                .border_b_1()
                .border_color(theme.border)
                .child(
                    // A bordered control, not accent-coloured text: this is the
                    // only way out of the review step, and it has to carry the
                    // same weight as "Back to the gallery" one step earlier.
                    Button::new("undo-import-btn")
                        .cursor_pointer()
                        .toolbar_secondary()
                        .icon(lucide("undo"))
                        .label("Undo import")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            on_undo(this, cx);
                        })),
                )
                .child(
                    div()
                        .font_family(crate::theme::MONO)
                        .text_size(px(11.5))
                        .text_color(theme.text_subtle)
                        .child(imported.format_name.clone()),
                ),
        )
        // Overview Header
        .child(
            div()
                .px(px(22.0))
                .pt(px(18.0))
                .pb(px(8.0))
                .child(
                    div()
                        .font_family(SERIF)
                        .text_size(px(20.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .mb(px(4.0))
                        .child(format!("{total_count} sections found")),
                )
                .child(
                    div()
                        .font_family(crate::theme::MONO)
                        .text_size(px(12.5))
                        .text_color(if flagged_count > 0 {
                            theme.accent
                        } else {
                            theme.text_muted
                        })
                        .child(if flagged_count > 0 {
                            format!("{flagged_count} need a quick look")
                        } else {
                            "All sections extracted cleanly".to_string()
                        }),
                ),
        )
        // Section Cards List
        .child(
            div()
                .id("review-items-scroll")
                .flex_1()
                .overflow_y_scroll()
                .px(px(22.0))
                .py(px(10.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(items.into_iter().enumerate().map(|(idx, item)| {
                    let is_flagged = item.needs_review;
                    div()
                        .id(SharedString::from(format!("review-item-{idx}")))
                        .flex()
                        // A flagged card grows to fit its evidence, so the dot
                        // and the badge align to the title rather than drifting
                        // to the middle of the block.
                        .items_start()
                        .gap(px(11.0))
                        .px(px(13.0))
                        .py(px(11.0))
                        .rounded(px(9.0))
                        .border_1()
                        .when(is_flagged, |s| {
                            s.border_color(theme.warning)
                                .bg(theme.hover)
                        })
                        .when(!is_flagged, |s| {
                            s.border_color(theme.border)
                                .bg(theme.surface)
                        })
                        // Status dot
                        .child(
                            div()
                                .mt(px(6.0))
                                .w(px(8.0))
                                .h(px(8.0))
                                .rounded_full()
                                .flex_none()
                                .bg(if is_flagged {
                                    theme.warning
                                } else {
                                    theme.success
                                }),
                        )
                        // Content
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.text)
                                        .child(item.name),
                                )
                                // The count stays put when a section is
                                // flagged. Replacing it with the reason left
                                // the user with a warning and no facts —
                                // nothing to review against.
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(theme.text_muted)
                                        .child(item.detail),
                                )
                                .when_some(item.review_reason, |el, reason| {
                                    el.child(
                                        div()
                                            .mt(px(2.0))
                                            .text_size(px(11.5))
                                            .text_color(theme.warning)
                                            .child(reason),
                                    )
                                })
                                .when(is_flagged && !item.preview.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .mt(px(7.0))
                                            .flex()
                                            .flex_col()
                                            .gap(px(3.0))
                                            .font_family(crate::theme::MONO)
                                            .text_size(px(11.0))
                                            .text_color(theme.text)
                                            .children(
                                                item.preview
                                                    .iter()
                                                    .take(PREVIEW_LINES)
                                                    .map(|l| div().child(l.clone())),
                                            )
                                            .when(item.preview.len() > PREVIEW_LINES, |el| {
                                                el.child(
                                                    div().text_color(theme.text_muted).child(
                                                        format!(
                                                            "+{} more",
                                                            item.preview.len() - PREVIEW_LINES
                                                        ),
                                                    ),
                                                )
                                            }),
                                    )
                                }),
                        )
                        // Label badge
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(12.0))
                                .text_color(if is_flagged {
                                    theme.warning
                                } else {
                                    theme.success
                                })
                                .child(if is_flagged {
                                    "Needs review"
                                } else {
                                    "Looks good"
                                }),
                        )
                }))
                .children(render_unparsed(cx, imported)),
        )
        // Bottom Action Bar
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(22.0))
                .py(px(16.0))
                .border_t_1()
                .border_color(theme.border)
                // A statement of what is left to do, not a control. It used to
                // read "Review flagged first" as plain text in the slot a
                // secondary button occupies, so it looked clickable and was
                // not — the same false affordance P-09 names. Now it says
                // something true and says nothing when there is nothing to
                // say.
                .children((flagged_count > 0).then(|| {
                    div()
                        .text_size(px(13.0))
                        .text_color(theme.warning)
                        .child(format!(
                            "{flagged_count} section{} to check above",
                            if flagged_count == 1 { "" } else { "s" }
                        ))
                }))
                .when(flagged_count == 0, |el| el.child(div()))
                .child(
                    Button::new("continue-to-editor")
                        .cursor_pointer()
                        .toolbar_primary()
                        .label("Looks good — continue")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            on_continue(this, cx);
                        })),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::SectionReviewItem;
    use crate::import::model::{Confidence, ImportedDoc};
    use crate::resume::model::{Education, Resume, ResumeDoc, SkillGroup};

    fn imported_with(education: usize, skill_groups: usize) -> ImportedDoc {
        let resume = Resume {
            education: (0..education).map(|_| Education::default()).collect(),
            skills: (0..skill_groups).map(|_| SkillGroup::default()).collect(),
            ..Default::default()
        };
        ImportedDoc::new("PDF", ResumeDoc::from_resume(resume, "Base"))
    }

    /// The regression this guards: review flags used to be inferred from list
    /// lengths — more than one education entry became "Dates look reversed",
    /// one skill group became "Couldn't tell categories apart". Both shapes are
    /// perfectly ordinary, so the screen was telling users their data was
    /// suspect on no evidence. A section the classifier said nothing about is
    /// not flagged.
    #[test]
    fn a_section_the_parser_said_nothing_about_is_not_flagged() {
        let imported = imported_with(3, 1);
        let items = SectionReviewItem::from_imported(&imported);

        assert!(
            items.iter().all(|i| !i.needs_review),
            "nothing was reported low-confidence, so nothing may be flagged"
        );
        assert!(items.iter().all(|i| i.review_reason.is_none()));
    }

    /// …and what the classifier *did* report still comes through, at the
    /// severity it reported.
    #[test]
    fn what_the_parser_flagged_reaches_the_review_list() {
        let mut imported = imported_with(1, 4);
        imported.set_confidence("work", Confidence::Low);
        imported.set_confidence("profile.name", Confidence::Medium);

        let items = SectionReviewItem::from_imported(&imported);
        let flagged: Vec<(&str, Option<&str>)> = items
            .iter()
            .filter(|i| i.needs_review)
            .map(|i| (i.name.as_str(), i.review_reason))
            .collect();

        assert_eq!(flagged.len(), 2, "got {flagged:?}");
        assert_eq!(
            flagged
                .iter()
                .find(|(name, _)| *name == "Work Experience")
                .and_then(|(_, reason)| *reason),
            Some("Couldn't read this reliably — check it")
        );
        assert_eq!(
            flagged
                .iter()
                .find(|(name, _)| *name == "Profile")
                .and_then(|(_, reason)| *reason),
            Some("Partly guessed — worth a look")
        );
    }
}

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

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, FontWeight, IntoElement, SharedString};

use dockcv_ui_components::{
    lucide, Button, ButtonExt, Card, DockIcon, Icon, IconName, ScrollableElement, Sizable, Spinner,
};

use crate::import::model::ImportedDoc;
use crate::import::notes::Part;
use crate::theme::{ActiveTheme, StyledText, TextStyle};

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
    /// The file would not come in.
    ///
    /// A step of its own rather than an error string dropped on the drop zone —
    /// which is what it was, and the drop zone never rendered it, so a failed
    /// import bounced back to the start with no explanation at all. This is the
    /// first thing a new user can hit (US-01), and it has to answer two
    /// questions: whether the file is at fault, and where to go instead.
    CouldNotRead {
        filename: String,
        error: Box<crate::import::ImportError>,
    },
}

/// How many extracted entries a flagged section shows before it summarises the
/// rest. Enough to recognise a mis-parse; short of turning the card into the
/// document.
const PREVIEW_LINES: usize = 4;

/// How long one of those entries may run before it is cut.
///
/// A preview line is a job title or a degree — a few words. When the parser has
/// gone wrong it is sometimes a whole paragraph it mistook for one, and that is
/// worth *seeing* but not worth six lines of the card: the point is to recognise
/// the mistake, not to read it.
const PREVIEW_CHARS: usize = 88;

fn shorten(line: &str) -> String {
    if line.chars().count() <= PREVIEW_CHARS {
        return line.to_string();
    }
    let cut: String = line.chars().take(PREVIEW_CHARS).collect();
    // Break at the last space so the cut lands between words.
    let cut = match cut.rsplit_once(' ') {
        Some((head, _)) if head.chars().count() > PREVIEW_CHARS / 2 => head.to_string(),
        _ => cut,
    };
    format!("{}…", cut.trim_end_matches([',', '.', ' ']))
}

/// Standardized section item data for Step 2 review.
struct SectionReviewItem {
    name: String,
    detail: String,
    needs_review: bool,
    /// Nothing came out of this section.
    ///
    /// An empty built-in section with nothing noticed about it is not shown at
    /// all: `Certificates · 0 entries` on a CV that has no certificates is a row
    /// that tells the reader nothing and pushes the ones that matter down. An
    /// empty section the *source* said was there keeps its place, because then
    /// the emptiness is the finding and a note says so.
    empty: bool,
    /// What the parser noticed about this section, one sentence each.
    ///
    /// Was a single `Option<&'static str>` carrying `"Partly guessed — worth a
    /// look"`, which is a phrase and not a finding. A note names what was seen
    /// and the numbers behind it, and there can be more than one.
    notes: Vec<String>,
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

        // What the parser actually noticed, in its own words. Empty for a
        // section it had nothing to say about — which is most of them on a
        // clean import, and is the whole reason a flag now means something.
        let notes = |part: Part| imported.notes_for(part).collect::<Vec<_>>();

        let work_highlights: usize = work.iter().map(|w| w.highlights.len()).sum();

        // `1 entries` and `1 category groups` were on screen. A count the user
        // reads is a sentence, not a template with a number pushed into it.
        let count =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });

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
                false,
                notes(Part::Profile),
                vec![
                    line([&profile.name, &profile.label]),
                    line([&profile.email, &profile.location]),
                ],
            ),
            (
                "Work Experience",
                format!(
                    "{}, {}",
                    count(work.len(), "role", "roles"),
                    count(work_highlights, "highlight", "highlights")
                ),
                work.is_empty(),
                notes(Part::Work),
                work.iter().map(|w| line([&w.position, &w.name])).collect(),
            ),
            (
                "Education",
                count(edu.len(), "entry", "entries"),
                edu.is_empty(),
                notes(Part::Education),
                edu.iter()
                    .map(|e| line([&e.study_type, &e.institution]))
                    .collect(),
            ),
            (
                "Skills",
                count(skills.len(), "group", "groups"),
                skills.is_empty(),
                notes(Part::Skills),
                skills.iter().map(|s| s.name.clone()).collect(),
            ),
            (
                "Certificates",
                count(certs.len(), "entry", "entries"),
                certs.is_empty(),
                notes(Part::Certificates),
                certs.iter().map(|c| line([&c.name, &c.issuer])).collect(),
            ),
        ]
        .into_iter()
        .map(|(name, detail, empty, notes, preview)| Self {
            name: name.to_string(),
            detail,
            empty,
            needs_review: !notes.is_empty(),
            notes,
            preview: preview.into_iter().filter(|l| !l.is_empty()).collect(),
        })
        .filter(|item| !item.empty || item.needs_review)
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
                    if entries.len() == 1 {
                        "entry"
                    } else {
                        "entries"
                    }
                ),
                empty: entries.is_empty(),
                needs_review: false,
                notes: Vec::new(),
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

pub fn render_step1_bring_document<V: 'static>(
    cx: &mut Context<V>,
    on_browse: impl Fn(&mut V, &mut Context<V>) + 'static + Copy,
    on_skip_blank: impl Fn(&mut V, &mut Context<V>) + 'static + Copy,
) -> impl IntoElement {
    let theme = *cx.theme();

    div()
        .w(px(560.0))
        .h(px(600.0))
        .rounded(theme.radius_lg())
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
                // Matched to the rail's wordmark, which is the same mark on
                // the same product; 21/700 was the loudest thing on a screen
                // whose job is to get out of the way.
                .child(
                    div()
                        .text_size(px(17.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child("Dock"),
                )
                .child(
                    div()
                        .text_size(px(17.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.accent)
                        .child("CV"),
                ),
        )
        // Title
        .child(
            div()
                .text_style(TextStyle::hero())
                .text_size(px(28.0))
                .text_color(theme.text)
                .text_center()
                .mb(px(8.0))
                .child("Bring what you already have"),
        )
        // Subtitle
        .child(
            div()
                .text_style(TextStyle::body())
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
                        .text_style(TextStyle::chip())
                        .px(px(9.0))
                        .py(px(4.0))
                        .rounded(theme.radius_sm())
                        .bg(theme.hover)
                        .text_color(theme.text_muted)
                        .child(*format)
                })),
        )
        // Drag and Drop Zone
        .child(
            Card::new()
                .outline()
                .interactive("dropzone-area")
                .border_dashed()
                .border_color(theme.border_strong)
                .max_w(px(420.0))
                .h(px(150.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    on_browse(this, cx);
                }))
                .child(
                    div()
                        .w(px(34.0))
                        .h(px(34.0))
                        .rounded(theme.radius_md())
                        .border_1()
                        .border_color(theme.text_subtle)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.text_subtle)
                        // `DockIcon::Download` exists for precisely this glyph;
                        // it was being drawn as a `↑` character, at whatever
                        // size and in whatever font happened to carry it.
                        .child(Icon::new(DockIcon::Download).with_size(theme.icon_lg())),
                )
                .child(
                    div()
                        .text_style(TextStyle::body())
                        .text_color(theme.text)
                        .child("Drop your CV here"),
                )
                .child(
                    div()
                        .text_style(TextStyle::label())
                        .text_color(theme.text_subtle)
                        .child("or click to browse"),
                )
                .child(
                    // The LinkedIn archive is three clicks deep in a settings
                    // screen most people have never opened, and naming the
                    // format without saying where to get it is a dead end.
                    div()
                        .mt(px(10.0))
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_subtle)
                        .child(LINKEDIN_HINT),
                ),
        )
        // Skip link
        .child(
            Button::new("skip-start-blank")
                .quiet()
                .mt(px(26.0))
                .text_color(theme.text_subtle)
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    on_skip_blank(this, cx);
                }))
                .child("Skip — start blank →"),
        )
}

pub fn render_parsing_step<V: 'static>(cx: &mut Context<V>, filename: &str) -> impl IntoElement {
    let theme = *cx.theme();

    div()
        .w(px(560.0))
        .h(px(600.0))
        .rounded(theme.radius_lg())
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
                .mb(px(12.0))
                .child(Spinner::new().large().color(theme.accent)),
        )
        .child(
            div()
                .text_style(TextStyle::title())
                .text_color(theme.text)
                .mb(px(6.0))
                .child("Parsing document..."),
        )
        .child(
            div()
                .text_style(TextStyle::body())
                .text_color(theme.text_muted)
                .child(filename.to_string()),
        )
}

/// What the importer read and had nowhere to put.
///
/// This is the requirement US-01 is actually about — "everything not understood
/// is flagged, not silently dropped" — and until now nothing rendered
/// `ImportedDoc::unplaced` at all: the importer computed it and threw it away,
/// so a CV could lose paragraphs on the way in with no trace. Shown verbatim,
/// because a summary of what was lost is not evidence of what was lost.
fn render_unplaced<V: 'static>(
    cx: &mut Context<V>,
    imported: &ImportedDoc,
) -> Option<impl IntoElement> {
    if imported.unplaced.is_empty() {
        return None;
    }
    let theme = *cx.theme();
    let count = imported.unplaced.len();
    // Long tails are common in a scanned PDF; show enough to judge by and say
    // how many are behind it rather than pretending the list is complete.
    const SHOWN: usize = 12;
    let shown: Vec<String> = imported.unplaced.iter().take(SHOWN).cloned().collect();
    let hidden = count.saturating_sub(shown.len());

    Some(
        div()
            .mt(px(6.0))
            .p(px(12.0))
            .rounded(theme.radius_md())
            .bg(theme.surface)
            .border_1()
            .border_color(theme.warning)
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_style(TextStyle::label())
                    .text_color(theme.warning)
                    .child(format!(
                        "{count} thing{} came in that DockCV has nowhere for",
                        if count == 1 { "" } else { "s" }
                    )),
            )
            .child(
                div()
                    .text_style(TextStyle::label())
                    .text_color(theme.text_muted)
                    .child(
                        "They were read from your file and are not in the imported CV. Copy \
                         anything worth keeping before you continue, or undo the import and bring \
                         the file in another format.",
                    ),
            )
            .children(shown.into_iter().map(|line| {
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child(line)
            }))
            .children((hidden > 0).then(|| {
                div()
                    .text_style(TextStyle::meta())
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
    let theme = *cx.theme();
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
        .rounded(theme.radius_lg())
        .overflow_hidden()
        .border_1()
        .border_color(theme.border)
        .bg(theme.elevated)
        .shadow_lg()
        .flex()
        .flex_col()
        // Header. `Undo import` used to sit alone in a bar of its own at the
        // top, which made the one control on screen the one that throws the
        // work away. Both decisions now live together in the footer, where a
        // decision belongs.
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap(px(16.0))
                .px(px(22.0))
                .pt(px(20.0))
                .pb(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .text_style(TextStyle::title())
                                .text_color(theme.text)
                                .child(format!("{total_count} sections found")),
                        )
                        .child(
                            div()
                                // Amber, not accent: this is a warning about
                                // work left to do, and accent-on-mono read as a
                                // link that went nowhere.
                                .text_style(TextStyle::label())
                                .text_color(if flagged_count > 0 {
                                    theme.warning
                                } else {
                                    theme.text_muted
                                })
                                .child(if flagged_count > 0 {
                                    format!(
                                        "{flagged_count} need{} a quick look before you continue",
                                        if flagged_count == 1 { "s" } else { "" }
                                    )
                                } else {
                                    "Everything came through cleanly".to_string()
                                }),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .text_style(TextStyle::chip())
                        .text_color(theme.text_subtle)
                        .child(imported.format_name.clone()),
                ),
        )
        // Section Cards List
        .child(
            div()
                .id("review-items-scroll")
                .flex_1()
                .overflow_y_scrollbar()
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
                        .flex_col()
                        .px(px(13.0))
                        .py(px(10.0))
                        .rounded(theme.radius_md())
                        // A section with nothing to say says nothing. It used
                        // to carry a green dot and a `Looks good` badge, and
                        // seven of those competed with the two that mattered —
                        // the same "a flag that is always lit" failure the
                        // notes mechanism was built to end, wearing the
                        // opposite colour. `Certificates · 0 entries · Looks
                        // good` was the tell: an empty section is not good, it
                        // is empty.
                        .when(is_flagged, |s| {
                            s.bg(theme.hover).border_l_2().border_color(theme.warning)
                        })
                        .when(!is_flagged, |s| s.bg(theme.surface))
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .justify_between()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .text_style(TextStyle::control())
                                        .text_color(theme.text)
                                        .child(item.name),
                                )
                                // The count stays put when a section is
                                // flagged. Replacing it with the reason left
                                // the user with a warning and no facts —
                                // nothing to review against.
                                .child(
                                    div()
                                        .flex_none()
                                        .text_style(TextStyle::chip())
                                        .text_color(theme.text_subtle)
                                        .child(item.detail),
                                ),
                        )
                        // One line per thing the parser noticed. There can be
                        // more than one — a section can be both undated and
                        // missing its employers, and saying only the first
                        // would hide half the work.
                        .children(item.notes.iter().map(|note| {
                            div()
                                .mt(px(4.0))
                                .text_style(TextStyle::label())
                                .text_color(theme.warning)
                                .child(note.clone())
                        }))
                        .when(is_flagged && !item.preview.is_empty(), |el| {
                            el.child(
                                div()
                                    .mt(px(8.0))
                                    .pt(px(8.0))
                                    .border_t_1()
                                    .border_color(theme.border)
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.0))
                                    // Sans, not mono. These are the entries a
                                    // CV is made of — a job title, a degree —
                                    // and mono is for data (L-05). One of them
                                    // being a stray paragraph the parser turned
                                    // into a job is exactly what the user is
                                    // here to notice.
                                    .text_style(TextStyle::body())
                                    .text_color(theme.text_muted)
                                    .children(
                                        item.preview
                                            .iter()
                                            .take(PREVIEW_LINES)
                                            .map(|l| div().child(shorten(l))),
                                    )
                                    .when(item.preview.len() > PREVIEW_LINES, |el| {
                                        el.child(div().text_color(theme.text_subtle).child(
                                            format!("+{} more", item.preview.len() - PREVIEW_LINES),
                                        ))
                                    }),
                            )
                        })
                }))
                .children(render_unplaced(cx, imported)),
        )
        // Bottom Action Bar. Both decisions, side by side: this is the last
        // screen before a document exists, and the two things a person can do
        // here are keep it or throw it away.
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .px(px(22.0))
                .py(px(14.0))
                .border_t_1()
                .border_color(theme.border)
                .child(
                    Button::new("undo-import-btn")
                        .quiet()
                        .icon(lucide("undo"))
                        .label("Undo import")
                        .tooltip("Throw this away and pick another file")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            on_undo(this, cx);
                        })),
                )
                .child(
                    // Named for what it does. "Looks good — continue" is a
                    // claim about the import that the screen has just spent
                    // three amber lines contradicting; pressing it creates the
                    // document, and it can say so whether or not anything is
                    // flagged.
                    Button::new("continue-to-editor")
                        .action_primary()
                        .label("Create the CV")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            on_continue(this, cx);
                        })),
                ),
        )
}

/// The screen a failed import lands on.
///
/// Everything here points one way. The remedies are worth trying and are listed
/// in the order worth trying them, but the primary action is **Start blank** —
/// because writing the CV by hand is the one route that cannot fail, and a
/// person who has just been told their file will not open should not have to
/// work out that it is still available.
pub fn render_could_not_read<V: 'static>(
    cx: &mut Context<V>,
    filename: &str,
    error: &crate::import::ImportError,
    on_retry: impl Fn(&mut V, &mut Context<V>) + 'static + Copy,
    on_start_blank: impl Fn(&mut V, &mut Context<V>) + 'static + Copy,
) -> impl IntoElement {
    let theme = *cx.theme();

    div()
        .w(px(560.0))
        .rounded(theme.radius_lg())
        .border_1()
        .border_color(theme.border)
        .bg(theme.elevated)
        .shadow_lg()
        .flex()
        .flex_col()
        .p(px(36.0))
        .gap(px(6.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(9.0))
                .mb(px(6.0))
                .child(
                    Icon::new(IconName::TriangleAlert)
                        .with_size(theme.icon_md())
                        .text_color(theme.warning),
                )
                .child(
                    div()
                        .text_style(TextStyle::eyebrow())
                        .text_color(theme.warning)
                        .child(TextStyle::eyebrow().apply_case("Could not read")),
                ),
        )
        .child(
            div()
                .text_style(TextStyle::title())
                .text_color(theme.text)
                .child(error.headline.clone()),
        )
        .child(
            div()
                .mt(px(2.0))
                .text_style(TextStyle::meta())
                .text_color(theme.text_subtle)
                .child(filename.to_string()),
        )
        .children((!error.detail.is_empty()).then(|| {
            div()
                .mt(px(14.0))
                .text_style(TextStyle::prose())
                .text_color(theme.text_muted)
                .child(error.detail.clone())
        }))
        .children((!error.remedies.is_empty()).then(|| {
            div()
                .mt(px(20.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_style(TextStyle::eyebrow())
                        .text_color(theme.text_subtle)
                        .mb(px(2.0))
                        .child(TextStyle::eyebrow().apply_case("What to try")),
                )
                .children(error.remedies.iter().map(|remedy| {
                    div()
                        .flex()
                        .items_start()
                        .gap(px(9.0))
                        .child(
                            div()
                                .flex_none()
                                .mt(px(6.0))
                                .w(px(4.0))
                                .h(px(4.0))
                                .rounded_full()
                                .bg(theme.text_subtle),
                        )
                        .child(
                            div()
                                .text_style(TextStyle::body())
                                .text_color(theme.text_muted)
                                .child(remedy.clone()),
                        )
                }))
        }))
        .child(
            div()
                .mt(px(28.0))
                .pt(px(20.0))
                .border_t_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    div()
                        .text_style(TextStyle::body())
                        .text_color(theme.text_muted)
                        .child(
                            "You do not have to import anything. Start a blank CV and write it \
                             here — you can always bring a file in later.",
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            Button::new("could-not-read-blank")
                                .action_primary()
                                .label("Start a blank CV")
                                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                    on_start_blank(this, cx);
                                })),
                        )
                        .child(
                            Button::new("could-not-read-retry")
                                .action_secondary()
                                .label("Try another file")
                                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                    on_retry(this, cx);
                                })),
                        ),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::SectionReviewItem;
    use crate::import::model::ImportedDoc;
    use crate::resume::model::{Education, Resume, ResumeDoc, SkillGroup};

    fn imported_with(education: usize, skill_groups: usize) -> ImportedDoc {
        let resume = Resume {
            education: (0..education).map(|_| Education::default()).collect(),
            skills: (0..skill_groups).map(|_| SkillGroup::default()).collect(),
            ..Default::default()
        };
        ImportedDoc::new("PDF", ResumeDoc::from_resume(resume, "Base"))
    }

    /// `Certificates · 0 entries · Looks good` was on screen: a row that tells
    /// the reader nothing and pushes the ones that matter down.
    #[test]
    fn an_empty_section_with_nothing_to_report_is_not_listed() {
        let imported = imported_with(0, 0);
        let names: Vec<&str> = SectionReviewItem::from_imported(&imported)
            .iter()
            .map(|i| i.name.clone())
            .collect::<Vec<_>>()
            .leak()
            .iter()
            .map(|s| s.as_str())
            .collect();
        assert!(!names.contains(&"Certificates"), "{names:?}");
        assert!(!names.contains(&"Education"), "{names:?}");
        // Profile is never a count, so it always has something to say.
        assert!(names.contains(&"Profile"), "{names:?}");
    }

    /// …but an empty section the *source* said was there keeps its place,
    /// because then the emptiness is the finding.
    #[test]
    fn an_empty_section_the_parser_flagged_keeps_its_place() {
        use crate::import::notes::{Note, Part};

        let mut imported = imported_with(0, 0);
        imported.note(Part::Certificates, Note::Empty);

        let items = SectionReviewItem::from_imported(&imported);
        let certificates = items
            .iter()
            .find(|i| i.name == "Certificates")
            .expect("an empty section with a note is still listed");
        assert!(certificates.needs_review);
        assert_eq!(certificates.detail, "0 entries");
    }

    /// A preview line is a job title. When the parser has gone wrong it is
    /// sometimes a paragraph it mistook for one — worth seeing, not worth six
    /// lines of the card.
    #[test]
    fn a_paragraph_mistaken_for_an_entry_is_cut_between_words() {
        let long = "Responsibilities: overseeing daily operations, managing staff, ensuring \
                    compliance with banking regulations, and providing exceptional service";
        let short = super::shorten(long);

        assert!(short.chars().count() <= super::PREVIEW_CHARS + 1, "{short}");
        assert!(short.ends_with('…'), "{short}");
        assert!(
            !short.contains("  ") && !short.trim_end_matches('…').ends_with(' '),
            "the cut lands between words: {short}"
        );
        assert_eq!(
            super::shorten("Assistant Manager — Woodgrove Bank"),
            "Assistant Manager — Woodgrove Bank"
        );
    }

    /// The regression this guards: review flags used to be inferred from list
    /// lengths — more than one education entry became "Dates look reversed",
    /// one skill group became "Couldn't tell categories apart". Both shapes are
    /// perfectly ordinary, so the screen was telling users their data was
    /// suspect on no evidence. A section nothing was noticed about is not
    /// flagged.
    #[test]
    fn a_section_nothing_was_noticed_about_is_not_flagged() {
        let imported = imported_with(3, 1);
        let items = SectionReviewItem::from_imported(&imported);

        for item in &items {
            // Education and Skills carry entries and nothing odd about them.
            if item.name == "Education" || item.name == "Skills" {
                assert!(
                    !item.needs_review,
                    "{} was flagged: {:?}",
                    item.name, item.notes
                );
                assert!(item.notes.is_empty());
            }
        }
    }

    /// …and every note the parser raised reaches the list, in full. More than
    /// one per section, because a section can be wrong in more than one way.
    #[test]
    fn every_note_reaches_the_review_list() {
        use crate::import::notes::{Note, Part};

        let mut imported = imported_with(1, 4);
        imported.note(Part::Work, Note::Empty);
        imported.note(
            Part::Education,
            Note::MissingDates {
                found: 1,
                without: 1,
            },
        );
        imported.note(
            Part::Education,
            Note::MissingOrg {
                found: 1,
                without: 1,
            },
        );

        let items = SectionReviewItem::from_imported(&imported);
        let find = |name: &str| items.iter().find(|i| i.name == name).expect(name);

        let work = find("Work Experience");
        assert!(work.needs_review);
        assert_eq!(work.notes.len(), 1);
        assert!(
            work.notes[0].contains("nothing came out of it"),
            "{:?}",
            work.notes
        );

        // Two notes on one section, both shown: saying only the first would
        // hide half the work the user has to do.
        let education = find("Education");
        assert!(education.needs_review);
        assert_eq!(education.notes.len(), 2, "{:?}", education.notes);
        assert!(education.notes.iter().any(|n| n.contains("without dates")));
        assert!(education
            .notes
            .iter()
            .any(|n| n.contains("without an institution")));
    }
}

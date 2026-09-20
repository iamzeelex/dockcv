//! The Import screen's chrome: the heading, the two columns, and the rail that
//! says where you are.
//!
//! What goes *in* the left panel is `import_flow.rs` — the drop zone, the
//! parsing state, the review list, the failure. This file is the frame around
//! whichever of those is showing, and it exists because the frame is the part
//! that was wrong.
//!
//! It used to be a 560×600 card floating in the middle of the pane. Three
//! things followed from that and all three were costs. The review list — the
//! densest thing in the flow, a row per section with a verdict and an action —
//! was squeezed into 560px on a window with room to spare. The card carried its
//! own `DockCV` wordmark two hundred pixels from the rail's. And a flow of
//! three steps showed no sign of which one you were in or what came next.
//!
//! The rail on the right answers the last one continuously, and its copy is the
//! point rather than decoration: `Nothing is written yet` under step one is the
//! promise that makes dropping a stranger's file on this screen a safe thing to
//! try. The promise is true — nothing reaches the vault until `Create the CV` —
//! and until now nothing said so.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, Div, FontWeight, IntoElement};

use dockcv_ui_components::{Button, ButtonExt, ScrollableElement, MONO, SANS};

use super::import_flow::ImportStep;
use super::shell::Shell;
use crate::theme::{ActiveTheme, StyledText, TextStyle};

/// Where the rail's mark sits for a given step.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Done,
    Here,
    Ahead,
}

/// The three steps, and what each one promises.
///
/// There is no success step in the app — `Create the CV` writes the document
/// and opens the editor — so the third is never `Here`. That is not a gap: it
/// describes what is going to happen, which is what a person reading step one
/// wants to know.
///
/// The words are the person's, not ours. "Three calm steps" was a claim about
/// our design rather than information, and an odd one to make over a flow that
/// can fail. "Facts stay visible beside warnings" was the same idea in jargon;
/// what it means is that you see what came out before anything is saved.
const STEPS: [(&str, &str); 3] = [
    ("Pick the file", "Nothing is saved yet."),
    ("See what came out", "Section by section, before it becomes a CV."),
    ("Keep it", "Only then is anything written to your vault."),
];

/// The questions people have with a file in their hand and a drop zone in
/// front of them.
///
/// Four, and each answers something that decides what the person does next —
/// not a help page. The one this replaces was a single paragraph about scans
/// doing the work of all four, which meant the other three went unanswered:
/// whether their file is at risk, what happens when the parser is wrong, and
/// which of the files on their disk to reach for.
const FAQ: [(&str, &str); 4] = [
    (
        "Does this change my file?",
        "No. DockCV reads it and writes a new document into your vault. The file you picked is          left exactly as it was, wherever it was.",
    ),
    (
        "Which file works best?",
        "The one the app you wrote it in exports: a Word .docx, or a PDF straight out of that          app. A PDF that has been printed and scanned is the hardest thing to read.",
    ),
    (
        "What if my PDF is a picture?",
        "Scans and flattened exports have no text in them, so there is nothing to read. DockCV          says so rather than making an empty CV, and offers what to try instead.",
    ),
    (
        "What if it gets something wrong?",
        "You see every section before anything is saved, with the parser's own doubts marked.          One click throws the whole import away.",
    ),
];

impl Shell {
    /// Leave the import flow without importing anything.
    pub(super) fn close_import(&mut self, cx: &mut Context<Self>) {
        self.gallery_creating = false;
        self.import_step = ImportStep::Step1Drop;
        cx.notify();
    }

    /// The whole screen: its own heading, not the CV list's.
    ///
    /// The gallery's header — the person's name, the search box, the sort, `Add
    /// a CV` — stayed on screen above the import flow, offering to search a
    /// list that was not showing and to add a CV you were in the middle of
    /// adding. A screen that is doing one thing says which.
    pub(super) fn render_import_screen(&self, cx: &mut Context<Self>) -> Div {
        let theme = *cx.theme();

        let heading = div()
            .flex()
            .flex_wrap()
            .items_start()
            .justify_between()
            .gap_4()
            .px(px(34.0))
            .pt(px(30.0))
            .pb(px(22.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(280.0))
                    .flex()
                    .flex_col()
                    .gap(px(9.0))
                    .child(
                        div()
                            .text_style(TextStyle::title())
                            .text_color(theme.text)
                            .child("Bring in an existing CV"),
                    )
                    .child(
                        div()
                            .max_w(px(560.0))
                            .text_style(TextStyle::body())
                            .text_color(theme.text_muted)
                            // The promise, where the promise belongs. The old
                            // subtitle — "we'll split it into sections and
                            // blocks you can edit right away" — described our
                            // work. This describes the thing the person is
                            // actually weighing up before they hand over a file.
                            .child(
                                "Pick a file and DockCV shows you what it found, section by \
                                 section, before anything is saved. Your file is read, never \
                                 changed, and never leaves this machine.",
                            ),
                    ),
            )
            .child(
                Button::new("import-cancel")
                    .toolbar()
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.close_import(cx);
                    }))
                    .child("‹ Back to CVs"),
            );

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(heading)
            .child(
                div()
                    .id("import-scroll")
                    .flex_1()
                    .min_h_0()
                    // The review list keeps its own scroll so its footer stays
                    // reachable; this one is for the short window where even
                    // the drop zone and the rail together do not fit.
                    .overflow_y_scrollbar()
                    .px(px(34.0))
                    .pb(px(30.0))
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(20.0))
                    // The panel takes the room; the rail is a fixed 290 and
                    // drops below it when there is not enough for both. GPUI
                    // has no media queries, so this is flex doing what flex
                    // does — see the note on the CV list's own header.
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(380.0))
                            .rounded(theme.radius_md())
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.surface)
                            .p(px(22.0))
                            .child(self.render_import_panel(cx)),
                    )
                    .child(self.render_import_rail(cx)),
            )
    }

    /// Whichever step is showing, without chrome of its own.
    fn render_import_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        use super::import_flow;
        match &self.import_step {
            ImportStep::Step1Drop => import_flow::render_drop_panel(
                cx,
                |this, cx| this.import_existing_resume(cx),
                |this, cx| this.start_blank_cv(cx),
            )
            .into_any_element(),
            ImportStep::Parsing { filename } => {
                import_flow::render_parsing_step(cx, filename).into_any_element()
            }
            ImportStep::CouldNotRead {
                filename,
                path,
                error,
            } => div()
                .flex()
                .flex_col()
                .child(import_flow::render_could_not_read(
                    cx,
                    filename,
                    error,
                    |this, cx| {
                        this.import_step = ImportStep::Step1Drop;
                        cx.notify();
                    },
                    |this, cx| this.start_blank_cv(cx),
                ))
                // Only under a file that is a picture. Offered under "this is
                // not readable JSON" it would be advice to ask a model about a
                // download that got cut off.
                .children(
                    error
                        .is_unreadable_image()
                        .then(|| self.render_assistant_handoff(cx, path.as_deref())),
                )
                .into_any_element(),
            ImportStep::Step2Review { imported } => import_flow::render_step2_review_split(
                cx,
                imported,
                self.import_section_name.as_ref(),
                |this, cx| {
                    this.import_step = ImportStep::Step1Drop;
                    cx.notify();
                },
                |this, cx| {
                    if let ImportStep::Step2Review { imported } = &this.import_step.clone() {
                        let doc = imported.doc.clone();
                        this.create_doc(doc, "imported", cx);
                        this.gallery_creating = false;
                        this.import_step = ImportStep::Step1Drop;
                    }
                },
                // Adopting mutates the *pending* import, not a document on
                // disk: nothing has been created yet, and Undo import still
                // throws the whole thing away. The section is simply there
                // when Continue writes the file.
                |this, heading, items, cx| {
                    if let ImportStep::Step2Review { imported } = &mut this.import_step {
                        let created = crate::import::model::adopt_as_section(
                            &mut imported.doc,
                            &heading,
                            &items,
                        );
                        if created > 0 {
                            imported.unplaced.retain(|left| !items.contains(left));
                        }
                        cx.notify();
                    }
                },
            )
            .into_any_element(),
        }
    }

    /// Where you are, what is next, and the one thing this flow cannot do.
    fn render_import_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let reached = match &self.import_step {
            // A file that would not come in leaves you back at choosing one,
            // so the rail says so rather than claiming progress.
            ImportStep::Step1Drop | ImportStep::CouldNotRead { .. } => 0,
            ImportStep::Parsing { .. } | ImportStep::Step2Review { .. } => 1,
        };

        div()
            .flex_none()
            .w(px(290.0))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .p(px(18.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .text_style(TextStyle::eyebrow())
                    .text_color(theme.text_subtle)
                    .mb(px(14.0))
                    .child(TextStyle::eyebrow().apply_case("What happens")),
            )
            .children(STEPS.iter().enumerate().map(|(index, (title, promise))| {
                let mark = match index.cmp(&reached) {
                    std::cmp::Ordering::Less => Mark::Done,
                    std::cmp::Ordering::Equal => Mark::Here,
                    std::cmp::Ordering::Greater => Mark::Ahead,
                };
                div()
                    .flex()
                    .items_start()
                    .gap(px(11.0))
                    .when(index > 0, |step| step.mt(px(16.0)))
                    .child(
                        div()
                            .flex_none()
                            .w(px(21.0))
                            .h(px(21.0))
                            .rounded_full()
                            .border_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .font_family(MONO)
                            .text_size(px(10.0))
                            .map(|circle| match mark {
                                Mark::Done => circle
                                    .border_color(theme.success)
                                    .text_color(theme.success),
                                Mark::Here => circle
                                    .border_color(theme.accent)
                                    .bg(theme.accent.opacity(0.16))
                                    .text_color(theme.accent),
                                Mark::Ahead => circle
                                    .border_color(theme.border_strong)
                                    .text_color(theme.text_subtle),
                            })
                            .child(if mark == Mark::Done {
                                "✓".to_string()
                            } else {
                                (index + 1).to_string()
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .font_family(SANS)
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if mark == Mark::Ahead {
                                        theme.text_muted
                                    } else {
                                        theme.text
                                    })
                                    .child(*title),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_subtle)
                                    .child(*promise),
                            ),
                    )
            }))
            .child(
                // Answered here, before the attempt, rather than only in the
                // failure state a person reaches already annoyed.
                div()
                    .mt(px(22.0))
                    .pt(px(16.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap(px(13.0))
                    .children(FAQ.iter().map(|(question, answer)| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .font_family(SANS)
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child(*question),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.text_muted)
                                    .child(*answer),
                            )
                    })),
            )
    }
}

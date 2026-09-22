//! `Tailor for a job`: the front door's primary action.
//!
//! Huntr's 2025 numbers are the whole argument for this screen. Of the résumés
//! people *made* in their builder, 64.5% were job-tailored; of the applications
//! they *sent*, about 6% were. Nobody stops tailoring because they changed
//! their mind about whether it works — they stop because the second tailored CV
//! costs almost as much as the first. So the path from "there is a job" to "a
//! reading meant for it" has to be one gesture, not ten steps across three
//! screens.
//!
//! What it does, in order: makes the application card, makes a preset in the
//! chosen document copied from the reading that has been working, pins the card
//! to it, and opens the matrix there.
//!
//! ## Where this departs from the design doc
//!
//! §7 had the card arrive at the *end* — you tailor, you export, and the export
//! offers to file a card. Building it the other way round turned out better and
//! cheaper. Better, because the Wishlist column is exactly "a job I mean to
//! apply to", so the card belongs there from the moment you say the company's
//! name, and the JD link and notes have somewhere to live while you work rather
//! than after. Cheaper, because moving the card to Applied already captures the
//! PDF snapshot (D4a) — inventing a second route from the export sheet would
//! have been a parallel mechanism for something that works.

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, FontWeight, SharedString, Window};

use dockcv_ui_components::{
    Button, ButtonExt, Disableable, SelectableRow, TextField, TextFieldState, SANS,
};

use crate::theme::{ActiveTheme, StyledText, TextStyle};

use crate::resume::posting::{coverage, terms, unused, Coverage, Term, Unused, UnusedSource};
use crate::resume::model::SectionKind;

use super::front_door::{readings, Reading};
use super::shell::Shell;

/// The sheet's live state. Takes over the front door's body the way the
/// template chooser does, rather than being a modal over it — one less
/// mechanism, and the same shape a person has already met when making a CV.
pub(super) struct TailorSheet {
    pub company: Entity<TextFieldState>,
    pub role: Entity<TextFieldState>,
    /// The posting itself. Optional: naming the company is enough to start,
    /// and the read is what the text buys you.
    pub posting: Entity<TextFieldState>,
    /// Which reading to start from: its document and its preset index.
    /// `None` when the vault holds nothing to start from.
    pub base: Option<(std::path::PathBuf, usize)>,
    /// The last read of the posting, or `None` before there has been one.
    ///
    /// Computed on demand rather than per keystroke: it composes and renders
    /// every candidate reading to plain text, which is cheap enough to ask for
    /// and wasteful to repeat on every character.
    pub read: Option<PostingRead>,
}

/// What the posting says, measured against what the vault holds.
pub(super) struct PostingRead {
    pub terms: Vec<Term>,
    /// Per reading, how much of the posting it already answers.
    pub coverage: Vec<((std::path::PathBuf, usize), Coverage)>,
    /// What the vault has for the gaps in the *selected* reading.
    pub unused: Vec<Unused>,
}

impl Shell {
    /// Open the sheet, defaulting the base to the reading that has been sent
    /// most — the one that has been working is the one worth starting from.
    pub(super) fn open_tailor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let company = cx.new(|cx| TextFieldState::single_line(window, cx));
        let role = cx.new(|cx| TextFieldState::single_line(window, cx));
        // `auto_grow` rather than a fixed box: a posting is two paragraphs or
        // two pages, and a tall empty well above an empty card was most of what
        // this screen looked like.
        let posting = cx.new(|cx| TextFieldState::auto_grow(4, 14, window, cx));
        let base = self.busiest_reading();

        let handle = company.read(cx).focus_handle(cx);
        self.tailoring = Some(TailorSheet {
            company,
            role,
            posting,
            base,
            read: None,
        });
        handle.focus(window, cx);
        cx.notify();
    }

    pub(super) fn cancel_tailor(&mut self, cx: &mut Context<Self>) {
        self.tailoring = None;
        cx.notify();
    }

    /// The reading the most applications went out under, or the first one.
    fn busiest_reading(&self) -> Option<(std::path::PathBuf, usize)> {
        let rows = readings(self.cache.metadata());
        let applications = self.cache.applications();
        rows.iter()
            .filter_map(|row| Some((row, row.preset.as_ref()?.0)))
            .max_by_key(|(row, _)| {
                let (stem, preset) = row.sent_as();
                applications.record_for(stem, preset).sent
            })
            .map(|(row, index)| (row.path.clone(), index))
    }

    /// Read the posting against the vault.
    ///
    /// Everything here is arithmetic over text the app already produces — the
    /// terms the posting repeats, and which of them each reading's own plain
    /// text already contains. **No model is involved and nothing is written.**
    /// The output is a set of pointers at things the user wrote.
    ///
    /// Done on demand rather than per keystroke: it composes and renders every
    /// candidate reading, which is cheap to ask for once and wasteful to repeat
    /// on every character.
    pub(super) fn read_posting(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.tailoring.as_ref() else {
            return;
        };
        let text = sheet.posting.read(cx).value(cx).to_string();
        let base = sheet.base.clone();
        let terms = terms(&text);
        if terms.is_empty() {
            if let Some(sheet) = self.tailoring.as_mut() {
                sheet.read = None;
            }
            cx.notify();
            return;
        }

        // One plain-text rendering per reading, from the cache rather than the
        // disk: the documents are already parsed, and a preset is applied to a
        // clone so nothing here can touch the working copy.
        let mut readings_text: Vec<((std::path::PathBuf, usize), String)> = Vec::new();
        for (meta, doc) in self.cache.readable_documents() {
            for index in 0..doc.presets.len() {
                let mut copy = doc.clone();
                copy.apply_preset(index);
                readings_text.push((
                    (meta.path.clone(), index),
                    crate::resume::export_plain_text(&copy.compose()),
                ));
            }
        }

        let coverage_rows: Vec<_> = readings_text
            .iter()
            .map(|(key, body)| (key.clone(), coverage(&terms, body)))
            .collect();

        // The gaps are the selected reading's gaps — "what else do I have" is a
        // question about the version you are actually about to send.
        let chosen = base
            .as_ref()
            .and_then(|key| readings_text.iter().find(|(k, _)| k == key))
            .map(|(_, body)| body.clone())
            .unwrap_or_default();
        let missing = coverage_rows
            .iter()
            .find(|(k, _)| Some(k) == base.as_ref())
            .map(|(_, c)| c.missing.clone())
            .unwrap_or_default();
        let found = unused(
            &missing,
            &self.cache.diary().entries,
            self.cache.library(),
            &chosen,
        );

        if let Some(sheet) = self.tailoring.as_mut() {
            sheet.read = Some(PostingRead {
                terms,
                coverage: coverage_rows,
                unused: found,
            });
        }
        cx.notify();
    }

    /// Hand the named job to the constructor.
    ///
    /// This used to make the preset, make the application card, pin them to
    /// each other and open the matrix — all on the strength of a company name
    /// typed into a box. Three things were written before the person had seen
    /// anything, and the first change of mind left a card pinned to a preset
    /// that had been deleted.
    ///
    /// Now the sheet does what a sheet should: it collects the two facts the
    /// screen after it needs. `front_door/draft.rs` writes, once, on `Save
    /// version`.
    pub(super) fn start_tailoring(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.tailoring.as_ref() else {
            return;
        };
        let company = sheet.company.read(cx).value(cx).trim().to_string();
        let role = sheet.role.read(cx).value(cx).trim().to_string();
        let posting = sheet.posting.read(cx).value(cx).trim().to_string();
        let Some((path, base)) = sheet.base.clone() else {
            return;
        };
        if company.is_empty() {
            return;
        }

        self.tailoring = None;
        self.open_version_draft(path, base, company, role, posting, cx);
        let _ = window;
    }

    /// The sheet.
    ///
    /// One surface with three parts in the order the work happens: the job,
    /// what to start from, and what the vault has that the starting point does
    /// not. Before this it was two text boxes and a centred list of preset
    /// names floating on the background — no container, no rows, and nothing on
    /// any row that helped choose between them.
    pub(super) fn render_tailor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let Some(sheet) = self.tailoring.as_ref() else {
            return div();
        };
        let rows = readings(self.cache.metadata());
        let can_start = sheet.base.is_some();
        let read = sheet.read.as_ref();

        let mut choices = div().flex().flex_col().gap(px(4.0));
        for row in &rows {
            let Some((index, _)) = row.preset.clone() else {
                continue;
            };
            let key = (row.path.clone(), index);
            let chosen = sheet.base.as_ref() == Some(&key);
            let cov = read.and_then(|r| {
                r.coverage
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, c)| c)
            });
            let path = row.path.clone();
            choices = choices.child(
                SelectableRow::new(SharedString::from(format!(
                    "tailor-base-{}-{}",
                    row.stem,
                    row.label()
                )))
                .selected(chosen)
                .aria_label(format!("{} · {}", row.stem, row.label()))
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(1.0))
                        .child(
                            div()
                                .truncate()
                                .text_color(theme.text)
                                .child(format!("{} · {}", row.stem, row.label())),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_style(TextStyle::body())
                                .text_color(theme.text_subtle)
                                .child(self.base_choice_label(row)),
                        ),
                )
                .when_some(cov, |el, cov| {
                    // The number is the reason this row is above or below the
                    // others, so it sits where the eye compares them.
                    el.trailing(
                        div()
                            .flex_none()
                            .px(px(8.0))
                            .text_style(TextStyle::chip())
                            .text_color(if cov.matched.len() * 2 >= cov.total() {
                                theme.success
                            } else {
                                theme.text_subtle
                            })
                            .child(format!("{} / {}", cov.matched.len(), cov.total())),
                    )
                })
                .on_click(cx.listener(move |this, _, _window, cx| {
                    if let Some(sheet) = this.tailoring.as_mut() {
                        sheet.base = Some((path.clone(), index));
                    }
                    // "What else do I have" is a question about the version you
                    // are about to send, so changing it changes the answer.
                    if this.tailoring.as_ref().is_some_and(|s| s.read.is_some()) {
                        this.read_posting(cx);
                    }
                    cx.notify();
                })),
            );
        }

        div()
            .w(px(620.0))
            .flex()
            .flex_col()
            .gap(px(20.0))
            .p(px(26.0))
            .rounded(theme.radius_md())
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .text_style(TextStyle::title())
                                    .text_color(theme.text)
                                    .child("Tailor for a job"),
                            )
                            // Which of the two screens this is. The card used
                            // to end in a button that left for another one
                            // without ever saying there was another one.
                            .child(
                                div()
                                    .flex_none()
                                    .text_style(TextStyle::chip())
                                    .text_color(theme.text_subtle)
                                    .child("STEP 1 OF 2"),
                            ),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_muted)
                            .child(
                                "Copy a version you already have, change what does not fit, \
                                 and save it under this job's name. Nothing is written until \
                                 you save.",
                            ),
                    ),
            )
            .child({
                // Built first: `tailor_step` takes `cx` too, and the two
                // borrows cannot overlap inside one call.
                let fields = div()
                    .flex()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.tailor_field(cx, "Company", &sheet.company)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.tailor_field(cx, "Role", &sheet.role)),
                    )
                    .into_any_element();
                self.tailor_step(cx, 1, "The job", None, fields)
            })
            .child(self.tailor_posting(cx, sheet))
            .child(self.tailor_step(
                cx,
                3,
                "Start from",
                Some("A copy of this becomes the new version."),
                if rows.iter().any(|r| r.preset.is_some()) {
                    choices.into_any_element()
                } else {
                    div()
                        .text_style(TextStyle::body())
                        .text_color(theme.text_muted)
                        .child("No preset to start from yet — save one on a CV first.")
                        .into_any_element()
                },
            ))
            .children(self.tailor_unused(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(7.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                Button::new("tailor-start")
                                    .action_primary()
                                    .disabled(!can_start)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.start_tailoring(window, cx);
                                    }))
                                    .child("Start tailoring"),
                            )
                            .child(
                                Button::new("tailor-cancel")
                                    .quiet()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.cancel_tailor(cx);
                                    }))
                                    .child("Cancel"),
                            ),
                    )
                    // Where the button goes. It leaves this screen for another
                    // one, and saying so is cheaper than the surprise.
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_subtle)
                            .child(
                                "Next: choose which sections read differently, watch the \
                                 page change, and save.",
                            ),
                    ),
            )
    }

    /// The posting box, and what the read of it found.
    fn tailor_posting(&self, cx: &mut Context<Self>, sheet: &TailorSheet) -> impl IntoElement {
        let theme = *cx.theme();
        let read = sheet.read.as_ref();
        let has_text = !sheet.posting.read(cx).value(cx).trim().is_empty();

        let body = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(div().flex_1().min_w_0().child(
                        TextField::new(&sheet.posting).placeholder(
                            "Paste the job description here",
                        ),
                    ))
                    .child(
                        Button::new("tailor-read")
                            .action_secondary()
                            .flex_none()
                            .disabled(!has_text)
                            .tooltip("Match it against your versions and your vault")
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.read_posting(cx);
                            }))
                            .child(if read.is_some() { "Read again" } else { "Read it" }),
                    ),
            )
            .children(read.map(|r| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_subtle)
                            .child(format!(
                                "It leans on {} term{}. The count beside each version below \
                                 is how many of them that version already says.",
                                r.terms.len(),
                                if r.terms.len() == 1 { "" } else { "s" }
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(5.0))
                            .children(r.terms.iter().map(|term| {
                                div()
                                    .px(px(7.0))
                                    .py(px(2.0))
                                    .rounded(theme.radius_sm())
                                    .bg(theme.elevated)
                                    .text_style(TextStyle::chip())
                                    .text_color(theme.text_muted)
                                    .child(if term.count > 1 {
                                        format!("{} ×{}", term.word, term.count)
                                    } else {
                                        term.word.clone()
                                    })
                            })),
                    )
            }))
            .into_any_element();

        self.tailor_step(
            cx,
            2,
            "The posting",
            Some("Optional — it is what the two lists below are measured against."),
            body,
        )
    }

    /// What the vault holds for the gaps in the chosen reading.
    ///
    /// This is the part that is not keyword matching. Nothing is proposed for
    /// the CV and nothing is written: these are entries the user made, pointed
    /// at because the posting asks about them and the version they are about to
    /// send does not mention them.
    fn tailor_unused(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let theme = *cx.theme();
        let read = self.tailoring.as_ref()?.read.as_ref()?;
        if read.unused.is_empty() {
            return None;
        }
        let rows = read.unused.iter().take(5).map(|item| {
            let (where_from, body) = match &item.source {
                UnusedSource::Diary { date, confidential } => (
                    format!("Diary · {date}"),
                    match (&item.text, confidential) {
                        // US-36: a confidential entry is never quoted outward.
                        // It is still worth naming, because the fact of it is
                        // the useful part.
                        (_, true) => "Marked confidential — open the Diary to read it.".to_string(),
                        (Some(text), _) => text.clone(),
                        (None, _) => String::new(),
                    },
                ),
                UnusedSource::Library { section } => (
                    format!("Library · {}", section_word(*section)),
                    item.text.clone().unwrap_or_default(),
                ),
            };
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .py(px(5.0))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_none()
                                .text_style(TextStyle::meta())
                                .text_color(theme.text_subtle)
                                .child(where_from),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_style(TextStyle::chip())
                                .text_color(theme.accent)
                                .child(item.terms.join(" · ")),
                        ),
                )
                .child(
                    div()
                        .text_style(TextStyle::body())
                        .text_color(theme.text_muted)
                        .child(body),
                )
        });
        Some(self.tailor_step(
            cx,
            4,
            "You have written this down",
            Some("In your vault, and not on the version above."),
            div()
                .flex()
                .flex_col()
                .children(rows)
                .into_any_element(),
        ))
    }

    /// What this version has done, in a sentence. **Not its name** — the row
    /// prints that above, and printing it twice was this screen's own bug.
    fn base_choice_label(&self, row: &Reading) -> String {
        let (stem, preset) = row.sent_as();
        let record = self.cache.applications().record_for(stem, preset);
        let record = match (record.sent, record.interviewed) {
            (0, _) => "never sent".to_string(),
            (1, 0) => "sent once, nothing back yet".to_string(),
            (sent, 0) => format!("sent {sent} times, nothing back yet"),
            (1, _) => "sent once, and it got an interview".to_string(),
            (sent, 1) => format!("sent {sent} times, one interview"),
            (sent, got) => format!("sent {sent} times, {got} interviews"),
        };
        match row.subtitle() {
            Some(what) => format!("{what} · {record}"),
            None => record,
        }
    }

    /// One numbered part of the card.
    ///
    /// Numbers rather than three eyebrows, because the complaint the old screen
    /// earned was not "this is ugly", it was "I cannot tell what I am doing" —
    /// and a form with a count reads as a sequence where the same form without
    /// one reads as a pile of boxes.
    fn tailor_step(
        &self,
        cx: &mut Context<Self>,
        number: u8,
        title: &'static str,
        note: Option<&'static str>,
        body: gpui::AnyElement,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        div()
            .flex()
            .gap(px(12.0))
            .child(
                div()
                    .flex_none()
                    .size(px(22.0))
                    .mt(px(1.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(theme.elevated)
                    .text_style(TextStyle::chip())
                    .text_color(theme.text_muted)
                    .child(number.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_style(TextStyle::control())
                                    .text_color(theme.text)
                                    .child(title),
                            )
                            .children(note.map(|note| {
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_style(TextStyle::body())
                                    .text_color(theme.text_subtle)
                                    .child(note)
                            })),
                    )
                    .child(body),
            )
    }

    fn tailor_field(
        &self,
        cx: &mut Context<Self>,
        label: &'static str,
        state: &Entity<TextFieldState>,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_subtle)
                    .child(label),
            )
            .child(TextField::new(state))
    }
}

/// `Northwind`, or `Northwind 2` when that name is taken.
///
/// A preset's name is how the board, the export filename and the funnel all
/// refer to it, so two readings of one document sharing a name would make three
/// surfaces ambiguous at once.
pub(super) fn unique_preset_name(doc: &crate::resume::model::ResumeDoc, company: &str) -> String {
    let taken = |name: &str| doc.presets.iter().any(|p| p.name == name);
    if !taken(company) {
        return company.to_string();
    }
    (2..)
        .map(|n| format!("{company} {n}"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| company.to_string())
}

/// A section's name in a sentence — `Library · Skills`.
fn section_word(section: SectionKind) -> &'static str {
    match section {
        SectionKind::Work => "Work",
        SectionKind::Education => "Education",
        SectionKind::Skills => "Skills",
        SectionKind::Certificates => "Certifications",
        SectionKind::Organizations => "Organizations",
        SectionKind::Profile => "Profile",
        SectionKind::Custom(_) => "Section",
    }
}

#[cfg(test)]
mod tests {
    use super::unique_preset_name;
    use crate::resume::model::{Resume, ResumeDoc};

    #[test]
    fn a_second_reading_for_one_company_does_not_share_its_name() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        assert_eq!(unique_preset_name(&doc, "Northwind"), "Northwind");

        doc.add_preset("Northwind");
        assert_eq!(unique_preset_name(&doc, "Northwind"), "Northwind 2");

        doc.add_preset("Northwind 2");
        assert_eq!(unique_preset_name(&doc, "Northwind"), "Northwind 3");
    }
}

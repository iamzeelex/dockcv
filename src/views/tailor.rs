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
use gpui::{div, px, ClickEvent, Context, Div, Entity, FontWeight, SharedString, Window};

use dockcv_ui_components::{
    Button, ButtonExt, Disableable, ScrollableElement, SelectableRow, TextField, TextFieldState,
    SANS,
};

use crate::theme::{ActiveTheme, StyledText, TextStyle};

use crate::resume::model::SectionKind;
use crate::resume::posting::{
    coverage, deciding_terms, gaps, unused, Coverage, Term, Unused, UnusedSource,
};

use super::front_door::{groups, readings, Reading};
use super::shell::Shell;

/// Where in the flow the sheet is.
///
/// One question per screen, the same shape `import.rs` uses — and for the same
/// reason. The card that held all of this at once was 900px tall, fixed at
/// 620px wide whatever the window was, and answered three questions before the
/// person had answered one. A person who has just found a job has a posting in
/// their hand and one question in their head; the rest is bookkeeping and can
/// wait its turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TailorStep {
    Posting,
    Version,
    Name,
}

impl TailorStep {
    fn index(self) -> usize {
        match self {
            Self::Posting => 0,
            Self::Version => 1,
            Self::Name => 2,
        }
    }
}

/// The three steps, and what each one promises.
///
/// The words are the person's, not ours — the same rule `import.rs` sets.
const STEPS: [(&str, &str); 3] = [
    ("Paste the posting", "Optional. Compared on this device."),
    (
        "Choose a starting CV",
        "Saved version or current file; the source stays untouched.",
    ),
    (
        "Name the job",
        "A company name is required; nothing is saved yet.",
    ),
];

/// The sheet's live state. Takes over the front door's body the way the
/// template chooser does, rather than being a modal over it — one less
/// mechanism, and the same shape a person has already met when making a CV.
pub(super) struct TailorSheet {
    pub company: Entity<TextFieldState>,
    pub role: Entity<TextFieldState>,
    /// The posting itself. Optional: naming the company is enough to start,
    /// and the read is what the text buys you.
    pub posting: Entity<TextFieldState>,
    /// Which reading to start from: its document and optional preset index.
    /// `None` as the preset means the document as it is saved now.
    pub base: Option<(std::path::PathBuf, Option<usize>)>,
    pub step: TailorStep,
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
    pub coverage: Vec<((std::path::PathBuf, Option<usize>), Coverage)>,
    /// What the vault has for the gaps in the *selected* reading.
    pub unused: Vec<Unused>,
}

impl Shell {
    /// Open the sheet, defaulting the base to the reading that has been sent
    /// most — the one that has been working is the one worth starting from.
    pub(super) fn open_tailor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_tailor_with_base(None, window, cx);
    }

    /// Open tailoring from the version whose menu the person chose.
    pub(super) fn open_tailor_from(
        &mut self,
        path: std::path::PathBuf,
        preset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_tailor_with_base(Some((path, Some(preset))), window, cx);
    }

    fn open_tailor_with_base(
        &mut self,
        base: Option<(std::path::PathBuf, Option<usize>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let company = cx.new(|cx| TextFieldState::single_line(window, cx));
        let role = cx.new(|cx| TextFieldState::single_line(window, cx));
        // `auto_grow` rather than a fixed box: a posting is two paragraphs or
        // two pages, and a tall empty well above an empty card was most of what
        // this screen looked like.
        let posting = cx.new(|cx| TextFieldState::auto_grow(4, 14, window, cx));
        let base = base.or_else(|| self.busiest_reading());

        let handle = posting.read(cx).focus_handle(cx);
        self.tailoring = Some(TailorSheet {
            company,
            role,
            posting,
            base,
            step: TailorStep::Posting,
            read: None,
        });
        handle.focus(window, cx);
        cx.notify();
    }

    /// Move to a step, and read the posting on the way out of the first one.
    pub(super) fn tailor_go(
        &mut self,
        step: TailorStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let leaving_posting = self
            .tailoring
            .as_ref()
            .is_some_and(|s| s.step == TailorStep::Posting && step != TailorStep::Posting);
        if let Some(sheet) = self.tailoring.as_mut() {
            sheet.step = step;
        }
        if leaving_posting {
            self.read_posting(cx);
        }
        let focus = self.tailoring.as_ref().and_then(|sheet| match step {
            TailorStep::Posting => Some(sheet.posting.read(cx).focus_handle(cx)),
            TailorStep::Name => Some(sheet.company.read(cx).focus_handle(cx)),
            TailorStep::Version => None,
        });
        if let Some(handle) = focus {
            handle.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn cancel_tailor(&mut self, cx: &mut Context<Self>) {
        self.tailoring = None;
        cx.notify();
    }

    /// The reading the most applications went out under, or the first one.
    fn busiest_reading(&self) -> Option<(std::path::PathBuf, Option<usize>)> {
        let rows = readings(self.cache.metadata());
        let applications = self.cache.applications();
        rows.iter()
            .filter(|row| !row.unreadable)
            .filter_map(|row| Some((row, row.preset.as_ref()?.0)))
            .max_by_key(|(row, _)| {
                let (stem, preset) = row.sent_as();
                applications.record_for(stem, preset).sent
            })
            .map(|(row, index)| (row.path.clone(), Some(index)))
            .or_else(|| {
                rows.iter()
                    .find(|row| !row.unreadable)
                    .map(|row| (row.path.clone(), None))
            })
    }

    /// Every word the vault has: the readings, the library and the diary, run
    /// together.
    ///
    /// This is what grounds the read. A term the user has never written
    /// anywhere is not a gap DockCV can help with, and one it can is worth
    /// naming — the difference between the two is a substring search over this.
    fn vault_corpus(&self, readings: &[String]) -> String {
        let mut corpus = readings.join(" ");
        for entry in &self.cache.diary().entries {
            corpus.push(' ');
            corpus.push_str(&entry.text);
        }
        let library = self.cache.library();
        for block in &library.work {
            corpus.push_str(&format!(
                " {} {} {} {}",
                block.position,
                block.name,
                block.summary,
                block.highlights.join(" ")
            ));
        }
        for block in &library.skills {
            corpus.push_str(&format!(" {} {}", block.name, block.keywords.join(" ")));
        }
        for block in &library.education {
            corpus.push_str(&format!(
                " {} {} {}",
                block.institution,
                block.study_type,
                block.highlights.join(" ")
            ));
        }
        for block in &library.certificates {
            corpus.push_str(&format!(" {} {}", block.name, block.issuer));
        }
        for block in &library.volunteer {
            corpus.push_str(&format!(
                " {} {} {}",
                block.position,
                block.organization,
                block.highlights.join(" ")
            ));
        }
        corpus
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

        // One plain-text rendering per reading, from the cache rather than the
        // disk: the documents are already parsed, and a preset is applied to a
        // clone so nothing here can touch the working copy.
        let mut readings_text: Vec<((std::path::PathBuf, Option<usize>), String)> = Vec::new();
        for (meta, doc) in self.cache.readable_documents() {
            if doc.active_preset_index().is_none() {
                readings_text.push((
                    (meta.path.clone(), None),
                    crate::resume::export_plain_text(&doc.compose()),
                ));
            }
            for index in 0..doc.presets.len() {
                let mut copy = doc.clone();
                copy.apply_preset(index);
                readings_text.push((
                    (meta.path.clone(), Some(index)),
                    crate::resume::export_plain_text(&copy.compose()),
                ));
            }
        }

        // The score is over the words the reader's own versions disagree about.
        // Frequency in the posting was measuring the posting, and a posting is
        // mostly the sentence every posting is written in.
        let bodies: Vec<String> = readings_text.iter().map(|(_, b)| b.clone()).collect();
        let terms = deciding_terms(&text, &bodies);
        if terms.is_empty() && text.trim().is_empty() {
            if let Some(sheet) = self.tailoring.as_mut() {
                sheet.read = None;
            }
            cx.notify();
            return;
        }

        let coverage_rows: Vec<_> = readings_text
            .iter()
            .map(|(key, body)| (key.clone(), coverage(&terms, body)))
            .collect();

        // "What else do I have" is a different question from "which version",
        // and it is asked of the whole vault rather than of the versions.
        let chosen = base
            .as_ref()
            .and_then(|key| readings_text.iter().find(|(k, _)| k == key))
            .map(|(_, body)| body.clone())
            .unwrap_or_default();
        let vault = self.vault_corpus(&bodies);
        let missing = gaps(&text, &chosen, &vault);
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

    fn starting_point_available(&self, path: &std::path::PathBuf, preset: Option<usize>) -> bool {
        self.cache.readable_documents().any(|(meta, doc)| {
            meta.path == *path && preset.is_none_or(|index| doc.presets.get(index).is_some())
        })
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
        let base = sheet.base.clone();
        let Some((path, source)) = base else {
            return;
        };
        if company.is_empty() || !self.starting_point_available(&path, source) {
            if let Some(sheet) = self.tailoring.as_mut() {
                sheet.base = None;
                sheet.step = TailorStep::Version;
            }
            cx.notify();
            return;
        }

        if self.open_version_draft(path, source, company, role, posting, cx) {
            self.tailoring = None;
        }
        let _ = window;
    }

    /// The screen: a rail of steps, and one question beside it.
    ///
    /// Pane-wide and flexible rather than a fixed 620px card, so it uses the
    /// window it is given instead of leaving two thirds of it empty.
    pub(super) fn render_tailor(&self, cx: &mut Context<Self>) -> Div {
        let theme = *cx.theme();
        let Some(sheet) = self.tailoring.as_ref() else {
            return div();
        };

        let has_readable_source = self.cache.readable_documents().next().is_some();
        let (title, lede): (&str, String) = match sheet.step {
            TailorStep::Posting if has_readable_source => (
                "Tailor for a job",
                "Paste the posting to compare it with your CVs and the notes already in your \
                 vault, or skip it and choose a starting point by hand."
                    .to_string(),
            ),
            TailorStep::Posting => (
                "Tailor for a job",
                "Paste the posting now, then import the CV you want to use. Your job details \
                 will stay in this flow."
                    .to_string(),
            ),
            TailorStep::Version => (
                "Choose a CV to start from",
                "Pick a saved version, or use an imported CV as it is. DockCV works on a copy; \
                 your source stays unchanged."
                    .to_string(),
            ),
            TailorStep::Name => (
                "Name it",
                "This names the version, the file it exports as, and the card on the \
                 applications board."
                    .to_string(),
            ),
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .px(px(34.0))
            .pt(px(30.0))
            .pb(px(30.0))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .pb(px(22.0))
                    .child(
                        div()
                            .text_style(TextStyle::title())
                            .text_color(theme.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .max_w(px(620.0))
                            .text_style(TextStyle::body())
                            .text_color(theme.text_muted)
                            .child(lede),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    // Wraps rather than truncating: the rail drops under the
                    // panel in a narrow window instead of squeezing it.
                    .flex_wrap()
                    .items_start()
                    .gap(px(22.0))
                    .child(self.render_tailor_rail(cx, sheet))
                    .child(
                        // The panel is a frame with its own scroll, and the
                        // buttons sit **outside** it. Inside, they went below
                        // the fold the moment the vault had a few diary
                        // entries — a screen with no visible way forward.
                        div()
                            .flex_1()
                            .min_w(px(360.0))
                            .h_full()
                            .flex()
                            .flex_col()
                            .rounded(theme.radius_md())
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.surface)
                            .child(
                                div()
                                    .id("tailor-panel")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scrollbar()
                                    .p(px(22.0))
                                    .child(match sheet.step {
                                        TailorStep::Posting => {
                                            self.render_tailor_posting(cx, sheet).into_any_element()
                                        }
                                        TailorStep::Version => {
                                            self.render_tailor_version(cx, sheet).into_any_element()
                                        }
                                        TailorStep::Name => {
                                            self.render_tailor_name(cx, sheet).into_any_element()
                                        }
                                    }),
                            )
                            .child(self.render_tailor_footer(cx, sheet)),
                    ),
            )
    }

    /// The one row that never scrolls: what to press, and what it costs.
    fn render_tailor_footer(&self, cx: &mut Context<Self>, sheet: &TailorSheet) -> Div {
        let theme = *cx.theme();
        let has_text = !sheet.posting.read(cx).value(cx).trim().is_empty();
        let has_base = sheet
            .base
            .as_ref()
            .is_some_and(|(path, preset)| self.starting_point_available(path, *preset));
        let has_company = !sheet.company.read(cx).value(cx).trim().is_empty();

        let (primary, enabled, note): (&str, bool, &str) = match sheet.step {
            TailorStep::Posting if has_text => (
                "Read posting",
                true,
                "Compared with your vault on this device.",
            ),
            TailorStep::Posting => (
                "Skip posting",
                true,
                "Choose a starting CV without pasting a job description.",
            ),
            TailorStep::Version => (
                "Continue",
                has_base,
                "The saved CV stays untouched. You can choose changes on the next screen.",
            ),
            TailorStep::Name => (
                "Build the version",
                has_base && has_company,
                if has_company {
                    "Nothing is saved until you choose Save version on the next screen."
                } else {
                    "Add a company name to continue."
                },
            ),
        };
        let next = match sheet.step {
            TailorStep::Posting => TailorStep::Version,
            TailorStep::Version => TailorStep::Name,
            TailorStep::Name => TailorStep::Name,
        };
        let back = match sheet.step {
            TailorStep::Posting => None,
            TailorStep::Version => Some(TailorStep::Posting),
            TailorStep::Name => Some(TailorStep::Version),
        };

        div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .px(px(22.0))
            .py(px(16.0))
            .border_t_1()
            .border_color(theme.border)
            .child(
                Button::new("tailor-primary")
                    .action_primary()
                    .disabled(!enabled)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.tailoring.as_ref().map(|s| s.step) == Some(TailorStep::Name) {
                            this.start_tailoring(window, cx);
                        } else {
                            this.tailor_go(next, window, cx);
                        }
                    }))
                    .child(primary),
            )
            .children(back.map(|step| {
                Button::new("tailor-back")
                    .quiet()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.tailor_go(step, window, cx);
                    }))
                    .child("Back")
            }))
            .child(
                Button::new("tailor-cancel")
                    .quiet()
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.cancel_tailor(cx);
                    }))
                    .child("Cancel"),
            )
            .child(div().flex_1().min_w(px(12.0)))
            .child(
                div()
                    .min_w_0()
                    .text_style(TextStyle::body())
                    .text_color(theme.text_subtle)
                    .child(note),
            )
    }

    /// The rail: where you are, and what each step costs.
    fn render_tailor_rail(&self, cx: &mut Context<Self>, sheet: &TailorSheet) -> impl IntoElement {
        let theme = *cx.theme();
        let reached = sheet.step.index();
        div()
            .flex_none()
            .w(px(272.0))
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
                self.render_step(cx, index, title, promise, reached)
            }))
    }

    /// Step 1: the posting.
    fn render_tailor_posting(&self, cx: &mut Context<Self>, sheet: &TailorSheet) -> Div {
        let theme = *cx.theme();
        let _ = cx;
        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_style(TextStyle::eyebrow())
                            .text_color(theme.text_subtle)
                            .child(TextStyle::eyebrow().apply_case("Job description")),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::chip())
                            .text_color(theme.text_subtle)
                            .child("OPTIONAL"),
                    ),
            )
            .child(
                TextField::new(&sheet.posting)
                    .placeholder("Paste the full posting: responsibilities, requirements, and details…"),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_none()
                            .text_style(TextStyle::body())
                            .text_color(theme.accent)
                            .child("↳"),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_subtle)
                            .child(
                                "DockCV compares the posting with your own CVs, library, and diary \
                                 on this device. Nothing is sent anywhere.",
                            ),
                    ),
            )
    }

    /// Step 2: which version, and what the read found.
    fn render_tailor_version(
        &self,
        cx: &mut Context<Self>,
        sheet: &TailorSheet,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        let file_groups = groups(self.cache.metadata());
        let read = sheet.read.as_ref();

        let mut choices = div().flex().flex_col().gap(px(4.0));
        for group in &file_groups {
            let mut available: Vec<_> = group
                .readings
                .iter()
                .filter(|row| !row.unreadable)
                .cloned()
                .collect();
            let has_current_file = self
                .cache
                .readable_documents()
                .find(|(meta, _)| meta.path == group.path)
                .is_some_and(|(_, doc)| doc.active_preset_index().is_none());
            if has_current_file && available.iter().all(|row| row.preset.is_some()) {
                available.insert(
                    0,
                    Reading {
                        path: group.path.clone(),
                        stem: group.stem.clone(),
                        preset: None,
                        description: None,
                        based_on: None,
                        modified_secs: group
                            .readings
                            .first()
                            .and_then(|row| row.modified_secs),
                        unreadable: false,
                    },
                );
            }
            if available.is_empty() {
                continue;
            }
            let group_has_multiple = available.len() > 1;
            if group_has_multiple {
                choices = choices.child(
                    div()
                        .mt(px(9.0))
                        .mb(px(2.0))
                        .min_w_0()
                        .truncate()
                        .text_style(TextStyle::eyebrow())
                        .text_color(theme.text_subtle)
                        .child(TextStyle::eyebrow().apply_case(&group.stem)),
                );
            }
            for row in &available {
                let preset = row.preset.as_ref().map(|(index, _)| *index);
                let key = (row.path.clone(), preset);
                let chosen = sheet.base.as_ref() == Some(&key);
                let cov =
                    read.and_then(|r| r.coverage.iter().find(|(k, _)| *k == key).map(|(_, c)| c));
                let path = row.path.clone();
                let title = match &row.preset {
                    Some((_, name)) if group_has_multiple => name.clone(),
                    Some((_, name)) => format!("{} · {name}", row.stem),
                    None if group_has_multiple => "Current CV".to_string(),
                    None => format!("{} · Current CV", row.stem),
                };
                let detail = if row.preset.is_none() {
                    Some("Start from this file as it is now".to_string())
                } else {
                    self.base_choice_label(row)
                };
                let id_part = preset
                    .map(|index| format!("preset-{index}"))
                    .unwrap_or_else(|| "current".to_string());
                choices = choices.child(
                    SelectableRow::new(SharedString::from(format!(
                        "tailor-base-{}-{id_part}",
                        row.stem
                    )))
                    .selected(chosen)
                    .aria_label(title.clone())
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(div().truncate().text_color(theme.text).child(title))
                            .children(detail.map(|line| {
                                div()
                                    .truncate()
                                    .text_style(TextStyle::body())
                                    .text_color(theme.text_subtle)
                                    .child(line)
                            })),
                    )
                    .when_some(cov.filter(|c| c.total() > 0), |el, cov| {
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
                            sheet.base = Some((path.clone(), preset));
                        }
                        if this.tailoring.as_ref().is_some_and(|s| s.read.is_some()) {
                            this.read_posting(cx);
                        }
                        cx.notify();
                    })),
                );
            }
        }

        let scored = read.map(|r| r.terms.len()).filter(|n| *n > 0);
        let has_choices = file_groups
            .iter()
            .any(|group| group.readings.iter().any(|row| !row.unreadable));

        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                // The list is the decision, so it is what the eye lands on. A
                // column header rather than a paragraph: the number on each row
                // needs one word of explanation, not two sentences of it.
                div()
                    .flex()
                    .items_baseline()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_style(TextStyle::eyebrow())
                            .text_color(theme.text_subtle)
                            .child(TextStyle::eyebrow().apply_case("Starting points")),
                    )
                    .children(scored.map(|n| {
                        div()
                            .flex_none()
                            .text_style(TextStyle::eyebrow())
                            .text_color(theme.text_subtle)
                            .child(TextStyle::eyebrow().apply_case(&format!("matches / {n}")))
                    })),
            )
            .child(if has_choices {
                choices.into_any_element()
            } else {
                self.render_tailor_no_sources(cx).into_any_element()
            })
            .children((sheet.base.is_some() && read.is_some_and(|r| r.terms.is_empty())).then(
                || {
                    div()
                        .px(px(12.0))
                        .py(px(10.0))
                        .rounded(theme.radius_sm())
                        .bg(theme.elevated)
                        .text_style(TextStyle::body())
                        .text_color(theme.text_subtle)
                        .child(
                            "No clear requirements stood out in this posting. You can still \
                             choose a starting CV and tailor it by hand.",
                        )
                },
            ))
            .children(read.and_then(|r| self.render_tailor_score_note(cx, r)))
            .children(self.tailor_unused(cx))
    }

    fn render_tailor_no_sources(&self, cx: &mut Context<Self>) -> Div {
        let theme = *cx.theme();
        let has_files = !self.cache.metadata().is_empty();
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(10.0))
            .p(px(14.0))
            .rounded(theme.radius_sm())
            .border_1()
            .border_color(theme.border)
            .bg(theme.elevated)
            .child(
                div()
                    .text_style(TextStyle::control())
                    .text_color(theme.text)
                    .child(if has_files {
                        "There is no readable CV to start from yet."
                    } else {
                        "Bring in a CV to create a version."
                    }),
            )
            .child(
                div()
                    .text_style(TextStyle::body())
                    .text_color(theme.text_subtle)
                    .child("Your job details will stay here while you import one."),
            )
            .child(
                Button::new("tailor-import-cv")
                    .action_primary()
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.begin_tailor_import(cx);
                    }))
                    .child("Import a CV"),
            )
    }

    pub(super) fn begin_tailor_import(&mut self, cx: &mut Context<Self>) {
        self.gallery_creating = true;
        self.import_step = super::import::panels::ImportStep::Step1Drop;
        cx.notify();
    }

    /// Save an imported CV without leaving the application flow. The import
    /// review has already been shown; after saving, the new file is selected
    /// as the starting point and the pasted posting is read against it.
    pub(super) fn create_tailoring_source(
        &mut self,
        doc: crate::resume::model::ResumeDoc,
        filename_base: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(vault_dir) = self.vault.clone() else {
            return;
        };
        match crate::vault::create_document(&vault_dir, &doc, filename_base) {
            Ok(path) => {
                super::save_status::record(cx, "document", Ok(()));
                let revision = super::save_status::vault_revision(cx);
                self.cache.refresh(Some(&vault_dir), revision);
                if let Some(sheet) = self.tailoring.as_mut() {
                    sheet.base = Some((path, None));
                    sheet.step = TailorStep::Version;
                }
                self.gallery_creating = false;
                self.import_step = super::import::panels::ImportStep::Step1Drop;
                self.read_posting(cx);
                cx.notify();
            }
            Err(message) => super::save_status::record(cx, "document", Err(message)),
        }
    }

    /// The words the versions disagree about, under the list rather than over
    /// it.
    ///
    /// It was above, and it was the first thing on the screen: two rows of
    /// chips and a sentence, before anything you could click. The decision goes
    /// first now and its footnote follows.
    fn render_tailor_score_note(&self, cx: &mut Context<Self>, read: &PostingRead) -> Option<Div> {
        let theme = *cx.theme();
        if read.terms.is_empty()
            || self
                .tailoring
                .as_ref()
                .is_none_or(|sheet| sheet.base.is_none())
        {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .pt(px(14.0))
                .border_t_1()
                .border_color(theme.border)
                .child(
                    div()
                        .text_style(TextStyle::body())
                        .text_color(theme.text_subtle)
                        .child("The words your versions disagree about:"),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(4.0))
                        .children(read.terms.iter().map(|term| {
                            div()
                                .px(px(7.0))
                                .py(px(2.0))
                                .rounded(theme.radius_sm())
                                .bg(theme.elevated)
                                .text_style(TextStyle::chip())
                                .text_color(theme.text_muted)
                                .child(term.word.clone())
                        })),
                ),
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
        let sheet = self.tailoring.as_ref()?;
        sheet.base.as_ref()?;
        let read = sheet.read.as_ref()?;
        if read.unused.is_empty() {
            return None;
        }
        let rows = read.unused.iter().take(5).map(|item| {
            let (where_from, body) = match &item.source {
                UnusedSource::Diary { date, confidential, .. } => (
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
                UnusedSource::Library { section, .. } => (
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
        // A plain block, not a numbered one: the steps live in the rail now,
        // and this appears inside step two rather than being one of its own.
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .pt(px(14.0))
                .border_t_1()
                .border_color(theme.border)
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
                                .child("You have written this down"),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_style(TextStyle::body())
                                .text_color(theme.text_subtle)
                                .child("in your vault, not on this version"),
                        ),
                )
                .children(rows),
        )
    }

    /// What this version has done, in a sentence. **Not its name** — the row
    /// prints that above, and printing it twice was this screen's own bug.
    fn base_choice_label(&self, row: &Reading) -> Option<String> {
        let (stem, preset) = row.sent_as();
        let record = self.cache.applications().record_for(stem, preset);
        // Nothing when nothing has gone out. Five rows each reading `never
        // sent` is a column with no information in it, and it crowded out the
        // descriptions, which are the only thing that told the versions apart.
        let record = match (record.sent, record.interviewed) {
            (0, _) => None,
            (1, 0) => Some("sent once, nothing back yet".to_string()),
            (sent, 0) => Some(format!("sent {sent} times, nothing back yet")),
            (1, _) => Some("sent once, and it got an interview".to_string()),
            (sent, 1) => Some(format!("sent {sent} times, one interview")),
            (sent, got) => Some(format!("sent {sent} times, {got} interviews")),
        };
        match (row.subtitle(), record) {
            (Some(what), Some(record)) => Some(format!("{what} · {record}")),
            (Some(what), None) => Some(what),
            (None, record) => record,
        }
    }

    /// Step 3: the bookkeeping.
    fn render_tailor_name(&self, cx: &mut Context<Self>, sheet: &TailorSheet) -> Div {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .child(div().flex_1().min_w(px(180.0)).child(self.tailor_field(
                        cx,
                        "Company · required",
                        &sheet.company,
                    )))
                    .child(div().flex_1().min_w(px(180.0)).child(self.tailor_field(
                        cx,
                        "Role · optional",
                        &sheet.role,
                    ))),
            )
            .child(
                div()
                    .text_style(TextStyle::body())
                    .text_color(theme.text_subtle)
                    .child(
                        "The company name becomes the version's name, the export filename \
                         and the card on the applications board. The role is what the board \
                         shows under it.",
                    ),
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
            .child(
                TextField::new(state).placeholder(if label.starts_with("Company") {
                    "Company name"
                } else {
                    "Role title"
                }),
            )
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
pub(crate) fn section_word(section: SectionKind) -> &'static str {
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

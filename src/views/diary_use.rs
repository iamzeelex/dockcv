//! `Use in a CV →` — turning a logged win into a bullet of a specific CV.
//!
//! The review calls this the most valuable transition in the product and the
//! one drawn as a promise rather than a flow (P-05, US-06): *«Куда именно? В
//! какой CV, в какую секцию, в какой вариант?»*. The mockup puts the link on
//! every diary entry; nothing was behind it. What existed went the other way —
//! `root.rs::insert_diary_highlight` pulls an entry in from **inside** an open
//! editor — which means the diary itself was a write-only box: you could log
//! wins there and never get one out.
//!
//! So this is the sheet the question deserves: which CV, under which job, with
//! the text editable before it lands.
//!
//! ## Two things it deliberately does not do
//!
//! **It does not rewrite the text.** US-06 asks for the wording "предлагается
//! переписанным в CV-формат", and turning *"finally fixed the p99 thing that's
//! been haunting us"* into a résumé bullet is a language task — the AI layer's,
//! under review (§5.1, M5). What it does instead is put the diary's own words
//! in an editable box and let the author do it, which is honest work rather
//! than a machine guessing at their voice.
//!
//! **It records the link on the entry, not on the bullet.** US-06 wants both
//! directions; a bullet is a bare `String` in the model, so bullet → entry
//! would mean changing `highlights: Vec<String>` everywhere it is read,
//! written, rendered and edited. The entry side is additive and is what
//! answers the question a person actually asks six months later — *did I ever
//! use this?* — which is the same question the library's `used in N CVs`
//! answers about blocks.
//!
//! ## Why Work, and only Work
//!
//! A win lands in a job's bullets, because that is what a diary entry is: one
//! line about something you did in a role. Education, Certificates and
//! Organizations do not take free bullets in a shape a win would fit, so the
//! sheet does not offer a section picker it would then have to talk you out of.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, Entity, IntoElement, SharedString, Window};

use dockcv_ui_components::{ScrollableElement, Button, ListItem, ListItemExt, ButtonExt, Disableable, TextField, TextFieldState};

use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;

use super::save_status;
use super::shell::Shell;

/// The open sheet's state.
pub(super) struct DiaryUse {
    /// Which entry is being promoted, by position in `Diary::entries`.
    pub entry: usize,
    /// The bullet as it will be inserted — seeded from the entry and editable.
    pub text: Entity<TextFieldState>,
    /// The chosen document, or `None` until one is picked.
    pub doc: Option<DocChoice>,
    /// Which job in that document the bullet goes under.
    pub work: Option<usize>,
    /// The entry is marked confidential (US-36), so the box opened **empty**
    /// and the original is shown as reference only.
    pub confidential: bool,
    /// The diary's own wording, kept for display when `confidential`. Never
    /// seeded into the editable field — that is the whole rule.
    pub original: SharedString,
}

/// A document the bullet could go into.
#[derive(Clone)]
pub(super) struct DocChoice {
    pub path: std::path::PathBuf,
    pub stem: String,
    /// The jobs in its **active** Work variant: `position · employer`, in
    /// document order. Read when the document is chosen rather than per
    /// frame — picking a CV is rare, and this parses a file.
    pub jobs: Vec<String>,
}

impl Shell {
    /// Open the sheet for a diary entry.
    pub(super) fn open_diary_use(
        &mut self,
        entry: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((text, confidential)) = self
            .cache
            .diary()
            .entries
            .get(entry)
            .map(|e| (e.text.clone(), e.confidential))
        else {
            return;
        };

        // `auto_grow` rather than `multi_line` in a fixed box: the field is
        // asking the author to edit a sentence, so it has to show the whole
        // sentence. Three lines to start, eight before it scrolls.
        let field = cx.new(|cx| TextFieldState::auto_grow(3, 8, window, cx));
        // **A confidential entry is never seeded.** US-36's rule is that it is
        // never offered to a CV verbatim, and the strongest way to hold that
        // line is for the words to never be in the box to begin with — a
        // pre-filled field that says "please rewrite this" is one Cmd-Enter
        // away from going out as-is. The original stays visible beside it, so
        // the author can abstract from it without retyping from memory.
        if !confidential {
            field.update(cx, |state, cx| state.seed(&text, window, cx));
        }
        self.diary_use = Some(DiaryUse {
            entry,
            text: field,
            doc: None,
            work: None,
            confidential,
            original: text.into(),
        });
        cx.notify();
    }

    pub(super) fn close_diary_use(&mut self, cx: &mut Context<Self>) {
        self.diary_use = None;
        cx.notify();
    }

    /// Pick the destination document, and read the jobs it offers.
    fn choose_diary_use_doc(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let Some(meta) = self
            .cache
            .metadata()
            .iter()
            .find(|m| m.path == path)
            .cloned()
        else {
            return;
        };
        let Ok(doc) = vault::load(&path) else {
            save_status::report_unreadable(cx, &path, "this document did not parse".into());
            cx.notify();
            return;
        };

        let jobs: Vec<String> = doc
            .work
            .active()
            .iter()
            .map(|w| {
                let position = w.position.trim();
                let employer = w.name.trim();
                match (position.is_empty(), employer.is_empty()) {
                    (false, false) => format!("{position} · {employer}"),
                    (false, true) => position.to_string(),
                    (true, false) => employer.to_string(),
                    (true, true) => "Untitled role".to_string(),
                }
            })
            .collect();

        if let Some(sheet) = self.diary_use.as_mut() {
            sheet.doc = Some(DocChoice {
                path,
                stem: meta.stem,
                jobs,
            });
            // One job means there is nothing to choose; more means the author
            // has to say which, because a bullet under the wrong employer is a
            // lie about where the work happened.
            sheet.work = if sheet.doc.as_ref().is_some_and(|d| d.jobs.len() == 1) {
                Some(0)
            } else {
                None
            };
        }
        cx.notify();
    }

    /// Write the bullet into the chosen job's highlights, and record on the
    /// diary entry that this win has been used.
    fn commit_diary_use(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.diary_use.as_ref() else {
            return;
        };
        let (Some(choice), Some(work_index)) = (sheet.doc.as_ref(), sheet.work) else {
            return;
        };
        let text = sheet.text.read(cx).value(cx).trim().to_string();
        if text.is_empty() {
            return;
        }
        let (path, stem, entry_index) = (choice.path.clone(), choice.stem.clone(), sheet.entry);

        let mut doc = match vault::load(&path) {
            Ok(doc) => doc,
            Err(message) => {
                save_status::report_unreadable(cx, &path, message);
                cx.notify();
                return;
            }
        };
        let Some(job) = doc.work.active_mut().get_mut(work_index) else {
            return;
        };
        job.highlights.push(text);
        save_status::record(cx, "document", vault::save(&doc, &path));

        // The reverse half: the entry now knows it has been used, so the
        // diary can answer "did I ever use this?" without opening every CV.
        if let Some(vault_dir) = self.vault.clone() {
            let mut diary = vault::load_diary(&vault_dir);
            if let Some(entry) = diary.entries.get_mut(entry_index) {
                if !entry.used_in.contains(&stem) {
                    entry.used_in.push(stem);
                }
                save_status::record(cx, "diary", vault::save_diary(&vault_dir, &diary));
            }
        }

        self.diary_use = None;
        cx.notify();
    }

    /// The sheet, or nothing when it is closed.
    pub(super) fn render_diary_use_sheet(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let sheet = self.diary_use.as_ref()?;
        let theme = *cx.theme();

        // Every readable document is a destination. An unreadable one is not
        // offered rather than offered and then refused.
        let destinations: Vec<(String, std::path::PathBuf)> = self
            .cache
            .metadata()
            .iter()
            .filter(|m| !m.unreadable)
            .map(|m| {
                let label = if m.name.trim().is_empty() {
                    m.stem.clone()
                } else {
                    format!("{} · {}", m.name, m.stem)
                };
                (label, m.path.clone())
            })
            .collect();

        let jobs = sheet.doc.as_ref().map(|d| d.jobs.clone()).unwrap_or_default();

        let ready = sheet.doc.is_some() && sheet.work.is_some();
        let chosen_path = sheet.doc.as_ref().map(|d| d.path.clone());
        let chosen_job = sheet.work;

        // Inline lists rather than dropdown menus. A popover opened from inside
        // this sheet did not appear at all — it renders into the window's own
        // overlay layer and the scrim sits on top of it — and a picker you
        // cannot see is worse than a list that takes four more lines. It also
        // suits the data: a vault holds a handful of CVs, not a hundred.
        let doc_list = div()
            .id("diary-use-docs")
            .max_h(px(112.0))
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(destinations.into_iter().map(|(label, path)| {
                let selected = chosen_path.as_deref() == Some(path.as_path());
                let for_click = path.clone();
                ListItem::new(SharedString::from(format!(
                    "diary-use-doc-{}",
                    path.display()
                )))
                .row()
                .selected(selected)
                .text_color(if selected { theme.text } else { theme.text_muted })
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.choose_diary_use_doc(for_click.clone(), cx);
                    }))
                    .child(label)
            }));

        let job_list: gpui::AnyElement = if sheet.doc.is_none() {
            div()
                .text_style(TextStyle::body())
                .text_color(theme.text_subtle)
                .child("Pick a CV first.")
                .into_any_element()
        } else if jobs.is_empty() {
            div()
                .text_style(TextStyle::body())
                .text_color(theme.text_subtle)
                .child("That CV has no jobs yet — add one in the editor first.")
                .into_any_element()
        } else {
            div()
                .id("diary-use-jobs")
                .max_h(px(112.0))
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .children(jobs.into_iter().enumerate().map(|(index, job)| {
                    let selected = chosen_job == Some(index);
                    ListItem::new(SharedString::from(format!("diary-use-job-{index}")))
                        .row()
                        .selected(selected)
                        .text_color(if selected { theme.text } else { theme.text_muted })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            if let Some(sheet) = this.diary_use.as_mut() {
                                sheet.work = Some(index);
                            }
                            cx.notify();
                        }))
                        .child(job)
                }))
                .into_any_element()
        };

        let field_row = |label: &'static str, body: gpui::AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_style(TextStyle::label())
                        .text_color(theme.text_muted)
                        .child(label),
                )
                .child(body)
        };

        let panel = div()
            .w(px(520.0))
            .flex()
            .flex_col()
            .rounded(theme.radius_md())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_style(TextStyle::heading())
                            .text_color(theme.text)
                            .child("Use this win in a CV"),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_muted)
                            .child(
                                "Your diary wording, as you wrote it. Edit it into a bullet \
                                 before it goes in — nothing here rewrites your voice.",
                            ),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .children(sheet.confidential.then(|| {
                        div()
                            .p_3()
                            .rounded(theme.radius_md())
                            .bg(theme.surface)
                            .border_1()
                            .border_color(theme.warning)
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_style(TextStyle::control())
                                    .text_color(theme.warning)
                                    .child("This win is marked confidential"),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::body())
                                    .text_color(theme.text_muted)
                                    .child(
                                        "Its wording is not offered here. Write the \
                                         abstracted version — the outcome and the number, \
                                         not the client, the system or the incident.",
                                    ),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_subtle)
                                    .child(format!("your note: {}", sheet.original)),
                            )
                    }))
                    .child(field_row(
                        "Bullet",
                        TextField::new(&sheet.text)
                            .placeholder(if sheet.confidential {
                                "Write the abstracted version"
                            } else {
                                "The bullet as it will read"
                            })
                            .into_any_element(),
                    ))
                    .child(field_row("CV", doc_list.into_any_element()))
                    .child(field_row("Under which job", job_list))
                    .child(
                        div()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(
                                "Goes into the CV's active Work variant. The entry keeps a \
                                 note that you used it.",
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                Button::new("diary-use-cancel")
                                    .toolbar()
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.close_diary_use(cx);
                                    })),
                            )
                            .child(
                                Button::new("diary-use-add")
                                    .toolbar_primary()
                                    .label("Add bullet")
                                    .disabled(!ready)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.commit_diary_use(cx);
                                    })),
                            ),
                    ),
            );

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.scrim)
                // No click-to-dismiss on the scrim, matching
                // `root_overlays::render_capture_sheet`. A handler here fires
                // for clicks on the panel too — GPUI bubbles to the parent, and
                // `occlude` only stops events reaching elements *behind*, not
                // the ancestor — so opening the CV picker closed the sheet
                // underneath it. Cancel is the way out, and it is visible.
                .child(panel)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::resume::model::DiaryEntry;

    /// The rule US-36 rests on, stated as a test so it cannot be softened by
    /// accident: what the sheet seeds into the editable field is the entry's
    /// wording **only** when the entry is not confidential.
    ///
    /// The seeding itself needs a `Window`, so this pins the decision rather
    /// than the call — which is the part that would go wrong, and the part a
    /// future edit would be tempted to "simplify".
    #[test]
    fn a_confidential_entry_is_never_the_seed() {
        fn seed_for(entry: &DiaryEntry) -> Option<&str> {
            (!entry.confidential).then_some(entry.text.as_str())
        }

        let ordinary = DiaryEntry {
            text: "Cut p99 latency in half".into(),
            ..Default::default()
        };
        assert_eq!(seed_for(&ordinary), Some("Cut p99 latency in half"));

        let sensitive = DiaryEntry {
            text: "Contained the PII leak at client ACME".into(),
            confidential: true,
            ..Default::default()
        };
        assert_eq!(
            seed_for(&sensitive),
            None,
            "a confidential entry must reach the CV box as nothing at all"
        );
    }
}

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

use dockcv_ui_components::{Button, ButtonExt, Disableable, TextField, TextFieldState, MONO, SANS};

use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::front_door::{readings, Reading};
use super::shell::Shell;

/// The sheet's live state. Takes over the front door's body the way the
/// template chooser does, rather than being a modal over it — one less
/// mechanism, and the same shape a person has already met when making a CV.
pub(super) struct TailorSheet {
    pub company: Entity<TextFieldState>,
    pub role: Entity<TextFieldState>,
    /// Which reading to start from: its document and its preset index.
    /// `None` when the vault holds nothing to start from.
    pub base: Option<(std::path::PathBuf, usize)>,
}

impl Shell {
    /// Open the sheet, defaulting the base to the reading that has been sent
    /// most — the one that has been working is the one worth starting from.
    pub(super) fn open_tailor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let company = cx.new(|cx| TextFieldState::single_line(window, cx));
        let role = cx.new(|cx| TextFieldState::single_line(window, cx));
        let base = self.busiest_reading();

        let handle = company.read(cx).focus_handle(cx);
        self.tailoring = Some(TailorSheet {
            company,
            role,
            base,
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
        let Some((path, base)) = sheet.base.clone() else {
            return;
        };
        if company.is_empty() {
            return;
        }

        self.tailoring = None;
        self.open_version_draft(path, base, company, role, cx);
        let _ = window;
    }

    /// The sheet.
    pub(super) fn render_tailor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let Some(sheet) = self.tailoring.as_ref() else {
            return div();
        };
        let rows = readings(self.cache.metadata());
        let can_start = sheet.base.is_some();

        let mut choices = div().flex().flex_col().gap(px(2.0));
        for row in &rows {
            let Some((index, _)) = row.preset.clone() else {
                continue;
            };
            let chosen = sheet
                .base
                .as_ref()
                .is_some_and(|(p, i)| *p == row.path && *i == index);
            let path = row.path.clone();
            choices = choices.child(
                Button::new(SharedString::from(format!(
                    "tailor-base-{}-{}",
                    row.stem,
                    row.label()
                )))
                .quiet()
                .w_full()
                .justify_start()
                .when(chosen, |b| b.text_color(theme.accent))
                .on_click(cx.listener(move |this, _, _window, cx| {
                    if let Some(sheet) = this.tailoring.as_mut() {
                        sheet.base = Some((path.clone(), index));
                        cx.notify();
                    }
                }))
                .child(self.base_choice_label(row)),
            );
        }

        div()
            .w(px(520.0))
            .flex()
            .flex_col()
            .gap(px(18.0))
            .child(
                div()
                    .text_style(TextStyle::title())
                    .text_color(theme.text)
                    .child("Tailor for a job"),
            )
            .child(self.tailor_field(cx, "Company", &sheet.company))
            .child(self.tailor_field(cx, "Role", &sheet.role))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(11.0))
                            .text_color(theme.text_subtle)
                            .child("START FROM"),
                    )
                    .child(if rows.iter().any(|r| r.preset.is_some()) {
                        choices
                    } else {
                        div().text_style(TextStyle::body()).text_color(theme.text_muted).child(
                            "No preset to start from yet — save one on a CV first.",
                        )
                    }),
            )
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
    }

    fn base_choice_label(&self, row: &Reading) -> String {
        let (stem, preset) = row.sent_as();
        let record = self.cache.applications().record_for(stem, preset);
        let mut label = format!("{} · {}", row.stem, row.label());
        if record.sent > 0 {
            label.push_str(&format!("   sent {}", record.sent));
        }
        label
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

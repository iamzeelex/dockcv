//! Section-header rename (both built-in and custom sections, `docs/design/
//! editor.md` §3 anatomy, §10 open question on the rename field's home).
//!
//! `ResumeDoc::set_section_title` already knows the model split — a built-in
//! section gets a printed-heading override that a blank name clears back to
//! the default, a custom section's own `title` is written directly — but the
//! *view* never branches on it: one pen trigger in the card header, for every
//! section, turns the printed title into an inline `TextField` seeded from
//! `ResumeDoc::section_title`. Enter or clicking away commits; `Escape`
//! cancels through the same `CloseOverlay` path every other overlay in this
//! screen uses (`Root::on_close_overlay`).
//!
//! Before this existed, a custom section's title was only editable through a
//! "Section name" field buried in the card body (`FieldId::CustomSectionTitle`,
//! still addressable — `sync_fields` still seeds a `TextFieldState` for it,
//! just one nothing here renders into) — reachable, but not the same gesture
//! a built-in section had (none at all), which is exactly the inconsistency
//! this file removes.

use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, ClickEvent, Context, Entity, FontWeight, IntoElement, SharedString,
    Subscription, Window,
};

use dockcv_ui_components::{
    Button, ButtonExt, DockIcon, TextField, TextFieldEvent, TextFieldState, SANS,
};

use crate::resume::model::SectionKind;
use crate::theme::ActiveTheme;

use super::Root;

/// Live state for whichever section's header is mid-rename. Only one at a
/// time — [`Root::renaming_section`] holds it.
pub(super) struct SectionRename {
    pub(super) section: SectionKind,
    field: Entity<TextFieldState>,
    /// Keeps the `TextFieldEvent` → commit/blur translation alive for as long
    /// as this rename is in progress.
    _subscription: Subscription,
}

impl Root {
    /// Begin renaming `section` from its header. Re-entrant on the same
    /// section (just refocuses); starting a rename on a *different* section
    /// while one is already open commits the first one, same as clicking
    /// away from any other field in this screen would.
    pub(super) fn start_rename(
        &mut self,
        section: SectionKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(existing) = self.renaming_section.as_ref() {
            if existing.section == section {
                let handle = existing.field.read(cx).focus_handle(cx);
                handle.focus(window, cx);
                return;
            }
        }
        if self.renaming_section.is_some() {
            self.commit_rename(window, cx);
        }

        let current = self.doc.section_title(section);
        let field = cx.new(|cx| {
            let state = TextFieldState::single_line(window, cx);
            state.seed(current, window, cx);
            state
        });

        let subscription = cx.subscribe_in(
            &field,
            window,
            move |this, _state, event: &TextFieldEvent, window, cx| match event {
                TextFieldEvent::Submitted | TextFieldEvent::Blurred => {
                    this.commit_rename(window, cx)
                }
                TextFieldEvent::Changed | TextFieldEvent::Focused => {}
            },
        );

        let handle = field.read(cx).focus_handle(cx);
        self.renaming_section = Some(SectionRename {
            section,
            field,
            _subscription: subscription,
        });
        handle.focus(window, cx);
        cx.notify();
    }

    /// Write the field's current value back through `ResumeDoc::set_section_title`
    /// and close the rename control. Reaches the exported PDF, not just the
    /// panel — the heading is composed straight from `section_title` — so this
    /// recompiles, unlike most other overlay dismissals in this screen.
    pub(super) fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.renaming_section.take() else {
            return;
        };
        let value = rename.field.read(cx).value(cx).to_string();
        self.doc.set_section_title(rename.section, value);
        self.fields_stale = true;
        self.schedule_save(cx);
        cx.notify();
        self.schedule_recompile(window, cx);
    }

    /// Discard whatever was typed and close the rename control without
    /// touching the document — `Escape`'s path, via `on_close_overlay`.
    pub(super) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        if self.renaming_section.take().is_some() {
            cx.notify();
        }
    }

    /// The card header's title slot: the printed heading, or — while
    /// `section` is being renamed — the live input in its place. Same
    /// `flex_1`/`min_w_0` sizing either way, so the rest of the header
    /// doesn't reflow when the control opens.
    pub(super) fn render_section_heading(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
        title: SharedString,
    ) -> AnyElement {
        let theme = *cx.theme();

        if let Some(rename) = &self.renaming_section {
            if rename.section == section {
                return div()
                    .id(SharedString::from(format!("rename-{section:?}")))
                    .flex_1()
                    .min_w_0()
                    .font_family(SANS)
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    // A click positioning the caret must not also bubble to
                    // the header's own `on_click` and collapse the card out
                    // from under the field being edited.
                    .on_click(|_ev, _window, cx| cx.stop_propagation())
                    .child(TextField::new(&rename.field).seamless())
                    .into_any_element();
            }
        }

        div()
            .flex_1()
            .min_w_0()
            .font_family(SANS)
            .text_size(px(15.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text)
            .child(title)
            .into_any_element()
    }

    /// The pen trigger that opens [`Self::start_rename`] for `section`. A
    /// `Button`, not a raw `div`, so its own propagation-stopping mouse
    /// handling keeps this from also toggling the card's collapse state —
    /// the same reason `root_custom_sections.rs`'s "···" menu uses one.
    /// Hidden while `section` is already the one being renamed — nothing to
    /// trigger.
    pub(super) fn rename_button(&self, cx: &mut Context<Self>, section: SectionKind) -> AnyElement {
        if self
            .renaming_section
            .as_ref()
            .is_some_and(|r| r.section == section)
        {
            return div().into_any_element();
        }
        Button::new(SharedString::from(format!("rename-btn-{section:?}")))
            .icon_only()
            .icon(DockIcon::Pen)
            .tooltip("Rename section")
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.start_rename(section, window, cx);
            }))
            .into_any_element()
    }
}

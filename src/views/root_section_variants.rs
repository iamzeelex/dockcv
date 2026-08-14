//! The per-section variant timeline (`docs/design/editor.md` §3 "Variant
//! switcher") — split out of `root_sidebar.rs` to keep that file under the
//! ~800-line house limit, same reasoning as `root_section_rename.rs`/
//! `root_section_drag.rs`.
//!
//! Renaming the active variant (`docs/design/editor-comfort.md` C-2) copies
//! the gesture the editor already uses for a section heading
//! (`root_section_rename.rs`) and the one the Preset Matrix already uses for
//! its left pill (`shell.rs::start_preset_rename`): the active chip alone
//! grows a small pen; clicking it swaps that one chip for an inline
//! `TextField`; `Enter` or clicking away commits, and a blank name is
//! refused rather than stored. There is no longer a standalone "Variant
//! name" field in the card body — two controls for one idea was the
//! complaint, and the second read as document content rather than chrome.

use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, ClickEvent, Context, Entity, FontWeight, IntoElement, SharedString,
    Subscription, Window,
};

use dockcv_ui_components::{
    DockIcon, Field, Icon, IconName, Sizable, TextField, TextFieldEvent, TextFieldState, Tooltip,
    SANS,
};

use crate::resume::edit::FieldId;
use crate::resume::model::SectionKind;
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::confirm;
use super::Root;

/// Live state while `section`'s active variant is mid-rename. One at a time
/// across every section (`Root::renaming_variant` holds at most one) — same
/// shape as [`super::root_section_rename::SectionRename`], the gesture this
/// copies.
pub(super) struct VariantRename {
    pub(super) section: SectionKind,
    field: Entity<TextFieldState>,
    /// Keeps the `TextFieldEvent` → commit/blur translation alive for as long
    /// as this rename is in progress.
    _subscription: Subscription,
}

impl Root {
    /// The per-section version timeline: a "Variant" kicker, a pill per named
    /// variant (click to switch), a ✕ to delete the active one, and "+ new"
    /// to duplicate it. Lives inside the section's own card (L-06) — a
    /// variant is a property of its section, not a global toolbar control.
    pub(super) fn variant_bar(&self, cx: &mut Context<Self>, section: SectionKind) -> AnyElement {
        let theme = cx.theme().clone();
        let names = self.doc.variant_names(section);
        let active = self.doc.active_variant(section);
        let count = names.len();

        // "Variant" kicker — panel chrome, sans rather than the mockup's
        // mono (design doc §5 flag), same reasoning as "SECTIONS" above.
        let kicker = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .mb(px(7.0))
            .font_family(SANS)
            .text_size(px(10.0))
            .text_color(theme.text_subtle)
            .child("VARIANT");

        // P-17 discoverability moved into the pills' own tooltips. Two `Kbd`
        // chips sitting after the kicker rendered as `^⇧↑ ^⇧↓` — glyph soup
        // with nothing to attach it to, and because it only appeared on the
        // *focused* section it read as a rendering glitch that came and went.
        // A chord belongs on the control it drives, where the label says what
        // it does.

        // Only the active variant is ever mid-rename — `filter` collapses
        // "a rename is open, but on a different section" to `None` so the
        // check inside the pill loop is one comparison, not two.
        let renaming = self
            .renaming_variant
            .as_ref()
            .filter(|r| r.section == section);

        let pills = names.into_iter().enumerate().map(|(i, name)| {
            let is_active = i == active;

            // The active slot becomes an inline `TextField` while its rename
            // is open — the same swap `render_section_heading` makes for a
            // section title, so the two gestures read as one idea.
            if is_active {
                if let Some(rename) = renaming {
                    return div()
                        .id(SharedString::from(format!("var-rename-{section:?}")))
                        .w(px(140.0))
                        .child(TextField::new(&rename.field).small())
                        .into_any_element();
                }
            }

            let mut pill = div()
                .id(SharedString::from(format!("var-{section:?}-{i}")))
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(11.0))
                .py(px(6.0))
                .rounded(px(7.0))
                .font_family(SANS)
                .text_size(px(12.5))
                .font_weight(if is_active {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .bg(if is_active {
                    theme.accent
                } else {
                    theme.chip_bg_neutral
                })
                .text_color(if is_active {
                    theme.on_accent
                } else {
                    theme.text_muted
                })
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    Tooltip::new(format!(
                        "Switch to this variant  ·  {} / {}",
                        super::root::keys::PREV_VARIANT,
                        super::root::keys::NEXT_VARIANT
                    ))
                    .build(window, cx)
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.flush_variant_rename(section, window, cx);
                    this.doc.set_active_variant(section, i);
                    this.schedule_save(cx);
                    this.fields_stale = true;
                    cx.notify();
                    this.schedule_recompile(window, cx);
                }))
                .child(name);

            // The pen: only the active chip grows one (C-2). An inactive
            // pill's own click already switches to it, and switching before
            // renaming would end up renaming whichever variant became active,
            // not the one the user meant.
            if is_active {
                pill = pill.child(
                    div()
                        .id(SharedString::from(format!("var-rename-btn-{section:?}")))
                        .cursor_pointer()
                        .opacity(0.75)
                        .hover(|s| s.opacity(1.0))
                        .text_color(theme.on_accent)
                        .tooltip(|window, cx| Tooltip::new("Rename this variant").build(window, cx))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            // Nested inside the pill's own clickable area —
                            // without this, the pill's "switch to self" click
                            // also fires on every rename click.
                            cx.stop_propagation();
                            this.start_variant_rename(section, window, cx);
                        }))
                        .child(Icon::new(DockIcon::Pen).with_size(px(11.0))),
                );
            }

            pill.into_any_element()
        });

        let mut pill_row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.0))
            .children(pills);

        // Delete the active variant (only when more than one exists). Not
        // drawn in this mockup slice; kept so the capability the old toolbar
        // offered isn't silently dropped (design doc §10 leaves per-preset/
        // variant management largely open).
        if count > 1 {
            pill_row = pill_row.child(
                div()
                    .id(SharedString::from(format!("var-del-{section:?}")))
                    .px_1()
                    .rounded_md()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_muted)
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme.danger))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        // Committed first: the name in the dialog has to be the
                        // one on screen, including an edit still in the box.
                        this.flush_variant_rename(section, window, cx);
                        let active = this.doc.active_variant(section);
                        let name = this
                            .doc
                            .variant_names(section)
                            .get(active)
                            .cloned()
                            .unwrap_or_default();
                        confirm::destructive(
                            format!("Delete the “{name}” variant?"),
                            format!(
                                "{} The section keeps its other variants, and any preset \
                                 that selected this one falls back to the section's first.",
                                confirm::CANNOT_UNDO
                            ),
                            "Delete",
                            window,
                            cx,
                            move |this, window, cx| {
                                // Re-read rather than closing over `active`: the
                                // dialog is modal, but the selection is state and
                                // reading it late is free.
                                let active = this.doc.active_variant(section);
                                this.doc.remove_variant(section, active);
                                this.schedule_save(cx);
                                this.fields_stale = true;
                                cx.notify();
                                this.schedule_recompile(window, cx);
                            },
                        );
                    }))
                    .child(Icon::new(IconName::Close).with_size(px(11.0))),
            );
        }

        // Duplicate the active variant into a new one.
        pill_row = pill_row.child(
            div()
                .id(SharedString::from(format!("var-add-{section:?}")))
                .px(px(9.0))
                .py(px(6.0))
                .rounded(px(7.0))
                .border_1()
                .border_dashed()
                .border_color(theme.border)
                .font_family(SANS)
                .text_size(px(12.5))
                .text_color(theme.text_subtle)
                .cursor_pointer()
                .hover(|s| s.text_color(theme.accent))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.flush_variant_rename(section, window, cx);
                    this.doc.add_variant(section);
                    this.schedule_save(cx);
                    this.fields_stale = true;
                    cx.notify();
                    this.schedule_recompile(window, cx);
                }))
                .child("+ new"),
        );

        div()
            .flex()
            .flex_col()
            .mb(px(16.0))
            .child(kicker)
            .child(pill_row)
            .into_any_element()
    }

    /// The leading row of a section card body: the variant timeline, alone.
    ///
    /// Before C-2 this also carried a standalone "Variant name" field bound to
    /// `FieldId::VariantName(section)` — one more label-plus-input row for an
    /// idea the pill row already showed. Renaming now happens from the active
    /// chip's own pen (`Self::start_variant_rename`); nothing else in this
    /// card draws `FieldId::VariantName` anymore.
    pub(super) fn variant_controls(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> Vec<Field> {
        vec![Self::wide(self.variant_bar(cx, section))]
    }

    /// Begin renaming `section`'s active variant from its chip. Re-entrant on
    /// the same section (just refocuses); starting a rename on a *different*
    /// section while one is already open commits the first one first — same
    /// re-entrancy `Root::start_rename` uses for section headings.
    pub(super) fn start_variant_rename(
        &mut self,
        section: SectionKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(existing) = self.renaming_variant.as_ref() {
            if existing.section == section {
                let handle = existing.field.read(cx).focus_handle(cx);
                handle.focus(window, cx);
                return;
            }
            self.commit_variant_rename(window, cx);
        }

        let current = self.doc.variant_name(section).clone();
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
                    this.commit_variant_rename(window, cx)
                }
                TextFieldEvent::Changed | TextFieldEvent::Focused => {}
            },
        );

        let handle = field.read(cx).focus_handle(cx);
        self.renaming_variant = Some(VariantRename {
            section,
            field,
            _subscription: subscription,
        });
        handle.focus(window, cx);
        cx.notify();
    }

    /// Write the field's value back through `FieldId::VariantName` — never
    /// straight at `ResumeDoc::variant_name_mut`, or the write lands past the
    /// addressing layer and the variant is left dead (E-42) — and close the
    /// control. A blank name is refused rather than stored, same as a preset
    /// rename (`Shell::commit_preset_rename`): an unnamed variant is a chip
    /// with nothing printed on it, and the timeline has no other way to tell
    /// its variants apart.
    pub(super) fn commit_variant_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.renaming_variant.take() else {
            return;
        };
        let value = rename.field.read(cx).value(cx).trim().to_string();
        if !value.is_empty() {
            if let Some(slot) = FieldId::VariantName(rename.section).get_mut(&mut self.doc) {
                *slot = value;
            }
            self.fields_stale = true;
            self.schedule_save(cx);
        }
        // A variant's name never reaches the composed Typst source — only its
        // *data* does (`ResumeDoc::compose` reads `Versioned::active()`, never
        // `Variant::name`) — so unlike a section-heading rename there is
        // nothing to recompile here (`Root::last_source`'s doc comment calls
        // out this exact case).
        cx.notify();
    }

    /// Discard whatever was typed and close the control without touching the
    /// document — `Escape`'s path, via `Root::on_close_overlay`.
    pub(super) fn cancel_variant_rename(&mut self, cx: &mut Context<Self>) {
        if self.renaming_variant.take().is_some() {
            cx.notify();
        }
    }

    /// Commit a rename in progress on `section`, if there is one, before
    /// switching, deleting or adding a variant moves the section's active
    /// index out from under it. The rename control is keyed by section, not
    /// by index, so an unflushed rename would otherwise reappear pinned to
    /// whichever variant becomes active next rather than the one the user
    /// was actually naming.
    fn flush_variant_rename(
        &mut self,
        section: SectionKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .renaming_variant
            .as_ref()
            .is_some_and(|r| r.section == section)
        {
            self.commit_variant_rename(window, cx);
        }
    }
}

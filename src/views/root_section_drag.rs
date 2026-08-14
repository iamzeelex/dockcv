//! Drag-to-reorder sections (B6; keyboard reordering is B6b, a separate,
//! not-yet-built task that calls the same [`ResumeDoc::move_section`]).
//!
//! The mechanism is GPUI's own drag-and-drop (`InteractiveElement::on_drag`/
//! `drag_over`/`on_drop`), not `gpui-component`'s `List`/`Dock` — this
//! sidebar's cards are hand-rolled `div`s (`root_sidebar.rs::card`), not an
//! upstream list primitive, so there is nothing to opt an existing widget
//! into. The `⠿` handle (already drawn, `DockIcon::Grip`) is the drag
//! *source*; the whole card is the drop *target*.
//!
//! `ResumeDoc::move_section` only swaps one adjacent pair at a time — the
//! shape a keyboard nudge needs. Dropping section A onto section C therefore
//! calls it once per slot between them rather than reimplementing an
//! insert-at-index: `sections()` is re-read fresh inside `move_section` on
//! every call, so repeated adjacent swaps compose into exactly the "pull A
//! out, drop it where C was, shift the rest over" a user expects from a
//! single drag — without this file duplicating `sections()`'s own repair
//! pass, which the design doc is explicit must not happen twice.
//!
//! The drop target is drawn *before* the drop, not just after: `drag_over`
//! only paints while a drag is actually hovering a valid target, using
//! `theme.accent`/`theme.selected` — the same pair `theme/bridge.rs` already
//! routes onto upstream's `drag.border`/`drop_target.background` for exactly
//! this purpose (`crates/ui-components/src/theme/bridge.rs`), just drawn by
//! hand here since these cards aren't an upstream drag-aware widget.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, IntoElement, Render, SharedString, Window};

use dockcv_ui_components::{DockIcon, Icon, Sizable};

use crate::resume::model::SectionKind;
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::Root;

/// The small chip that follows the cursor while a section is being dragged —
/// just enough to confirm what's moving. `SharedString` rather than
/// `SectionKind` so it doesn't need a `Root`/`ResumeDoc` lookup mid-drag.
pub(super) struct SectionDragPreview {
    title: SharedString,
}

impl Render for SectionDragPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(8.0))
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.accent)
            .text_style(TextStyle::control())
            .text_color(theme.text)
            .opacity(0.92)
            .child(self.title.clone())
    }
}

impl Root {
    /// The `⠿` handle: press-and-drag to reorder `section` among its
    /// siblings. Wrapped in its own `Stateful` element (`on_drag` needs one)
    /// so grabbing the handle doesn't also register as a click on the
    /// header's own collapse toggle.
    pub(super) fn render_drag_handle(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
        title: SharedString,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        div()
            .id(SharedString::from(format!("drag-{section:?}")))
            .cursor_grab()
            .child(
                Icon::new(DockIcon::Grip)
                    .with_size(px(13.0))
                    .text_color(theme.border_strong),
            )
            .on_drag(section, move |_dragged, _offset, _window, cx| {
                cx.new(|_| SectionDragPreview {
                    title: title.clone(),
                })
            })
            .into_any_element()
    }

    /// Marks `el` as a drop target for a dragged [`SectionKind`]: highlights
    /// while a *different* section is dragged over it (`can_drop` excludes a
    /// section from being dropped on itself), and reorders on release.
    pub(super) fn section_drop_target<E>(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
        el: E,
    ) -> E
    where
        E: InteractiveElement,
    {
        let theme = cx.theme().clone();
        el.can_drop(move |dragged, _window, _cx| {
            dragged
                .downcast_ref::<SectionKind>()
                .is_some_and(|k| *k != section)
        })
        .drag_over::<SectionKind>(move |style, _dragged, _window, _cx| {
            style.border_color(theme.accent).bg(theme.selected)
        })
        .on_drop(cx.listener(move |this, dragged: &SectionKind, window, cx| {
            this.reorder_section(*dragged, section, window, cx);
        }))
    }

    /// Move `dragged` to sit where `target` currently is, shifting whatever
    /// was between them by one slot — see this file's own doc comment for why
    /// that's `move_section` called repeatedly rather than a bespoke insert.
    pub(super) fn reorder_section(
        &mut self,
        dragged: SectionKind,
        target: SectionKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if dragged == target {
            return;
        }
        let order = self.doc.sections();
        let (Some(from), Some(to)) = (
            order.iter().position(|k| *k == dragged),
            order.iter().position(|k| *k == target),
        ) else {
            return;
        };
        let step: isize = if to > from { 1 } else { -1 };
        for _ in 0..(to as isize - from as isize).unsigned_abs() {
            self.checkpoint();
            self.doc.move_section(dragged, step);
        }
        self.schedule_save(cx);
        cx.notify();
        self.schedule_recompile(window, cx);
    }
}

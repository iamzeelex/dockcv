//! Drag a card between columns (O-16).
//!
//! The `···` menu shipped first and stays, because it is the only path that
//! works without a mouse (US-21) — but a board whose cards cannot be dragged
//! reads as broken to anyone who has used a tracker before. Drag is the
//! expected gesture on this surface in a way it is not for résumé sections.
//!
//! Same mechanism as `root_section_drag.rs`: GPUI's own `on_drag` /
//! `drag_over` / `on_drop`, drawn by hand, because these cards are hand-rolled
//! `div`s rather than an upstream drag-aware widget.
//!
//! Two differences from the section drag, both from what a kanban actually is:
//!
//! - **The whole card is the drag source**, not a handle. A section list has a
//!   `⠿` precisely because the row is also a click target; a board card is not,
//!   so there is nothing for a drag to be confused with.
//! - **The drop target is the column, not another card.** Dropping is a change
//!   of *status*, not of position — cards inside a column keep the order they
//!   were created in, and inventing a hand-sorted order within a column would
//!   be a second, unasked-for feature (and a stored field to go with it).
//!
//! The mockup draws no drag state at all, so the hover treatment here is this
//! codebase's own convention: `theme.accent` + `theme.selected`, the same pair
//! `theme/bridge.rs` routes onto upstream's `drag.border`/
//! `drop_target.background`, exactly as the section drag already draws it.

use gpui::prelude::*;
use gpui::{div, px, Context, IntoElement, Render, SharedString, Window};

use crate::resume::model::ApplicationStatus;
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::shell::Shell;

/// What is being dragged: which row, and the column it came from so a column
/// can refuse a card it already holds. The company name is not in here — the
/// preview closure captures its own copy, so the payload stays the two facts
/// the drop actually needs.
#[derive(Clone)]
pub(super) struct DraggedCard {
    pub index: usize,
    pub from: ApplicationStatus,
}

/// The chip that follows the cursor — just enough to confirm what is moving.
pub(super) struct CardDragPreview {
    company: SharedString,
}

impl Render for CardDragPreview {
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
            .child(self.company.clone())
    }
}

impl Shell {
    /// Make a card draggable. `el` must already be `Stateful` (the card
    /// carries `apps-card-{index}`), which `on_drag` requires.
    pub(super) fn card_drag_source<E>(
        &self,
        index: usize,
        from: ApplicationStatus,
        company: SharedString,
        el: E,
    ) -> E
    where
        E: InteractiveElement + StatefulInteractiveElement + Styled,
    {
        el.cursor_grab().on_drag(
            DraggedCard { index, from },
            move |_dragged, _offset, _window, cx| {
                cx.new(|_| CardDragPreview {
                    company: company.clone(),
                })
            },
        )
    }

    /// Make a column accept cards from any *other* column, moving the dropped
    /// card into this status.
    ///
    /// The move goes through `advance_application`, the same call the `···`
    /// menu uses — so a card dragged into Applied records its send date and
    /// captures its PDF snapshot (D4a) exactly as it would have via the menu.
    /// A drag must not be a second, quieter path that skips those.
    pub(super) fn column_drop_target<E>(&self, cx: &mut Context<Self>, status: ApplicationStatus, el: E) -> E
    where
        E: InteractiveElement,
    {
        let theme = cx.theme().clone();
        el.can_drop(move |dragged, _window, _cx| {
            dragged
                .downcast_ref::<DraggedCard>()
                .is_some_and(|card| card.from != status)
        })
        .drag_over::<DraggedCard>(move |style, _dragged, _window, _cx| {
            style.border_color(theme.accent).bg(theme.selected)
        })
        .on_drop(cx.listener(move |this, dragged: &DraggedCard, _window, cx| {
            this.advance_application(dragged.index, status, cx);
        }))
    }
}

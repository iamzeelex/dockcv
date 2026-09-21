//! [`SelectableRow`] — one entry in a list you navigate.
//!
//! ```ignore
//! SelectableRow::new("nav-work")
//!     .selected(true)
//!     .leading(drag_handle)
//!     .child(div().child("Work Experience"))
//!     .trailing(count_badge)
//!     .trailing(chevron_button)
//!     .on_click(cx.listener(..))
//! ```
//!
//! Upstream's `ListItem` is a `Stateful<Div>`: it has hover, `selected` and a
//! click handler, and no focus ring, no `Role::Button` and no keyboard
//! activation — a gap `ListItemExt::row` already records rather than fixes. A
//! navigator you move through with the keyboard cannot be built out of it.
//!
//! The other reason this exists is nesting. A row usually carries more than one
//! target — a drag handle at the front, a count and a disclosure at the back —
//! and putting them *inside* the clickable element makes every one of them also
//! a click on the row. So the row is a plain container: `leading` and
//! `trailing` are siblings of the click target, and only the content between
//! them selects. Hover and selection paint the whole row, because the row is
//! what the eye reads as one thing.

use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, App, ClickEvent, ElementId, IntoElement, ParentElement, Pixels,
    RenderOnce, SharedString, StyleRefinement, Styled, Window,
};

use crate::theme::ActiveTheme;
use crate::typography::TextStyle;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct SelectableRow {
    id: ElementId,
    selected: bool,
    /// How far the clickable content is inset — an entry under its section.
    indent: Pixels,
    /// What a screen reader is told the row is. Defaults to nothing, because a
    /// row whose content is already text does not need it repeated.
    aria_label: Option<SharedString>,
    tooltip: Option<SharedString>,
    leading: Option<AnyElement>,
    trailing: Vec<AnyElement>,
    children: Vec<AnyElement>,
    on_click: Option<ClickHandler>,
    style: StyleRefinement,
}

impl SelectableRow {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            selected: false,
            indent: px(0.0),
            aria_label: None,
            tooltip: None,
            leading: None,
            trailing: Vec::new(),
            children: Vec::new(),
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn indent(mut self, indent: Pixels) -> Self {
        self.indent = indent;
        self
    }

    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// A handle, a marker or an icon before the content — outside the click
    /// target, so dragging a row does not also select it.
    pub fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading = Some(element.into_any_element());
        self
    }

    /// A count, a chip or a button after the content. Repeatable, and each one
    /// keeps its own click.
    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing.push(element.into_any_element());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl Styled for SelectableRow {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for SelectableRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SelectableRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.theme();
        // Keyed by the row's own id, so the handle survives the re-render a
        // click causes and the ring does not blink off as you arrow down.
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let focused = focus_handle.is_focused(window);

        let mut target = div()
            .id(self.id.clone())
            .role(gpui::accesskit::Role::Button)
            // `tab_stop` defaults to false, and without it the handle is
            // focusable but Tab never reaches it — the row's ring and the
            // Enter/Space activation GPUI synthesizes for a focused element
            // with click listeners would both be unreachable.
            .track_focus(&focus_handle.tab_index(0).tab_stop(true))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.0))
            .pl(self.indent)
            .px(px(8.0))
            .py(px(5.0))
            .cursor_pointer()
            .font_family(TextStyle::control().role.family())
            .text_size(TextStyle::control().size)
            .text_color(if self.selected {
                theme.text
            } else {
                theme.text_muted
            })
            .children(self.children);

        if let Some(label) = self.aria_label {
            target = target.aria_label(label);
        }
        if let Some(tooltip) = self.tooltip {
            target = target.tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            });
        }
        if let Some(handler) = self.on_click {
            target = target.on_click(move |event, window, cx| handler(event, window, cx));
        }

        // Fill, ring and rounding all belong to the **row**. Putting the ring
        // on the click target instead drew a second outlined box inside the
        // selected fill, inset by however wide `leading` happened to be — two
        // boxes for one row, and a different overlap on every row that had a
        // drag handle.
        let mut row = div()
            .flex()
            .items_center()
            .w_full()
            .min_w_0()
            .rounded(theme.radius_sm())
            // Always drawn, usually invisible: a border that appears only on
            // focus shifts every row by a pixel as you arrow down.
            .border_1()
            .border_color(if focused {
                theme.accent
            } else {
                gpui::transparent_black()
            })
            .when(self.selected, |el| el.bg(theme.selected))
            .hover(|el| el.bg(theme.hover))
            .children(self.leading)
            .child(target)
            .children(self.trailing);
        row.style().refine(&self.style);
        row
    }
}

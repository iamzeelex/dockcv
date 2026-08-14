//! [`Card`] — a raised container.
//!
//! ```ignore
//! Card::new().elevated().child(body)
//! Card::new().elevated().interactive("cv-3").on_click(cx.listener(..))
//! ```
//!
//! Cards are the repeating unit of this product's main screens — a grid of CVs, a
//! board of applications, a library of blocks — and in every one of those the whole
//! card is the click target. So interactivity is first-class here, not something a
//! caller has to hand-roll a `div()` to get: [`Card::interactive`] gives the card an
//! id, a hover fill and a click handler.

use gpui::prelude::*;
use gpui::{
    div, AnyElement, App, ClickEvent, ElementId, Hsla, IntoElement, ParentElement, RenderOnce,
    StyleRefinement, Styled, Window,
};

use crate::theme::ActiveTheme;
use gpui_component::{Sizable, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardVariant {
    #[default]
    Surface,
    Elevated,
    Outline,
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Card {
    id: Option<ElementId>,
    variant: CardVariant,
    padding: Size,
    border: bool,
    full_width: bool,
    /// Set by [`Card::interactive`]: the card lifts on hover and takes a click.
    interactive: bool,
    on_click: Option<ClickHandler>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Card {
    pub fn new() -> Self {
        Self {
            id: None,
            variant: CardVariant::Surface,
            padding: Size::Medium,
            border: true,
            full_width: true,
            interactive: false,
            on_click: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn variant(mut self, variant: CardVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn surface(self) -> Self {
        self.variant(CardVariant::Surface)
    }

    pub fn elevated(self) -> Self {
        self.variant(CardVariant::Elevated)
    }

    pub fn outline(self) -> Self {
        self.variant(CardVariant::Outline)
    }

    pub fn border(mut self, border: bool) -> Self {
        self.border = border;
        self
    }

    /// Make the whole card a hover-and-click target. Required before
    /// [`Card::on_click`] does anything — GPUI needs an id to track the state.
    pub fn interactive(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self.interactive = true;
        self
    }

    /// Handle a click on the whole card. Pair with [`Card::interactive`].
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children.extend(children.into_iter().map(|c| c.into_any_element()));
        self
    }
}

impl Sizable for Card {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.padding = size.into();
        self
    }
}

/// Lets a caller size, space and position a card without unwrapping it into a
/// `div()` — the reason the gallery had to hand-roll its own card shell before.
impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let pad = match self.padding {
            Size::XSmall => gpui::px(8.0),
            Size::Small => gpui::px(12.0),
            Size::Large => gpui::px(20.0),
            Size::Size(v) => v,
            _ => gpui::px(16.0),
        };

        let (bg, border_color): (Hsla, Option<Hsla>) = match self.variant {
            CardVariant::Surface => (
                theme.surface,
                if self.border { Some(theme.border) } else { None },
            ),
            CardVariant::Elevated => (
                theme.elevated,
                if self.border { Some(theme.border) } else { None },
            ),
            CardVariant::Outline => (
                gpui::Hsla::transparent_black(),
                Some(theme.border),
            ),
        };

        let hover_bg = theme.hover;
        let mut el = div()
            .p(pad)
            .rounded_lg()
            .bg(bg)
            .when(self.full_width, |el| el.w_full())
            .when_some(border_color, |el, color| {
                el.border_1().border_color(color)
            });
        // `refine`, never assignment: the lines above set the card's own padding,
        // radius, fill and border *into* the same `StyleRefinement`, so `*style =
        // caller` would throw all of it away and render a bare box. Refining
        // merges, with the caller's explicit values winning.
        el.style().refine(&self.style);

        // An id makes it `Stateful<Div>`, which is what carries hover and click.
        let Some(id) = self.id else {
            return el.children(self.children).into_any_element();
        };

        el.id(id)
            .when(self.interactive, |el| {
                el.cursor_pointer().hover(|s| s.bg(hover_bg))
            })
            .when_some(self.on_click, |el, handler| el.on_click(handler))
            .children(self.children)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A click handler without `interactive(..)` has no id to attach to, so it
    /// would be silently dropped. Constructing that combination is a caller
    /// mistake worth catching in review, not at runtime — assert the shape.
    #[test]
    fn interactive_sets_both_the_id_and_the_flag() {
        let plain = Card::new();
        assert!(plain.id.is_none() && !plain.interactive);

        let live = Card::new().interactive("cv-3").on_click(|_, _, _| {});
        assert!(live.id.is_some() && live.interactive && live.on_click.is_some());
    }

    /// Regression guard. `Card::render` sets its padding, radius, fill and border
    /// into the very `StyleRefinement` a caller's `Styled` calls also land in, so
    /// applying the caller's style by **assignment** silently erased all of it and
    /// rendered a bare box. It has to `refine`.
    ///
    /// This asserts the merge semantics the render depends on, since a `Card`
    /// cannot be rendered without a GPUI context.
    #[test]
    fn caller_style_refines_the_card_defaults_rather_than_replacing_them() {
        use gpui::{px, Refineable as _, StyleRefinement};

        let mut built_in = StyleRefinement::default();
        built_in.size.width = Some(px(100.0).into());

        let mut caller = StyleRefinement::default();
        caller.size.height = Some(px(40.0).into());

        built_in.refine(&caller);

        assert_eq!(
            built_in.size.width,
            Some(px(100.0).into()),
            "the card's own value was dropped — this is the bug"
        );
        assert_eq!(built_in.size.height, Some(px(40.0).into()));
    }

    #[test]
    fn padding_follows_the_size_ladder() {
        use gpui::px;
        let pad = |size: Size| match size {
            Size::XSmall => px(8.0),
            Size::Small => px(12.0),
            Size::Large => px(20.0),
            Size::Size(v) => v,
            _ => px(16.0),
        };
        assert!(pad(Size::XSmall) < pad(Size::Small));
        assert!(pad(Size::Small) < pad(Size::Medium));
        assert!(pad(Size::Medium) < pad(Size::Large));
        assert_eq!(pad(Size::Size(px(3.0))), px(3.0));
    }
}

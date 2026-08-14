//! [`TextField`] — DockCV's text input, and [`TextFieldState`], the entity that
//! holds its text, caret and selection.
//!
//! ```ignore
//! // in new() — needs a Window, so build it on the first frame if you have none:
//! let summary = cx.new(|cx| TextFieldState::auto_grow(2, 12, window, cx));
//! // in render():
//! TextField::new(&self.summary).placeholder("One paragraph about you.")
//! ```
//!
//! Stateful, so the parent owns an [`Entity<TextFieldState>`]-shaped handle and
//! passes a reference in render — the pattern described in `CLAUDE.md`.

use gpui::{
    div, prelude::*, px, App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    RenderOnce, SharedString, Styled, Window,
};
use gpui_component::input::{Input, InputEvent, InputState};

use crate::theme::ActiveTheme;
use gpui_component::{Sizable, Size};

/// What a field tells its parent. Deliberately narrower than upstream's event
/// set: app code should never need to know which engine is underneath.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextFieldEvent {
    /// The text changed. Debounce before saving or recompiling.
    Changed,
    /// Enter was pressed in a single-line field.
    Submitted,
    Focused,
    Blurred,
}

/// Holds one field's text, caret and selection.
pub struct TextFieldState {
    inner: Entity<InputState>,
    /// Kept alive for as long as the state exists; dropping it stops the
    /// upstream → DockCV event translation.
    _translate: gpui::Subscription,
}

impl TextFieldState {
    /// A one-line field: Enter submits rather than inserting a newline.
    pub fn single_line(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::wrap(cx.new(|cx| InputState::new(window, cx).submit_on_enter(true)), cx)
    }

    /// A prose field: Enter inserts a newline and long lines wrap.
    pub fn multi_line(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::wrap(
            cx.new(|cx| InputState::new(window, cx).multi_line(true).soft_wrap(true)),
            cx,
        )
    }

    /// A prose field that grows with its content, between `min` and `max` rows.
    pub fn auto_grow(min: usize, max: usize, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::wrap(
            cx.new(|cx| {
                InputState::new(window, cx)
                    .multi_line(true)
                    .soft_wrap(true)
                    .auto_grow(min, max)
            }),
            cx,
        )
    }

    fn wrap(inner: Entity<InputState>, cx: &mut Context<Self>) -> Self {
        let translate = cx.subscribe(&inner, |_this, _inner, event: &InputEvent, cx| {
            cx.emit(match event {
                InputEvent::Change => TextFieldEvent::Changed,
                InputEvent::PressEnter { .. } => TextFieldEvent::Submitted,
                InputEvent::Focus => TextFieldEvent::Focused,
                InputEvent::Blur => TextFieldEvent::Blurred,
            });
        });
        Self {
            inner,
            _translate: translate,
        }
    }

    /// Seed the field from the model without announcing a change.
    ///
    /// Use this at construction and whenever the document moves underneath the
    /// field; [`Self::set_value`] would otherwise echo straight back through
    /// [`TextFieldEvent::Changed`] and mark the document dirty on load.
    pub fn seed(&self, value: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
        self.inner
            .update(cx, |state, cx| state.set_value(value, window, cx));
    }

    pub fn value(&self, cx: &App) -> SharedString {
        self.inner.read(cx).value()
    }

    pub fn set_value(&self, value: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
        self.inner
            .update(cx, |state, cx| state.set_value(value, window, cx));
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.inner.update(cx, |state, cx| state.focus(window, cx));
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.inner.read(cx).focus_handle(cx)
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle(cx).is_focused(window)
    }
}

impl EventEmitter<TextFieldEvent> for TextFieldState {}

/// The element. Draws DockCV's own chrome — surface, hairline, focus ring — and
/// hands the text itself to the engine with `appearance(false)` so upstream
/// paints no border or background of its own.
#[derive(IntoElement)]
pub struct TextField {
    state: Entity<TextFieldState>,
    placeholder: Option<SharedString>,
    size: Size,
    disabled: bool,
    /// A bare field with no box around it, for inline editing inside a card.
    seamless: bool,
}

impl TextField {
    pub fn new(state: &Entity<TextFieldState>) -> Self {
        Self {
            state: state.clone(),
            placeholder: None,
            size: Size::Medium,
            disabled: false,
            seamless: false,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Drop the box: no background, no border, no padding. For fields that sit
    /// directly on a card and should read as text until you click them.
    pub fn seamless(mut self) -> Self {
        self.seamless = true;
        self
    }
}

impl Sizable for TextField {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for TextField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Tokens are `Copy`, so take them before anything borrows `cx` mutably.
        let (bg, border, accent, fg, fg_disabled) = {
            let theme = cx.theme();
            (
                theme.elevated,
                theme.border,
                theme.accent,
                theme.text,
                theme.text_subtle,
            )
        };

        let inner = self.state.read(cx).inner.clone();
        let focused = inner.read(cx).focus_handle(cx).is_focused(window);

        // Upstream carries the placeholder on the state, not the element, so a
        // presentational prop has to be pushed down here.
        if let Some(placeholder) = self.placeholder {
            inner.update(cx, |state, cx| {
                state.set_placeholder(placeholder, window, cx)
            });
        }
        let input = Input::new(&inner).appearance(false).disabled(self.disabled);

        let (pad_x, pad_y) = match self.size {
            Size::Small => (px(8.0), px(4.0)),
            Size::Large => (px(14.0), px(10.0)),
            _ => (px(11.0), px(7.0)),
        };

        div()
            .w_full()
            .when(!self.seamless, |this| {
                this.px(pad_x)
                    .py(pad_y)
                    .rounded_md()
                    .bg(bg)
                    .border_1()
                    .border_color(if focused { accent } else { border })
            })
            .text_color(if self.disabled { fg_disabled } else { fg })
            .child(input)
    }
}

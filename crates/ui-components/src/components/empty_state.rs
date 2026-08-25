//! [`EmptyState`] — the screen a user meets before they have any data.
//!
//! The design review calls a barren empty state out by name (P-13): a dotted
//! outline with no path forward, on what is often *the first screen in a
//! user's life* with this product. So `EmptyState` is never a shrug — it is a
//! headline in the display serif, one line of plain explanation, and a real
//! action.
//!
//! ```ignore
//! EmptyState::new("No CVs yet")
//!     .icon(IconName::File)
//!     .body("Bring in the CV you already have, or start from a template.")
//!     .action(Button::new("import").primary().label("Import a PDF"))
//!     .secondary_action(Button::new("blank").ghost().label("Start blank"))
//! ```

use gpui::{
    div, px, AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window,
};

use crate::components::icon::{Icon, IconName};
use gpui_component::Sizable as _;
use crate::theme::ActiveTheme;
use crate::typography::{StyledText, TextStyle};

#[derive(IntoElement)]
pub struct EmptyState {
    headline: SharedString,
    icon: Option<IconName>,
    body: Option<SharedString>,
    action: Option<AnyElement>,
    secondary_action: Option<AnyElement>,
}

impl EmptyState {
    pub fn new(headline: impl Into<SharedString>) -> Self {
        Self {
            headline: headline.into(),
            icon: None,
            body: None,
            action: None,
            secondary_action: None,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// The primary way out of the empty state. Takes an already-built
    /// element — usually a [`Button`](crate::components::Button).
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    /// A quieter alternative below the primary action ("Start blank" beside
    /// "Import a PDF").
    pub fn secondary_action(mut self, action: impl IntoElement) -> Self {
        self.secondary_action = Some(action.into_any_element());
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let icon = self
            .icon
            // Off the icon ladder on purpose: this glyph is not labelling a
            // control, it is the illustration at the top of an empty screen.
            .map(|name| div().mb_3().child(Icon::new(name)
                .with_size(px(30.0))
                .text_color(theme.text_subtle)));

        let headline = div()
            .text_style(TextStyle::heading())
            .text_color(theme.text)
            .child(self.headline.to_string());

        // Capped width so two or three sentences wrap short rather than
        // running the full column — a barren-feeling empty state is often
        // one badly-wrapped line, not a missing action.
        let body = self.body.map(|body| {
            div()
                .mt_2p5()
                .max_w(px(380.0))
                .text_style(TextStyle::body())
                .text_color(theme.text_muted)
                .child(body.to_string())
        });

        let action = self.action.map(|action| div().mt_5().child(action));
        let secondary = self
            .secondary_action
            .map(|action| div().mt_2().child(action));

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .text_center()
            .w_full()
            .py_12()
            .children(icon)
            .child(headline)
            .children(body)
            .children(action)
            .children(secondary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the headline is required; the builder must not panic or need an
    /// icon, body, or action to construct.
    #[test]
    fn headline_only_builds() {
        let state = EmptyState::new("No CVs yet");
        assert_eq!(state.headline.as_ref(), "No CVs yet");
        assert!(state.icon.is_none());
        assert!(state.body.is_none());
        assert!(state.action.is_none());
        assert!(state.secondary_action.is_none());
    }

    #[test]
    fn body_and_icon_are_recorded() {
        let state = EmptyState::new("No CVs yet")
            .icon(IconName::File)
            .body("Bring in the CV you already have.");
        // Upstream's `IconName` is neither `Debug` nor `PartialEq`, so the icon
        // can only be checked for presence.
        assert!(state.icon.is_some());
        assert_eq!(
            state.body.as_deref(),
            Some("Bring in the CV you already have.")
        );
    }
}

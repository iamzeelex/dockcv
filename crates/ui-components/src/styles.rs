use gpui::{px, FontWeight, Styled};
use gpui_component::button::{Button, ButtonVariants};

/// The height of a screen's top chrome bar.
///
/// One value, not one per screen. The editor's bar and the Preset Matrix's were
/// both 46px, chosen separately and never compared — and 46 is too little for
/// what the editor's carries: a two-line identity block beside four controls,
/// with macOS's traffic lights already occupying the left 80px. A bar that
/// tight reads as a strip the window is trying to hide rather than as the place
/// the document's controls live.
pub const CHROME_HEIGHT: gpui::Pixels = px(56.0);

pub trait StyledExt: Styled {
    /// Applies subtle hairline border styling.
    fn hairline_border(self) -> Self
    where
        Self: Sized,
    {
        self.border_1()
    }
}

impl<T: Styled> StyledExt for T {}

pub trait ButtonExt {
    /// Hero header primary button (`+ New CV`, `+ New block`, `+ New application`):
    /// 38px height, 16px horizontal padding, 9px border radius, 14px semibold text.
    fn header_primary(self) -> Self;

    /// Toolbar primary button (`Export PDF`):
    /// 32px height, 14px horizontal padding, 8px border radius, 13px semibold text.
    fn toolbar_primary(self) -> Self;

    /// Toolbar secondary outline button (`Capture`):
    /// 32px height, 12px horizontal padding, 8px border radius, 13px text.
    fn toolbar_secondary(self) -> Self;
}

impl ButtonExt for Button {
    fn header_primary(self) -> Self {
        self.primary()
            .h(px(38.0))
            .px(px(16.0))
            .rounded(px(9.0))
            .text_size(px(13.0))
            .font_weight(FontWeight::MEDIUM)
    }

    fn toolbar_primary(self) -> Self {
        self.primary()
            .h(px(32.0))
            .px(px(14.0))
            .rounded(px(8.0))
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
    }

    fn toolbar_secondary(self) -> Self {
        self.outline()
            .h(px(32.0))
            .px(px(12.0))
            .rounded(px(8.0))
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
    }
}


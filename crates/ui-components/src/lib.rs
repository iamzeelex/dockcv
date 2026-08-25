//! `dockcv-ui`: Reusable UI component library for DockCV.
//!
//! Inspired by `gpui-component` (https://github.com/longbridge/gpui-component).

pub mod components;
pub mod input;
pub mod styles;
pub mod theme;
pub mod typography;

pub use components::*;
pub use input::{TextField, TextFieldEvent, TextFieldState};
pub use styles::*;
pub use theme::{
    bridge, init_theme, set_theme_mode, tint::StatusTint, ActiveTheme, Theme, ThemeMode,
};

/// Upstream's control traits. There is deliberately only one `Sizable` and one
/// `Size` in the project — a second ladder was the same zombie shape as a second
/// theme.
pub use gpui_component::{Disableable, Selectable, Sizable, Size};
pub use typography::{StyledText, TextRole, TextStyle, MONO, SANS, SERIF};

use gpui::App;

/// Initialize the `dockcv-ui` component library on the GPUI application context,
/// with the palette the user last chose.
pub fn init(cx: &mut App, mode: ThemeMode) {
    let theme = Theme::of(mode);
    init_theme(cx, theme);
    input::init(cx, &theme);
}

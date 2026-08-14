//! The text-input seam.
//!
//! **This module is the only place in DockCV allowed to name `gpui_component`.**
//! Everything else — app code and every other component — talks to [`TextField`]
//! and [`TextFieldState`]. That is what keeps `crates/ui-components/THIRD_PARTY.md`'s
//! exit path cheap: replacing the engine underneath means rewriting this directory
//! and nothing else.
//!
//! Upstream widgets colour themselves from upstream's theme, so
//! [`crate::theme::bridge`] projects the DockCV palette onto it — see there.

mod field;

pub use field::{TextField, TextFieldEvent, TextFieldState};

/// The overlay host the text input requires as a window's first layer.
///
/// Exported under our own name so app code never writes `gpui_component`.
pub use gpui_component::Root as WindowRoot;

/// The editing actions the text input already binds (`cmd-c`, `cmd-v`, `cmd-z`…).
///
/// Re-exported because the app's **menu bar** has to name them: a macOS Edit
/// menu is not decoration, it is where the OS looks to decide whether Cut,
/// Copy, Paste and Undo are available at all, and `MenuItem::os_action` needs
/// the real action type to pair each item with its responder-chain equivalent.
/// Without this, app code would have to reach for `gpui_component::` directly —
/// the one thing this crate exists to prevent.
pub use gpui_component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};

use gpui::{AnyView, App, AppContext as _, Entity, Window};

use crate::theme::Theme;

/// Bring up the text-input engine and point it at our palette.
///
/// Must run once, before any [`TextFieldState`] is constructed — upstream binds
/// its key bindings here.
pub fn init(cx: &mut App, theme: &Theme) {
    gpui_component::init(cx);
    crate::theme::bridge::apply(cx, theme);
}

/// Wrap a window's top-level view in the layer the text input needs.
///
/// The input element reaches for `WindowRoot` through the window to place its
/// selection popovers and native menus, and **panics** if the window's first
/// layer is not one — so every window that can contain a [`TextField`] has to be
/// opened through this:
///
/// ```ignore
/// cx.open_window(options, |window, cx| {
///     let shell = cx.new(Shell::new);
///     input::window_root(shell, window, cx)
/// })
/// ```
pub fn window_root(
    view: impl Into<AnyView>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<WindowRoot> {
    cx.new(|cx| WindowRoot::new(view, window, cx))
}

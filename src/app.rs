//! Application bootstrap: process the GPUI run loop, register global actions
//! and menus, and open the main window.

use gpui::{
    actions, point, px, size, App, AppContext, Bounds, KeyBinding, Menu, MenuItem, SharedString,
    TitlebarOptions, WindowBounds, WindowOptions,
};

use crate::views::settings_window::SettingsWindow;
use crate::views::Shell;

/// Human-readable application name, surfaced in the title bar and menus.
pub const APP_NAME: &str = "DockCV";

// Top-level actions. `actions!` generates a unit struct per name plus the
// machinery GPUI needs to bind them to keys and menu items.
actions!(dockcv, [Quit, OpenSettings]);

/// Start the GPUI application. Blocks until the last window closes.
pub fn run() {
    gpui_platform::application()
        // Lucide plus DockCV's three additions, composed. SVG icons resolve by
        // path through this source; without it every glyph is blank.
        .with_assets(dockcv_ui_components::Assets)
        .run(|cx: &mut App| {
            register_fonts(cx);
            init(cx);
            open_main_window(cx);
            cx.activate(true);
        });
}

/// Register every bundled face so the UI can ask for it by family name.
///
/// All three families ship inside the binary: the app makes **zero** network
/// calls, in the editor or anywhere else (US-10). The mockup's Google Fonts
/// `<link>` is a mockup artifact and must never be reproduced.
fn register_fonts(cx: &mut App) {
    macro_rules! face {
        ($file:literal) => {
            std::borrow::Cow::Borrowed(
                include_bytes!(concat!("../assets/fonts/", $file)).as_slice(),
            )
        };
    }

    let fonts: Vec<std::borrow::Cow<'static, [u8]>> = vec![
        // Interface sans.
        face!("Geist-Regular.ttf"),
        face!("Geist-Medium.ttf"),
        face!("Geist-SemiBold.ttf"),
        face!("Geist-Bold.ttf"),
        // Editorial serif. Variable across optical size and weight.
        face!("Newsreader.ttf"),
        face!("Newsreader-Italic.ttf"),
        // Data.
        face!("JetBrainsMono-Regular.ttf"),
        face!("JetBrainsMono-Bold.ttf"),
    ];

    if let Err(error) = cx.text_system().add_fonts(fonts) {
        // Falling back to system faces silently would change the whole look of
        // the app without saying so.
        eprintln!("DockCV: bundled fonts failed to register: {error}");
    }
}

/// One-time application setup that does not depend on a particular window:
/// global key bindings, the application menu, quit handling, and UI component initialization.
fn init(cx: &mut App) {
    dockcv_ui_components::init(cx, crate::config::load().theme);

    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        // Where every macOS app puts it. Settings is a window rather than a
        // pane (O-21) precisely so this shortcut means what it means
        // everywhere else.
        KeyBinding::new("cmd-,", OpenSettings, None),
    ]);
    crate::views::init_keybindings(cx);

    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
    cx.on_action(|_: &OpenSettings, cx: &mut App| open_settings_window(cx));

    cx.set_menus(vec![Menu {
        name: SharedString::from(APP_NAME),
        items: vec![
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::action("Quit", Quit),
        ],
        disabled: false,
    }]);
}

/// Handles the app needs after the first window exists.
///
/// Settings is a window of its own (O-21) and has to reach the running
/// [`Shell`] — it owns no data, it edits the vault the main window has open.
/// The main window's id is kept beside it because "any window closed" is no
/// longer the same event as "the app is done": closing Settings must not quit.
struct AppWindows {
    shell: gpui::WeakEntity<Shell>,
    main: gpui::AnyWindowHandle,
    settings: Option<gpui::AnyWindowHandle>,
}

impl gpui::Global for AppWindows {}

/// Open Settings, or bring it forward if it is already up. A second Settings
/// window would be two views of one truth, each able to contradict the other.
fn open_settings_window(cx: &mut App) {
    let Some(existing) = cx.try_global::<AppWindows>().map(|w| w.settings) else {
        return;
    };
    if let Some(handle) = existing {
        if handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
            return;
        }
    }

    let Some(shell) = cx.try_global::<AppWindows>().map(|w| w.shell.clone()) else {
        return;
    };
    let bounds = Bounds::centered(None, size(px(720.), px(520.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from("Settings")),
            ..Default::default()
        }),
        ..Default::default()
    };

    match cx.open_window(options, |window, cx| {
        let view = cx.new(|_| SettingsWindow::new(shell));
        dockcv_ui_components::input::window_root(view, window, cx)
    }) {
        Ok(handle) => {
            if cx.has_global::<AppWindows>() {
                cx.global_mut::<AppWindows>().settings = Some(handle.into());
            }
        }
        Err(error) => eprintln!("DockCV: could not open the Settings window: {error}"),
    }
}

/// Open the primary window with a macOS-style transparent title bar so the
/// content can extend under the traffic-light buttons.
fn open_main_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from(APP_NAME)),
            appears_transparent: true,
            traffic_light_position: Some(point(px(12.0), px(12.0))),
        }),
        ..Default::default()
    };

    // Only the *main* window closing ends the app. Before Settings had a
    // window of its own, "a window closed" and "the app is done" were the same
    // event; now closing Settings would have quit DockCV.
    cx.on_window_closed(|cx, id| {
        if !cx.has_global::<AppWindows>() {
            cx.quit();
            return;
        }
        let windows = cx.global_mut::<AppWindows>();
        if windows.settings.map(|h| h.window_id()) == Some(id) {
            windows.settings = None;
            return;
        }
        if windows.main.window_id() == id {
            cx.quit();
        }
    })
    .detach();

    // The window's first layer must be the component library's overlay host —
    // the text input places its popovers there and panics without it.
    // The `Shell` is built before the window rather than inside it, so a
    // handle survives out here: the Settings window edits this exact instance
    // and must not construct a second one.
    let shell = cx.new(Shell::new);
    let weak_shell = shell.downgrade();
    let handle = cx
        .open_window(options, move |window, cx| {
            dockcv_ui_components::input::window_root(shell, window, cx)
        })
        .expect("failed to open the main DockCV window");

    cx.set_global(AppWindows {
        shell: weak_shell,
        main: handle.into(),
        settings: None,
    });
}

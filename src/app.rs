//! Application bootstrap: process the GPUI run loop, register global actions
//! and menus, and open the main window.

use gpui::{
    actions, point, px, size, App, AppContext, Bounds, KeyBinding, Menu, MenuItem, OsAction,
    SharedString, TitlebarOptions, WindowBounds, WindowOptions,
};

use crate::views::settings_window::SettingsWindow;
use crate::views::Shell;

/// Human-readable application name, surfaced in the title bar and menus.
pub const APP_NAME: &str = "DockCV";

/// The version the About box and the bundle report, from one place.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// Top-level actions. `actions!` generates a unit struct per name plus the
// machinery GPUI needs to bind them to keys and menu items.
actions!(
    dockcv,
    [
        Quit,
        OpenSettings,
        About,
        NewCv,
        RevealVault,
        SaveNow,
        CloseWindow,
        MinimizeWindow,
        ShowNotices,
    ]
);

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
        KeyBinding::new("cmd-n", NewCv, None),
        // Not "save" in the sense of a document that would otherwise be lost —
        // writes are automatic. This flushes the 600 ms debounce, which is what
        // the reflex is actually asking for.
        KeyBinding::new("cmd-s", SaveNow, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-m", MinimizeWindow, None),
    ]);
    crate::views::init_keybindings(cx);

    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
    cx.on_action(|_: &OpenSettings, cx: &mut App| open_settings_window(cx));
    cx.on_action(|_: &About, cx: &mut App| show_about(cx));
    cx.on_action(|_: &ShowNotices, cx: &mut App| show_notices(cx));
    cx.on_action(|_: &SaveNow, cx: &mut App| flush_open_document(cx));
    cx.on_action(|_: &NewCv, cx: &mut App| {
        with_shell(cx, |shell, cx| shell.start_new_cv(cx));
    });
    cx.on_action(|_: &RevealVault, cx: &mut App| {
        with_shell(cx, |shell, cx| shell.reveal_vault(cx));
    });
    cx.on_action(|_: &CloseWindow, cx: &mut App| {
        // Whichever window is in front — Settings closing must not take the
        // main window with it, and `on_window_closed` already tells them apart.
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
    });
    cx.on_action(|_: &MinimizeWindow, cx: &mut App| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.minimize_window());
        }
    });

    // The last chance to write. Quit observers run while the windows — and so
    // the `Shell` — are still alive, which is why the flush hangs here rather
    // than off a `Drop`.
    cx.on_app_quit(|cx: &mut App| {
        flush_open_document(cx);
        async {}
    })
    .detach();

    cx.set_menus(menus());
}

/// The application menu bar.
///
/// It used to be three items — Settings, a separator, Quit — under one
/// "DockCV" title. That is not a small omission on macOS: the **Edit** menu is
/// where the system looks to decide whether Cut, Copy, Paste and Undo are
/// offered at all (which is what `MenuItem::os_action` pairs up), and an app
/// with no File and no Window menu reads as unfinished before the user has
/// clicked anything.
///
/// Every item here does something. There is no Save: writes are debounced and
/// automatic, so a "Save" that pretended otherwise would be theatre — but
/// people press ⌘S anyway, so it is bound to an honest **Save Now**, which
/// flushes the pending write instead of waiting out the debounce.
fn menus() -> Vec<Menu> {
    use dockcv_ui_components::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};

    vec![
        Menu {
            name: SharedString::from(APP_NAME),
            items: vec![
                MenuItem::action("About DockCV", About),
                MenuItem::separator(),
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("Quit DockCV", Quit),
            ],
            disabled: false,
        },
        Menu {
            name: SharedString::from("File"),
            items: vec![
                MenuItem::action("New CV", NewCv),
                MenuItem::separator(),
                MenuItem::action("Save Now", SaveNow),
                MenuItem::action("Export PDF…", crate::views::ExportPdf),
                MenuItem::separator(),
                MenuItem::action("Reveal Vault in Finder", RevealVault),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
            ],
            disabled: false,
        },
        Menu {
            name: SharedString::from("Edit"),
            items: vec![
                // `os_action` is what connects each to the responder chain, so
                // the items are live inside a focused text field rather than
                // greyed out next to a caret that plainly can copy.
                MenuItem::os_action("Undo", Undo, OsAction::Undo),
                MenuItem::os_action("Redo", Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            ],
            disabled: false,
        },
        Menu {
            name: SharedString::from("Window"),
            // No "Zoom". `Window::zoom_window` is a no-op against this window —
            // verified by clicking it — and a menu item that does nothing is
            // the thing this menu was rebuilt to stop having. macOS still
            // offers its own Window ▸ Fill / Center, which do work.
            items: vec![MenuItem::action("Minimize", MinimizeWindow)],
            disabled: false,
        },
        Menu {
            name: SharedString::from("Help"),
            items: vec![MenuItem::action("Licences & Notices", ShowNotices)],
            disabled: false,
        },
    ]
}

/// Handles the app needs after the first window exists.
///
/// Settings is a window of its own (O-21) and has to reach the running
/// [`Shell`] — it owns no data, it edits the vault the main window has open.
/// The main window's id is kept beside it because "any window closed" is no
/// longer the same event as "the app is done": closing Settings must not quit.
struct AppWindows {
    /// A **strong** handle, deliberately. The window's root view held the only
    /// other one, so on the close-the-window path the `Shell` was already
    /// released by the time anything could ask it to flush its pending write —
    /// and that is exactly the path that has to flush. Keeping the app's one
    /// and only `Shell` alive for the process's lifetime costs nothing;
    /// [`Shell::flush_open_document`] having something to talk to is the point.
    shell: gpui::Entity<Shell>,
    main: gpui::AnyWindowHandle,
    settings: Option<gpui::AnyWindowHandle>,
}

impl gpui::Global for AppWindows {}

/// Write out whatever document is open, right now.
///
/// Called from both ways the app can end — `Quit` and the main window closing.
/// Edits are saved on a 600 ms debounce that lives on a `Task` the editor
/// entity owns, so shutting down without this cancelled the timer and threw
/// away the last thing the user typed. Which is most of a sentence, every time.
fn flush_open_document(cx: &mut App) {
    with_shell(cx, |shell, cx| shell.flush_open_document(cx));
}

/// Run something against the running [`Shell`], if there is one.
fn with_shell(cx: &mut App, f: impl FnOnce(&mut Shell, &mut gpui::Context<Shell>)) {
    let Some(shell) = cx.try_global::<AppWindows>().map(|w| w.shell.clone()) else {
        return;
    };
    shell.update(cx, f);
}

/// The About box: the app's name, its version, and where the licences are.
///
/// A native alert rather than a window, because there are three facts in it.
/// The version in particular was previously nowhere in the UI at all — the
/// Settings "About" row read the literal string "DockCV" — which makes a bug
/// report from a user impossible to place against a build.
fn show_about(cx: &mut App) {
    let Some(window) = cx.active_window() else {
        return;
    };
    let _ = window.update(cx, |_, window, cx| {
        // The receiver is dropped: there is one button, so there is no answer
        // worth waiting for. Dropping it does not close the dialog.
        let _dismissed = window.prompt(
            gpui::PromptLevel::Info,
            &format!("{APP_NAME} {APP_VERSION}"),
            Some(
                "A local-first CV workbench. Your documents are plain TOML files \
                 in a folder you chose; nothing is sent anywhere.\n\n\
                 MIT OR Apache-2.0. Bundled fonts, the Typst compiler and the icon \
                 set ship under their own licences — see Help → Licences & Notices.",
            ),
            &["OK"],
            cx,
        );
    });
}

/// Open the licence notices that ship inside the bundle.
///
/// `THIRD-PARTY-NOTICES.md` is copied into `Contents/Resources` by
/// `scripts/bundle.sh` precisely so it travels with the binary; this is the
/// path a user can reach it by. Running from `cargo run` there is no bundle, so
/// it falls back to the file in the source tree.
fn show_notices(cx: &mut App) {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("Resources").join("THIRD-PARTY-NOTICES.md")))
        .filter(|path| path.exists());

    // The repo copy, for `cargo run` — where there is no bundle to read from.
    // Debug builds only, and deliberately: `CARGO_MANIFEST_DIR` is the path the
    // binary was *built* in, so shipping it bakes one machine's home directory
    // into every copy and points the fallback at a directory the recipient does
    // not have. In a bundle the first branch always wins, which is the other
    // half of why this can go.
    #[cfg(debug_assertions)]
    let bundled = bundled.or_else(|| {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("THIRD-PARTY-NOTICES.md");
        repo.exists().then_some(repo)
    });

    match bundled {
        Some(path) => cx.open_with_system(&path),
        None => eprintln!("DockCV: THIRD-PARTY-NOTICES.md is not where it should be"),
    }
}

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

    // Weak on the way *into* the Settings window: it edits the running Shell
    // and must never be what keeps it alive.
    let Some(shell) = cx
        .try_global::<AppWindows>()
        .map(|w| w.shell.downgrade())
    else {
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
            // Write before quitting, not after. `cx.quit()` runs the quit
            // observers, which flush too — but only because `AppWindows` now
            // holds a *strong* `Shell` handle; the window's own reference is
            // already gone by the time this fires.
            flush_open_document(cx);
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
    let kept_shell = shell.clone();
    let handle = cx
        .open_window(options, move |window, cx| {
            dockcv_ui_components::input::window_root(shell, window, cx)
        })
        .expect("failed to open the main DockCV window");

    cx.set_global(AppWindows {
        shell: kept_shell,
        main: handle.into(),
        settings: None,
    });
}

//! Application bootstrap: process the GPUI run loop, register global actions
//! and menus, and open the main window.

use gpui::{
    actions, point, px, size, App, AppContext, Bounds, KeyBinding, Menu, MenuItem, OsAction,
    SharedString, TitlebarOptions, WindowBounds, WindowDecorations, WindowOptions,
};

use crate::views::settings_window::SettingsWindow;
use crate::views::{Shell, VaultScreen};

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
        RevealLog,
        // The rail's four destinations. The rail shows each chord on hover, and
        // a hint for a chord that does nothing is worse than no hint — so they
        // are bound here rather than drawn and left inert.
        GoToCvs,
        GoToLibrary,
        GoToDiary,
        GoToApplications,
    ]
);

/// Start the GPUI application. Blocks until the last window closes.
pub fn run() {
    let smoke = is_smoke_test();
    if smoke {
        log::info!("running DockCV in automated smoke-test mode");
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        let compositor = gpui::guess_compositor();
        log::info!("Linux display compositor detected: {compositor}");
        if compositor == "Headless" && !smoke {
            log::warn!(
                "No Wayland or X11 display detected ($WAYLAND_DISPLAY and $DISPLAY are unset). \
                 DockCV is running in headless mode — no GUI window will be displayed on screen."
            );
        }
    }

    gpui_platform::application()
        // Lucide plus DockCV's three additions, composed. SVG icons resolve by
        // path through this source; without it every glyph is blank.
        .with_assets(dockcv_ui_components::Assets)
        .run(move |cx: &mut App| {
            register_fonts(cx);
            init(cx);
            open_main_window(cx, smoke);
            cx.activate(true);

            if smoke {
                let budget = smoke_test_budget();
                std::thread::spawn(move || {
                    std::thread::sleep(budget);
                    log::error!(
                        "Smoke test watchdog timed out after {} seconds without completing \
                         initial frame",
                        budget.as_secs()
                    );
                    std::process::exit(1);
                });
            }
        });
}

/// How long the smoke test waits for a first frame.
///
/// Fifteen seconds is a generous budget on a GPU and not obviously one at all
/// on a software rasteriser: a CI runner has no graphics card, so Vulkan
/// resolves to llvmpipe and the first frame — which composes a document,
/// compiles it with Typst and rasterises a whole page — is doing on two shared
/// cores what a GPU does in parallel.
///
/// So the budget is the caller's to set, and the default stays tight. A
/// watchdog that is generous everywhere stops being a watchdog: it is there to
/// turn a hang into a failure, and a hang nobody notices for two minutes is a
/// hang that wasted two minutes.
fn smoke_test_budget() -> std::time::Duration {
    budget_from(std::env::var("DOCKCV_SMOKE_TEST_SECONDS").ok().as_deref())
}

/// The parsing half, taking the value rather than reading it.
///
/// Split so its test needs no environment. `set_var` is `unsafe` because the
/// C library's environment is not thread-safe, and `cargo test` runs threads:
/// a test that writes one while another reads `HOME` is a data race, and it
/// showed up as a failure somewhere else entirely.
fn budget_from(value: Option<&str>) -> std::time::Duration {
    const DEFAULT_SECONDS: u64 = 15;
    let seconds = value
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_SECONDS);
    std::time::Duration::from_secs(seconds)
}

/// Whether the application was launched in automated smoke-test mode.
///
/// Triggered via `--smoke-test` command-line argument or `DOCKCV_SMOKE_TEST=1` env var.
/// In smoke-test mode, the app boots, opens the window, renders the first frame,
/// and exits cleanly with code 0 (or exits 1 on watchdog timeout).
pub fn is_smoke_test() -> bool {
    smoke_test_requested(
        std::env::args(),
        std::env::var_os("DOCKCV_SMOKE_TEST").is_some(),
    )
}

/// The decision, taking its inputs rather than reading them. Same reason as
/// [`budget_from`]: a test must not write the process environment.
fn smoke_test_requested(mut args: impl Iterator<Item = String>, env_set: bool) -> bool {
    env_set || args.any(|argument| argument == "--smoke-test")
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
        log::error!("bundled fonts failed to register: {error}");
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
        KeyBinding::new("cmd-1", GoToCvs, None),
        KeyBinding::new("cmd-2", GoToLibrary, None),
        KeyBinding::new("cmd-3", GoToDiary, None),
        KeyBinding::new("cmd-4", GoToApplications, None),
    ]);
    crate::views::init_keybindings(cx);

    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
    cx.on_action(|_: &OpenSettings, cx: &mut App| open_settings_window(cx));
    cx.on_action(|_: &About, cx: &mut App| show_about(cx));
    cx.on_action(|_: &ShowNotices, cx: &mut App| show_notices(cx));
    cx.on_action(|_: &RevealLog, cx: &mut App| {
        // The folder, not the file: a log is something a user drags into a
        // chat, and the same wording the vault's own menu item uses.
        if let Some(dir) = crate::logging::log_path().parent() {
            cx.open_with_system(dir);
        }
    });
    cx.on_action(|_: &SaveNow, cx: &mut App| flush_open_document(cx));
    cx.on_action(|_: &NewCv, cx: &mut App| {
        with_shell(cx, |shell, cx| shell.start_new_cv(cx));
    });
    cx.on_action(|_: &RevealVault, cx: &mut App| {
        with_shell(cx, |shell, cx| shell.reveal_vault(cx));
    });
    // Deliberately global rather than scoped to the vault screens: the chord
    // has to work from the editor too, and `go_to` routes through the same
    // flush-then-leave path the rail's own click does.
    cx.on_action(|_: &GoToCvs, cx: &mut App| {
        with_shell(cx, |shell, cx| shell.go_to(VaultScreen::Cvs, cx));
    });
    cx.on_action(|_: &GoToLibrary, cx: &mut App| {
        with_shell(cx, |shell, cx| shell.go_to(VaultScreen::Library, cx));
    });
    cx.on_action(|_: &GoToDiary, cx: &mut App| {
        with_shell(cx, |shell, cx| shell.go_to(VaultScreen::Diary, cx));
    });
    cx.on_action(|_: &GoToApplications, cx: &mut App| {
        with_shell(cx, |shell, cx| shell.go_to(VaultScreen::Applications, cx));
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
        // Logged so a report can be read for what it is: a session that ends
        // here exited cleanly, and one that stops mid-file did not.
        log::info!("quitting — flushing any pending write");
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
            items: vec![
                MenuItem::action("Reveal Log in Finder", RevealLog),
                MenuItem::separator(),
                MenuItem::action("Licences & Notices", ShowNotices),
            ],
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
        .and_then(|exe| {
            Some(
                exe.parent()?
                    .parent()?
                    .join("Resources")
                    .join("THIRD-PARTY-NOTICES.md"),
            )
        })
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
        None => log::warn!("THIRD-PARTY-NOTICES.md is not where it should be"),
    }
}

/// Builds [`WindowOptions`] given platform traits, so unit tests can assert
/// the configuration for macOS, Windows, and Linux on any host.
pub(crate) fn build_window_options(
    title: &str,
    bounds: Bounds<gpui::Pixels>,
    is_macos: bool,
    is_linux: bool,
) -> WindowOptions {
    let titlebar = Some(TitlebarOptions {
        title: Some(SharedString::from(title.to_string())),
        appears_transparent: is_macos,
        traffic_light_position: if is_macos {
            Some(point(px(12.0), px(12.0)))
        } else {
            None
        },
    });

    let window_decorations = if is_linux {
        Some(WindowDecorations::Server)
    } else {
        None
    };

    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar,
        window_decorations,
        // A floor for the layout, rather than a fixed width chased out of
        // every control that has one.
        //
        // The rail is 228 and does not shrink — it is navigation, and a nav
        // rail narrow enough to be unreadable has stopped being one. Beside it
        // the widest pane that cannot wrap any further is the import screen's
        // panel at 380 plus its 68 of padding. Below about 700 the panel
        // starts leaving the window, and nothing in GPUI stops a person
        // dragging there, because no minimum was ever set.
        //
        // Every screen is built to reflow above this, so this is the width the
        // reflow is designed *for* rather than a number that merely happens to
        // work today.
        window_min_size: Some(gpui::size(px(760.0), px(560.0))),
        ..Default::default()
    }
}

/// Construct [`WindowOptions`] shaped for the host platform.
///
/// - macOS: transparent titlebar (`appears_transparent: true`) with traffic-light
///   buttons inset at (12, 12), allowing content to extend under the window controls.
/// - Windows: standard native titlebar (`appears_transparent: false`), so DWM
///   draws the native title bar, window title, and min/max/close controls.
/// - Linux: native server-side decorations (`WindowDecorations::Server`), so Wayland
///   (via `zxdg_toplevel_decoration_v1`) and X11 (via `_MOTIF_WM_HINTS`) render
///   the desktop environment's native title and caption buttons.
pub(crate) fn platform_window_options(title: &str, bounds: Bounds<gpui::Pixels>) -> WindowOptions {
    build_window_options(
        title,
        bounds,
        cfg!(target_os = "macos"),
        cfg!(any(target_os = "linux", target_os = "freebsd")),
    )
}

/// Open Settings, or bring it forward if it is already up. A second Settings
/// window would be two views of one truth, each able to contradict the other.
fn open_settings_window(cx: &mut App) {
    let Some(existing) = cx.try_global::<AppWindows>().map(|w| w.settings) else {
        return;
    };
    if let Some(handle) = existing {
        if handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return;
        }
    }

    // Weak on the way *into* the Settings window: it edits the running Shell
    // and must never be what keeps it alive.
    let Some(shell) = cx.try_global::<AppWindows>().map(|w| w.shell.downgrade()) else {
        return;
    };
    let bounds = Bounds::centered(None, size(px(720.), px(520.)), cx);
    let options = platform_window_options("Settings", bounds);

    match cx.open_window(options, |window, cx| {
        window.activate_window();
        let view = cx.new(|_| SettingsWindow::new(shell));
        dockcv_ui_components::input::window_root(view, window, cx)
    }) {
        Ok(handle) => {
            let _ = handle.update(cx, |_, window, _| {
                window.activate_window();
            });
            if cx.has_global::<AppWindows>() {
                cx.global_mut::<AppWindows>().settings = Some(handle.into());
            }
        }
        Err(error) => log::error!("could not open the Settings window: {error}"),
    }
}

/// Open the primary window.
///
/// On macOS, uses an integrated transparent title bar with native traffic lights.
/// On Windows, enables native titlebar/DWM caption buttons.
/// On Linux, requests native server-side decorations.
fn open_main_window(cx: &mut App, smoke_test: bool) {
    let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
    let options = platform_window_options(APP_NAME, bounds);

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
            window.activate_window();
            dockcv_ui_components::input::window_root(shell, window, cx)
        })
        .expect("failed to open the main DockCV window");

    let _ = handle.update(cx, |_, window, _| {
        window.activate_window();
    });

    if smoke_test {
        let _ = handle.update(cx, |_, window, _| {
            window.on_next_frame(|_, cx| {
                log::info!("Smoke test passed: initial window frame rendered successfully");
                cx.quit();
            });
        });
    }

    cx.set_global(AppWindows {
        shell: kept_shell,
        main: handle.into(),
        settings: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_window_options_shape_matches_target_os_conventions() {
        let bounds = Bounds::default();

        // macOS: transparent titlebar, traffic lights positioned at (12, 12), no server decorations
        let mac = build_window_options("DockCV", bounds, true, false);
        let mac_tb = mac.titlebar.expect("macos titlebar");
        assert!(
            mac_tb.appears_transparent,
            "macOS must use transparent titlebar"
        );
        assert_eq!(
            mac_tb.traffic_light_position,
            Some(point(px(12.0), px(12.0))),
            "macOS must inset traffic lights"
        );
        assert_eq!(mac.window_decorations, None);

        // Windows: non-transparent titlebar so DWM draws native frame & caption buttons
        let win = build_window_options("DockCV", bounds, false, false);
        let win_tb = win.titlebar.expect("windows titlebar");
        assert!(
            !win_tb.appears_transparent,
            "Windows must not hide native titlebar"
        );
        assert_eq!(
            win_tb.traffic_light_position, None,
            "Windows must not position traffic lights"
        );
        assert_eq!(win_tb.title.as_deref(), Some("DockCV"));
        assert_eq!(win.window_decorations, None);

        // Linux: Server-side decorations requested, non-transparent titlebar
        let lin = build_window_options("DockCV", bounds, false, true);
        let lin_tb = lin.titlebar.expect("linux titlebar");
        assert!(!lin_tb.appears_transparent);
        assert_eq!(lin_tb.traffic_light_position, None);
        assert_eq!(
            lin.window_decorations,
            Some(WindowDecorations::Server),
            "Linux must request server-side decorations"
        );
    }

    /// The budget is the caller's, and a nonsense value is not a budget.
    #[test]
    fn the_smoke_test_budget_defaults_tight_and_ignores_rubbish() {
        use std::time::Duration;
        assert_eq!(budget_from(None), Duration::from_secs(15));
        assert_eq!(budget_from(Some("90")), Duration::from_secs(90));

        for rubbish in ["0", "", "soon", "-5"] {
            assert_eq!(
                budget_from(Some(rubbish)),
                Duration::from_secs(15),
                "{rubbish:?} is not a number of seconds; the default stands"
            );
        }
    }

    /// Either route asks for it, and neither is read from the process here —
    /// `logging.rs` states the rule this used to break: tests share a process,
    /// and one writing the environment while another reads it is a data race
    /// that surfaces as a failure somewhere else entirely.
    #[test]
    fn smoke_test_flag_detection() {
        let none = || std::iter::empty::<String>();
        let flag = || ["dockcv".to_string(), "--smoke-test".to_string()].into_iter();

        assert!(
            smoke_test_requested(none(), true),
            "the variable alone asks"
        );
        assert!(smoke_test_requested(flag(), false), "the flag alone asks");
        assert!(smoke_test_requested(flag(), true));
        assert!(!smoke_test_requested(none(), false));
        assert!(
            !smoke_test_requested(["dockcv".to_string()].into_iter(), false),
            "an ordinary launch is not a smoke test"
        );
    }
}

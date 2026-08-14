# DockCV

A cross-platform desktop application built with [GPUI](https://www.gpui.rs/), the
GPU-accelerated UI framework from the [Zed](https://github.com/zed-industries/zed)
editor.

**macOS is the primary target.** Linux and Windows are wired up through
`gpui_platform` feature selection and are intended to build without source
changes, but are not yet a focus.

## Project layout

```
src/
├── main.rs        # entry point + platform attributes
├── app.rs         # GPUI bootstrap: run loop, actions, menus, main window
├── theme.rs       # color palette (single dark theme for now)
└── ui/
    ├── mod.rs
    └── root.rs    # root view: title bar + sidebar + content shell
```

## Requirements

- Latest **stable** Rust (`rustup update stable`).
- **macOS:** Xcode + command line tools installed and selected
  (`xcode-select --install`), needed for Metal rendering.
- **Linux/FreeBSD:** a Wayland or X11 development environment.

## Running

```sh
cargo run
```

The first build is slow: GPUI and its dependency tree are compiled from the
Zed git repository. Subsequent builds are incremental.

## Dependency pinning

`gpui` and `gpui_platform` are pulled from the Zed git repo. GPUI is pre-1.0 and
changes frequently, so pin both to the **same** git revision when you want
reproducible builds:

```toml
gpui = { git = "https://github.com/zed-industries/zed", rev = "<commit>" }
```

(Apply the matching `rev` to every `gpui_platform` target entry as well.)

## License

MIT OR Apache-2.0.

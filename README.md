# DockCV

A local-first résumé workbench: a native desktop app built with
[GPUI](https://www.gpui.rs/) — the GPU-accelerated UI framework from the
[Zed](https://github.com/zed-industries/zed) editor — with an **in-process
Typst compiler** driving a live paper preview. No cloud, no Electron, no web
view, and **no network request you did not ask for**: the app is fully
functional with the network off, and the one request it can make — an update
check that is off until you turn it on — is described under *Updates* below.

Your CVs live in a *vault*: a plain folder of human-readable TOML files, one per
document, that you can open in any text editor, sync with iCloud or Dropbox, and
keep under version control. File-over-App — the files are the product.

**macOS is the primary target.** Linux and Windows are wired up through
`gpui_platform` feature selection and are expected to build without source
changes, but are not tested.

## Project layout

```
src/
  main.rs            entry point, platform attributes
  app.rs             GPUI bootstrap: run loop, fonts, actions, menus, windows
  config.rs          app preferences outside the vault (last vault path, theme)
  vault.rs           the File-over-App store: TOML on disk, one file per document
  typst_engine.rs    in-process Typst compile → rasterized pixels
  render.rs          pixels → gpui::RenderImage
  resume/            the data model, field addressing, Typst codegen, importers
  import/            PDF / DOCX / LinkedIn / JSON Resume / text engines
  views/             the screens: gallery, editor, library, diary, applications…
crates/ui-components/  the design system: theme tokens and the widget facade
assets/                bundled fonts, the app icon, the vendored Typst package
packaging/             Info.plist template
scripts/               bundle.sh
docs/                  design specs, roadmap, the open-decisions ledger
```

`CLAUDE.md` is the contributor guide: architecture, design-system rules, data
model invariants and conventions.

## Requirements

- Rust — the toolchain is **pinned** in `rust-toolchain.toml`; `rustup` picks it
  up automatically.
- **macOS:** Xcode command line tools (`xcode-select --install`), for Metal.
- **Linux/FreeBSD:** a Wayland or X11 development environment.
- To build the `.app` icon: `brew install librsvg`.

## Running

```bash
cargo run
```

The first build is slow — GPUI and its dependency tree are compiled from the Zed
git repository. Subsequent builds are incremental (`cargo check` on a warm
target is a couple of seconds).

## Verifying a change

A change is not done until all three are clean:

```bash
cargo check --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace
```

CI runs the same three on macOS, plus `cargo-audit` and `cargo-deny`.

## Building a distributable app

```bash
./scripts/bundle.sh
```

produces `dist/DockCV.app` — **unsigned**, which means it runs on the machine
that built it and is blocked by Gatekeeper everywhere else. To ship it you need
an Apple Developer ID certificate:

```bash
./scripts/bundle.sh --sign "Developer ID Application: Name (TEAMID)" --notarize PROFILE --dmg
```

`scripts/bundle.sh --help` explains how to store the notarisation credentials.

## Logs

`~/Library/Logs/DockCV/dockcv.log`, or **Help ▸ Reveal Log in Finder**. One
previous log is kept as `dockcv.log.1`; both are capped at 2 MB.

The log records what the app did — vault opened, import outcome, export, every
failed read or write, and any panic with its backtrace — and never what the
user wrote. No field values, no bullets, no diary entries; the home directory
is rewritten to `~` so the log does not carry an account name either. It is a
local file and nothing sends it anywhere.

`DOCKCV_LOG` sets the level for DockCV's own crates (default `info`),
`DOCKCV_LOG_DEPS` for everything else (default `warn`, which is what keeps
GPUI's chatter out of an ordinary session):

```bash
DOCKCV_LOG=debug cargo run
```

## Updates

DockCV never updates itself and never downloads anything on its own.

**Settings ▸ General ▸ Updates** offers three states — `Never`, `When I ask`
(the default) and `Weekly`. Until you change it, the only request that can
happen is the one you make by pressing *Check now*.

A check is a single `GET` of a small static file attached to the newest
release. It carries no query string, no identifier and not even the version
being compared — the comparison happens locally. The request is made by the
system's `curl` rather than by an HTTP client compiled into the binary, so the
capability is a visible subprocess and no TLS stack ships inside the app. When
a newer version exists, one line appears in the rail with a link that opens the
release page in your browser; installing is a download you make, the same way
you installed it the first time.

The app is not self-updating on purpose. Replacing your own binary safely needs
a signature to verify the replacement against, and DockCV has no Apple
Developer ID — a self-updater without one is a hole with a progress bar.

## Dependency pinning

`gpui` and `gpui_platform` come from the Zed git repository and are deliberately
**unpinned in the manifests** — `Cargo.lock` is the pin. Adding an explicit
`rev` makes Cargo see a different source from `gpui-component`'s unpinned one
and puts two incompatible copies of GPUI in the graph. Read the "Version
coupling" section of `crates/ui-components/THIRD_PARTY.md` before touching a
gpui line, and never run a plain `cargo update`.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.

DockCV embeds fonts, a Typst package and an icon set in its binary under their
own licences — several of which require their notices to be distributed with it.
Those are collected in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md), which
ships inside the `.app` and must accompany any build.

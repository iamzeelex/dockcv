# Contributing to DockCV

Bug reports are welcome from anyone. Patches are welcome too, with one caveat
worth stating up front: DockCV is an opinionated application built to a fairly
specific argument about what a résumé tool should be, and a pull request that
arrives with no warning may run into an opinion it could not have known about.
Open an issue first for anything larger than a fix. It costs a day and saves a
weekend.

## Building it

The toolchain is pinned in `rust-toolchain.toml`; rustup reads that file and
installs the right version without being asked. You will also need:

- **macOS** — the Xcode command line tools, for Metal. `xcode-select --install`.
- **Linux or FreeBSD** — a Wayland or X11 development environment.
- **The app icon**, if you intend to build a bundle: `brew install librsvg`.

```bash
cargo run
```

The first build compiles GPUI and most of its tree from the Zed repository, so
put the kettle on. After that `cargo check` on a warm target takes a couple of
seconds.

## The gate

A change is not finished until all three of these are clean. Not "clean apart
from"; clean.

```bash
cargo check --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace
```

`cargo fmt --check` is enforced in CI as well, and CI additionally builds the
engine for `wasm32-unknown-unknown`, runs `cargo-audit` and `cargo-deny`, and
checks that the version in `Cargo.toml` agrees with the newest entry in
`CHANGELOG.md`.

Please run the gate yourself rather than letting CI find out. Tests here are
written to fail for a stated reason — each one carries a comment saying what
breaks in the product if it goes red — and a new test is expected to do the same.

## Never run a bare `cargo update`

`gpui` and `gpui_platform` come from Zed's git repository and are deliberately
**unpinned in the manifests**, with `Cargo.lock` acting as the pin. This looks
like an oversight and is not one. Adding an explicit `rev` makes Cargo treat our
dependency as a different source from `gpui-component`'s unpinned one, which puts
two incompatible copies of GPUI into the graph; `[patch]` on the same URL is
rejected outright. A plain `cargo update` drags GPUI to whatever the default
branch happens to be that morning and breaks the text input.

Moving GPUI is its own task, done with `cargo update -p gpui@<version> --precise
<sha>` and a full compile sweep afterwards. The reasoning, at length, is in
`crates/ui-components/THIRD_PARTY.md` under "Version coupling". Read it before
touching a `gpui` line.

## How the code is laid out

```
src/
  main.rs, app.rs      entry point and the GPUI bootstrap: fonts, actions, menus
  config.rs            preferences that live outside the vault
  update.rs            the update check, and nothing that could grow into more
  vault.rs             the store: one TOML file per document, on disk, readable
  render.rs            rasterised pages into GPUI images
  resume/, import/     the data model and the five importers
  views/               the screens
crates/dockcv-core     the engine: model, Typst compilation, export. No window.
crates/ui-components   the design system: tokens, type scale, widget facade
crates/dockcv-wasm     the engine again, for the browser demo
assets/                bundled fonts, the icon, the vendored Typst package
```

Two boundaries matter. `dockcv-core` must keep compiling for
`wasm32-unknown-unknown`, which is why it holds no window code and touches the
filesystem in exactly one place. `dockcv-ui-components` must know nothing about
résumés, vaults or Typst: a widget that needs a `ResumeDoc` belongs in
`src/views/` instead.

Rust files stay under roughly 800 lines. When one grows past that it gets split
along a seam rather than trimmed.

## The design system, briefly

**Colour comes from tokens.** One `Theme` of `Hsla` values, held as a GPUI
`Global` and read through `cx.theme()`. No view hard-codes a colour. If you need
a shade the palette does not have, add the token — and remember that metadata
text has to clear 4.5:1 against the surface it sits on, which is checked by a
test rather than by eye.

**Type has three roles and they do not mix.** A serif for display, a sans for
the interface, a mono for data. Dates, counts, paths and keyboard hints are data.
The prose in a CV is not, which is why bullets are set in the sans.

**The widget set is [`gpui-component`](https://github.com/longbridge/gpui-component),
reached through our own facade.** App code imports from `dockcv_ui_components`
and never from `gpui_component` directly, so replacing a widget is one file's
work. Before writing a new component, check whether upstream already has it;
today the only two we own are `Card` and `EmptyState`, because those are the two
it lacks. Buttons come from a small table of named roles in `styles.rs` rather
than from per-site sizes — if a control needs a shape that is not in the table,
the table is what should change.

## Invariants you should not quietly break

- A section owns its variants and exactly one is active. Editing only ever
  touches the active one.
- A preset is a named set of variant selections and holds no content of its own.
- A library block is a copy, and every path that changes copies elsewhere has to
  say so before it does.
- No number reaches a CV bullet unless it traces to something the user wrote.
  The placeholder is `[metric?]`, and inventing a plausible figure is the one
  unforgivable bug in a product like this.
- Any schema change needs a forward migration and a round-trip test in
  `vault.rs`. Files written by an older build must still open.
- No network. The update check in `update.rs` is the single exception, it is off
  by default, and it exists in one module so that it stays one exception.

## Style

Comments explain **why**, not what. The codebase documents decisions and the
things that surprised somebody, not the mechanics of a `for` loop; match the
density of the file you are in. Where a comment refers to `US-nn`, `P-nn` or
`L-nn`, it is citing the product review, its findings, or a limitation shipped
knowingly — planning material that lives outside this repository, with whatever
is still open tracked in issues.

Avoid `unwrap()` on anything that can fail while the app is running.
`Result<_, String>` is the established error type in `vault.rs` and the Typst
engine; stay consistent within a module rather than importing a new error
philosophy into one corner.

New dependencies need a sentence explaining themselves in the pull request.
Several versions in the tree are pinned to unify with GPUI's own — check before
bumping `image`, `smallvec` or anything `typst`.

Commit messages: a short imperative subject, then a body that says what was
wrong and why this is the fix. The history is meant to be readable a year later
by someone who was not here.

## Packaging and releases

`scripts/bundle.sh` turns a release build into `dist/DockCV.app`, complete with
the icon, the `Info.plist` and the licence notices that have to travel with the
binary. Unsigned, it runs on the machine that built it and Gatekeeper stops it
everywhere else; with an Apple Developer ID it can be signed, notarised and
wrapped in a disk image:

```bash
./scripts/bundle.sh --sign "Developer ID Application: Name (TEAMID)" --notarize PROFILE --dmg
```

`scripts/bundle.sh --help` explains where the notarisation credentials come from.

Cutting a release means writing the entry in `CHANGELOG.md` first, then running
`scripts/release.sh <version>`, which refuses to proceed unless the working tree
is clean and the changelog leads with the version being released. It bumps the
manifest, runs the tests and creates the tag; pushing the tag is what starts the
build. GitHub Actions then produces the macOS bundle, the two portable archives
and the small file the update check reads.

## Reporting a bug

Attach the log. It is at `~/Library/Logs/DockCV/dockcv.log`, opens from
**Settings ▸ Storage**, and records what the app did without recording anything
you wrote. Say which build you are on — the version is in **Settings ▸ Storage ▸
About** — and, if the problem involves a specific document, whether the TOML
still opens in a text editor.

## Licence

Contributions are accepted under the same dual MIT / Apache-2.0 licence the
project ships under. By opening a pull request you are agreeing to that.

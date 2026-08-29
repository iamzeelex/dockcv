# DockCV

Local-first résumé workbench. Native desktop app, Rust + [GPUI](https://www.gpui.rs/)
(Zed's GPU UI framework), with an **in-process Typst compiler** driving a live paper
preview. No cloud, no Electron, no web view.

The product bet: the moat is not the model and not the typography — it is *the user's
own data* (diary of wins, block library with provenance, application outcomes).
Everything below serves that. The review that argues the case, the roadmap and the
per-screen specs are planning material and live outside this repository; work in
progress is tracked in issues. Comments here cite them by their shorthand — `US-nn`
for a user story, `P-nn` for a review finding, `L-nn` for a known limitation.

---

## Build & verify

```bash
cargo check --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace
```

```bash
cargo run
```

- Toolchain is **pinned** in `rust-toolchain.toml` (1.95.0). GPUI tracks recent stdlib
  stabilizations; do not downgrade.
- `gpui` and `gpui_platform` come from the Zed git repo **unpinned in the manifests**;
  `Cargo.lock` is the pin (currently rev `1a246ef`). This is deliberate — an explicit
  `rev` makes Cargo see a different source from `gpui-component`'s unpinned one and
  puts two incompatible gpui copies in the graph. Read the "Version coupling" section
  of `crates/ui-components/THIRD_PARTY.md` **before** editing a gpui line. Never run a
  plain `cargo update`: it drags gpui to the default branch and breaks the text input.
  A GPUI bump is its own task, done with
  `cargo update -p gpui@<version> --precise <sha>` plus a full compile sweep.
- First build is slow (all of Zed's GPUI tree). `cargo check` on a warm target is ~2s.
- macOS is the primary target. Linux/Windows are wired through per-target
  `gpui_platform` features and must keep compiling, but are not tested.

**A change is not done until `cargo check --workspace` and `cargo clippy -- -D warnings`
are both clean.** Never report success on a self-assessment; run the commands.

---

## Layout

```
src/
  main.rs           entry point, platform attributes
  app.rs            GPUI bootstrap: run loop, fonts, actions, menus, window
  config.rs         app-level prefs outside the vault (last vault path, …)
  theme.rs          thin re-export of dockcv-ui-components::theme
  vault.rs          File-over-App store: TOML on disk, one file per document
  typst_engine.rs   in-process Typst compile → rasterized Pixels
  render.rs         Pixels → gpui::RenderImage (BGRA swap)
  resume/
    model.rs        ResumeDoc / Versioned<T> / Preset — the data model
    edit.rs         FieldId / ListId addressing + key-event text editing
    template.rs     codegen: Resume → self-contained Typst source
    altacv.rs       AltaCV importer
  views/
    shell.rs        screen router (Welcome/Setup/Gallery/Library/Diary/Applications/Editor)
    sidebar.rs      the nav rail + `Shell::with_rail`, the vault chrome
    root.rs         the résumé editor screen
    root_undo.rs    document-level undo/redo — snapshot stacks + checkpoints
    vault_cache.rs  the vault parsed once per change, not once per frame
    save_status.rs  one banner for every failed vault read or write
    confirm.rs      the alert in front of anything that cannot be undone
crates/ui-components/  reusable widgets + theme tokens (own crate, no app deps)
.research/             read-only reference checkouts — never edit, never import
```

### Screens are tabs, not pages

Gallery, Library, Diary, Applications and Settings are **one window with the rail
always up**; only the main pane changes. `Shell::with_rail` (`views/sidebar.rs`) mounts
that chrome once, around whichever pane is showing.

Two rules follow, and both were violated by screens written before this was settled:

1. A pane inside the chrome renders **itself only** — `flex_1().min_w_0().h_full()`, never
   `size_full()`, and never its own copy of the rail.
2. It draws **no back control** and no 80px traffic-light inset. The rail is the way back
   and sits under the traffic lights already; a second back affordance is two navigations
   for one move.

Outside the chrome, deliberately: Welcome and Setup (no vault yet), the Editor (its own
46px titlebar, editor spec §3) and the Preset Matrix (scoped to one document,
drawn with a `‹ <document> / Presets` breadcrumb).

### Crate boundary

`crates/ui-components` (`dockcv-ui-components`) is the design system: the facade over
`gpui-component`, the Slate palette, the type scale, and the few widgets upstream
lacks. It must **not** know about résumés, vaults, or Typst — if a widget needs a
`ResumeDoc`, it belongs in `src/views/`.

---

## Where the design came from

The first pass was built from an imported Claude Design mockup of thirteen screens —
exact colours, spacing and copy. It did its job and is no longer carried by the
repository: **the application is the reference now.** Read the running app and the
tokens in `crates/ui-components`, not a picture of an older version of it.

Two things survive from that period and still bind. Screens have a settled shape (the
rail, the panes, the editor's own titlebar), described above. And the copy in the
mockup was written with care — where an existing screen's wording looks odd, it is
usually deliberate, so change it as a decision rather than as tidying.

Screens the mockup drew that are still unbuilt: a language axis beside the variant
axis, export naming with a history of what was sent, and the six AI surfaces. The
first two are ordinary work. The third is blocked on a product question, not on
engineering.

---

## Design system rules

### Theme

One `Theme` struct of `Hsla` tokens, held as a GPUI `Global`, read via
`cx.theme()` (the `ActiveTheme` trait). **Never hard-code a color in a view.** If a view
needs a shade that isn't a token, add the token.

Shipping palettes: **Slate dark (default) and Slate light**. The mockup's warm *Clay*
temperature is deliberately deferred — but keep the token layer palette-agnostic so Clay
drops in as one more `Theme::…()` constructor, never as a fork of the views.

Slate tokens (from the mockup specimen): base `#14161a`, raised `#23272e`,
accent `#6f8fd6`, positive `#7fa28f`, paper `#f6f7f9`.

Contrast floor: metadata text (dates, counts, "updated 3w ago") must clear **4.5:1**
against its own surface. The mockup's muted greys do not, on purpose — the review calls
this out as P-16. Fix it in the tokens, not per-view.

### Typography

Three roles, never mixed up:

| Role | Family | Used for |
|---|---|---|
| Display / editorial | serif | screen titles, empty states, section headings |
| Interface | sans | everything else — labels, field values, buttons, prose |
| Data | mono | dates, counts, file paths, keyboard hints, metrics chips |

Mono is for *data only*. Résumé prose (Summary, bullets) is sans — the review's L-05.

Fonts are **bundled and registered in `app.rs::register_fonts`**. Zero network calls at
runtime, ever (US-10). The mockup's Google Fonts `<link>` is a mockup artifact and must
not be reproduced.

### Components

**The widget set is `gpui-component`.** `dockcv-ui-components` is the facade over
it — Button, Badge, Tag, Switch, Checkbox, Tooltip, Kbd, Avatar, Label, Separator,
Icon and, as screens need them, menus, Select, Sheet, Scrollbar, Table. Icons are
[Lucide](https://lucide.dev) (ISC), shipped by `gpui-component-assets`. Rationale,
licences and the theme projection are in `crates/ui-components/THIRD_PARTY.md`.

Three rules follow:

1. **Never import `gpui_component::` from app code.** Everything is re-exported
   from `dockcv_ui_components`, so there is one file to edit if a widget has to
   be replaced. Inside `crates/ui-components` naming it is normal.
2. **Write a new component only when upstream has none.** Today that is `Card`
   and `EmptyState`. Check `.research/gpui-component/crates/ui/src/` first; a
   near-duplicate of an upstream widget is the thing this crate exists to avoid.
3. **Colours reach upstream through `theme/bridge.rs`**, which projects our 22
   tokens onto its 129 fields. Add a token to `theme/mod.rs` first, then route it
   there — a widget that reads an unrouted field falls back to upstream's demo
   palette, silently.

When rule 2 does send you to write one, follow the shape of `card.rs` /
`empty_state.rs`: stateless widget = plain struct + builder methods +
`impl RenderOnce`; stateful widget = an `Entity<XState>` the parent owns, with the
widget taking `&Entity<XState>` in render. Sizes come from upstream's `Sizable`.
Every component gets a doc comment saying what it is and one usage line.

`.research/gpui-component/` is also the reference for *how* GPUI works — read its
`skills/gpui/references/*.md` for element, entity, focus and event patterns.

---

## Data model invariants

- A **section** owns its `Versioned<T>`: N named variants, exactly one `active`.
  Editing only ever mutates the active variant (`edit.rs` addresses through it).
- A **preset** is a *named set of variant selections* — `Vec<(SectionKind, String)>`.
  It has no content of its own. This is the answer to the review's open question 1, and
  the Preset Matrix screen renders exactly this: section × variant.
- A **library block is a copy**, and that answer is now load-bearing rather than
  provisional: a document has to stay a self-contained file that opens on a machine
  which never had your library. What US-03 required, and what shipped, is that every
  path which changes copies elsewhere says so first — `library_link.rs` names the CVs,
  marks the ones that reworded the block, and writes nothing without a click. Do not
  add a silent third behaviour, and do not turn a copy into a reference.
- **Nothing in the model may be a float or an unnamed tuple where a struct fits.**
  TOML is the wire format; field order matters for readability (see the `Versioned`
  comment about `active` before `variants`).
- Never invent a metric. Any number that reaches a résumé bullet must trace to a diary
  entry or a field the user typed (US-14). Placeholder is `[metric?]`.

## Storage

- Vault = a directory. One TOML per document; `library.toml`; `diary.toml`.
- Writes are debounced auto-saves. The user must always be able to see the real path and
  open it in Finder (US-09).
- Every schema change needs a forward migration and a round-trip test in `vault.rs`.

---

## Conventions

- Comments explain **why**, not what. Match the density already in the file — this
  codebase documents intent at module and non-obvious-decision level, not per line.
- No `unwrap()` on anything that can fail at runtime. `Result<_, String>` is the
  established error type in `vault.rs`/`typst_engine.rs`; stay consistent within a module.
- Don't add dependencies without saying why in the PR/summary. `image`, `smallvec` and
  `typst*` versions are pinned to unify with GPUI's own tree — check before bumping.
- Rust files stay under ~800 lines. `shell.rs` and `root.rs` were split in F8; `views/root.rs`
  (1201) and `resume/model.rs` (1142) are the two still over and are scheduled to be split
  further — don't grow them.

## Working style

Opus plans and reviews; granular, well-specified units go to worker agents. A task
worth delegating names the files to touch, the tokens or components to use, what is out
of scope, and the command that proves it worked. If it cannot be written that tightly,
it is not ready to hand over.

`.claude/agents/release-manager.md` is the one agent the repository carries, because
cutting a release is a checklist and a checklist is exactly what should not live in
somebody's head.

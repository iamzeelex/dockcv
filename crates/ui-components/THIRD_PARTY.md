# Third-party UI in `dockcv-ui-components`

DockCV's widget set **is** `gpui-component`. This crate is the facade over it plus
the handful of things it has no answer for. That is a deliberate reversal of an
earlier decision to keep the two apart: half-using an upstream library means two
theme systems, two icon sets and two `Button`s, and the seam costs more than it
saves.

## `gpui-component`

- **Upstream:** <https://github.com/longbridge/gpui-component>
- **Rev:** `6c804fa7acaf0bce4659401821969da2b283dc30`
- **License:** Apache-2.0 (© 2024–2025 Longbridge)
- **Features:** `default-features = false`. Upstream declares no `default` set, so
  tree-sitter and its ~35 grammar crates stay out of the graph.

Used for: the text input (`InputState`), `Button`, `Badge`, `Tag`, `Switch`,
`Checkbox`, `Tooltip`, `Kbd`, `Avatar`, `Label`, `Separator`, `Icon`, and — as
screens need them — menus, `Select`, `Sheet`, `Scrollbar`, `Table`, `Tree`.

Everything is re-exported from `components/mod.rs`, so app code imports from
`dockcv_ui_components` and never names `gpui_component` directly. That is not
about hiding the dependency; it is so there is exactly one file to edit if a
widget ever has to be replaced.

## Lucide icons

The glyph set is [Lucide](https://lucide.dev), shipped embedded by
`gpui-component-assets` and addressed through `IconName`.

- **License:** ISC (© 2022 Lucide Contributors), a permissive licence requiring
  the copyright notice be retained. See <https://lucide.dev/license>.

`assets/icons/` holds five local glyphs, and they come from **two different
places** — worth stating precisely, because the licence follows the provenance:

- **Authored for this project:** the drag handle (`grip`), the import arrow
  (`download`) and the board icon (`kanban`). Lucide carries no equivalent.
  They follow its drawing conventions (24×24, `currentColor`,
  `stroke-width="2"`, round caps and joins) but are **not** derived from Lucide
  artwork.
- **Lucide's own artwork, absent from upstream's curated `IconName`:** `pen` and
  `pencil-sparkles`. Lucide draws both; `gpui-component`'s enum simply does not
  expose them, so they are vendored here as files. They are covered by the same
  ISC licence and attribution as the rest of the set above — this is not a
  separate grant.

All five reach `Icon` through upstream's `IconNamed` trait, the extension point
its docs describe for custom sets. A test asserts none of them duplicates an
upstream icon, so the local set shrinks if `gpui-component` ever exposes one.

`pencil-sparkles` is registered and **deliberately unused**: sparkles read as
"a model produced this" across this audience's tools, so the glyph is reserved
for the AI layer (M5) rather than spent on an ordinary edit control.

`components::icon::Assets` composes both sources, because
`gpui::Application::with_assets` takes exactly one. Note that upstream reports a
missing path as `Err`, not `Ok(None)` — the composite treats that as "try the
other set" rather than propagating it.

## What stays ours

| | Why |
|---|---|
| `Card` | No upstream equivalent. |
| `EmptyState` | No upstream equivalent, and the review (P-13) makes it load-bearing: the first screen a user ever sees must offer an action, not a dotted outline. |
| `Theme` + `typography` | The definition of Slate, and where the WCAG floor is enforced by test. |
| `TextField` | A thin wrapper over `InputState` with our own event vocabulary. |

## The theme is projected, not adopted

Upstream widgets colour themselves from upstream's `Theme` — 129 configurable
fields. `theme/bridge.rs` projects our 22 semantic tokens onto them.

Our `Theme` remains the source of truth: it is where the palette lives and where
`foreground_tokens_meet_wcag_aa` enforces 4.5:1. The bridge only translates. Add a
token there first, then route it in the bridge.

Two things to know before editing the bridge:

- The config is built as **JSON**, upstream's own `themes/*.json` shape. That is
  not a stylistic choice — `ThemeConfigColors` keeps some fields private, so a
  struct literal cannot be written from outside their crate.
- serde **ignores unknown fields**, so a mistyped key would not fail to parse; it
  would silently leave that surface on upstream's demo palette. The test
  `every_key_is_one_upstream_knows` checks every key against the set upstream
  actually serializes. Do not delete it.

## Version coupling — read before touching a manifest

`gpui-component` depends on `gpui` from the Zed repo **without a rev**. Cargo
treats `git = "…/zed"` and `git = "…/zed", rev = "…"` as *different sources*, so
the obvious hardening move — pinning a `rev` in our manifests — silently puts a
**second gpui** in the graph. The symptom is not a version conflict but missing
items, e.g.

```
error[E0432]: unresolved imports `gpui::AssetSource`, `gpui::Result`, `gpui::SharedString`
```

`[patch."https://github.com/zed-industries/zed"]` does not fix it either — Cargo
rejects a patch pointing at the same source.

So: **our gpui entries stay unpinned, and `Cargo.lock` is the pin.** It holds rev
`1a246ef`, the one `gpui-component@6c804fa` builds against (also the first with
`gpui::container_query`, which upstream needs).

To move gpui deliberately:

```bash
cargo update -p gpui@<version> --precise <full-sha>
```

then re-run the full gate and check `grep -c 'name = "gpui"$' Cargo.lock` still
prints `1`. Never a plain `cargo update`.

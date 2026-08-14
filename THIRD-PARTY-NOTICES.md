# Third-party notices

DockCV compiles fonts, a Typst package and an icon set **into its binary**
(`include_bytes!` / `include_str!`), because the app makes no network request at
runtime (US-10). Embedding is redistribution, so the licences below travel with
every build. **This file must be shipped alongside any DockCV binary**, in the
`.app` bundle's `Contents/Resources/` and in any archive offered for download.

DockCV's own source is dual-licensed MIT OR Apache-2.0 — see `LICENSE`.

---

## Fonts embedded by DockCV

All four families are licensed under the **SIL Open Font License, Version 1.1**.
The full licence text is in [`assets/fonts/LICENSE-OFL.txt`](assets/fonts/LICENSE-OFL.txt);
it is one text and it covers all four. The copyright notices differ and are
reproduced here in full, as the licence requires.

| Family | Files | Copyright |
|---|---|---|
| **Geist** | `Geist-{Regular,Medium,SemiBold,Bold}.ttf` | Copyright 2024 The Geist Project Authors (<https://github.com/vercel/geist-font>) |
| **Newsreader** | `Newsreader.ttf`, `Newsreader-Italic.ttf` | Copyright 2020 The Newsreader Project Authors (<http://github.com/productiontype/Newsreader>) |
| **JetBrains Mono** | `JetBrainsMono-{Regular,Bold}.ttf` | Copyright 2020 The JetBrains Mono Project Authors (<https://github.com/JetBrains/JetBrainsMono>) |
| **PT Serif** | `PTSerif-{Regular,Bold}.ttf` | Copyright © 2010 ParaType Ltd., with Reserved Font Names "PT Sans", "PT Serif" and "ParaType" |

Two obligations the OFL puts on us, stated so they are not forgotten:

- **Reserved Font Names.** PT Serif reserves "PT Sans", "PT Serif" and
  "ParaType"; Libertinus (below) reserves "Linux Libertine", "Biolinum" and
  "STIX Fonts". A *modified* version of any of those may not be distributed
  under the reserved name. DockCV ships them byte-for-byte unmodified, which is
  what keeps this simple — if a font is ever subsetted or patched, it must be
  renamed.
- **No standalone sale.** The fonts may not be sold on their own. Bundling them
  inside DockCV is expressly permitted.

Where each family is used: Geist is the interface sans, Newsreader the display
serif, JetBrains Mono the data face (`src/app.rs::register_fonts`); all four are
additionally registered with the Typst compiler so a CV can be *set* in them
(`src/typst_engine.rs::DOCUMENT_FONTS`).

## Fonts embedded by `typst-assets`

Compiling `typst-assets` with its `fonts` feature — which DockCV does, so that
Typst has a default family and maths glyphs — embeds a further set:

| Family | Licence |
|---|---|
| Libertinus Serif | SIL OFL 1.1, © 2012–2024 The Libertinus Project Authors, RFN "Linux Libertine", "Biolinum", "STIX Fonts" |
| New Computer Modern (text + math) | SIL OFL 1.1, © 2019–2024 Antonis Tsolomitis |
| DejaVu Sans Mono | Bitstream Vera Fonts Copyright © 2003 Bitstream, Inc.; DejaVu changes © 2006 Tavmjong Bah |
| Foxit base-14 PDF fonts | see the crate's own NOTICE |

The authoritative, complete text for all of these is the crate's own `NOTICE`,
vendored here as
[`assets/fonts/NOTICE-typst-assets.txt`](assets/fonts/NOTICE-typst-assets.txt).
It is copied into the repository rather than fetched at package time for the
same reason the AltaCV sources are: a licence obligation that depends on a
build step succeeding is an obligation waiting to be missed. Re-copy it from
the crate whenever `typst-assets` is upgraded.

## Typst

- **Typst** (`typst`, `typst-render`, `typst-pdf`, `typst-layout`, `typst-assets`)
  — Apache License 2.0, © The Typst Project Developers.
  <https://github.com/typst/typst>

DockCV embeds the compiler in-process; there is no Typst binary and no package
download.

## AltaCV (vendored Typst package)

- **Package:** `altacv` 1.6.0 · **Licence:** MIT, © 2023 George Honeywood ·
  Typst port by Shane Murphy · <https://github.com/smur89/alta-typst>

The MIT text is at `assets/typst/altacv/LICENSE`. 25 `.typ` sources plus two
data files are vendored into the repository and compiled into the binary with
`include_str!`, so a CV renders with no network and no package cache. Rationale
and the list of what was forked is in `assets/typst/altacv/THIRD_PARTY.md`.

## Widget set and icons

- **`gpui-component`** — Apache License 2.0, © 2024–2025 Longbridge.
  <https://github.com/longbridge/gpui-component>
- **Lucide icons** — ISC, © 2022 Lucide Contributors. <https://lucide.dev/license>
  Shipped embedded by `gpui-component-assets`; two Lucide glyphs absent from
  upstream's enum (`pen`, `pencil-sparkles`) are vendored as files under the
  same grant. Three glyphs (`grip`, `download`, `kanban`) were **authored for
  this project** and are not derived from Lucide artwork.
- **GPUI** (`gpui`, `gpui_platform`) — Apache License 2.0, © Zed Industries.
  <https://github.com/zed-industries/zed>

Details, and the reasoning behind depending on `gpui-component` wholesale, are
in `crates/ui-components/THIRD_PARTY.md`.

## Everything else

The remaining dependencies are ordinary Rust crates under MIT, Apache-2.0 or
both, and are not embedded as assets. A machine-readable inventory of the whole
graph, suitable for auditing this file, comes from:

```bash
cargo install cargo-about && cargo about generate about.hbs
```

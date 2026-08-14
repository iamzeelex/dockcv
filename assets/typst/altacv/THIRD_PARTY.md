# Vendored: AltaCV (Typst)

| | |
|---|---|
| Package | `altacv` **1.6.0** |
| Upstream | <https://github.com/smur89/alta-typst> |
| Licence | MIT — `LICENSE` in this directory, copyright 2023 George Honeywood |
| Author | Shane Murphy (Typst port) |
| Requires | Typst `0.15.0` — the version DockCV compiles with |

## Why the source is here and not fetched

DockCV makes **no network request at runtime** (US-10, a P0 trust promise). A
Typst package it renders through therefore cannot be downloaded, resolved from a
package cache, or vendored at build time — it is copied into the repository and
compiled into the binary with `include_str!`, exactly like the fonts.

`src/resume/altacv_package.rs` is the table of those files, and
`TypstEngine::source` / `::file` serve them. Nothing here is read from disk.

## What is vendored

25 `.typ` sources (`lib.typ`, `internal/`, `sections/`), plus two files the
package **reads** rather than imports:

- `assets/avatar-placeholder.svg`
- `internal/labels-en.toml` — the section headings

The data files are as load-bearing as the code and were missed by a first sweep
that collected `.typ` and `.svg` only. `every_internal_import_resolves_to_a_vendored_file`
now guards the import graph; the data files surface only as compile errors, so
add them by running the compile test after any update.

## Forked — deliberate divergence from upstream

Two files import packages that serve features DockCV does not offer, at the top
level, so they load whether or not the feature is reached. Both are replaced by
a stub that keeps the export's name and arity and says *why* when called — a
document asking for the feature gets a sentence, not a resolution failure.

| File | Dropped | Because |
|---|---|---|
| `internal/json-resume.typ` | `@preview/gairm-import` | Parses a `resume.json` through a schema. DockCV never takes that route: the dictionary is built in Rust from a `ResumeDoc` the type system already validated. |
| `internal/qr.typ` | `@preview/zebra` | QR codes are an AltaCV feature DockCV does not offer. `_check_qr_code` is left exactly as upstream wrote it. |

Every forked line is marked `DOCKCV FORK`. Re-copying either file from upstream
will silently restore the import — check for that marker after any update.

## No external packages remain

`lib.typ` imported three `@preview` packages unconditionally. None is vendored,
and none is needed:

| Package | Was pulled in by | Resolved by |
|---|---|---|
| `@preview/gairm-import` | `internal/json-resume.typ` | forked — DockCV builds the dictionary in Rust |
| `@preview/zebra` | `internal/qr.typ` | forked — QR codes are not a DockCV feature |
| `@preview/fontawesome` | `internal/icons.typ` | forked — icons are drawn from **Lucide** |

`external_package_dependencies_are_known` pins that list at empty.

### Why Lucide rather than FontAwesome

The FontAwesome Typst package carries **no fonts**: it is a generated
name→codepoint table that expects the FA desktop fonts to be installed. Vendoring
it meant ~430 KB of generated `.typ` plus about a megabyte of fonts under a
second licence, for a set the application does not otherwise use.

DockCV already ships Lucide (ISC) and draws its whole interface with it. Using
the same set costs one small SVG per icon and makes the exported document look
like the application rather than like a different product.

Lucide draws with `stroke="currentColor"`, which Typst does not resolve, so the
colour is substituted into the markup and the image is built from bytes. That is
also what lets an icon take the document's own text colour.

**Brand marks are the one loss.** Lucide deliberately has no brand set, so only
`github` is available — from the icon bundle the app already carries. Every other
network falls back to the generic link mark rather than to a wrong logo: an
approximate brand is worse than an honest link.

## Updating

Re-copy from the tag, then run `cargo test --workspace`. Three tests speak for
this directory: the import graph resolves, the external dependency list is
unchanged, and a document compiles as far as it is expected to. Do not edit the
vendored sources — a fork has to be a deliberate decision recorded here, not a
patch that quietly diverges from upstream.

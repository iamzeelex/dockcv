# Changelog

All notable changes to **DockCV** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1] - 2026-08-31

### Added
- **Undo and redo toolbar buttons**, complete with disabled states when no
  history is available and tooltips showing their keyboard shortcuts.

### Fixed
- Choosing **Edit ▸ Undo** or **Edit ▸ Redo** from the system menu bar now
  works when no text field is focused, dispatching document-level undo instead
  of being silently ignored.
- Structural undo and redo history now survives closing and reopening a
  document during the same session.
- Document-level undo and redo restore focus back to the editor window, so
  repeated `Cmd+Z` shortcuts continue working without requiring a manual click
  first.

## [0.2.0] - 2026-08-29

The first release anyone outside the project can use. Everything below happened
after the initial import, and the shape of the application settled in the middle
of it.

### Added
- **Import from what you already have.** PDF, DOCX, a LinkedIn data export, JSON
  Resume, Markdown or plain text. Anything the classifier cannot place is shown
  rather than dropped, and a file that genuinely cannot be read — an image-only
  PDF, most often — says so, says the file is fine, and offers the routes worth
  trying, the last of which always works: writing the CV by hand.
- **Applications, with evidence.** A board from wishlist to closed, cards that
  can be dragged between columns, and a real PDF snapshot taken the moment one is
  sent, so a card opened in November shows what the company actually read in July.
  A funnel drawn from those movements answers which preset gets replies.
- **A diary that survives the year.** Paste a status report, a retro or a
  self-review draft and it splits into candidate wins you accept, edit or discard
  one at a time (US-34). Entries can be marked confidential and are then never
  offered to a CV verbatim (US-36). A win becomes a bullet in a named CV, under a
  named job, in a named variant — and the entry remembers where it went (US-06).
- **A library that answers for itself (US-03).** Blocks report where they are
  already used and which of those copies were reworded. Saving a block that three
  CVs hold names all three, marks the tailored ones, and pushes the change only
  into the ones you tick.
- **Control over the page.** Page size, margins, text scale, leading, typeface,
  date format and per-section layout, all of them properties of the document and
  saved with it. The preview follows the drag.
- **Undo and redo across the document**, not merely inside one field.
- **Update checks, off by default.** `Never`, `When I ask` (the default) or
  `Weekly`. A check looks up one version number and sends nothing about you, not
  even the version being compared, which is compared locally. Nothing is ever
  downloaded or installed on your behalf: a newer version appears as one line in
  the sidebar with a link to the download page.
- **The log, reachable from Settings ▸ Storage** — opened, revealed in Finder, and
  named in full, so a problem can be investigated without knowing where macOS
  keeps application logs.

### Changed
- **The interface was rebuilt on a design system**: one palette of tokens, a type
  scale with three roles that do not mix, a geometry ladder, and buttons drawn
  from a table of named roles rather than sized at each call site. Metadata text
  now clears 4.5:1 against its own surface, which a test enforces.
- **The welcome screen became an entrance** — three sheets of paper settling into
  a stack, then the wordmark, a rule, and one way in.
- **The engine moved into `dockcv-core`**, which compiles without a window and
  builds for `wasm32-unknown-unknown`; `dockcv-wasm` renders CVs in a browser from
  the same code.
- **The vault is parsed once per change** rather than once per frame.

### Fixed
- Importing a malformed PDF no longer takes the application down with it.
- Phone numbers written `+49 30 123456` are recognised, and single-column CSVs —
  `Skills.csv` in many LinkedIn exports — are read at all.
- The layout rail no longer overlaps the page it exists to adjust.
- Text scale applies to the whole document rather than to body text alone.
- The application reported version `0.1.0` while the changelog announced `0.2.0`.
  The release guard now compares the two in both directions.

### Infrastructure
- Releases build `DockCV.app` and a disk image through `scripts/bundle.sh`, with
  the icon, the `Info.plist` and the licence notices that must travel with the
  binary — where a bare executable used to be published. Every platform builds
  `--locked`, and a tag that disagrees with the manifest stops the release before
  anything is compiled.
- CI runs formatting, clippy, the test suite, a cross-platform build check,
  `cargo-audit`, `cargo-deny`, and the browser build of the engine.
- The repository moved to `github.com/iamzeelex/dockcv`.
- Planning material is no longer published, the README is written for people who
  want to use DockCV, and everything a contributor needs lives in
  `CONTRIBUTING.md`.

## [0.1.0] - 2026-08-25

### Added
- Initial import of DockCV 0.1.0 cross-platform desktop application built with GPUI and Typst typesetting engine.

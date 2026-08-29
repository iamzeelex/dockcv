# Changelog

All notable changes to **DockCV** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Reused blocks answer for themselves (US-03)**: saving a library block that other
  CVs hold a copy of now opens a dialog naming exactly which ones, marking those that
  reworded it, and offering to push the new wording into the ones you tick. Copies that
  were tailored start unticked. Library cards report `used in 3 CVs · 1 tailored`.

- **Update checks, off by default**: Settings ▸ General ▸ Updates offers `Never`,
  `When I ask` (the default) and `Weekly`. A check is one request for a static file
  and sends nothing about the user — not even the version being compared, which is
  compared locally. It is made by the system's `curl`, so no HTTP client is compiled
  into the app. Nothing is ever downloaded or installed automatically: a newer version
  appears as one line in the rail with a link that opens the download page.
- **The log is reachable from Settings**: Settings ▸ Storage now opens the log file or
  shows it in Finder, and names the path, so a problem can be looked into without
  knowing where macOS keeps application logs.

### Changed
- **The repository moved** to `github.com/iamzeelex/dockcv`; the update check and its
  download link follow it.
- **README rewritten for people who want to use DockCV** rather than compile it, with
  everything a contributor needs moved to a new `CONTRIBUTING.md`. Planning material
  (the product review, roadmap, per-screen specs and decisions ledger) is no longer
  published; open work belongs in issues.

### Fixed
- **Version reporting**: the workspace manifest said `0.1.0` while the changelog
  announced `0.2.0`, so the app misreported itself in Settings ▸ About. The release
  guard now compares the manifest against the changelog's newest entry in both
  directions instead of merely checking that an entry exists.
- **Releases are an application again**: the macOS release job builds `DockCV.app` and a
  disk image through `scripts/bundle.sh` — with the icon, `Info.plist` and the licence
  notices that must travel with the binary — instead of publishing a bare executable.
  Every platform now builds `--locked`, and a tag that disagrees with the manifest stops
  the release before it builds.

## [0.2.0] - 2026-08-29

### Added
- **JSON Resume Importer**: Added support for importing résumés in JSON Resume standard format (`src/import/engines/json_resume.rs`).
- **UI Design System**: Redesigned UI components, tokenized color/typography scales, spacing ladders, and dark mode elevation (`crates/ui-components`).
- **Applications Tracking & Analytics**: Introduced interactive List view, status board, Sankey funnel diagram, and card movement insights.
- **Library & Diary Enhancements**: Added editable blocks, interactive wins extraction (US-34), confidential win storage (US-36), and win export.
- **Undo / Redo Support**: Added document-level undo/redo state management (`src/views/shell.rs`).
- **WASM Browser Engine**: Added `dockcv-wasm` crate for in-browser Typst rendering.
- **Release Automation**: Added GitHub Actions release workflow (`.github/workflows/release.yml`) and local release helper script (`scripts/release.sh`).

### Changed
- Refactored `dockcv-core` engine logic out of main binary for modular testing and compilation without window dependencies.
- Updated main layout structure into modular section components (`src/views/root_*.rs`).
- Improved asset pipeline with updated SVG app icons.

### Fixed
- Fixed panic on importing malformed PDF documents.
- Fixed document parsing performance and data corruption issue during vault re-reads.
- Fixed layout rail overlapping rendered Typst preview document.
- Fixed text scaling applying uniformly across all document headings and paragraphs.

### Infrastructure & CI
- Added `scripts/release.sh` to automate release checks, version bumping, and git tagging.
- Added version consistency check job to `.github/workflows/ci.yml`.

## [0.1.0] - 2026-08-25

### Added
- Initial import of DockCV 0.1.0 cross-platform desktop application built with GPUI and Typst typesetting engine.

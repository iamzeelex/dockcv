# Changelog

All notable changes to **DockCV** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-08-25

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

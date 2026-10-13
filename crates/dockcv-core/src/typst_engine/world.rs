use std::sync::OnceLock;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

use crate::resume::altacv::package;

use super::TypstEngine;

static GLOBAL_LIBRARY: OnceLock<LazyHash<Library>> = OnceLock::new();

pub(crate) fn global_library() -> &'static LazyHash<Library> {
    GLOBAL_LIBRARY.get_or_init(|| LazyHash::new(Library::builder().build()))
}

/// How many compilation generations comemo keeps before evicting stale cache
/// entries. Small because each keystroke produces a new generation.
pub(crate) const COMEMO_MAX_AGE: usize = 8;

// The `World` is how Typst reaches back for inputs (sources, fonts, files).
// This implementation is deliberately minimal: one in-memory main file, the
// bundled fonts, and no external file or package access.
/// The path Typst is asking for, in the form the vendored table is keyed by.
#[allow(deprecated)]
fn virtual_path_of(id: &FileId) -> String {
    id.vpath()
        .as_rootless_path()
        .to_string_lossy()
        .replace('\\', "/")
}

impl World for TypstEngine {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main_id {
            return Ok(self.source.clone());
        }
        // The vendored AltaCV package (C7). Nothing is read from disk and
        // nothing is fetched: the files are compiled into the binary, so a
        // document that imports the package still renders with no network and
        // no package cache (US-10).
        let path = virtual_path_of(&id);
        if let Some(text) = package::source(&path) {
            return Ok(Source::new(id, text.to_string()));
        }
        Err(FileError::Other(Some(
            format!("`{path}` is not part of this document or the vendored package").into(),
        )))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = virtual_path_of(&id);
        if let Some(data) = package::bytes(&path) {
            return Ok(Bytes::new(data));
        }
        Err(FileError::Other(Some(
            format!("`{path}` is not a vendored asset").into(),
        )))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

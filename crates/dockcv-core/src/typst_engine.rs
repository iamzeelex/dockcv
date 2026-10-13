//! Stage 1 of the preview pipeline: compile a Typst source string into an SVG
//! string, entirely **in-process**.
//!
//! GPUI is native Rust and so is Typst, so there is no process/JS boundary to
//! cross — we embed the `typst` crate directly and let `comemo` handle
//! incremental recompilation. The only thing leaving this module is an SVG
//! string, which keeps stages 3–4 (rasterization) completely decoupled: the
//! SVG source could later come from `tinymist` over a socket without touching
//! the renderer.

use typst::diag::{SourceDiagnostic, Warned};
#[cfg(feature = "raster")]
use typst::layout::Abs;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
#[cfg(feature = "raster")]
use typst::utils::Scalar;
use typst::Library;
#[cfg(feature = "raster")]
use typst::WorldExt;
use typst_layout::PagedDocument;
#[cfg(feature = "raster")]
use typst_render::RenderOptions;

pub mod diagnostics;
pub mod fonts;
pub mod geometry;
pub mod raster;
pub mod world;

#[cfg(test)]
mod tests;

pub use diagnostics::{CompileAttempt, Diagnostic, Severity};
pub use fonts::characters_no_bundled_face_can_set;
pub use geometry::PageGeometry;
#[cfg(feature = "raster")]
pub use raster::Pixels;

use self::diagnostics::{humanize, join_diagnostics};
use self::fonts::global_fonts;
use self::geometry::{MIN_RENDER_SCALE, PAGE_GAP_PT};
use self::world::{global_library, COMEMO_MAX_AGE};

/// An in-process Typst compiler bound to a single in-memory `main.typ`.
pub struct TypstEngine {
    pub(crate) library: LazyHash<Library>,
    pub(crate) book: LazyHash<FontBook>,
    pub(crate) fonts: Vec<Font>,
    pub(crate) main_id: FileId,
    pub(crate) source: Source,
}

impl TypstEngine {
    /// Construct an engine initialized with the default template and fonts.
    pub fn new(initial_source: impl Into<String>) -> Self {
        let (book, fonts) = global_fonts();
        let library = global_library();

        let main_id = FileId::new(RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("main.typ").expect("valid virtual path"),
        ));
        let source = Source::new(main_id, initial_source.into());

        Self {
            library: library.clone(),
            book: book.clone(),
            fonts: fonts.clone(),
            main_id,
            source,
        }
    }

    /// Replace the document text. The next `compile_to_svg` will incrementally
    /// recompile — comemo reuses everything that did not transitively depend on
    /// the changed source.
    pub fn set_source(&mut self, text: String) {
        self.source = Source::new(self.main_id, text);
    }

    /// Compile the current document and rasterize all pages (stacked
    /// vertically) directly to a pixel buffer at `scale` pixels per point.
    ///
    /// This goes straight from Typst frames to pixels via `typst-render` — no
    /// SVG serialization or re-parsing — which is the bulk of the speedup over
    /// the previous `typst-svg` + `resvg` path. Returns a human-readable error
    /// joining compiler diagnostics on failure.
    ///
    /// Alongside the merged pixmap (which is what the preview displays — one
    /// image, not per-page virtualization; see the PDF-architecture notes
    /// §3) this returns [`PageGeometry`] measured from the same laid-out
    /// pages, before that per-page structure is flattened away.
    #[cfg(feature = "raster")]
    pub fn compile_to_pixels(&self, scale: f32) -> Result<(Pixels, PageGeometry), String> {
        let Warned { output, .. } = typst::compile(self);

        let document: PagedDocument = output.map_err(join_diagnostics)?;
        let geometry = PageGeometry::measure(&document);

        let options = RenderOptions {
            pixel_per_pt: Scalar::new(scale.max(MIN_RENDER_SCALE) as f64),
            render_bleed: false,
        };
        // No background fill: filling the whole merged raster white made the
        // inter-page gap indistinguishable from the page itself, so a
        // two-page CV read as one very long sheet with a blank stretch in the
        // middle — the break was there and invisible. Transparent gaps let the
        // preview's canvas show through, which is what makes a page boundary
        // look like one.
        let pixmap = typst_render::render_merged(&document, &options, Abs::pt(PAGE_GAP_PT), None);

        // Age out cache entries so memory does not grow unbounded across edits.
        comemo::evict(COMEMO_MAX_AGE);

        Ok((
            Pixels {
                width: pixmap.width(),
                height: pixmap.height(),
                rgba: pixmap.take(),
            },
            geometry,
        ))
    }

    /// Lay the document out and measure it, without drawing anything.
    ///
    /// For the Preset Matrix, which wants to know how many pages each reading
    /// takes — including readings the person has never opened. Rasterizing N
    /// documents to count their pages would pay for a pixmap per preset and
    /// throw every one away; layout is the part that answers the question.
    ///
    /// Unlike the three methods around it this needs no feature gate: page
    /// geometry comes out of `typst` itself, not out of a renderer.
    pub fn measure(&self) -> Result<PageGeometry, String> {
        let Warned { output, .. } = typst::compile::<PagedDocument>(self);
        let document = output.map_err(join_diagnostics)?;
        let geometry = PageGeometry::measure(&document);
        comemo::evict(COMEMO_MAX_AGE);
        Ok(geometry)
    }

    /// Compile the current document to one SVG per page.
    ///
    /// The browser's output. `typst-render` gives the app a pixmap because
    /// GPUI draws textures; a page in a web page wants to stay sharp when the
    /// visitor zooms, and SVG also skips the pixmap → BGRA conversion that
    /// only exists to feed `RenderImage`.
    ///
    /// One string per page rather than a merged canvas: the app's raster path
    /// merges pages so it can paint one image with a gap drawn between them,
    /// and a document in a web page is better served by real elements the page
    /// can lay out, scroll and style itself.
    #[cfg(feature = "svg")]
    pub fn compile_to_svg(&self) -> Result<Vec<String>, String> {
        let Warned { output, .. } = typst::compile::<PagedDocument>(self);
        let document = output.map_err(join_diagnostics)?;
        let options = typst_svg::SvgOptions::default();
        let pages: Vec<String> = document
            .pages()
            .iter()
            .map(|page| typst_svg::svg(page, &options))
            .collect();
        comemo::evict(COMEMO_MAX_AGE);
        Ok(pages)
    }

    /// Compile the current document to PDF bytes.
    #[cfg(feature = "pdf")]
    pub fn compile_to_pdf(&self) -> Result<Vec<u8>, String> {
        let Warned { output, .. } = typst::compile(self);
        let document = output.map_err(join_diagnostics)?;
        typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default()).map_err(join_diagnostics)
    }

    /// The same PDF, refused unless it conforms to **PDF/UA-1**.
    ///
    /// Not what the app exports — it is a second opinion, and a free one.
    /// UA-1's rules are written for a screen reader, and a screen reader wants
    /// what a CV parser wants: a heading tree that does not skip a level, a
    /// document title, every figure carrying text, and nothing meaningful left
    /// in an untagged artifact. Typst validates all of that at export and
    /// refuses the file with a reason rather than writing something subtly
    /// wrong, so a test that calls this is a standards body reviewing the
    /// template for us on every commit.
    ///
    /// Errors are the validator's own words. See `src/ats/conformance.rs`.
    #[cfg(feature = "pdf")]
    pub fn compile_to_pdf_ua1(&self) -> Result<Vec<u8>, String> {
        let Warned { output, .. } = typst::compile(self);
        let document = output.map_err(join_diagnostics)?;
        let standards = typst_pdf::PdfStandards::new(&[typst_pdf::PdfStandard::Ua_1])
            .map_err(|e| format!("PDF/UA-1 is not a standard this build can ask for: {e:?}"))?;
        let options = typst_pdf::PdfOptions {
            standards,
            ..Default::default()
        };
        typst_pdf::pdf(&document, &options).map_err(join_diagnostics)
    }

    /// Compile and rasterize like [`Self::compile_to_pixels`], but return
    /// every diagnostic instead of collapsing them into one error string —
    /// see [`CompileAttempt`]. Duplicates that method's rasterization step
    /// rather than sharing it, so `compile_to_pixels` and its three tests
    /// stay exactly as they were.
    #[cfg(feature = "raster")]
    pub fn compile_with_diagnostics(&self, scale: f32) -> CompileAttempt {
        let Warned { output, warnings } = typst::compile::<PagedDocument>(self);

        let mut diagnostics: Vec<Diagnostic> = warnings.iter().map(|d| self.translate(d)).collect();

        let result = match output {
            Ok(document) => {
                let geometry = PageGeometry::measure(&document);
                let options = RenderOptions {
                    pixel_per_pt: Scalar::new(scale.max(MIN_RENDER_SCALE) as f64),
                    render_bleed: false,
                };
                // Transparent gaps, same as `compile_to_pixels` — this is the
                // path the editor preview actually renders through.
                let pixmap =
                    typst_render::render_merged(&document, &options, Abs::pt(PAGE_GAP_PT), None);
                Ok((
                    Pixels {
                        width: pixmap.width(),
                        height: pixmap.height(),
                        rgba: pixmap.take(),
                    },
                    geometry,
                ))
            }
            Err(errors) => {
                diagnostics.extend(errors.iter().map(|d| self.translate(d)));
                Err(())
            }
        };

        // Age out cache entries so memory does not grow unbounded across edits.
        comemo::evict(COMEMO_MAX_AGE);

        CompileAttempt {
            result,
            diagnostics,
        }
    }

    /// Turn one raw `SourceDiagnostic` into a [`Diagnostic`]: humanize the
    /// message, and resolve the span to a byte offset via `WorldExt::range`
    /// (blanket-implemented for any `World`, which `Self` is) when Typst can
    /// give one.
    #[cfg(feature = "raster")]
    fn translate(&self, diag: &SourceDiagnostic) -> Diagnostic {
        Diagnostic {
            severity: diag.severity.into(),
            message: humanize(&diag.message),
            source_offset: self.range(diag.span).map(|range| range.start),
        }
    }
}

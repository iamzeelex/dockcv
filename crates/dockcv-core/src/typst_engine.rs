//! Stage 1 of the preview pipeline: compile a Typst source string into an SVG
//! string, entirely **in-process**.
//!
//! GPUI is native Rust and so is Typst, so there is no process/JS boundary to
//! cross — we embed the `typst` crate directly and let `comemo` handle
//! incremental recompilation. The only thing leaving this module is an SVG
//! string, which keeps stages 3–4 (rasterization) completely decoupled: the
//! SVG source could later come from `tinymist` over a socket without touching
//! the renderer.

use std::sync::OnceLock;
use typst::diag::{FileError, FileResult, Severity as TypstSeverity, SourceDiagnostic, Warned};
use typst::foundations::{Bytes, Datetime, Duration};
#[cfg(feature = "raster")]
use typst::layout::{Abs, Frame, FrameItem, Transform};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};

use crate::resume::altacv_package;
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
#[cfg(feature = "raster")]
use typst::utils::Scalar;
use typst::{Library, LibraryExt, World};
// Resolving a diagnostic's span to a source offset — the editor's
// section-attribution path, and nothing the browser reports.
#[cfg(feature = "raster")]
use typst::WorldExt;
use typst_layout::PagedDocument;
#[cfg(feature = "raster")]
use typst_render::RenderOptions;

static GLOBAL_FONTS_AND_BOOK: OnceLock<(LazyHash<FontBook>, Vec<Font>)> = OnceLock::new();
static GLOBAL_LIBRARY: OnceLock<LazyHash<Library>> = OnceLock::new();

fn global_fonts() -> &'static (LazyHash<FontBook>, Vec<Font>) {
    GLOBAL_FONTS_AND_BOOK.get_or_init(|| {
        let mut fonts = Vec::new();
        let mut load = |data: &'static [u8]| {
            let bytes = Bytes::new(data);
            let mut index = 0;
            while let Some(font) = Font::new(bytes.clone(), index) {
                fonts.push(font);
                index += 1;
            }
        };
        // Typst's fallback pack. Behind a feature because it is 9.2 MB and a
        // browser has no use for three maths faces — see this crate's
        // `Cargo.toml`. The document faces below always load.
        #[cfg(feature = "fallback-fonts")]
        for data in typst_assets::fonts() {
            load(data);
        }
        for data in DOCUMENT_FONTS {
            load(data);
        }
        #[cfg(feature = "libertinus")]
        for data in LIBERTINUS {
            load(data);
        }
        (LazyHash::new(FontBook::from_fonts(&fonts)), fonts)
    })
}

/// The browser's faces: the same families, subsetted.
///
/// Selected by **target**, not by a Cargo feature, and that is deliberate.
/// Features unify across a workspace build, so a feature that swapped the font
/// list would swap it for the desktop app too — which is exactly the bug
/// `every_offered_font_is_registered_and_changes_the_render` caught the last
/// time this was tried. A `cfg(target_arch)` cannot leak: nothing else in the
/// workspace compiles to wasm.
///
/// The trade is 3574 KB of faces down to 557 KB — the largest single thing a
/// visitor waits for, since fonts are already-compressed bytes that Brotli
/// cannot help with. What it costs is coverage beyond the repertoire in
/// `scripts/subset-fonts.sh`; `fonts_for_the_browser_cover_what_a_cv_can_contain`
/// is what keeps that honest.
#[cfg(target_arch = "wasm32")]
const DOCUMENT_FONTS: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/web/Geist-Regular.subset.ttf"),
    include_bytes!("../../../assets/fonts/web/Geist-Bold.subset.ttf"),
];

/// Libertinus Serif, for builds without `typst-assets`' font pack.
///
/// `DocumentFont` defaults to this family, so a build that dropped the pack
/// and did not put it back would render the default face as whatever Typst
/// fell back to — silently, which is the whole complaint in L-11. Three faces,
/// not six: regular, bold and italic are what a CV sets.
#[cfg(all(feature = "libertinus", not(target_arch = "wasm32")))]
const LIBERTINUS: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/web/LibertinusSerif-Regular.otf"),
    include_bytes!("../../../assets/fonts/web/LibertinusSerif-Bold.otf"),
    include_bytes!("../../../assets/fonts/web/LibertinusSerif-Italic.otf"),
];

/// The same three, subsetted, for the browser. See `DOCUMENT_FONTS` above for
/// why this is chosen by target rather than by feature.
#[cfg(all(feature = "libertinus", target_arch = "wasm32"))]
const LIBERTINUS: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/web/LibertinusSerif-Regular.subset.otf"),
    include_bytes!("../../../assets/fonts/web/LibertinusSerif-Bold.subset.otf"),
    include_bytes!("../../../assets/fonts/web/LibertinusSerif-Italic.subset.otf"),
];

fn global_library() -> &'static LazyHash<Library> {
    GLOBAL_LIBRARY.get_or_init(|| LazyHash::new(Library::builder().build()))
}

/// Faces registered with the Typst compiler on top of `typst-assets`, so a
/// résumé can be set in a family the user recognises. Bundled, never fetched
/// (US-10) — the same bytes the UI already carries.
#[cfg(not(target_arch = "wasm32"))]
const DOCUMENT_FONTS: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../../../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../../../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../../../assets/fonts/Geist-Bold.ttf"),
    include_bytes!("../../../assets/fonts/Newsreader.ttf"),
    include_bytes!("../../../assets/fonts/Newsreader-Italic.ttf"),
    include_bytes!("../../../assets/fonts/PTSerif-Regular.ttf"),
    include_bytes!("../../../assets/fonts/PTSerif-Bold.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf"),
];

/// Gap inserted between pages in the merged raster, in typographic points.
///
/// Wide enough to read as a break between two sheets rather than as slack at
/// the bottom of one. The gap is left **transparent** (see `compile_to_pixels`)
/// so the preview's canvas shows through it; the pages themselves are opaque
/// because the template sets `#set page(fill: white)`.
#[cfg(feature = "raster")]
const PAGE_GAP_PT: f64 = 28.0;

/// Floor for `pixels_per_pt`, guarding only against a zero or negative scale
/// producing a degenerate pixmap.
///
/// It was `1.0`, which was not a guard but an accident: gallery thumbnails ask
/// for `THUMB_SCALE = 0.5` and were silently rasterized at full size — a whole
/// A4 page each, four times the intended pixels, cached in memory and never
/// evicted. The editor's own scale is clamped to `1.0` at its low end by
/// `Root::crisp_scale`, so nothing that wants a sharp page is affected by
/// lowering this.
#[cfg(feature = "raster")]
const MIN_RENDER_SCALE: f32 = 0.05;

/// A rasterized document: RGBA-premultiplied pixels plus dimensions.
#[cfg(feature = "raster")]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    /// RGBA, premultiplied alpha (tiny-skia order).
    pub rgba: Vec<u8>,
}

/// Layout facts read directly off the compiled document's page frames — never
/// estimated from source text (US-08, review P-07: a wrong number sends the
/// user trimming the wrong bullet, so an honest smaller answer beats a
/// fabricated precise one).
///
/// Typst 0.15 gives us, per page, a [`typst_layout::Page`] whose `frame` is
/// fixed to the full page size regardless of how much of it is used — pages
/// don't shrink-wrap their content, so page count alone cannot say how full
/// the last page is. What *is* available is the frame's tree of positioned
/// items (`Frame::items`), the same data `typst-render` paints from, which we
/// walk to find how far down the last page real content reaches. There is no
/// notion of "N lines over" to read off the layout — Typst does not expose a
/// line count, only positions and sizes — so overflow is reported in points,
/// the unit the layout itself is expressed in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeometry {
    /// How many pages the document laid out into.
    pub page_count: usize,
    /// The page height, in points. The template applies one `#set page` rule
    /// for the whole document, so every page shares this height; we read it
    /// off the first page.
    pub page_height_pt: f64,
    /// How far down the *last* page's frame actual content reaches, in
    /// points, measured from the top of the frame (so margin is already
    /// included, matching what the preview shows).
    pub last_page_used_pt: f64,
    /// Where content *starts* on the last page, in points from the frame top.
    ///
    /// The top margin lives between those two numbers, and it is not
    /// overflow: a page's margin is not something the user can trim. Counting
    /// it inflated `overflow_pt` by a whole margin's worth — at 14 mm and a
    /// ~16 pt line that is two and a half lines of pure paper reported as
    /// content that does not fit.
    pub last_page_content_top_pt: f64,
    /// Points of content beyond what a single page holds. `0.0` when the
    /// document fits on one page. Every page but the last is a full page of
    /// content (that is why Typst broke there); only the last page's fill is
    /// uncertain, so this is `(page_count - 1) * page_height + last_page_used
    /// - page_height`, clamped at zero.
    pub overflow_pt: f64,
    /// The document's own line advance, in points — the vertical distance
    /// between consecutive text baselines, **measured** from the laid-out
    /// first page rather than derived from the leading we set.
    ///
    /// This is what turns `overflow_pt` into the design's "+N lines over the
    /// page". Computing it from `#set par(leading:)` would be a guess: Typst
    /// adds leading to the *font's* own line height, which varies per family,
    /// so the arithmetic would quietly disagree with the page it describes.
    /// `None` when the page holds too little text to measure (nothing to
    /// average), in which case the UI must say something it can back up
    /// instead of inventing a count.
    pub line_advance_pt: Option<f64>,
}

impl PageGeometry {
    /// Measure a compiled document's pages. `document.pages()` and each
    /// page's `frame` are exactly what `typst-render` and `typst-pdf` paint
    /// from, so this reads the same layout the user sees, not a re-derived
    /// approximation of it.
    #[cfg(feature = "raster")]
    fn measure(document: &PagedDocument) -> Self {
        let pages = document.pages();
        let page_count = pages.len();
        let page_height_pt = pages
            .first()
            .map(|p| p.frame.height().to_pt())
            .unwrap_or(0.0);
        let last_page_used_pt = pages
            .last()
            .map(|p| frame_content_bottom(&p.frame).to_pt())
            .unwrap_or(0.0);

        // Only the *content* that spilled counts, never the paper around it:
        // for every page after the first, the distance from its first item to
        // its last.
        let overflow_pt: f64 = pages
            .iter()
            .skip(1)
            .map(|p| {
                let top = frame_content_top(&p.frame).to_pt();
                (frame_content_bottom(&p.frame).to_pt() - top).max(0.0)
            })
            .sum();
        let last_page_content_top_pt = pages
            .last()
            .map(|p| frame_content_top(&p.frame).to_pt())
            .unwrap_or(0.0);

        Self {
            page_count,
            page_height_pt,
            last_page_used_pt,
            last_page_content_top_pt,
            overflow_pt,
            line_advance_pt: pages.first().and_then(|p| measure_line_advance(&p.frame)),
        }
    }
}

/// The lowest y-coordinate reached by any item in `frame`, measured from the
/// frame's own top edge. This walks the same positioned-item tree
/// `typst-render` paints, so it is a measurement of the actual layout, not an
/// estimate from source text.
///
/// Nested groups (the frames blocks, paragraphs, grids and lists produce) are
/// assumed to carry an identity or pure-translation transform — true for
/// every layout our template produces, since nothing in it rotates, scales or
/// skews content. A transformed group instead falls back to its own declared
/// frame height, which in practice still bounds its content.
#[cfg(feature = "raster")]
fn frame_content_bottom(frame: &Frame) -> Abs {
    let mut bottom = Abs::zero();
    for (pos, item) in frame.items() {
        // A frame item can report an extent that is not a finite number —
        // observed on a dense Skills section, where the measurement came back
        // `inf` and would have reached the overflow chip as "+inf lines over".
        // Which item does it is Typst's business; that this function only ever
        // returns a real measurement is ours.
        if !pos.y.to_pt().is_finite() {
            continue;
        }
        let extent = match item {
            FrameItem::Group(group) => {
                if group.transform == Transform::identity() {
                    frame_content_bottom(&group.frame)
                } else {
                    group.frame.height()
                }
            }
            FrameItem::Text(text) => text.bbox().max.y.max(Abs::zero()),
            FrameItem::Shape(shape, _) => shape.bbox(true).max.y.max(Abs::zero()),
            FrameItem::Image(_, size, _) | FrameItem::Link(_, size) => size.y,
            FrameItem::Tag(_) => continue, // no visual extent
        };
        if !extent.to_pt().is_finite() {
            continue;
        }
        bottom = bottom.max(pos.y + extent);
    }
    bottom
}

/// The highest y-coordinate any item in `frame` starts at, measured from the
/// frame's top edge — i.e. where content begins under the top margin. Mirrors
/// [`frame_content_bottom`], including its identity/translation assumption.
#[cfg(feature = "raster")]
fn frame_content_top(frame: &Frame) -> Abs {
    let mut top: Option<Abs> = None;
    for (pos, item) in frame.items() {
        let candidate = match item {
            FrameItem::Group(group) if group.transform == Transform::identity() => {
                pos.y + frame_content_top(&group.frame)
            }
            FrameItem::Tag(_) => continue,
            FrameItem::Group(_)
            | FrameItem::Text(_)
            | FrameItem::Shape(..)
            | FrameItem::Image(..)
            | FrameItem::Link(..) => pos.y,
        };
        // Same guard as `frame_content_bottom`: a non-finite candidate would
        // win every `min` and make the whole page look like it starts nowhere.
        if !candidate.to_pt().is_finite() {
            continue;
        }
        top = Some(top.map_or(candidate, |t: Abs| t.min(candidate)));
    }
    top.unwrap_or(Abs::zero())
}

/// The document's typical baseline-to-baseline distance, in points.
///
/// Collects every text baseline on the page, takes the gaps between
/// consecutive ones, and returns the **median** gap. Median rather than mean
/// because a CV page is mostly body lines punctuated by much larger jumps —
/// section headings, entry spacing — and a mean would be dragged upward by
/// them, understating how many lines the overflow is worth.
///
/// `None` when there are fewer than three baselines: below that there is no
/// typical anything, and the caller must say what it can measure instead of
/// reporting a line count from one sample.
#[cfg(feature = "raster")]
fn measure_line_advance(frame: &Frame) -> Option<f64> {
    let mut baselines: Vec<f64> = Vec::new();
    collect_baselines(frame, Abs::zero(), &mut baselines);
    if baselines.len() < 3 {
        return None;
    }

    baselines.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    baselines.dedup_by(|a, b| (*a - *b).abs() < 0.01);

    let mut gaps: Vec<f64> = baselines
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|gap| *gap > 0.5)
        .collect();
    if gaps.is_empty() {
        return None;
    }
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(gaps[gaps.len() / 2])
}

/// Absolute y of every text baseline in `frame`, following the same
/// identity/translation assumption `frame_content_bottom` documents.
#[cfg(feature = "raster")]
fn collect_baselines(frame: &Frame, offset: Abs, out: &mut Vec<f64>) {
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) if group.transform == Transform::identity() => {
                collect_baselines(&group.frame, offset + pos.y, out);
            }
            FrameItem::Text(_) => out.push((offset + pos.y).to_pt()),
            _ => {}
        }
    }
}

/// How many compilation generations comemo keeps before evicting stale cache
/// entries. Small because each keystroke produces a new generation.
const COMEMO_MAX_AGE: usize = 8;

/// Join compiler diagnostics into one human-readable error string.
fn join_diagnostics<I: IntoIterator<Item = SourceDiagnostic>>(diagnostics: I) -> String {
    diagnostics
        .into_iter()
        .map(|d| d.message.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Severity Typst itself assigned a diagnostic, carried through unchanged so
/// a warning is never surfaced to the user as if it were a failure (US-07,
/// review P-06: today's `join_diagnostics` flattens both into one string —
/// this is what replaces it for callers that need to tell them apart).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

impl From<TypstSeverity> for Severity {
    fn from(severity: TypstSeverity) -> Self {
        match severity {
            TypstSeverity::Error => Severity::Error,
            TypstSeverity::Warning => Severity::Warning,
        }
    }
}

/// One compiler diagnostic, translated into a short clause a person can
/// read, in place of `SourceDiagnostic`'s own message/span/hint triplet.
///
/// `message` deliberately carries **no** location, span or line/column
/// notation — "not a raw span dump" (review P-06) — and no leading
/// "Couldn't compile" or trailing punctuation, since only the caller knows
/// whether it can name the résumé section responsible
/// (`resume::diagnostics::describe` composes the final sentence).
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// Byte offset into the compiled source this diagnostic's span starts
    /// at, when Typst can resolve one (it cannot for a "detached" span — a
    /// diagnostic about the compiler's own behavior rather than a
    /// particular piece of source text, e.g. the missing-font warning).
    pub source_offset: Option<usize>,
}

/// Translate a raw Typst diagnostic message into a clause a résumé author —
/// who has never seen Typst — can read without translation.
///
/// Only the free-text fields the template embeds as raw Typst markup
/// (`summary`, `highlights` — see `resume::template`'s module doc) can ever
/// carry user-typed syntax that breaks compilation; every plain field is
/// quoted into a string literal first, so it cannot. That bounds the
/// realistic failure modes to markup mistakes (an unbalanced bracket, quote
/// or the odd stray `#`), which is what this covers explicitly. Anything
/// else falls back to Typst's own wording — still not a span dump, just not
/// independently translated.
#[cfg(feature = "raster")]
fn humanize(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("unclosed delimiter") {
        "a bracket, parenthesis or quote here is never closed".to_string()
    } else if lower.contains("unexpected closing bracket") {
        "there's an extra `]` with no opening bracket to match it".to_string()
    } else if lower.contains("string not terminated") || lower.contains("unterminated string") {
        "a quote (\") here is never closed".to_string()
    } else if lower.starts_with("unknown variable") {
        "there's a `#` here that Typst reads as code, but what follows isn't something it recognizes"
            .to_string()
    } else {
        raw.trim().to_string()
    }
}

/// Everything one compile attempt produced: the rendered pixels/geometry on
/// success (the same shape [`TypstEngine::compile_to_pixels`] returns), and
/// every diagnostic Typst emitted along the way, translated via
/// [`Diagnostic`]. `compile_to_pixels` itself is unchanged by this — it
/// keeps testing the rasterization/geometry path in isolation; this is the
/// entry point the editor's preview should call instead, since it needs the
/// warnings `compile_to_pixels` throws away to answer "compiling / ready /
/// error" (US-07) rather than just "did it produce pixels."
#[cfg(feature = "raster")]
pub struct CompileAttempt {
    /// `Ok` exactly when the compile succeeded. `Err(())` carries no
    /// message of its own — the *why* is always in `diagnostics`, which is
    /// guaranteed to hold at least one `Severity::Error` entry in that case.
    pub result: Result<(Pixels, PageGeometry), ()>,
    /// Every diagnostic from this attempt, in emission order: any warnings
    /// first, then any compile errors. A non-empty list does not mean
    /// failure — check `result`, or filter on `Severity` directly.
    pub diagnostics: Vec<Diagnostic>,
}

/// An in-process Typst compiler bound to a single in-memory `main.typ`.
pub struct TypstEngine {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main_id: FileId,
    source: Source,
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
    /// image, not per-page virtualization; see `docs/research/gpui-pdf-architecture.md`
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
        if let Some(text) = altacv_package::source(&path) {
            return Ok(Source::new(id, text.to_string()));
        }
        Err(FileError::Other(Some(
            format!("`{path}` is not part of this document or the vendored package").into(),
        )))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = virtual_path_of(&id);
        if let Some(data) = altacv_package::bytes(&path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A short document, well within one A4 page at the default size.
    const ONE_PAGE_SOURCE: &str = "= Hello\nJust one short paragraph of text.";

    /// A tiny page size with enough repeated paragraphs to force pagination.
    /// Deliberately not derived from any real résumé template — this test is
    /// about `TypstEngine` in general, not about `resume::template`.
    fn overflowing_source() -> String {
        let mut source = String::from("#set page(width: 200pt, height: 150pt, margin: 8pt)\n");
        for i in 0..40 {
            source.push_str(&format!(
                "Paragraph number {i} of filler text to push layout past a single small page.\n\n"
            ));
        }
        source
    }

    /// A document broken the way a résumé author could actually break one —
    /// a stray `]` typed into free-text markup (a summary or bullet) closes
    /// the surrounding content block early. Real Typst wording for this is
    /// "unexpected closing bracket"; `humanize` is what stands between that
    /// and the person editing the document.
    const BROKEN_SOURCE: &str = "A summary with a stray ] bracket that breaks the markup.";

    #[test]
    fn broken_document_yields_a_human_readable_error() {
        let engine = TypstEngine::new(BROKEN_SOURCE);
        let attempt = engine.compile_with_diagnostics(1.0);

        assert!(
            attempt.result.is_err(),
            "expected this document to fail to compile"
        );
        let errors: Vec<&Diagnostic> = attempt
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            !errors.is_empty(),
            "a failed compile must report at least one error"
        );

        for error in &errors {
            // Never a raw span/location dump: no "3:5" style position, no
            // mention of the word "span".
            assert!(
                !error.message.contains(':') || error.message.matches(':').count() <= 1,
                "message reads like a location dump: {:?}",
                error.message
            );
            assert!(!error.message.to_ascii_lowercase().contains("span"));
            assert!(!error.message.is_empty());
        }
    }

    #[test]
    fn warning_does_not_present_as_an_error() {
        // Triggers a genuine Typst warning (unknown font family) with no
        // compile error at all.
        let src = "#set text(font: \"Totally Nonexistent Font XYZ\")\nHello, world.";
        let engine = TypstEngine::new(src);
        let attempt = engine.compile_with_diagnostics(1.0);

        assert!(
            attempt.result.is_ok(),
            "a missing font is a warning, not a failure"
        );
        assert!(
            attempt
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Warning),
            "expected at least one warning diagnostic"
        );
        assert!(
            !attempt
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error),
            "a successful compile must not report any Severity::Error diagnostic"
        );
    }

    #[test]
    fn successful_compile_reports_no_diagnostics() {
        let engine = TypstEngine::new(ONE_PAGE_SOURCE);
        let attempt = engine.compile_with_diagnostics(1.0);

        assert!(attempt.result.is_ok());
        assert!(
            attempt.diagnostics.is_empty(),
            "a clean compile should not manufacture diagnostics: {:?}",
            attempt.diagnostics
        );
    }

    #[test]
    fn one_page_document_reports_one_page() {
        let engine = TypstEngine::new(ONE_PAGE_SOURCE);
        let (_pixels, geometry) = engine.compile_to_pixels(1.0).expect("compiles");

        assert_eq!(geometry.page_count, 1);
        assert_eq!(geometry.overflow_pt, 0.0);
        // Content plainly does not reach the bottom of a full A4 page.
        assert!(geometry.last_page_used_pt < geometry.page_height_pt);
    }

    #[test]
    fn overflowing_document_reports_more_pages_and_positive_overflow() {
        let engine = TypstEngine::new(overflowing_source());
        let (_pixels, geometry) = engine.compile_to_pixels(1.0).expect("compiles");

        assert!(
            geometry.page_count > 1,
            "expected pagination, got {} page(s)",
            geometry.page_count
        );
        assert!(
            geometry.overflow_pt > 0.0,
            "expected positive overflow, got {}pt",
            geometry.overflow_pt
        );
        // Sanity: reported overflow cannot exceed the content stacked across
        // every page but the first.
        assert!(geometry.overflow_pt < geometry.page_height_pt * geometry.page_count as f64);
    }

    #[test]
    fn overflow_measurement_is_stable_for_the_same_input() {
        let source = overflowing_source();
        let a = TypstEngine::new(source.clone())
            .compile_to_pixels(1.0)
            .expect("compiles")
            .1;
        let b = TypstEngine::new(source)
            .compile_to_pixels(1.0)
            .expect("compiles")
            .1;

        assert_eq!(a, b, "measuring the same source twice should agree exactly");
    }
}

#[cfg(test)]
mod line_advance_tests {
    use super::*;
    use crate::resume::model::{Resume, ResumeDoc, Work};
    use crate::resume::template;
    /// The measurement that turns points into the design's "+N lines".
    /// A real document must yield a plausible advance, and the number has to
    /// track the document's own text scale — if it did not, it would be a
    /// constant wearing a measurement's clothes.
    #[test]
    fn the_line_advance_is_measured_from_the_page_and_follows_text_scale() {
        let mut resume = Resume::default();
        resume.basics.name = "Sofiia Medvedenko".into();
        resume.work = (0..6)
            .map(|i| Work {
                position: format!("Engineer {i}"),
                name: "Acme".into(),
                highlights: vec!["Did a thing that took a whole line of text".into(); 3],
                ..Default::default()
            })
            .collect();

        let mut doc = ResumeDoc::from_resume(resume, "Base");
        let mut engine = TypstEngine::new(String::new());

        engine.set_source(template::generate_with_layout(&doc.compose(), &doc.layout));
        let (_, small) = engine.compile_to_pixels(1.0).expect("compiles");
        let base = small.line_advance_pt.expect("a page of text is measurable");
        // 10pt body text: anything outside this band means we are measuring
        // something other than consecutive body lines.
        assert!(
            (8.0..24.0).contains(&base),
            "implausible line advance {base}pt"
        );

        doc.layout.text_scale_pct = 150;
        engine.set_source(template::generate_with_layout(&doc.compose(), &doc.layout));
        let (_, large) = engine.compile_to_pixels(1.0).expect("compiles");
        let scaled = large.line_advance_pt.expect("still measurable");
        assert!(
            scaled > base * 1.1,
            "advance must grow with text scale: {base} -> {scaled}"
        );
    }

    /// Overflow is **content**, never paper. A page-2 that holds three short
    /// lines must report roughly three lines' worth — not those plus the top
    /// margin, which is what measuring from the frame's top edge did. The
    /// margin is not something a user can trim, so counting it made the number
    /// overstate the problem by a fixed amount on every document.
    #[test]
    fn the_top_margin_is_not_counted_as_overflow() {
        let mut resume = Resume::default();
        resume.basics.name = "Sofiia Medvedenko".into();
        // Just enough to push a little content onto a second page.
        resume.work = (0..26)
            .map(|i| Work {
                position: format!("Engineer {i}"),
                name: "Acme".into(),
                highlights: vec!["A bullet long enough to take a full line".into(); 2],
                ..Default::default()
            })
            .collect();

        let doc = ResumeDoc::from_resume(resume, "Base");
        let mut engine = TypstEngine::new(String::new());
        engine.set_source(template::generate_with_layout(&doc.compose(), &doc.layout));
        let (_, geometry) = engine.compile_to_pixels(1.0).expect("compiles");

        assert!(geometry.page_count > 1, "fixture must overflow");
        assert!(
            geometry.last_page_content_top_pt > 0.0,
            "there is a top margin above the spilled content"
        );
        // The old formula was `last_page_used_pt`, margin included. Overflow
        // must be strictly less than that by at least the margin.
        assert!(
            geometry.overflow_pt < geometry.last_page_used_pt,
            "overflow {} must exclude the {}pt of margin above it",
            geometry.overflow_pt,
            geometry.last_page_content_top_pt
        );
        let content_height = geometry.last_page_used_pt - geometry.last_page_content_top_pt;
        assert!((geometry.overflow_pt - content_height).abs() < 0.5);
    }

    /// An empty document has nothing to average, and must say so rather than
    /// returning a made-up number the overflow chip would then quote.
    #[test]
    fn a_document_with_no_text_reports_no_measurement() {
        let doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let mut engine = TypstEngine::new(String::new());
        engine.set_source(template::generate_with_layout(&doc.compose(), &doc.layout));
        let (_, geometry) = engine.compile_to_pixels(1.0).expect("compiles");
        assert_eq!(geometry.line_advance_pt, None);
    }
}

#[cfg(test)]
mod page_break_tests {
    use super::*;
    use crate::resume::model::{Resume, ResumeDoc, Work};
    use crate::resume::template;

    /// A two-page CV must *look* like two pages.
    ///
    /// `render_merged` was filling the whole raster white, so the gap between
    /// pages was white on white: the break existed and was invisible, and a
    /// long CV read as one sheet with a blank stretch in the middle. The pages
    /// are opaque because the template sets `#set page(fill: white)`; the gap
    /// is transparent so the preview's canvas shows through it.
    #[test]
    fn the_gap_between_pages_is_transparent_and_the_pages_are_not() {
        let mut resume = Resume::default();
        resume.basics.name = "Sofiia Medvedenko".into();
        // Enough content to certainly spill onto a second page.
        resume.work = (0..40)
            .map(|i| Work {
                position: format!("Senior Engineer {i}"),
                name: "Acme Corp".into(),
                highlights: vec!["A bullet point long enough to occupy a line".into(); 3],
                ..Default::default()
            })
            .collect();

        let doc = ResumeDoc::from_resume(resume, "Base");
        let mut engine = TypstEngine::new(String::new());
        engine.set_source(template::generate_with_layout(&doc.compose(), &doc.layout));

        let (pixels, geometry) = engine.compile_to_pixels(1.0).expect("compiles");
        assert!(geometry.page_count > 1, "fixture must overflow one page");

        // Walk down the middle column: an opaque page, then a transparent
        // band, then an opaque page again.
        let x = pixels.width / 2;
        let alpha_at = |y: u32| pixels.rgba[((y * pixels.width + x) * 4 + 3) as usize];
        let transparent_rows = (0..pixels.height).filter(|y| alpha_at(*y) == 0).count();
        let opaque_rows = (0..pixels.height).filter(|y| alpha_at(*y) == 255).count();

        assert!(
            transparent_rows > 0,
            "the inter-page gap must be transparent, or the break is invisible"
        );
        assert!(
            opaque_rows > transparent_rows,
            "pages must stay opaque: {opaque_rows} opaque vs {transparent_rows} transparent rows"
        );
    }
}

#[cfg(test)]
mod font_tests {
    use super::*;
    use crate::resume::model::{DocumentFont, LayoutSettings, Resume};
    use crate::resume::template;

    /// Every family the picker offers must be one the compiler can actually
    /// resolve. A missing face does not fail the compile — Typst quietly falls
    /// back — so a picker entry with no font behind it would look like it
    /// worked and change nothing.
    #[test]
    fn every_offered_font_is_registered_and_changes_the_render() {
        let engine = TypstEngine::new(String::new());
        let families: Vec<String> = engine
            .book
            .families()
            .map(|(n, _)| n.to_lowercase())
            .collect();

        for font in DocumentFont::ALL {
            assert!(
                families.iter().any(|f| f == &font.family().to_lowercase()),
                "`{}` is offered by the picker but not registered with the compiler",
                font.family()
            );
        }

        // …and choosing one has to reach the page. Two families with very
        // different metrics must not rasterize to the same bytes.
        let mut resume = Resume::default();
        resume.basics.name = "Seán Ó Murchú".into();
        resume.basics.summary = "Backend engineer with eight years of experience.".into();

        let render = |font: DocumentFont| {
            let layout = LayoutSettings {
                font,
                ..Default::default()
            };
            let mut engine = TypstEngine::new(String::new());
            engine.set_source(template::generate_with_layout(&resume, &layout));
            engine.compile_to_pixels(1.0).expect("compiles").0.rgba
        };

        assert_ne!(
            render(DocumentFont::LibertinusSerif),
            render(DocumentFont::Geist),
            "switching serif to sans must change what is drawn"
        );
    }

    /// The default must stay the family every existing document was already
    /// rendered in, or adding this feature would silently reflow every CV in
    /// every vault.
    #[test]
    fn the_default_font_is_the_one_documents_already_used() {
        assert_eq!(DocumentFont::default(), DocumentFont::LibertinusSerif);
    }

    /// Which offered families can actually set a CV in Cyrillic.
    ///
    /// This matters because Typst answers a missing **glyph** the same way it
    /// answers a missing family: silently, by falling back to another font. A
    /// CV written in Ukrainian and set in a Latin-only face does not fail and
    /// does not warn — it comes out in a face the author did not choose, with
    /// the name and the body in visibly different type.
    ///
    /// The test pins the fact rather than asserting a policy: it fails if a
    /// family's coverage *changes*, which is the thing that would otherwise
    /// happen quietly during a font bump. What to do about Newsreader — drop
    /// it from the picker, mark it in the UI, or accept it — is L-11 in
    /// `docs/OPEN.md`.
    /// The browser ships subsetted faces (`scripts/subset-fonts.sh`) — 557 KB
    /// instead of 3574 KB, which is the largest saving available in a module a
    /// visitor downloads. A subset trades weight for coverage, and Typst
    /// answers a missing glyph the way it answers a missing family: silently,
    /// by falling back. That is L-11 again, so it gets a guard.
    ///
    /// Read from the files rather than from `DOCUMENT_FONTS`, because this
    /// test runs on the host where that constant is the *full* set. What is
    /// asserted is the repertoire the subsetting script promises, plus every
    /// non-ASCII character the generator and the vendored AltaCV package can
    /// actually emit — collected by grepping both, not by imagination.
    #[test]
    fn fonts_for_the_browser_cover_what_a_cv_can_contain() {
        const SUBSETS: &[(&str, &str)] = &[
            (
                "assets/fonts/web/LibertinusSerif-Regular.subset.otf",
                "assets/fonts/web/LibertinusSerif-Regular.otf",
            ),
            (
                "assets/fonts/web/LibertinusSerif-Bold.subset.otf",
                "assets/fonts/web/LibertinusSerif-Bold.otf",
            ),
            (
                "assets/fonts/web/LibertinusSerif-Italic.subset.otf",
                "assets/fonts/web/LibertinusSerif-Italic.otf",
            ),
            (
                "assets/fonts/web/Geist-Regular.subset.ttf",
                "assets/fonts/Geist-Regular.ttf",
            ),
            (
                "assets/fonts/web/Geist-Bold.subset.ttf",
                "assets/fonts/Geist-Bold.ttf",
            ),
        ];

        // Latin with the accents a European CV carries; Ukrainian, which needs
        // the four letters Russian does not have; and the punctuation the
        // template and the package emit — `–` `—` `…` `•` `·` `§` `©` `→` `≈`.
        const REPERTOIRE: &str = "AZaz09 éòäûüñçßÅØ ĀŁŐ \
                                  Софія Медведенко ЄІЇҐ ЁЪЫЭ \
                                  –—…•·§©→≈×±≤≥ €₴ №";

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");

        let load = |relative: &str| {
            let bytes = std::fs::read(root.join(relative)).unwrap_or_else(|e| {
                panic!("{relative} is missing ({e}) — run scripts/subset-fonts.sh")
            });
            Font::new(Bytes::new(bytes), 0)
                .unwrap_or_else(|| panic!("{relative} did not parse as a font"))
        };

        for (subset, original) in SUBSETS {
            let (small, full) = (load(subset), load(original));

            // Against the *original*, not against a list of characters I
            // believe a font ought to have: Libertinus carries no ₴, and a
            // test that demanded one would be asserting my imagination rather
            // than the only thing that can regress — subsetting dropping
            // something the face actually had.
            let lost: Vec<char> = REPERTOIRE
                .chars()
                .filter(|c| !c.is_whitespace())
                .filter(|c| full.info().coverage.contains(*c as u32))
                .filter(|c| !small.info().coverage.contains(*c as u32))
                .collect();

            assert!(
                lost.is_empty(),
                "{subset} lost {lost:?} that {original} had — widen RANGES in \
                 scripts/subset-fonts.sh and re-run it, or the browser will \
                 silently set them in another face"
            );
        }
    }

    /// The overflow chip does arithmetic on these numbers, so a non-finite
    /// one reaches the user as "+inf lines over". It happened: a dense Skills
    /// section measured `inf`, because some frame item reported an extent that
    /// was not a real number and the walk trusted it.
    ///
    /// Asserted across arrangements that stress the layout differently, since
    /// the one that broke looked like all the others from the outside.
    #[test]
    fn page_geometry_is_always_a_real_measurement() {
        use crate::resume::model::{
            CategoryMark, HeadingLayout, HeadingStyle, LayoutSettings, Resume, RowSpacing,
            SkillGroup, SkillSeparator, SkillsLayout, SkillsStyle,
        };
        use crate::resume::template;

        let resume = Resume {
            skills: (0..8)
                .map(|i| SkillGroup {
                    name: format!("A rather long category name {i}"),
                    keywords: (0..9).map(|k| format!("Technology {i}-{k}")).collect(),
                })
                .collect(),
            ..Resume::default()
        };

        // Heading styles are in the sweep because two of them put a `line`
        // into the flow, and a rule is exactly the kind of zero-height,
        // full-width element the geometry walk has come back `inf` from
        // before.
        for style in SkillsStyle::ALL {
            for spacing in RowSpacing::ALL {
                for heading in HeadingStyle::ALL {
                    let layout = LayoutSettings {
                        skills: SkillsLayout {
                            style,
                            separator: SkillSeparator::Rule,
                            mark: CategoryMark::Dash,
                            spacing,
                            bullets: true,
                        },
                        headings: HeadingLayout {
                            style: heading,
                            ..Default::default()
                        },
                        ..LayoutSettings::default()
                    };
                    let engine = TypstEngine::new(template::generate_with_layout(&resume, &layout));
                    let (_, geometry) = engine.compile_to_pixels(1.0).expect("compiles");

                    for (what, value) in [
                        ("last_page_used_pt", geometry.last_page_used_pt),
                        ("page_height_pt", geometry.page_height_pt),
                        ("overflow_pt", geometry.overflow_pt),
                        (
                            "last_page_content_top_pt",
                            geometry.last_page_content_top_pt,
                        ),
                    ] {
                        assert!(
                            value.is_finite(),
                            "{what} was {value} for {} / {} / {}",
                            style.label(),
                            spacing.label(),
                            heading.label()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn which_document_fonts_can_set_a_cv_in_cyrillic() {
        // Ukrainian needs the four letters Russian does not have; a face that
        // stops at the Russian alphabet is still wrong for this persona.
        const SAMPLE: &str = "Софія Медведенко ЄІЇҐ";

        let engine = TypstEngine::new(String::new());
        let covers = |font: DocumentFont| -> bool {
            let family = font.family().to_lowercase();
            engine
                .fonts
                .iter()
                .filter(|f| f.info().family.to_lowercase() == family)
                .any(|f| {
                    SAMPLE
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .all(|c| f.info().coverage.contains(c as u32))
                })
        };

        for font in DocumentFont::ALL {
            let expected = !matches!(font, DocumentFont::Newsreader);
            assert_eq!(
                covers(font),
                expected,
                "Cyrillic coverage of `{}` changed — update this test and L-11 \
                 in docs/OPEN.md, do not just flip the expectation",
                font.label()
            );
        }

        // The point of recording it: there *is* a serif that works, so the
        // limitation is a picker problem and not a missing-font problem.
        assert!(covers(DocumentFont::PtSerif));
        assert!(covers(DocumentFont::LibertinusSerif));
    }
}

#[cfg(test)]
mod vendored_package_tests {
    use super::*;

    /// C7 step 1, complete: a document that imports the vendored package
    /// compiles in-process, with no network and no package cache.
    ///
    /// Three of AltaCV's external dependencies were resolved by forking the
    /// files that pulled them (THIRD_PARTY.md): `gairm-import` and `zebra`
    /// served features DockCV does not offer, and FontAwesome was replaced by
    /// the Lucide set the app already ships.
    #[test]
    fn the_vendored_package_compiles_with_no_external_dependencies() {
        let engine = TypstEngine::new(
            "#import \"altacv/lib.typ\": alta\n\
             #alta((basics: (name: \"Test\", email: \"a@b.co\", url: \"https://b.co\")))\n",
        );
        let attempt = engine.compile_with_diagnostics(1.0);
        assert!(
            attempt.result.is_ok(),
            "vendored AltaCV did not compile: {:#?}",
            attempt.diagnostics
        );
    }

    /// Compiling is not drawing. The Lucide icons are substituted into their
    /// SVG markup at render time (`internal/icons.typ`), and a wrong colour
    /// string or a missing file would compile perfectly and put no ink on the
    /// page — so the proof has to be the raster.
    #[test]
    fn the_contact_icons_actually_put_ink_on_the_page() {
        let ink_of = |source: &str| {
            let (pixels, _) = TypstEngine::new(source)
                .compile_to_pixels(1.0)
                .expect("the sample compiles");
            pixels
                .rgba
                .chunks_exact(4)
                .filter(|px| px[0] < 200 || px[1] < 200 || px[2] < 200)
                .count()
        };

        let bare = ink_of("#import \"altacv/lib.typ\": alta\n#alta((basics: (name: \"Test\")))\n");
        let with_icons = ink_of(
            "#import \"altacv/lib.typ\": alta\n\
             #alta((basics: (name: \"Test\", email: \"a@b.co\", phone: \"1\")))\n",
        );
        assert!(
            with_icons > bare,
            "the contact bar drew no more ink than an empty one: {bare} vs {with_icons}"
        );
    }
}

use typst::layout::{Abs, Frame, FrameItem, Transform};
use typst_layout::PagedDocument;

/// Gap inserted between pages in the merged raster, in typographic points.
///
/// Wide enough to read as a break between two sheets rather than as slack at
/// the bottom of one. The gap is left **transparent** (see `compile_to_pixels`)
/// so the preview's canvas shows through it; the pages themselves are opaque
/// because the template sets `#set page(fill: white)`.
#[cfg(feature = "raster")]
pub(crate) const PAGE_GAP_PT: f64 = 28.0;

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
pub(crate) const MIN_RENDER_SCALE: f32 = 0.05;

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
    /// The height of one full column of text, in points — the first page's
    /// content extent, top item to bottom item.
    ///
    /// The denominator for "how much of a page does the spill fill". Dividing
    /// `overflow_pt` by `page_height_pt` instead mixes two units: the overflow
    /// is content height, the page height is the whole sheet including its
    /// margins, and the answer comes out too small by however much paper
    /// surrounds the column. The first page is the right ruler because a
    /// document that spilled at all has a full first page — Typst broke it
    /// precisely because the next block did not fit.
    ///
    /// Zero for a document with nothing on its first page, which is the one
    /// case a caller must not divide by.
    pub column_pt: f64,
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
    pub(crate) fn measure(document: &PagedDocument) -> Self {
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
        let column_pt = pages
            .first()
            .map(|p| {
                let top = frame_content_top(&p.frame).to_pt();
                (frame_content_bottom(&p.frame).to_pt() - top).max(0.0)
            })
            .unwrap_or(0.0);

        Self {
            page_count,
            page_height_pt,
            last_page_used_pt,
            last_page_content_top_pt,
            overflow_pt,
            column_pt,
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

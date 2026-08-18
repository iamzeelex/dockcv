//! The final pipeline stage: turn a rasterized pixel buffer into a GPUI image.
//!
//! `typst-render` produces RGBA with premultiplied alpha; GPUI's `RenderImage`
//! wants **BGRA**, so we swap the R and B channels in place and wrap the buffer
//! as a `gpui::RenderImage` for the `img()` element. Because the document is
//! rendered onto an opaque white page, alpha is 1.0 everywhere and premultiplied
//! equals straight — a plain channel swap is correct.

use std::sync::Arc;

use gpui::RenderImage;
use image::{Frame, RgbaImage};
use smallvec::smallvec;

use crate::typst_engine::Pixels;

/// A rasterized document page ready for display.
#[derive(Clone)]
pub struct Rendered {
    /// The GPU-uploadable image (BGRA).
    pub image: Arc<RenderImage>,
    /// Display width in logical pixels (raster size divided by `scale`).
    pub width: f32,
    /// Display height in logical pixels.
    pub height: f32,
}

/// Convert a rasterized `Pixels` buffer (rendered at `scale`) into a
/// `gpui::RenderImage`.
pub fn pixels_to_render_image(mut pixels: Pixels, scale: f32) -> Result<Rendered, String> {
    // Matches `typst_engine::MIN_RENDER_SCALE`: this divides the raster back
    // into logical pixels, so it has to use the same number the raster was
    // produced with or a thumbnail reports the wrong display size.
    let scale = scale.max(0.05);

    // RGBA (premultiplied) -> BGRA.
    for pixel in pixels.rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    let rgba = RgbaImage::from_raw(pixels.width, pixels.height, pixels.rgba)
        .ok_or("rasterized buffer size did not match dimensions")?;
    let image = Arc::new(RenderImage::new(smallvec![Frame::new(rgba)]));

    Ok(Rendered {
        image,
        width: pixels.width as f32 / scale,
        height: pixels.height as f32 / scale,
    })
}

#[cfg(test)]
mod tests {
    use dockcv_core::resume::model::{Resume, ResumeDoc};
    use dockcv_core::resume::template;
    use dockcv_core::typst_engine::TypstEngine;

    /// The GPUI half of what used to be an engine test: `dockcv-core` proves
    /// the compile produces pixels, and this proves those pixels become an
    /// image GPUI will draw. The split is the crate boundary — core knows
    /// nothing about `RenderImage`, and this is the only place that should.
    #[test]
    fn a_compiled_page_becomes_an_image_gpui_can_draw() {
        // Any real document will do, and an empty one keeps the fixture out
        // of it — the sample résumé is core's own `cfg(test)` data and has no
        // business crossing a crate boundary to prove a channel swap.
        let doc = ResumeDoc::from_resume(Resume::default(), "Base");
        let engine = TypstEngine::new(template::generate_for(&doc));
        let (pixels, _) = engine.compile_to_pixels(2.0).expect("compiles");

        let rendered = super::pixels_to_render_image(pixels, 2.0).expect("rasterizes");
        assert!(rendered.width > 0.0 && rendered.height > 0.0);
    }
}

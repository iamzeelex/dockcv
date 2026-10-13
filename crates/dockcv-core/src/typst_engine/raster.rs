/// A rasterized document: RGBA-premultiplied pixels plus dimensions.
#[cfg(feature = "raster")]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    /// RGBA, premultiplied alpha (tiny-skia order).
    pub rgba: Vec<u8>,
}

//! Safe Rust wrapper over the C++ engine's C ABI (`engine/include/aibs/engine.h`).
//!
//! This crate is the only place in the Rust code that contains `unsafe`
//! calls into the engine.

use std::ffi::CStr;
use std::fmt;
use std::os::raw::c_char;

#[repr(C)]
struct AibsImage {
    data: *mut u8,
    width: u32,
    height: u32,
    source_width: u32,
    source_height: u32,
}

const AIBS_OK: i32 = 0;
const AIBS_ERR_INVALID_ARGUMENT: i32 = 1;
const AIBS_ERR_DECODE_FAILED: i32 = 2;
const AIBS_ERR_OUT_OF_MEMORY: i32 = 3;

extern "C" {
    fn aibs_engine_version() -> *const c_char;
    fn aibs_decode_rgba8(
        bytes: *const u8,
        len: usize,
        max_dimension: u32,
        out: *mut AibsImage,
    ) -> i32;
    fn aibs_image_free(image: *mut AibsImage);
    fn aibs_source_open(bytes: *const u8, len: usize, out: *mut *mut AibsSourceRaw) -> i32;
    fn aibs_source_size(source: *const AibsSourceRaw, width: *mut u32, height: *mut u32);
    fn aibs_source_render(
        source: *const AibsSourceRaw,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        out_width: u32,
        out_height: u32,
        out: *mut AibsImage,
    ) -> i32;
    fn aibs_source_free(source: *mut AibsSourceRaw);
}

/// Opaque engine type behind `aibs_source*`.
#[repr(C)]
struct AibsSourceRaw {
    _private: [u8; 0],
}

fn status_to_result(status: i32) -> Result<(), EngineError> {
    match status {
        AIBS_OK => Ok(()),
        AIBS_ERR_INVALID_ARGUMENT => Err(EngineError::InvalidArgument),
        AIBS_ERR_DECODE_FAILED => Err(EngineError::DecodeFailed),
        AIBS_ERR_OUT_OF_MEMORY => Err(EngineError::OutOfMemory),
        _ => Err(EngineError::Internal),
    }
}

/// Copy an engine image into Rust memory and release the engine's copy.
///
/// # Safety
/// `raw` must have been filled by a successful engine call.
unsafe fn take_image(mut raw: AibsImage) -> RgbaImage {
    let len = raw.width as usize * raw.height as usize * 4;
    let pixels = std::slice::from_raw_parts(raw.data, len).to_vec();
    let image = RgbaImage {
        pixels,
        width: raw.width,
        height: raw.height,
        source_width: raw.source_width,
        source_height: raw.source_height,
    };
    aibs_image_free(&mut raw);
    image
}

fn empty_image() -> AibsImage {
    AibsImage {
        data: std::ptr::null_mut(),
        width: 0,
        height: 0,
        source_width: 0,
        source_height: 0,
    }
}

/// A decoded image held at full resolution by the engine. Any region can be
/// rendered at any size; see [`Source::render`].
pub struct Source {
    raw: *mut AibsSourceRaw,
    width: u32,
    height: u32,
}

// SAFETY: the engine never modifies a source after `aibs_source_open`
// returns, and documents concurrent renders from one source as safe.
unsafe impl Send for Source {}
unsafe impl Sync for Source {}

impl Source {
    /// Decode encoded image bytes at full resolution.
    pub fn open(bytes: &[u8]) -> Result<Source, EngineError> {
        let mut raw: *mut AibsSourceRaw = std::ptr::null_mut();
        // SAFETY: `bytes` is valid for its length and only read; `raw` is writable.
        status_to_result(unsafe { aibs_source_open(bytes.as_ptr(), bytes.len(), &mut raw) })?;
        let (mut width, mut height) = (0u32, 0u32);
        // SAFETY: `raw` is a valid source returned above.
        unsafe { aibs_source_size(raw, &mut width, &mut height) };
        Ok(Source { raw, width, height })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Render the region at (`x`, `y`) of size `w` x `h` source pixels into
    /// an `out_width` x `out_height` image. Parts outside the source are
    /// transparent.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        out_width: u32,
        out_height: u32,
    ) -> Result<RgbaImage, EngineError> {
        let mut raw = empty_image();
        // SAFETY: `self.raw` is valid for the lifetime of `self`; `raw` is writable.
        status_to_result(unsafe {
            aibs_source_render(self.raw, x, y, w, h, out_width, out_height, &mut raw)
        })?;
        // SAFETY: the render succeeded, so `raw` holds an engine image.
        Ok(unsafe { take_image(raw) })
    }

    /// Render the whole image so that its longer side is at most `max_side`.
    pub fn render_fit(&self, max_side: u32) -> Result<RgbaImage, EngineError> {
        let longer = self.width.max(self.height).max(1);
        let scale = (max_side as f64 / longer as f64).min(1.0);
        let out_w = ((self.width as f64 * scale).round() as u32).max(1);
        let out_h = ((self.height as f64 * scale).round() as u32).max(1);
        self.render(0.0, 0.0, self.width as f64, self.height as f64, out_w, out_h)
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `aibs_source_open` and is freed once.
        unsafe { aibs_source_free(self.raw) };
    }
}

/// Why the engine refused a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineError {
    InvalidArgument,
    DecodeFailed,
    OutOfMemory,
    Internal,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            EngineError::InvalidArgument => "invalid argument passed to the engine",
            EngineError::DecodeFailed => "the data is not a readable image",
            EngineError::OutOfMemory => "not enough memory to decode the image",
            EngineError::Internal => "internal engine error",
        };
        f.write_str(text)
    }
}

impl std::error::Error for EngineError {}

/// An 8-bit RGBA image copied out of the engine into Rust-owned memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    /// `width * height * 4` bytes, row-major.
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Size of the image in the file, before any downscale.
    pub source_width: u32,
    pub source_height: u32,
}

/// Version string reported by the linked engine.
pub fn engine_version() -> String {
    // SAFETY: the engine returns a pointer to a static NUL-terminated string.
    unsafe { CStr::from_ptr(aibs_engine_version()) }
        .to_string_lossy()
        .into_owned()
}

/// Decode an encoded image (JPEG, PNG, TIFF, WebP) to RGBA8.
///
/// `max_dimension` of zero keeps the full size; otherwise the longer side is
/// limited to it. The input bytes are never modified.
pub fn decode_rgba8(bytes: &[u8], max_dimension: u32) -> Result<RgbaImage, EngineError> {
    let mut raw = empty_image();
    // SAFETY: `bytes` is valid for `len` bytes for the duration of the call
    // and the engine only reads it; `raw` is a valid, writable struct.
    status_to_result(unsafe { aibs_decode_rgba8(bytes.as_ptr(), bytes.len(), max_dimension, &mut raw) })?;
    // SAFETY: the decode succeeded, so `raw` holds an engine image.
    Ok(unsafe { take_image(raw) })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = include_bytes!("../../../tests/fixtures/gradient_64x48.png");
    const CORRUPTED: &[u8] = include_bytes!("../../../tests/fixtures/corrupted.jpg");

    #[test]
    fn reports_a_version() {
        assert!(!engine_version().is_empty());
    }

    #[test]
    fn decodes_png_with_exact_pixels() {
        let image = decode_rgba8(PNG, 0).unwrap();
        assert_eq!((image.width, image.height), (64, 48));
        assert_eq!(image.pixels.len(), 64 * 48 * 4);
        // Fixture: r = x * 4, g = y * 5, b = 128.
        assert_eq!(&image.pixels[0..4], &[0, 0, 128, 255]);
        let offset = (10 * 64 + 20) * 4;
        assert_eq!(&image.pixels[offset..offset + 4], &[80, 50, 128, 255]);
    }

    #[test]
    fn downscales_to_max_dimension() {
        let image = decode_rgba8(PNG, 32).unwrap();
        assert_eq!((image.width, image.height), (32, 24));
        assert_eq!((image.source_width, image.source_height), (64, 48));
    }

    #[test]
    fn rejects_corrupted_data() {
        assert_eq!(decode_rgba8(CORRUPTED, 0), Err(EngineError::DecodeFailed));
    }

    #[test]
    fn rejects_empty_input() {
        assert_eq!(decode_rgba8(&[], 0), Err(EngineError::InvalidArgument));
    }

    #[test]
    fn source_renders_regions() {
        let source = Source::open(PNG).unwrap();
        assert_eq!((source.width(), source.height()), (64, 48));
        let full = source.render(0.0, 0.0, 64.0, 48.0, 64, 48).unwrap();
        assert_eq!(full.pixels, decode_rgba8(PNG, 0).unwrap().pixels);
        let zoomed = source.render(10.0, 5.0, 8.0, 6.0, 16, 12).unwrap();
        // Output (0, 0) and (1, 1) both show source pixel (10, 5).
        assert_eq!(&zoomed.pixels[0..4], &[40, 25, 128, 255]);
        let p = (16 + 1) * 4;
        assert_eq!(&zoomed.pixels[p..p + 4], &[40, 25, 128, 255]);
    }

    #[test]
    fn source_fit_keeps_aspect_ratio_and_never_enlarges() {
        let source = Source::open(PNG).unwrap();
        let small = source.render_fit(32).unwrap();
        assert_eq!((small.width, small.height), (32, 24));
        let same = source.render_fit(4096).unwrap();
        assert_eq!((same.width, same.height), (64, 48));
    }

    #[test]
    fn source_rejects_bad_input() {
        assert!(matches!(Source::open(CORRUPTED), Err(EngineError::DecodeFailed)));
        let source = Source::open(PNG).unwrap();
        assert!(matches!(source.render(0.0, 0.0, 0.0, 1.0, 1, 1), Err(EngineError::InvalidArgument)));
    }

    #[test]
    fn source_can_be_shared_between_threads() {
        let source = std::sync::Arc::new(Source::open(PNG).unwrap());
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let source = source.clone();
                std::thread::spawn(move || source.render(i as f64, 0.0, 16.0, 16.0, 32, 32).unwrap())
            })
            .collect();
        for handle in handles {
            assert_eq!(handle.join().unwrap().width, 32);
        }
    }
}

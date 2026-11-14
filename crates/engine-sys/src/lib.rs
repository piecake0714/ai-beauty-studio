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
    let mut raw = AibsImage {
        data: std::ptr::null_mut(),
        width: 0,
        height: 0,
        source_width: 0,
        source_height: 0,
    };
    // SAFETY: `bytes` is valid for `len` bytes for the duration of the call
    // and the engine only reads it; `raw` is a valid, writable struct.
    let status = unsafe { aibs_decode_rgba8(bytes.as_ptr(), bytes.len(), max_dimension, &mut raw) };
    match status {
        AIBS_OK => {}
        AIBS_ERR_INVALID_ARGUMENT => return Err(EngineError::InvalidArgument),
        AIBS_ERR_DECODE_FAILED => return Err(EngineError::DecodeFailed),
        AIBS_ERR_OUT_OF_MEMORY => return Err(EngineError::OutOfMemory),
        _ => return Err(EngineError::Internal),
    }

    let len = raw.width as usize * raw.height as usize * 4;
    // SAFETY: on AIBS_OK the engine guarantees `data` points to
    // `width * height * 4` initialized bytes that stay valid until freed.
    let pixels = unsafe { std::slice::from_raw_parts(raw.data, len) }.to_vec();
    let image = RgbaImage {
        pixels,
        width: raw.width,
        height: raw.height,
        source_width: raw.source_width,
        source_height: raw.source_height,
    };
    // SAFETY: `raw` was filled by the engine and is released exactly once.
    unsafe { aibs_image_free(&mut raw) };
    Ok(image)
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
}

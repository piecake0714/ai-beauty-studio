//! Exporting the edited image as JPEG or PNG.
//!
//! The image is rendered at full (or reduced) resolution with every edit,
//! encoded, and written without ever replacing the original photo.

use std::path::Path;

use engine_sys::{ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::files;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Jpeg,
    Png,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub format: ExportFormat,
    /// JPEG quality 1..=100 (ignored for PNG).
    pub quality: u32,
    /// Longest side in pixels; None keeps the original size. Images are
    /// never enlarged.
    #[serde(default)]
    pub long_side: Option<u32>,
}

/// Why an export failed. `code()` is the stable identifier for the UI.
#[derive(Debug)]
pub enum ExportError {
    /// The target is the original photo (or another protected file).
    WouldOverwriteOriginal,
    InvalidOptions,
    EncodeFailed,
    WriteFailed(std::io::Error),
}

impl ExportError {
    pub fn code(&self) -> &'static str {
        match self {
            ExportError::WouldOverwriteOriginal => "export_would_overwrite_original",
            ExportError::InvalidOptions => "export_invalid_options",
            ExportError::EncodeFailed => "export_encode_failed",
            ExportError::WriteFailed(_) => "export_write_failed",
        }
    }
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::WouldOverwriteOriginal => f.write_str("exporting there would replace the original photo"),
            ExportError::InvalidOptions => f.write_str("invalid export options"),
            ExportError::EncodeFailed => f.write_str("the image could not be encoded"),
            ExportError::WriteFailed(e) => write!(f, "the file could not be written: {e}"),
        }
    }
}

impl std::error::Error for ExportError {}

/// Output size for an image of `width` x `height`.
pub fn output_size(width: u32, height: u32, long_side: Option<u32>) -> (u32, u32) {
    let longer = width.max(height).max(1);
    match long_side {
        Some(target) if target > 0 && target < longer => {
            let scale = target as f64 / longer as f64;
            (
                ((width as f64 * scale).round() as u32).max(1),
                ((height as f64 * scale).round() as u32).max(1),
            )
        }
        _ => (width, height),
    }
}

/// True when the image has transparent pixels (so JPEG would lose them).
pub fn has_transparency(image: &RgbaImage) -> bool {
    image.pixels.chunks_exact(4).any(|p| p[3] < 255)
}

/// Encode a rendered image. JPEG has no transparency: transparent areas
/// are placed on white.
pub fn encode(image: &RgbaImage, options: &ExportOptions) -> Result<Vec<u8>, ExportError> {
    match options.format {
        ExportFormat::Png => engine_sys::encode(&image.pixels, image.width, image.height, 4, ImageFormat::Png)
            .map_err(|_| ExportError::EncodeFailed),
        ExportFormat::Jpeg => {
            if !(1..=100).contains(&options.quality) {
                return Err(ExportError::InvalidOptions);
            }
            let rgb: Vec<u8> = image
                .pixels
                .chunks_exact(4)
                .flat_map(|p| {
                    let a = p[3] as u32;
                    [p[0], p[1], p[2]].map(|c| ((c as u32 * a + 255 * (255 - a) + 127) / 255) as u8)
                })
                .collect();
            engine_sys::encode(&rgb, image.width, image.height, 3, ImageFormat::Jpeg { quality: options.quality })
                .map_err(|_| ExportError::EncodeFailed)
        }
    }
}

/// Write exported bytes to `target`, refusing to replace any of
/// `protected` (the original photo, background images).
pub fn write(target: &Path, bytes: &[u8], protected: &[&Path]) -> Result<(), ExportError> {
    check_target(target, protected)?;
    files::write_atomically(target, bytes).map_err(ExportError::WriteFailed)
}

pub fn check_target(target: &Path, protected: &[&Path]) -> Result<(), ExportError> {
    if protected.iter().any(|p| files::same_file(target, p)) {
        return Err(ExportError::WouldOverwriteOriginal);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::tests::scratch;
    use std::fs;

    fn image(alpha: u8) -> RgbaImage {
        RgbaImage {
            width: 4,
            height: 2,
            source_width: 4,
            source_height: 2,
            pixels: (0..8).flat_map(|_| [200, 100, 0, alpha]).collect(),
        }
    }

    #[test]
    fn sizes_are_reduced_but_never_enlarged() {
        assert_eq!(output_size(4000, 3000, None), (4000, 3000));
        assert_eq!(output_size(4000, 3000, Some(2048)), (2048, 1536));
        assert_eq!(output_size(3000, 4000, Some(1080)), (810, 1080));
        assert_eq!(output_size(800, 600, Some(2048)), (800, 600));
        assert_eq!(output_size(800, 600, Some(0)), (800, 600));
    }

    #[test]
    fn jpeg_places_transparency_on_white_and_png_keeps_it() {
        let clear = image(0);
        assert!(has_transparency(&clear));
        assert!(!has_transparency(&image(255)));
        let jpeg = encode(&clear, &ExportOptions { format: ExportFormat::Jpeg, quality: 95, long_side: None }).unwrap();
        let back = engine_sys::decode_rgba8(&jpeg, 0).unwrap();
        assert!(back.pixels.chunks_exact(4).all(|p| p[0] > 245 && p[1] > 245 && p[2] > 245), "white");
        let png = encode(&clear, &ExportOptions { format: ExportFormat::Png, quality: 0, long_side: None }).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert!(encode(&clear, &ExportOptions { format: ExportFormat::Jpeg, quality: 0, long_side: None }).is_err());
    }

    #[test]
    fn never_overwrites_the_original() {
        let dir = scratch("export");
        let original = dir.join("Photo.JPG");
        fs::write(&original, b"original").unwrap();
        let err = write(&dir.join("photo.jpg"), b"new", &[&original]).unwrap_err();
        assert_eq!(err.code(), "export_would_overwrite_original");
        assert_eq!(fs::read(&original).unwrap(), b"original");
        let out = dir.join("export").join("photo-edited.jpg");
        write(&out, b"new", &[&original]).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"new");
    }
}

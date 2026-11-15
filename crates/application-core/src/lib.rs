//! Application core.
//!
//! The core does all file I/O (so Unicode paths work on Windows) and hands
//! byte buffers to the engine. Original files are opened read-only and are
//! never written.

pub mod ai;
pub mod background;
pub mod document;
pub mod export;
pub mod files;
pub mod info;
pub mod jobs;
pub mod models;
pub mod portrait;
pub mod project;
pub mod recovery;
pub mod settings;

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

pub use engine_sys::{
    encode, AiSession, Background, BackgroundMode, EngineError, Face, ImageFormat as EncodeFormat, Light, LightKind, Matte,
    Plane, Portrait, Provider, RgbaImage, Shape, Source,
};

/// Why an image could not be opened. `code()` is the stable identifier the
/// UI maps to a localized message.
#[derive(Debug)]
pub enum OpenError {
    NotFound,
    ReadFailed(io::Error),
    NotAnImage,
    OutOfMemory,
    Internal,
}

impl OpenError {
    pub fn code(&self) -> &'static str {
        match self {
            OpenError::NotFound => "not_found",
            OpenError::ReadFailed(_) => "read_failed",
            OpenError::NotAnImage => "not_an_image",
            OpenError::OutOfMemory => "out_of_memory",
            OpenError::Internal => "internal",
        }
    }
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::NotFound => f.write_str("the file does not exist"),
            OpenError::ReadFailed(e) => write!(f, "the file could not be read: {e}"),
            OpenError::NotAnImage => f.write_str("the file is damaged or not a supported image"),
            OpenError::OutOfMemory => f.write_str("not enough memory to open the image"),
            OpenError::Internal => f.write_str("internal error while opening the image"),
        }
    }
}

impl std::error::Error for OpenError {}

/// Read an image file and decode it for display.
///
/// `max_dimension` limits the longer side of the returned pixels (zero keeps
/// the full size). The file is only read.
pub fn open_image(path: &Path, max_dimension: u32) -> Result<RgbaImage, OpenError> {
    let bytes = read_image_file(path)?;
    engine_sys::decode_rgba8(&bytes, max_dimension).map_err(map_engine_error)
}

fn read_image_file(path: &Path) -> Result<Vec<u8>, OpenError> {
    let bytes = fs::read(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => OpenError::NotFound,
        _ => OpenError::ReadFailed(e),
    })?;
    if bytes.is_empty() {
        return Err(OpenError::NotAnImage);
    }
    Ok(bytes)
}

fn map_engine_error(error: EngineError) -> OpenError {
    match error {
        EngineError::DecodeFailed => OpenError::NotAnImage,
        EngineError::OutOfMemory => OpenError::OutOfMemory,
        EngineError::InvalidArgument
        | EngineError::Internal
        | EngineError::ModelInvalid
        | EngineError::InferenceFailed => OpenError::Internal,
    }
}

/// Decode image bytes (for example a project's preview) for display.
pub fn decode_image(bytes: &[u8], max_dimension: u32) -> Result<RgbaImage, OpenError> {
    engine_sys::decode_rgba8(bytes, max_dimension).map_err(map_engine_error)
}

/// Open an image for editing: decode it at full resolution and keep it in
/// the engine so any region can be rendered at any zoom. The file is only read.
pub fn open_document(path: &Path) -> Result<Source, OpenError> {
    let bytes = read_image_file(path)?;
    Source::open(&bytes).map_err(map_engine_error)
}

/// Size of the header written by [`encode_frame`].
pub const FRAME_HEADER_LEN: usize = 16;

/// Serialize an image for transfer to the UI: four little-endian `u32`
/// (width, height, source width, source height) followed by RGBA8 pixels.
pub fn encode_frame(image: &RgbaImage) -> Vec<u8> {
    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + image.pixels.len());
    for value in [image.width, image.height, image.source_width, image.source_height] {
        frame.extend_from_slice(&value.to_le_bytes());
    }
    frame.extend_from_slice(&image.pixels);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
    }

    /// A fresh directory under the system temp dir, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("aibs-test-{}-{name}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn opens_every_supported_format() {
        for name in [
            "gradient_64x48.png",
            "gradient_64x48.jpg",
            "gradient_64x48.tif",
            "gradient_64x48.webp",
        ] {
            let image = open_image(&fixture(name), 0).unwrap();
            assert_eq!((image.width, image.height), (64, 48), "{name}");
        }
    }

    #[test]
    fn lossless_formats_decode_to_identical_pixels() {
        let png = open_image(&fixture("gradient_64x48.png"), 0).unwrap();
        for name in ["gradient_64x48.tif", "gradient_64x48.webp"] {
            let other = open_image(&fixture(name), 0).unwrap();
            assert_eq!(other.pixels, png.pixels, "{name}");
        }
    }

    #[test]
    fn opens_image_from_a_chinese_path() {
        let dir = TempDir::new("中文資料夾");
        let path = dir.0.join("測試圖片 一.png");
        fs::copy(fixture("gradient_64x48.png"), &path).unwrap();
        let image = open_image(&path, 0).unwrap();
        assert_eq!((image.width, image.height), (64, 48));
    }

    #[test]
    fn corrupted_file_gives_a_clear_error() {
        let err = open_image(&fixture("corrupted.jpg"), 0).unwrap_err();
        assert_eq!(err.code(), "not_an_image");
    }

    #[test]
    fn empty_file_gives_a_clear_error() {
        let dir = TempDir::new("empty");
        let path = dir.0.join("empty.jpg");
        fs::write(&path, b"").unwrap();
        assert_eq!(open_image(&path, 0).unwrap_err().code(), "not_an_image");
    }

    #[test]
    fn missing_file_gives_not_found() {
        let err = open_image(&fixture("does-not-exist.png"), 0).unwrap_err();
        assert_eq!(err.code(), "not_found");
    }

    #[test]
    fn opening_never_modifies_the_original() {
        let path = fixture("gradient_64x48.jpg");
        let before = fs::read(&path).unwrap();
        open_image(&path, 16).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn opens_a_document_at_full_resolution() {
        let source = open_document(&fixture("gradient_64x48.jpg")).unwrap();
        assert_eq!((source.width(), source.height()), (64, 48));
        assert_eq!(open_document(&fixture("corrupted.jpg")).unwrap_err().code(), "not_an_image");
        assert_eq!(open_document(&fixture("nope.jpg")).unwrap_err().code(), "not_found");
    }

    /// Acceptance check: 200 mixed edits, all undone, give back exactly the
    /// starting picture; redone, exactly the final one. The file is untouched.
    #[test]
    fn undoing_200_edits_restores_the_exact_original_render() {
        use crate::document::{Adjustments, History};

        let path = fixture("gradient_64x48.png");
        let file_before = fs::read(&path).unwrap();
        let source = open_document(&path).unwrap();
        let render = |history: &History| {
            let engine = history.current().to_engine();
            source
                .render_adjusted(0.0, 0.0, 64.0, 48.0, 64, 48, None, Some(&engine), None, None, &[], 0)
                .unwrap()
                .pixels
        };

        let mut history = History::default();
        let start = render(&history);
        // Deterministic pseudo-random edits.
        let mut seed: u32 = 12345;
        let mut next = |range: f32| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * range
        };
        for _ in 0..200 {
            let mut a = Adjustments {
                exposure: next(3.0),
                brightness: next(100.0),
                contrast: next(100.0),
                highlights: next(100.0),
                shadows: next(100.0),
                whites: next(100.0),
                blacks: next(100.0),
                temperature: next(100.0),
                tint: next(100.0),
                vibrance: next(100.0),
                saturation: next(100.0),
                ..Default::default()
            };
            for band in 0..8 {
                a.hsl_hue[band] = next(100.0);
                a.hsl_saturation[band] = next(100.0);
                a.hsl_luminance[band] = next(100.0);
            }
            history.commit(a);
        }
        let end = render(&history);
        assert_ne!(start, end);

        while history.undo() {}
        assert_eq!(render(&history), start);
        while history.redo() {}
        assert_eq!(render(&history), end);
        assert_eq!(fs::read(&path).unwrap(), file_before);
    }

    #[test]
    fn frame_has_header_then_pixels() {
        let image = open_image(&fixture("gradient_64x48.png"), 32).unwrap();
        let frame = encode_frame(&image);
        assert_eq!(frame.len(), FRAME_HEADER_LEN + 32 * 24 * 4);
        assert_eq!(&frame[0..4], &32u32.to_le_bytes());
        assert_eq!(&frame[4..8], &24u32.to_le_bytes());
        assert_eq!(&frame[8..12], &64u32.to_le_bytes());
        assert_eq!(&frame[12..16], &48u32.to_le_bytes());
        assert_eq!(&frame[16..], &image.pixels[..]);
    }

    /// Codes listed in the UI's error-code file (see error-codes.json).
    pub(crate) fn listed_error_codes() -> Vec<String> {
        let text = include_str!("../../../apps/desktop/src/error-codes.json");
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        value["codes"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn every_error_code_of_the_core_has_a_message_in_the_ui() {
        use crate::export::ExportError;
        use crate::models::ModelError;
        use crate::project::ProjectError;
        let listed = listed_error_codes();
        let io = || io::Error::other("x");
        let codes = [
            OpenError::NotFound.code(),
            OpenError::ReadFailed(io()).code(),
            OpenError::NotAnImage.code(),
            OpenError::OutOfMemory.code(),
            OpenError::Internal.code(),
            ProjectError::NotFound.code(),
            ProjectError::NotAProject.code(),
            ProjectError::NewerVersion(2).code(),
            ProjectError::Damaged(String::new()).code(),
            ProjectError::OriginalMissing(String::new()).code(),
            ProjectError::OriginalChanged(String::new()).code(),
            ProjectError::Io(io()).code(),
            ExportError::WouldOverwriteOriginal.code(),
            ExportError::InvalidOptions.code(),
            ExportError::EncodeFailed.code(),
            ExportError::WriteFailed(io()).code(),
            ModelError::RegistryUnreadable(String::new()).code(),
            ModelError::UnknownModel(String::new()).code(),
            ModelError::FileMissing(String::new()).code(),
            ModelError::HashMismatch(String::new()).code(),
            ModelError::LicenseMissing(String::new()).code(),
            ModelError::LicenseNotApproved(String::new()).code(),
        ];
        for code in codes {
            assert!(listed.iter().any(|c| c == code), "error code {code} is not in error-codes.json");
        }
        // Codes the core's jobs and analyses produce as text.
        let sources = [
            include_str!("ai.rs"),
            include_str!("background.rs"),
            include_str!("jobs.rs"),
            include_str!("portrait.rs"),
        ];
        for code in sources.iter().flat_map(|text| quoted_codes(text)) {
            assert!(listed.contains(&code), "error code {code} is not in error-codes.json");
        }
    }

    /// Error codes written as `JobError::new("code"` or `"code".to_string()`.
    pub(crate) fn quoted_codes(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        for (marker, before) in [("JobError::new(\"", true), ("\".to_string()", false)] {
            let mut rest = text;
            while let Some(at) = rest.find(marker) {
                let code = if before {
                    let tail = &rest[at + marker.len()..];
                    tail.split('"').next().unwrap_or("")
                } else {
                    let head = &rest[..at];
                    head.rsplit('"').next().unwrap_or("")
                };
                if !code.is_empty() && code.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                    found.push(code.to_string());
                }
                rest = &rest[at + marker.len()..];
            }
        }
        found
    }
}

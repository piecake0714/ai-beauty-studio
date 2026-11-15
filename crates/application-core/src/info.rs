//! Facts about an image file shown to the user: name, size on disk, format.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ImageFormat {
    #[serde(rename = "JPEG")]
    Jpeg,
    #[serde(rename = "PNG")]
    Png,
    #[serde(rename = "TIFF")]
    Tiff,
    #[serde(rename = "WebP")]
    WebP,
}

/// Identify the format from the file's first bytes (its signature), not
/// from the file name, which can be wrong.
pub fn detect_format(head: &[u8]) -> Option<ImageFormat> {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageFormat::Jpeg)
    } else if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        Some(ImageFormat::Tiff)
    } else if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some(ImageFormat::WebP)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub name: String,
    pub size_bytes: u64,
    /// `None` when the signature is not a supported format.
    pub format: Option<ImageFormat>,
}

pub fn file_info(path: &Path) -> io::Result<FileInfo> {
    let metadata = fs::metadata(path)?;
    let mut head = [0u8; 16];
    let mut file = fs::File::open(path)?;
    let mut filled = 0;
    while filled < head.len() {
        match file.read(&mut head[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(FileInfo {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size_bytes: metadata.len(),
        format: detect_format(&head[..filled]),
    })
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

    #[test]
    fn detects_every_supported_format_from_its_signature() {
        let cases = [
            ("gradient_64x48.jpg", ImageFormat::Jpeg),
            ("gradient_64x48.png", ImageFormat::Png),
            ("gradient_64x48.tif", ImageFormat::Tiff),
            ("gradient_64x48.webp", ImageFormat::WebP),
        ];
        for (name, expected) in cases {
            let info = file_info(&fixture(name)).unwrap();
            assert_eq!(info.format, Some(expected), "{name}");
            assert_eq!(info.name, name);
            assert_eq!(info.size_bytes, fs::metadata(fixture(name)).unwrap().len());
        }
    }

    #[test]
    fn unknown_or_short_data_is_not_a_format() {
        assert_eq!(detect_format(b"hello world"), None);
        assert_eq!(detect_format(b""), None);
        assert_eq!(detect_format(b"RIFF"), None);
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(file_info(&fixture("does-not-exist.png")).is_err());
    }

    #[test]
    fn serializes_format_names_for_display() {
        let json = serde_json::to_string(&FileInfo {
            name: "a.webp".into(),
            size_bytes: 3,
            format: Some(ImageFormat::WebP),
        })
        .unwrap();
        assert_eq!(json, r#"{"name":"a.webp","sizeBytes":3,"format":"WebP"}"#);
    }
}

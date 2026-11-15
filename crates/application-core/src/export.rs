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

/// Rows rendered at a time when exporting.
pub const STRIP_ROWS: u32 = 1024;

/// Extra rows rendered above and below each strip and then dropped, so
/// filters that look at neighbours (skin smoothing, edge colours) see the
/// same surroundings as in a whole render: enough for the widest of them
/// (about 4.5% of the longer side, used twice).
pub fn strip_overlap(out_width: u32, out_height: u32) -> u32 {
    let longer = out_width.max(out_height) as f64;
    ((0.1 * longer).ceil() as u32).max(64)
}

/// Render an `out_width` x `out_height` image of the source region
/// `region` (x, y, width, height in source pixels; the whole photo or its
/// crop) in horizontal strips of `strip_rows` rows (plus `overlap` rows on
/// each side), so memory stays bounded for large exports. `render` draws a
/// source region (x, y, w, h) at an output size; `between` runs after each
/// strip with the fraction done and may stop the work.
#[allow(clippy::too_many_arguments)]
pub fn render_in_strips<E>(
    region: (u32, u32, u32, u32),
    out_width: u32,
    out_height: u32,
    strip_rows: u32,
    overlap: u32,
    mut render: impl FnMut(f64, f64, f64, f64, u32, u32) -> Result<RgbaImage, E>,
    mut between: impl FnMut(f32) -> Result<(), E>,
) -> Result<RgbaImage, E> {
    let row_bytes = out_width as usize * 4;
    let mut pixels = vec![0u8; row_bytes * out_height as usize];
    let (region_x, region_y, width, height) = region;
    let per_row = height as f64 / out_height as f64;
    let strip_rows = strip_rows.max(1);
    let mut start = 0;
    while start < out_height {
        let end = (start + strip_rows).min(out_height);
        let top = start.saturating_sub(overlap);
        let bottom = (end + overlap).min(out_height);
        // Output row i of the strip shows the same source row as output
        // row top + i of a whole render, so the strips line up exactly.
        let strip = render(
            region_x as f64,
            region_y as f64 + top as f64 * per_row,
            width as f64,
            (bottom - top) as f64 * per_row,
            out_width,
            bottom - top,
        )?;
        let skip = (start - top) as usize * row_bytes;
        let take = (end - start) as usize * row_bytes;
        pixels[start as usize * row_bytes..end as usize * row_bytes].copy_from_slice(&strip.pixels[skip..skip + take]);
        start = end;
        between(end as f32 / out_height as f32)?;
    }
    Ok(RgbaImage {
        pixels,
        width: out_width,
        height: out_height,
        source_width: width,
        source_height: height,
    })
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

    use engine_sys::{Background, BackgroundMode, Face, Light, LightKind, Matte, Portrait, Shape, Source};

    /// A drawn test scene (no photographs): grey background, a skin-coloured
    /// oval with blotches, a darker "subject" half for the matte.
    fn scene(w: u32, h: u32) -> Source {
        let mut rgb = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                let (cx, cy) = (w as f64 * 0.5, h as f64 * 0.45);
                let inside = ((x as f64 - cx) / (w as f64 * 0.18)).powi(2) + ((y as f64 - cy) / (h as f64 * 0.3)).powi(2) < 1.0;
                let blotch = (((x / 7) * 31 + (y / 7) * 17) % 23) as i32 - 11;
                let grain = (((x * 13 + y * 7) % 9) as i32) - 4;
                let (r, g, b) = if inside {
                    (205 + blotch + grain, 165 + blotch + grain, 140 + blotch + grain)
                } else {
                    let checker = if ((x / 9) + (y / 9)) % 2 == 0 { 30 } else { -30 };
                    (90 + checker, 120 + checker, 150 + checker)
                };
                rgb[i] = r.clamp(0, 255) as u8;
                rgb[i + 1] = g.clamp(0, 255) as u8;
                rgb[i + 2] = b.clamp(0, 255) as u8;
            }
        }
        let png = engine_sys::encode(&rgb, w, h, 3, ImageFormat::Png).unwrap();
        Source::open(&png).unwrap()
    }

    #[test]
    fn strips_match_a_whole_render_with_every_effect() {
        let (w, h) = (420u32, 360u32);
        let source = scene(w, h);
        let face = Face {
            x: w as f32 * 0.32,
            y: h as f32 * 0.15,
            width: w as f32 * 0.36,
            height: h as f32 * 0.6,
            landmarks: [
                w as f32 * 0.43, h as f32 * 0.35, w as f32 * 0.57, h as f32 * 0.35, w as f32 * 0.5, h as f32 * 0.45,
                w as f32 * 0.45, h as f32 * 0.58, w as f32 * 0.55, h as f32 * 0.58,
            ],
            score: 0.95,
            // Cb, Cr of the skin colour; variances; covariance.
            skin: [109.0, 150.0, 12.0, 12.0, 0.0],
        };
        let coverage: Vec<u8> = (0..w * h).map(|i| if (i % w) < w / 2 + 30 && (i % w) > w / 2 - 110 { 255 } else { 0 }).collect();
        let matte = Matte::from_values(&coverage, w, h, w, h).unwrap();
        let depth: Vec<f32> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32 / w as f32 - 0.5, (i / w) as f32 / h as f32 - 0.45);
                (1.0 - (x * x * 8.0 + y * y * 4.0)).max(0.0)
            })
            .collect();
        let shape = Shape::from_depth(&depth, w, h, w, h).unwrap();
        let colour = engine_sys::Adjustments { exposure: 0.3, contrast: 20.0, vibrance: 15.0, ..Default::default() };
        let faces = [face];
        // A soft brush stroke across the middle on a darkening layer, and
        // a multiply image layer.
        let darker = engine_sys::Adjustments { exposure: -0.7, saturation: 30.0, ..Default::default() };
        let points = [20.0, 170.0, 200.0, 190.0, 400.0, 160.0];
        let strokes = [engine_sys::Stroke { points: &points, size: 60.0, hardness: 0.3, opacity: 0.8, erase: false }];
        let texture = scene(30, 20);
        let layers = [
            engine_sys::Layer {
                content: engine_sys::LayerContent::Adjustment(&darker),
                blend: engine_sys::BlendMode::Normal,
                opacity: 0.9,
                mask_base: engine_sys::MaskBase::None,
                mask_invert: false,
                matte: None,
                strokes: &strokes,
            },
            engine_sys::Layer {
                content: engine_sys::LayerContent::Image(&texture),
                blend: engine_sys::BlendMode::SoftLight,
                opacity: 0.5,
                mask_base: engine_sys::MaskBase::Subject,
                mask_invert: false,
                matte: Some(&matte),
                strokes: &[],
            },
        ];
        let render = |x: f64, y: f64, rw: f64, rh: f64, ow: u32, oh: u32| {
            source.render_adjusted(
                x,
                y,
                rw,
                rh,
                ow,
                oh,
                Some(Portrait { faces: &faces, smoothing: 100.0 }),
                Some(&colour),
                Some(Background {
                    matte: &matte,
                    mode: BackgroundMode::Blur,
                    blur: 100.0,
                    brightness: -20.0,
                    saturation: 0.0,
                    edge: 0.0,
                    image: None,
                }),
                Some(Light {
                    kind: LightKind::Softbox,
                    x: 0.2,
                    y: 0.3,
                    intensity: 70.0,
                    size: 60.0,
                    warmth: 20.0,
                    height: 40.0,
                    subject: None,
                    shape: Some(&shape),
                }),
                &layers,
                0,
            )
        };
        let whole = render(0.0, 0.0, w as f64, h as f64, w, h).unwrap();
        let mut fractions = Vec::new();
        let strips = render_in_strips((0, 0, w, h), w, h, 50, strip_overlap(w, h), render, |f| {
            fractions.push(f);
            Ok::<(), engine_sys::EngineError>(())
        })
        .unwrap();
        assert_eq!(fractions.len(), 8);
        assert_eq!(*fractions.last().unwrap(), 1.0);
        let worst = whole.pixels.iter().zip(&strips.pixels).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        let differing = whole.pixels.iter().zip(&strips.pixels).filter(|(a, b)| a != b).count();
        println!("strips vs whole render: largest difference {worst}, {differing} values differ");
        assert!(worst <= 1, "largest difference {worst}");

        // Reduced size works the same way.
        let (rw, rh) = output_size(w, h, Some(200));
        let small_whole = render(0.0, 0.0, w as f64, h as f64, rw, rh).unwrap();
        let small_strips = render_in_strips((0, 0, w, h), rw, rh, 37, strip_overlap(rw, rh), render, |_| Ok(())).unwrap();
        let worst = small_whole.pixels.iter().zip(&small_strips.pixels).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        assert!(worst <= 1, "reduced: largest difference {worst}");
    }

    #[test]
    fn strips_can_be_stopped() {
        let source = scene(64, 200);
        let mut calls = 0;
        let result = render_in_strips(
            (0, 0, 64, 200),
            64,
            200,
            20,
            8,
            |x, y, w, h, ow, oh| source.render_adjusted(x, y, w, h, ow, oh, None, None, None, None, &[], 0),
            |_| {
                calls += 1;
                if calls == 3 { Err(engine_sys::EngineError::Internal) } else { Ok(()) }
            },
        );
        assert!(result.is_err());
        assert_eq!(calls, 3);
    }

    /// Logs how long a 12-megapixel export with every effect takes (the CI
    /// machine has no usable GPU, so this is CPU time), and checks it stays
    /// within a generous bound.
    #[test]
    fn export_timing_12_megapixels() {
        let (w, h) = (4000u32, 3000u32);
        let source = scene(w, h);
        let face = Face {
            x: 1300.0,
            y: 450.0,
            width: 1400.0,
            height: 1800.0,
            landmarks: [1700.0, 1100.0, 2300.0, 1100.0, 2000.0, 1400.0, 1800.0, 1750.0, 2200.0, 1750.0],
            score: 0.95,
            skin: [109.0, 150.0, 12.0, 12.0, 0.0],
        };
        // Analysis results at their usual working sizes.
        let (mw, mh) = (2048u32, 1536u32);
        let coverage: Vec<u8> = (0..mw * mh).map(|i| if (i % mw) > 600 && (i % mw) < 1450 { 255 } else { 0 }).collect();
        let matte = Matte::from_values(&coverage, mw, mh, w, h).unwrap();
        let depth: Vec<f32> = (0..1024 * 768).map(|i| (i % 1024) as f32 / 1024.0).collect();
        let shape = Shape::from_depth(&depth, 1024, 768, w, h).unwrap();
        let colour = engine_sys::Adjustments { exposure: 0.3, contrast: 20.0, ..Default::default() };
        let faces = [face];
        let darker = engine_sys::Adjustments { exposure: -0.5, ..Default::default() };
        let points = [500.0, 2500.0, 3500.0, 2600.0];
        let strokes = [engine_sys::Stroke { points: &points, size: 400.0, hardness: 0.5, opacity: 1.0, erase: false }];
        let layers = [engine_sys::Layer {
            content: engine_sys::LayerContent::Adjustment(&darker),
            blend: engine_sys::BlendMode::Normal,
            opacity: 1.0,
            mask_base: engine_sys::MaskBase::None,
            mask_invert: false,
            matte: None,
            strokes: &strokes,
        }];
        let started = std::time::Instant::now();
        let image = render_in_strips(
            (0, 0, w, h),
            w,
            h,
            STRIP_ROWS,
            strip_overlap(w, h),
            |x, y, rw, rh, ow, oh| {
                source.render_adjusted(
                    x,
                    y,
                    rw,
                    rh,
                    ow,
                    oh,
                    Some(Portrait { faces: &faces, smoothing: 60.0 }),
                    Some(&colour),
                    Some(Background {
                        matte: &matte,
                        mode: BackgroundMode::Blur,
                        blur: 60.0,
                        brightness: 0.0,
                        saturation: 0.0,
                        edge: 0.0,
                        image: None,
                    }),
                    Some(Light {
                        kind: LightKind::Softbox,
                        x: 0.3,
                        y: 0.3,
                        intensity: 50.0,
                        size: 50.0,
                        warmth: 0.0,
                        height: 50.0,
                        subject: None,
                        shape: Some(&shape),
                    }),
                    &layers,
                    0,
                )
            },
            |_| Ok::<(), engine_sys::EngineError>(()),
        )
        .unwrap();
        let rendered = started.elapsed();
        let jpeg = encode(&image, &ExportOptions { format: ExportFormat::Jpeg, quality: 92, long_side: None }).unwrap();
        let total = started.elapsed();
        println!(
            "info: 12 MP export with every effect on CPU: render {:.1} s, total with JPEG {:.1} s, {} bytes",
            rendered.as_secs_f64(),
            total.as_secs_f64(),
            jpeg.len()
        );
        assert!(total.as_secs() < 180);
    }

    #[test]
    fn strips_of_a_cropped_region_match_a_whole_render_of_it() {
        let source = scene(300, 200);
        let region = (40u32, 30u32, 200u32, 120u32);
        let render = |x: f64, y: f64, w: f64, h: f64, ow: u32, oh: u32| {
            source.render_adjusted(x, y, w, h, ow, oh, None, None, None, None, &[], 0)
        };
        let whole = render(40.0, 30.0, 200.0, 120.0, 200, 120).unwrap();
        let strips = render_in_strips(region, 200, 120, 25, 16, render, |_| Ok::<(), engine_sys::EngineError>(())).unwrap();
        assert_eq!(whole.pixels, strips.pixels);
        assert_eq!((strips.width, strips.height, strips.source_width), (200, 120, 200));
    }
}

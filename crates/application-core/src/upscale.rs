//! AI upscaling for export (Real-ESRGAN x2plus): the finished image is
//! enlarged 2x (or 4x, as two passes) in overlapping tiles, so any size
//! fits in memory and the work can report progress and be stopped.
//! Transparency is enlarged smoothly on its own.

use engine_sys::{AiSession, EngineError, RgbaImage};

use crate::background::{self, ModelSlot};
use crate::jobs::{JobContext, JobError};
use crate::models::{ModelRef, Registry};

/// The upscaling model.
pub const UPSCALE_MODEL: &str = "upscale-realesrgan-x2";
/// Input tile size (before padding).
const TILE: u32 = 192;
/// Context added around each tile and cut off afterwards.
const PAD: u32 = 12;
/// Largest side and pixel count an upscaled export may have.
pub const MAX_SIDE: u32 = 16384;
pub const MAX_PIXELS: u64 = 160_000_000;

/// True when `width` x `height` enlarged by `factor` stays within limits.
pub fn fits(width: u32, height: u32, factor: u32) -> bool {
    let (w, h) = (width as u64 * factor as u64, height as u64 * factor as u64);
    w <= MAX_SIDE as u64 && h <= MAX_SIDE as u64 && w * h <= MAX_PIXELS
}

/// Enlarge `image` 2x with `model`, a function from a tile (RGB planes,
/// 0..1, width, height; both even) to its 2x version. `progress` gets the
/// fraction done and may stop the work by returning an error.
pub fn upscale2_with<E>(
    image: &RgbaImage,
    mut model: impl FnMut(&[f32], u32, u32) -> Result<Vec<f32>, E>,
    mut progress: impl FnMut(f32) -> Result<(), E>,
) -> Result<RgbaImage, E> {
    let (w, h) = (image.width, image.height);
    let (ow, oh) = (w * 2, h * 2);
    let mut out = vec![0u8; ow as usize * oh as usize * 4];
    let tiles_x = w.div_ceil(TILE);
    let tiles_y = h.div_ceil(TILE);
    let total = (tiles_x * tiles_y).max(1) as f32;
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            progress((ty * tiles_x + tx) as f32 / total)?;
            let (x0, y0) = (tx * TILE, ty * TILE);
            let (x1, y1) = ((x0 + TILE).min(w), (y0 + TILE).min(h));
            // Padded area; even sizes (the model halves internally).
            let px0 = x0 as i64 - PAD as i64;
            let py0 = y0 as i64 - PAD as i64;
            let mut pw = (x1 - x0 + 2 * PAD) as i64;
            let mut ph = (y1 - y0 + 2 * PAD) as i64;
            pw += pw % 2;
            ph += ph % 2;
            let plane = (pw * ph) as usize;
            let mut input = vec![0.0f32; 3 * plane];
            for yy in 0..ph {
                let sy = (py0 + yy).clamp(0, h as i64 - 1) as usize;
                for xx in 0..pw {
                    let sx = (px0 + xx).clamp(0, w as i64 - 1) as usize;
                    let p = &image.pixels[(sy * w as usize + sx) * 4..];
                    let i = (yy * pw + xx) as usize;
                    for c in 0..3 {
                        input[c * plane + i] = p[c] as f32 / 255.0;
                    }
                }
            }
            let result = model(&input, pw as u32, ph as u32)?;
            let (rw, rh) = (2 * pw as usize, 2 * ph as usize);
            let rplane = rw * rh;
            // Keep the tile's own part.
            for oy in 2 * y0..2 * y1 {
                let ry = (oy as i64 - 2 * py0) as usize;
                for ox in 2 * x0..2 * x1 {
                    let rx = (ox as i64 - 2 * px0) as usize;
                    let o = (oy as usize * ow as usize + ox as usize) * 4;
                    for c in 0..3 {
                        let v = result.get(c * rplane + ry * rw + rx).copied().unwrap_or(0.0);
                        out[o + c] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    }
    // Transparency: smooth (bilinear) enlargement.
    for oy in 0..oh {
        let fy = ((oy as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (h - 1) as f32);
        let (y0, ty) = (fy.floor() as u32, fy - fy.floor());
        let y1 = (y0 + 1).min(h - 1);
        for ox in 0..ow {
            let fx = ((ox as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (w - 1) as f32);
            let (x0, tx) = (fx.floor() as u32, fx - fx.floor());
            let x1 = (x0 + 1).min(w - 1);
            let a = |x: u32, y: u32| image.pixels[((y * w + x) * 4 + 3) as usize] as f32;
            let top = a(x0, y0) * (1.0 - tx) + a(x1, y0) * tx;
            let bottom = a(x0, y1) * (1.0 - tx) + a(x1, y1) * tx;
            out[((oy * ow + ox) * 4 + 3) as usize] = (top * (1.0 - ty) + bottom * ty).round() as u8;
        }
    }
    progress(1.0)?;
    Ok(RgbaImage { width: ow, height: oh, source_width: image.source_width, source_height: image.source_height, pixels: out })
}

fn run_model(session: &AiSession, tile: &[f32], w: u32, h: u32) -> Result<Vec<f32>, EngineError> {
    let (out, shape) = session.run(tile, &[1, 3, h as i64, w as i64])?;
    if shape != [1, 3, 2 * h as i64, 2 * w as i64] {
        return Err(EngineError::InferenceFailed);
    }
    Ok(out)
}

/// Enlarge `image` by `factor` (2 or 4) with the AI model; `progress`
/// receives 0..1 for the whole enlargement.
pub fn upscale(
    registry: &Registry,
    slot: &ModelSlot,
    image: &RgbaImage,
    factor: u32,
    ctx: &JobContext,
    progress: impl Fn(f32),
) -> Result<(RgbaImage, ModelRef), JobError> {
    if !matches!(factor, 2 | 4) {
        return Err(JobError::new("export_invalid_options", format!("upscale factor {factor}")));
    }
    if !fits(image.width, image.height, factor) {
        return Err(JobError::new("upscale_too_large", format!("{}x{} x{factor}", image.width, image.height)));
    }
    let passes = if factor == 4 { 2 } else { 1 };
    let mut current = image.clone();
    let mut model = None;
    for pass in 0..passes {
        let start = pass as f32 / passes as f32;
        let (next, report) = background::run_reporting(registry, slot, ctx, false, |session| {
            upscale2_with(&current, |tile, w, h| run_model(session, tile, w, h), |done| {
                progress(start + done / passes as f32);
                if ctx.check().is_err() {
                    // Stopped: the job reports the cancellation.
                    return Err(EngineError::Internal);
                }
                Ok(())
            })
        })
        // A stop shows as a failed run; report it as the stop it was.
        .map_err(|error| ctx.check().err().unwrap_or(error))?;
        current = next;
        model = Some(report.model);
    }
    Ok((current, model.expect("at least one pass")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{JobUpdate, Jobs};
    use crate::models::tests::repo_models;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    fn gradient(w: u32, h: u32) -> RgbaImage {
        let pixels = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [(x * 255 / w) as u8, (y * 255 / h) as u8, ((x + y) % 256) as u8, if x < w / 2 { 255 } else { 0 }]
            })
            .collect();
        RgbaImage { width: w, height: h, source_width: w, source_height: h, pixels }
    }

    /// A stand-in model: nearest-neighbour 2x.
    fn nearest(tile: &[f32], w: u32, h: u32) -> Result<Vec<f32>, ()> {
        let (w, h) = (w as usize, h as usize);
        let mut out = vec![0.0; 3 * 4 * w * h];
        for c in 0..3 {
            for y in 0..2 * h {
                for x in 0..2 * w {
                    out[c * 4 * w * h + y * 2 * w + x] = tile[c * w * h + (y / 2) * w + x / 2];
                }
            }
        }
        Ok(out)
    }

    #[test]
    fn tiles_join_without_seams() {
        // Not a multiple of the tile size, odd sizes included.
        let image = gradient(TILE * 2 + 37, TILE + 51);
        let mut calls = 0;
        let mut last = 0.0;
        let big = upscale2_with(
            &image,
            |tile, w, h| {
                calls += 1;
                assert!(w % 2 == 0 && h % 2 == 0);
                nearest(tile, w, h)
            },
            |done| {
                assert!(done >= last);
                last = done;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls, 6);
        assert_eq!(last, 1.0);
        assert_eq!((big.width, big.height), (image.width * 2, image.height * 2));
        for (x, y) in [(0u32, 0u32), (TILE * 2 - 1, 7), (TILE * 2, TILE * 2 - 1), (big.width - 1, big.height - 1), (401, 300)] {
            let o = ((y * big.width + x) * 4) as usize;
            let i = (((y / 2) * image.width + x / 2) * 4) as usize;
            assert_eq!(&big.pixels[o..o + 3], &image.pixels[i..i + 3], "at {x},{y}");
        }
        // Transparency follows smoothly: opaque left, clear right.
        assert_eq!(big.pixels[3], 255);
        assert_eq!(big.pixels[((big.width - 1) * 4 + 3) as usize], 0);
        // Stopping part-way.
        let stopped = upscale2_with(&image, nearest, |done| if done > 0.4 { Err(()) } else { Ok(()) });
        assert!(stopped.is_err());
    }

    #[test]
    fn limits_are_checked() {
        assert!(fits(4000, 3000, 2) && !fits(9000, 3000, 2) && !fits(7000, 7000, 2) && fits(3000, 2000, 4));
    }

    #[test]
    fn the_model_enlarges_a_photo_like_a_smooth_resize() {
        let png = include_bytes!("../../../tests/fixtures/gradient_64x48.png");
        let source = engine_sys::Source::open(png).unwrap();
        let image = source.render(0.0, 0.0, 64.0, 48.0, 64, 48).unwrap();
        let slot = Arc::new(ModelSlot::new(UPSCALE_MODEL));
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let jobs = Jobs::new(move |u| tx.lock().unwrap().send(u).unwrap());
        let out: Arc<Mutex<Option<(RgbaImage, ModelRef)>>> = Arc::new(Mutex::new(None));
        let (keep, input) = (out.clone(), image.clone());
        jobs.start(move |ctx| {
            let registry = Registry::load(&repo_models()).map_err(|e| JobError::new(e.code(), e.to_string()))?;
            *keep.lock().unwrap() = Some(upscale(&registry, &slot, &input, 2, ctx, |_| {})?);
            Ok(serde_json::Value::Null)
        });
        loop {
            match rx.recv_timeout(Duration::from_secs(600)).unwrap() {
                JobUpdate::Finished { .. } => break,
                JobUpdate::Failed { code, detail, .. } => panic!("{code}: {detail}"),
                JobUpdate::Cancelled { .. } => panic!("cancelled"),
                JobUpdate::Progress { .. } => {}
            }
        }
        let (big, model) = out.lock().unwrap().take().unwrap();
        assert_eq!(model.id, UPSCALE_MODEL);
        assert_eq!((big.width, big.height), (128, 96));
        // Compared with a plain smooth resize of the same image.
        let smooth = source.render(0.0, 0.0, 64.0, 48.0, 128, 96).unwrap();
        let mut error = 0.0f64;
        for (a, b) in big.pixels.chunks_exact(4).zip(smooth.pixels.chunks_exact(4)) {
            for c in 0..3 {
                error += (a[c] as f64 - b[c] as f64).powi(2);
            }
        }
        let mse = error / (128.0 * 96.0 * 3.0);
        let psnr = 10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10();
        println!("upscale: PSNR against a plain resize {psnr:.1} dB");
        assert!(psnr > 20.0);
    }
}

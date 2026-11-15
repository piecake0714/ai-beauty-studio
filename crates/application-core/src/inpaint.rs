//! Filling parts of a photo with the LaMa inpainting model: object removal
//! (the user paints over what to remove) and old-photo repair (dust and
//! scratches are found automatically).
//!
//! The model sees a 512 x 512 view of the area around the hole and paints
//! the hole from what surrounds it. The result is kept as a patch: pixels
//! with a soft edge over a rectangle of the photo, drawn over the original
//! when rendering (the original file is never changed). Patches are
//! numbered within the document; the adjustments list which are in effect,
//! so removing is undoable.

use engine_sys::{AiSession, EngineError, Patch, Source};
use serde::{Deserialize, Serialize};

use crate::background::{self, ModelSlot};
use crate::document::StrokeData;
use crate::jobs::{JobContext, JobError};
use crate::models::{ModelRef, Registry};

/// The inpainting model (LaMa, big-lama, ONNX).
pub const INPAINT_MODEL: &str = "inpaint-lama";
/// The model's fixed input size.
const SIZE: usize = 512;
/// Soft edge of a patch, in model pixels.
const FEATHER: usize = 4;
/// Extra fill around the painted or found area, in model pixels, so its
/// rim goes too.
const GROW: usize = 3;
/// Most dust-and-scratch tiles filled in one repair.
const MAX_REPAIR_TILES: usize = 24;

/// A rectangle in source pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Replacement pixels for a rectangle of the photo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchData {
    pub id: u32,
    /// The rectangle covered, in source pixels.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Size of the pixels below.
    pub pixel_width: u32,
    pub pixel_height: u32,
    /// RGBA8, the alpha being the soft-edged area replaced.
    #[serde(skip)]
    pub rgba: Vec<u8>,
    /// The model that made it.
    pub model: Option<ModelRef>,
}

impl PatchData {
    pub fn to_engine(&self) -> Result<Patch, EngineError> {
        Patch::new(&self.rgba, self.pixel_width, self.pixel_height, [self.x, self.y, self.width, self.height])
    }
}

/// Grow a 0/1 mask (side x side) by `radius` pixels (square neighbourhood).
fn dilate(mask: &[f32], side: usize, radius: usize) -> Vec<f32> {
    let mut rows = vec![0.0f32; mask.len()];
    for y in 0..side {
        for x in 0..side {
            let (a, b) = (x.saturating_sub(radius), (x + radius).min(side - 1));
            rows[y * side + x] = (a..=b).map(|i| mask[y * side + i]).fold(0.0, f32::max);
        }
    }
    let mut out = vec![0.0f32; mask.len()];
    for y in 0..side {
        let (a, b) = (y.saturating_sub(radius), (y + radius).min(side - 1));
        for x in 0..side {
            out[y * side + x] = (a..=b).map(|j| rows[j * side + x]).fold(0.0, f32::max);
        }
    }
    out
}

/// Box blur of a side x side plane (edges clamped).
fn blur(plane: &[f32], side: usize, radius: usize) -> Vec<f32> {
    let pass = |input: &[f32], horizontal: bool| -> Vec<f32> {
        let mut out = vec![0.0f32; input.len()];
        let n = (2 * radius + 1) as f32;
        for a in 0..side {
            for b in 0..side {
                let mut sum = 0.0;
                for k in 0..=2 * radius {
                    let c = (b + k).saturating_sub(radius).min(side - 1);
                    sum += if horizontal { input[a * side + c] } else { input[c * side + a] };
                }
                if horizontal {
                    out[a * side + b] = sum / n;
                } else {
                    out[b * side + a] = sum / n;
                }
            }
        }
        out
    };
    pass(&pass(plane, true), false)
}

/// Run the model on the 512 x 512 view of `rect` with `hole` (1 = fill)
/// and return the filled view as RGB8.
fn fill(session: &AiSession, source: &Source, rect: Rect, hole: &[f32]) -> Result<Vec<u8>, EngineError> {
    let view = source.render(rect.x, rect.y, rect.w, rect.h, SIZE as u32, SIZE as u32)?;
    let plane = SIZE * SIZE;
    let mut image = vec![0.0f32; 3 * plane];
    for (i, p) in view.pixels.chunks_exact(4).enumerate() {
        for c in 0..3 {
            image[c * plane + i] = p[c] as f32 / 255.0;
        }
    }
    let mask: Vec<f32> = hole.iter().map(|&v| if v > 0.5 { 1.0 } else { 0.0 }).collect();
    let side = SIZE as i64;
    let (out, shape) = session.run_inputs(&[(&image, &[1, 3, side, side]), (&mask, &[1, 1, side, side])])?;
    if shape != [1, 3, side, side] {
        return Err(EngineError::InferenceFailed);
    }
    let mut rgb = vec![0u8; 3 * plane];
    for i in 0..plane {
        for c in 0..3 {
            rgb[3 * i + c] = out[c * plane + i].round().clamp(0.0, 255.0) as u8;
        }
    }
    Ok(rgb)
}

/// The patch for a filled view: only the (grown, soft-edged) hole, cut to
/// its bounding box. None when the hole is empty.
fn patch_from(id: u32, rect: Rect, hole: &[f32], rgb: &[u8], model: Option<ModelRef>) -> Option<PatchData> {
    let grown = dilate(hole, SIZE, GROW + FEATHER);
    let alpha = blur(&grown, SIZE, FEATHER);
    let (mut x0, mut y0, mut x1, mut y1) = (SIZE, SIZE, 0usize, 0usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            if alpha[y * SIZE + x] > 0.002 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (pw, ph) = (x1 - x0, y1 - y0);
    let mut rgba = Vec::with_capacity(pw * ph * 4);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = y * SIZE + x;
            rgba.extend_from_slice(&rgb[3 * i..3 * i + 3]);
            rgba.push((alpha[i] * 255.0).round().clamp(0.0, 255.0) as u8);
        }
    }
    let (sx, sy) = (rect.w / SIZE as f64, rect.h / SIZE as f64);
    Some(PatchData {
        id,
        x: (rect.x + x0 as f64 * sx) as f32,
        y: (rect.y + y0 as f64 * sy) as f32,
        width: (pw as f64 * sx) as f32,
        height: (ph as f64 * sy) as f32,
        pixel_width: pw as u32,
        pixel_height: ph as u32,
        rgba,
        model,
    })
}

/// The part of the photo the model looks at around `area`: square, about
/// 2.5 times the area's larger side (at least 96 pixels), inside the photo.
pub fn context_rect(area: Rect, width: u32, height: u32) -> Rect {
    let (w, h) = (width as f64, height as f64);
    let side = (area.w.max(area.h) * 2.5).max(96.0);
    let (rw, rh) = (side.min(w), side.min(h));
    let cx = area.x + area.w / 2.0;
    let cy = area.y + area.h / 2.0;
    let x = (cx - rw / 2.0).clamp(0.0, w - rw);
    let y = (cy - rh / 2.0).clamp(0.0, h - rh);
    Rect { x, y, w: rw, h: rh }
}

/// Distance from (px, py) to the segment a-b.
fn segment_distance(px: f64, py: f64, a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 { (((px - a.0) * dx + (py - a.1) * dy) / length).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (a.0 + t * dx, a.1 + t * dy);
    ((px - qx).powi(2) + (py - qy).powi(2)).sqrt()
}

/// The area painted by `strokes` (source pixels), or None if nothing is
/// painted.
pub fn strokes_bounds(strokes: &[StrokeData], width: u32, height: u32) -> Option<Rect> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for stroke in strokes.iter().filter(|s| !s.erase && s.size > 0.0) {
        let r = stroke.size as f64 / 2.0;
        for p in stroke.points.chunks_exact(2) {
            if !(p[0].is_finite() && p[1].is_finite()) {
                continue;
            }
            x0 = x0.min(p[0] as f64 - r);
            y0 = y0.min(p[1] as f64 - r);
            x1 = x1.max(p[0] as f64 + r);
            y1 = y1.max(p[1] as f64 + r);
        }
    }
    let (x0, y0) = (x0.max(0.0), y0.max(0.0));
    let (x1, y1) = (x1.min(width as f64), y1.min(height as f64));
    (x1 > x0 && y1 > y0).then_some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
}

/// The painted strokes as a 512 x 512 hole over `rect` (eraser strokes
/// take paint away again).
pub fn strokes_hole(strokes: &[StrokeData], rect: Rect) -> Vec<f32> {
    let mut hole = vec![0.0f32; SIZE * SIZE];
    for stroke in strokes {
        let r = stroke.size as f64 / 2.0;
        let points: Vec<(f64, f64)> = stroke
            .points
            .chunks_exact(2)
            .filter(|p| p[0].is_finite() && p[1].is_finite())
            .map(|p| (p[0] as f64, p[1] as f64))
            .collect();
        if points.is_empty() || r <= 0.0 {
            continue;
        }
        let segments: Vec<((f64, f64), (f64, f64))> = if points.len() == 1 {
            vec![(points[0], points[0])]
        } else {
            points.windows(2).map(|w| (w[0], w[1])).collect()
        };
        for gy in 0..SIZE {
            let py = rect.y + (gy as f64 + 0.5) * rect.h / SIZE as f64;
            for gx in 0..SIZE {
                let px = rect.x + (gx as f64 + 0.5) * rect.w / SIZE as f64;
                if segments.iter().any(|(a, b)| segment_distance(px, py, *a, *b) <= r) {
                    hole[gy * SIZE + gx] = if stroke.erase { 0.0 } else { 1.0 };
                }
            }
        }
    }
    hole
}

/// Remove what `strokes` paint over: the model fills it from its
/// surroundings. Returns the patch (number `id`).
pub fn remove(
    registry: &Registry,
    slot: &ModelSlot,
    source: &Source,
    strokes: &[StrokeData],
    id: u32,
    ctx: &JobContext,
) -> Result<PatchData, JobError> {
    let area = strokes_bounds(strokes, source.width(), source.height())
        .ok_or_else(|| JobError::new("nothing_to_remove", "no painted area"))?;
    let rect = context_rect(area, source.width(), source.height());
    let hole = strokes_hole(strokes, rect);
    if !hole.iter().any(|&v| v > 0.0) {
        return Err(JobError::new("nothing_to_remove", "the painted area is empty"));
    }
    let (rgb, report) = background::run(registry, slot, ctx, |session| fill(session, source, rect, &hole))?;
    patch_from(id, rect, &hole, &rgb, Some(report.model))
        .ok_or_else(|| JobError::new("nothing_to_remove", "the painted area is empty"))
}

/// Dust and scratch finding: pixels much brighter or darker than the
/// median of their 5 x 5 neighbourhood, on a `w` x `h` luminance plane.
/// `strength` 0..1 lowers the threshold. At most 3% of the image is
/// marked (the strongest differences).
pub fn find_defects(luma: &[u8], w: usize, h: usize, strength: f32) -> Vec<bool> {
    let mut diff = vec![0u8; w * h];
    let mut window = [0u8; 25];
    for y in 0..h {
        for x in 0..w {
            let mut n = 0;
            for dy in -2i64..=2 {
                for dx in -2i64..=2 {
                    let yy = (y as i64 + dy).clamp(0, h as i64 - 1) as usize;
                    let xx = (x as i64 + dx).clamp(0, w as i64 - 1) as usize;
                    window[n] = luma[yy * w + xx];
                    n += 1;
                }
            }
            window.sort_unstable();
            diff[y * w + x] = (luma[y * w + x] as i16 - window[12] as i16).unsigned_abs().min(255) as u8;
        }
    }
    let mut threshold = (60.0 - 35.0 * strength.clamp(0.0, 1.0)) as u8;
    // Too much found (a detailed photo): keep only the strongest 3%.
    let mut counts = [0usize; 256];
    for &d in &diff {
        counts[d as usize] += 1;
    }
    let budget = w * h * 3 / 100;
    let mut above: usize = counts[threshold as usize..].iter().sum();
    while above > budget && threshold < 255 {
        above -= counts[threshold as usize];
        threshold += 1;
    }
    diff.iter().map(|&d| d >= threshold.max(1)).collect()
}

/// What an old-photo repair produces.
pub struct Repair {
    pub patches: Vec<PatchData>,
    /// Suggested light and colour values (faded colours and contrast
    /// restored) and noise reduction, to apply as one step.
    pub adjustments: crate::document::Adjustments,
}

/// Repair an old photo: find dust and scratches and fill them (patches
/// numbered from `first_id`), and suggest values that restore faded
/// contrast and colour and reduce grain. `strength` 0..1.
pub fn repair_old_photo(
    registry: &Registry,
    slot: &ModelSlot,
    source: &Source,
    current: &crate::document::Adjustments,
    strength: f32,
    first_id: u32,
    ctx: &JobContext,
) -> Result<Repair, JobError> {
    let (width, height) = (source.width(), source.height());
    // Look for defects on a working copy at most 1536 pixels across.
    let scale = (1536.0 / width.max(height) as f64).min(1.0);
    let (ww, wh) = (((width as f64 * scale).round() as u32).max(1), ((height as f64 * scale).round() as u32).max(1));
    let view = source
        .render(0.0, 0.0, width as f64, height as f64, ww, wh)
        .map_err(|e| JobError::new("internal", e.to_string()))?;
    let luma: Vec<u8> = view
        .pixels
        .chunks_exact(4)
        .map(|p| (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32).round() as u8)
        .collect();
    let defects = find_defects(&luma, ww as usize, wh as usize, strength);
    ctx.check()?;

    // Tiles of 512 working pixels (smaller photos: one tile) with defects.
    let tile = 512usize;
    let mut tiles = Vec::new();
    for ty in (0..wh as usize).step_by(tile) {
        for tx in (0..ww as usize).step_by(tile) {
            let (tw, th) = (tile.min(ww as usize - tx), tile.min(wh as usize - ty));
            let found = (ty..ty + th).any(|y| (tx..tx + tw).any(|x| defects[y * ww as usize + x]));
            if found {
                tiles.push((tx, ty, tw, th));
            }
        }
    }
    tiles.truncate(MAX_REPAIR_TILES);

    let mut patches = Vec::new();
    let total = tiles.len().max(1) as f32;
    for (n, (tx, ty, tw, th)) in tiles.into_iter().enumerate() {
        ctx.check()?;
        ctx.progress(0.1 + 0.8 * n as f32 / total, "repair");
        let rect = Rect { x: tx as f64 / scale, y: ty as f64 / scale, w: tw as f64 / scale, h: th as f64 / scale };
        // The defects of this tile on the 512 grid (nearest working pixel).
        let mut hole = vec![0.0f32; SIZE * SIZE];
        for gy in 0..SIZE {
            let y = ty + (gy * th / SIZE).min(th - 1);
            for gx in 0..SIZE {
                let x = tx + (gx * tw / SIZE).min(tw - 1);
                if defects[y * ww as usize + x] {
                    hole[gy * SIZE + gx] = 1.0;
                }
            }
        }
        let hole = dilate(&hole, SIZE, 1);
        let (rgb, report) =
            background::run_reporting(registry, slot, ctx, false, |session| fill(session, source, rect, &hole))?;
        if let Some(patch) = patch_from(first_id + patches.len() as u32, rect, &hole, &rgb, Some(report.model)) {
            patches.push(patch);
        }
    }

    // Restore faded contrast and colour; reduce grain.
    let suggested = source.auto_adjust(&[]).map_err(|e| JobError::new("internal", e.to_string()))?;
    let mut adjustments = current.with_colour(suggested);
    let s = strength.clamp(0.0, 1.0);
    adjustments.noise_luminance = adjustments.noise_luminance.max(20.0 + 30.0 * s);
    adjustments.noise_color = adjustments.noise_color.max(30.0 + 40.0 * s);
    adjustments.vibrance = (adjustments.vibrance + 15.0 * s).min(100.0);
    adjustments.removals.extend(patches.iter().map(|p| p.id));
    ctx.progress(1.0, "done");
    Ok(Repair { patches, adjustments: adjustments.clamped() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{JobUpdate, Jobs};
    use crate::models::tests::repo_models;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn context_stays_inside_the_photo_and_is_square_when_it_can_be() {
        let r = context_rect(Rect { x: 10.0, y: 10.0, w: 20.0, h: 10.0 }, 1000, 800);
        assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 96.0, 96.0));
        let r = context_rect(Rect { x: 500.0, y: 400.0, w: 100.0, h: 40.0 }, 1000, 800);
        assert_eq!((r.w, r.h), (250.0, 250.0));
        assert_eq!((r.x, r.y), (425.0, 295.0));
        let r = context_rect(Rect { x: 0.0, y: 0.0, w: 300.0, h: 100.0 }, 400, 200);
        assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 400.0, 200.0));
    }

    #[test]
    fn painted_strokes_become_a_hole_and_eraser_takes_it_back() {
        let rect = Rect { x: 0.0, y: 0.0, w: 512.0, h: 512.0 };
        let paint = StrokeData { points: vec![100.0, 100.0, 200.0, 100.0], size: 20.0, hardness: 100.0, opacity: 100.0, erase: false };
        let hole = strokes_hole(std::slice::from_ref(&paint), rect);
        assert_eq!(hole[100 * SIZE + 150], 1.0);
        assert_eq!(hole[100 * SIZE + 215], 0.0);
        assert_eq!(hole[115 * SIZE + 150], 0.0);
        let erase = StrokeData { points: vec![150.0, 100.0], size: 10.0, erase: true, ..paint.clone() };
        let hole = strokes_hole(&[paint.clone(), erase], rect);
        assert_eq!(hole[100 * SIZE + 150], 0.0);
        assert_eq!(hole[100 * SIZE + 120], 1.0);
        let bounds = strokes_bounds(std::slice::from_ref(&paint), 1000, 1000).unwrap();
        assert_eq!((bounds.x, bounds.y, bounds.w, bounds.h), (90.0, 90.0, 120.0, 20.0));
        assert!(strokes_bounds(&[], 100, 100).is_none());
    }

    #[test]
    fn patches_cover_only_the_hole_with_a_soft_edge() {
        let mut hole = vec![0.0f32; SIZE * SIZE];
        for y in 200..220 {
            for x in 300..330 {
                hole[y * SIZE + x] = 1.0;
            }
        }
        let rgb = vec![7u8; 3 * SIZE * SIZE];
        let rect = Rect { x: 1000.0, y: 2000.0, w: 1024.0, h: 1024.0 };
        let patch = patch_from(5, rect, &hole, &rgb, None).unwrap();
        // Grown by GROW + FEATHER, then softened by FEATHER on each side.
        let margin = (GROW + 2 * FEATHER) as u32;
        assert_eq!(patch.pixel_width, 30 + 2 * margin);
        assert_eq!(patch.pixel_height, 20 + 2 * margin);
        assert_eq!((patch.x, patch.y), (1000.0 + 2.0 * (300 - margin) as f32, 2000.0 + 2.0 * (200 - margin) as f32));
        let centre = ((patch.pixel_height / 2) * patch.pixel_width + patch.pixel_width / 2) as usize * 4;
        assert_eq!(&patch.rgba[centre..centre + 4], &[7, 7, 7, 255]);
        assert!(patch.rgba[3] < 40);
        assert!(patch_from(1, rect, &vec![0.0; SIZE * SIZE], &rgb, None).is_none());
        patch.to_engine().unwrap();
    }

    #[test]
    fn dust_and_scratches_are_found_but_not_smooth_shading() {
        let (w, h) = (120usize, 80usize);
        let mut luma: Vec<u8> = (0..w * h).map(|i| (60 + (i % w) * 120 / w) as u8).collect();
        luma[40 * w + 30] = 250; // a speck
        for y in 10..70 {
            luma[y * w + 90] = 10; // a thin scratch
        }
        let found = find_defects(&luma, w, h, 0.5);
        assert!(found[40 * w + 30] && found[35 * w + 90]);
        assert!(!found[40 * w + 60] && !found[20 * w + 20]);
        assert!(found.iter().filter(|&&f| f).count() < 80);
    }

    fn wait(rx: &mpsc::Receiver<JobUpdate>) -> serde_json::Value {
        loop {
            match rx.recv_timeout(Duration::from_secs(600)).unwrap() {
                JobUpdate::Finished { result, .. } => return result,
                JobUpdate::Failed { code, detail, .. } => panic!("{code}: {detail}"),
                JobUpdate::Cancelled { .. } => panic!("cancelled"),
                JobUpdate::Progress { .. } => {}
            }
        }
    }

    /// A textured grey photo: brick-like stripes with a small dark object.
    fn scene() -> (Source, Vec<u8>, u32, u32) {
        let (w, h) = (256u32, 192u32);
        let rgb: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                let stripe = if (y / 8) % 2 == 0 { 150 } else { 120 };
                let object = (100..130).contains(&x) && (80..110).contains(&y);
                let v = if object { 15 } else { stripe + ((x * 7 + y * 3) % 9) as u8 };
                [v, v, v]
            })
            .collect();
        let png = engine_sys::encode(&rgb, w, h, 3, engine_sys::ImageFormat::Png).unwrap();
        (Source::open(&png).unwrap(), rgb, w, h)
    }

    #[test]
    fn removal_fills_the_painted_object_from_its_surroundings() {
        let (source, _, w, h) = scene();
        let source = Arc::new(source);
        let slot = Arc::new(ModelSlot::new(INPAINT_MODEL));
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let jobs = Jobs::new(move |u| tx.lock().unwrap().send(u).unwrap());
        let result: Arc<Mutex<Option<PatchData>>> = Arc::new(Mutex::new(None));
        let (s2, slot2, out) = (source.clone(), slot.clone(), result.clone());
        jobs.start(move |ctx| {
            let registry = Registry::load(&repo_models()).map_err(|e| JobError::new(e.code(), e.to_string()))?;
            let strokes = [StrokeData {
                points: vec![105.0, 85.0, 125.0, 85.0, 125.0, 105.0, 105.0, 105.0, 105.0, 85.0, 115.0, 95.0],
                size: 22.0,
                hardness: 100.0,
                opacity: 100.0,
                erase: false,
            }];
            let patch = remove(&registry, &slot2, &s2, &strokes, 1, ctx)?;
            *out.lock().unwrap() = Some(patch);
            Ok(serde_json::Value::Null)
        });
        wait(&rx);
        let patch = result.lock().unwrap().take().unwrap();
        assert_eq!(patch.model.as_ref().unwrap().id, INPAINT_MODEL);
        let engine_patch = patch.to_engine().unwrap();
        let patches = [&engine_patch];
        let portrait = engine_sys::Portrait { patches: &patches, ..Default::default() };
        let out = source
            .render_adjusted(0.0, 0.0, w as f64, h as f64, w, h, Some(portrait), None, None, None, None, &[], 0)
            .unwrap();
        let mean = |image: &engine_sys::RgbaImage| {
            let mut sum = 0.0;
            for y in 85..105 {
                for x in 105..125 {
                    sum += image.pixels[((y * w + x) * 4) as usize] as f64;
                }
            }
            sum / 400.0
        };
        let before = source.render(0.0, 0.0, w as f64, h as f64, w, h).unwrap();
        println!("removal: object mean {:.0} -> {:.0}", mean(&before), mean(&out));
        assert!(mean(&before) < 30.0 && mean(&out) > 90.0);
        // Far from the object nothing changes.
        assert_eq!(&out.pixels[..4], &before.pixels[..4]);
    }

    #[test]
    fn old_photo_repair_fills_specks_and_suggests_values() {
        let (w, h) = (200u32, 150u32);
        let rgb: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                // Faded: low contrast, yellowish.
                let base = 110 + (x * 40 / w) as u8;
                let speck = (x as i64 - 60).pow(2) + (y as i64 - 50).pow(2) <= 4;
                let scratch = x == 150 && (20..130).contains(&y);
                let v = if speck { 245 } else if scratch { 20 } else { base };
                [v, v.saturating_sub(5), v.saturating_sub(25)]
            })
            .collect();
        let png = engine_sys::encode(&rgb, w, h, 3, engine_sys::ImageFormat::Png).unwrap();
        let source = Arc::new(Source::open(&png).unwrap());
        let slot = Arc::new(ModelSlot::new(INPAINT_MODEL));
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let jobs = Jobs::new(move |u| tx.lock().unwrap().send(u).unwrap());
        let result: Arc<Mutex<Option<Repair>>> = Arc::new(Mutex::new(None));
        let (s2, out) = (source.clone(), result.clone());
        jobs.start(move |ctx| {
            let registry = Registry::load(&repo_models()).map_err(|e| JobError::new(e.code(), e.to_string()))?;
            let repair = repair_old_photo(&registry, &slot, &s2, &Default::default(), 0.6, 10, ctx)?;
            *out.lock().unwrap() = Some(repair);
            Ok(serde_json::Value::Null)
        });
        wait(&rx);
        let repair = result.lock().unwrap().take().unwrap();
        assert!(!repair.patches.is_empty());
        assert_eq!(repair.patches[0].id, 10);
        assert_eq!(repair.adjustments.removals, repair.patches.iter().map(|p| p.id).collect::<Vec<_>>());
        assert!(repair.adjustments.noise_luminance > 0.0 && repair.adjustments.noise_color > 0.0);
        let engine: Vec<Patch> = repair.patches.iter().map(|p| p.to_engine().unwrap()).collect();
        let refs: Vec<&Patch> = engine.iter().collect();
        let portrait = engine_sys::Portrait { patches: &refs, ..Default::default() };
        let out = source
            .render_adjusted(0.0, 0.0, w as f64, h as f64, w, h, Some(portrait), None, None, None, None, &[], 0)
            .unwrap();
        let at = |x: u32, y: u32| out.pixels[((y * w + x) * 4) as usize];
        println!("repair: speck {}, scratch {}", at(60, 50), at(150, 75));
        assert!(at(60, 50) < 200 && at(150, 75) > 70);
    }
}

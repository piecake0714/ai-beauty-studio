//! Batch editing: the same edits applied to many photos, each exported to a
//! folder. Every photo gets its own analyses (faces, subject separation,
//! 3D shape) when the edits need them, so portrait, background and
//! lighting settings work on each photo as they did on the one they were
//! made on. Edits tied to one photo's content (healing spots, object
//! removals, painted masks, image layers and backgrounds, the crop) are
//! left out.

use std::path::{Path, PathBuf};

use engine_sys::{AiSession, Face};
use serde::{Deserialize, Serialize};

use crate::background::{self, ModelSlot};
use crate::document::{Adjustments, BackgroundSetting, Crop, LayerKind, LightSetting, MaskStart};
use crate::export::{self, ExportFormat, ExportOptions};
use crate::jobs::{JobContext, JobError};
use crate::looks::LutLibrary;
use crate::models::Registry;
use crate::render::EditInputs;

/// Most photos in one batch.
pub const MAX_BATCH: usize = 500;

/// What to do with every photo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSettings {
    /// The edits (usually the open photo's), made general by
    /// [`general_adjustments`].
    pub adjustments: Adjustments,
    /// Run the one-click enhancement on each photo first (its light and
    /// colour values replace those of `adjustments`).
    #[serde(default)]
    pub auto_enhance: bool,
    pub export: ExportOptions,
    /// Folder for the results.
    pub folder: String,
    /// Added to each photo's name, e.g. "-edited".
    #[serde(default)]
    pub suffix: String,
}

/// How one photo went.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItem {
    pub path: String,
    /// Where the result was written.
    pub output: Option<String>,
    /// Why it failed (an error code).
    pub code: Option<String>,
}

/// The edits made general: whatever depends on one photo's content is
/// removed.
pub fn general_adjustments(adjustments: &Adjustments) -> Adjustments {
    let mut a = adjustments.clamped();
    a.spots.clear();
    a.removals.clear();
    a.crop = Crop::default();
    if a.background_mode == BackgroundSetting::Image {
        a.background_mode = BackgroundSetting::Original;
    }
    a.background_image = 0;
    a.layers.retain(|l| l.kind == LayerKind::Adjustment);
    for layer in &mut a.layers {
        layer.mask.strokes.clear();
        layer.image = 0;
    }
    // A painted-only mask is now empty: such layers would do nothing.
    a.layers.retain(|l| !(l.mask.start == MaskStart::None && !l.mask.invert));
    a
}

/// What the edits need analysed on each photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Needs {
    pub faces: bool,
    pub matte: bool,
    pub shape: bool,
}

pub fn needs(a: &Adjustments, auto_enhance: bool) -> Needs {
    let portrait = a.skin_smoothing > 0.0
        || a.blemish_removal > 0.0
        || a.dark_circles > 0.0
        || a.eye_enhance > 0.0
        || a.teeth_whitening > 0.0;
    let background_changed =
        a.background_mode != BackgroundSetting::Original || a.background_brightness != 0.0 || a.background_saturation != 0.0;
    let masked_layers = a
        .layers
        .iter()
        .any(|l| l.visible && matches!(l.mask.start, MaskStart::Subject | MaskStart::Background));
    let lit = a.light_kind != LightSetting::None;
    Needs {
        faces: portrait || auto_enhance,
        matte: background_changed || masked_layers || (lit && a.light_subject_only),
        shape: lit,
    }
}

/// Where the result for `photo` goes: `folder`/name+suffix.ext.
pub fn output_path(photo: &Path, folder: &Path, suffix: &str, format: ExportFormat) -> PathBuf {
    let stem = photo.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "photo".into());
    let extension = match format {
        ExportFormat::Jpeg => "jpg",
        ExportFormat::Png => "png",
    };
    folder.join(format!("{stem}{suffix}.{extension}"))
}

/// The models a batch may use.
pub struct BatchModels<'a> {
    pub registry: &'a Registry,
    pub detector: Option<&'a AiSession>,
    pub matte: &'a ModelSlot,
    pub depth: &'a ModelSlot,
}

/// A name not yet taken: `path`, or with " (2)", " (3)"... added.
pub fn unused_path(path: &Path, taken: &std::collections::HashSet<PathBuf>) -> PathBuf {
    if !taken.contains(path) && !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = path.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}).{extension}")))
        .find(|p| !taken.contains(p) && !p.exists())
        .expect("a free name")
}

/// Edit and export one photo to `target`.
pub fn process_one(
    photo: &Path,
    target: &Path,
    settings: &BatchSettings,
    models: &BatchModels,
    luts: &LutLibrary,
    ctx: &JobContext,
) -> Result<PathBuf, JobError> {
    let source = crate::open_document(photo).map_err(|e| JobError::new(e.code(), e.to_string()))?;
    let mut adjustments = general_adjustments(&settings.adjustments);
    let need = needs(&adjustments, settings.auto_enhance);
    let faces: Vec<Face> = match (need.faces, models.detector) {
        (true, Some(detector)) => crate::portrait::detect(&source, detector).unwrap_or_default(),
        _ => Vec::new(),
    };
    if settings.auto_enhance {
        let suggested = source.auto_adjust(&faces).map_err(|e| JobError::new("internal", e.to_string()))?;
        adjustments = adjustments.with_colour(suggested);
    }
    ctx.check()?;
    let matte = if need.matte {
        Some(background::run_reporting(models.registry, models.matte, ctx, false, |s| source.segment(s))?.0)
    } else {
        None
    };
    ctx.check()?;
    let shape = if need.shape {
        Some(background::run_reporting(models.registry, models.depth, ctx, false, |s| source.estimate_shape(s))?.0)
    } else {
        None
    };
    ctx.check()?;
    let lut = luts.get(&adjustments.lut.id);
    let inputs = EditInputs {
        source: &source,
        faces: &faces,
        matte: matte.as_ref(),
        shape: shape.as_ref(),
        images: &[],
        lut: lut.as_deref(),
        patches: &[],
    };
    let (out_w, out_h) = export::output_size(source.width(), source.height(), settings.export.long_side);
    let image = export::render_in_strips(
        (0, 0, source.width(), source.height()),
        out_w,
        out_h,
        export::STRIP_ROWS,
        export::strip_overlap(out_w, out_h),
        |x, y, w, h, ow, oh| {
            inputs.render(&adjustments, x, y, w, h, ow, oh, 0).map_err(|e| JobError::new("internal", e.to_string()))
        },
        |_| ctx.check(),
    )?;
    let bytes = export::encode(&image, &settings.export).map_err(|e| JobError::new(e.code(), e.to_string()))?;
    export::write(target, &bytes, &[photo]).map_err(|e| JobError::new(e.code(), e.to_string()))?;
    Ok(target.to_path_buf())
}

/// Run a batch: every photo in turn, reporting progress; a photo that
/// fails is recorded and the rest go on. Stopping ends the batch.
pub fn run(
    photos: &[String],
    settings: &BatchSettings,
    models: &BatchModels,
    luts: &LutLibrary,
    ctx: &JobContext,
) -> Result<Vec<BatchItem>, JobError> {
    if photos.is_empty() || photos.len() > MAX_BATCH || settings.export.upscale != 1 {
        return Err(JobError::new("invalid_request", "batch size or options"));
    }
    std::fs::create_dir_all(&settings.folder).map_err(|e| JobError::new("export_write_failed", e.to_string()))?;
    let mut items = Vec::with_capacity(photos.len());
    // Results never replace existing files or each other.
    let mut taken = std::collections::HashSet::new();
    for (i, photo) in photos.iter().enumerate() {
        ctx.check()?;
        ctx.progress(i as f32 / photos.len() as f32, "photo");
        let wanted = output_path(Path::new(photo), Path::new(&settings.folder), &settings.suffix, settings.export.format);
        let target = unused_path(&wanted, &taken);
        taken.insert(target.clone());
        let result = process_one(Path::new(photo), &target, settings, models, luts, ctx);
        match result {
            Ok(output) => items.push(BatchItem {
                path: photo.clone(),
                output: Some(output.to_string_lossy().into_owned()),
                code: None,
            }),
            Err(error) if error.code == "cancelled" => return Err(error),
            Err(error) => items.push(BatchItem { path: photo.clone(), output: None, code: Some(error.code) }),
        }
    }
    ctx.progress(1.0, "done");
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Layer, LayerKind, Mask, StrokeData};
    use crate::files::tests::scratch;
    use crate::jobs::{JobUpdate, Jobs};
    use crate::models::tests::repo_models;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn photo_specific_edits_are_left_out() {
        let painted = Layer {
            id: 1,
            kind: LayerKind::Adjustment,
            mask: Mask { start: MaskStart::None, invert: false, strokes: vec![StrokeData::default()] },
            ..Default::default()
        };
        let subject = Layer { id: 2, mask: Mask { start: MaskStart::Subject, ..Default::default() }, ..Default::default() };
        let picture = Layer { id: 3, kind: LayerKind::Image, image: 1, ..Default::default() };
        let a = Adjustments {
            exposure: 0.5,
            spots: vec![crate::document::HealSpot { x: 1.0, y: 1.0, radius: 2.0 }],
            removals: vec![4],
            crop: Crop { x: 0.1, y: 0.1, width: 0.5, height: 0.5 },
            background_mode: BackgroundSetting::Image,
            background_image: 1,
            layers: vec![painted, subject, picture],
            ..Default::default()
        };
        let g = general_adjustments(&a);
        assert_eq!(g.exposure, 0.5);
        assert!(g.spots.is_empty() && g.removals.is_empty());
        assert_eq!(g.crop, Crop::default());
        assert_eq!((g.background_mode, g.background_image), (BackgroundSetting::Original, 0));
        assert_eq!(g.layers.iter().map(|l| l.id).collect::<Vec<_>>(), vec![2]);
        assert_eq!(needs(&g, false), Needs { faces: false, matte: true, shape: false });
        let lit = Adjustments { light_kind: LightSetting::Softbox, skin_smoothing: 20.0, ..Default::default() };
        assert_eq!(needs(&lit, false), Needs { faces: true, matte: false, shape: true });
        assert_eq!(needs(&Adjustments::default(), true), Needs { faces: true, matte: false, shape: false });
        assert_eq!(
            output_path(Path::new("C:/photos/IMG_1.CR2"), Path::new("D:/out"), "-edited", ExportFormat::Jpeg),
            Path::new("D:/out").join("IMG_1-edited.jpg")
        );
    }

    #[test]
    fn a_batch_edits_every_photo_and_reports_failures() {
        let dir = scratch("batch");
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        let photos = vec![
            fixtures.join("gradient_64x48.png").to_string_lossy().into_owned(),
            fixtures.join("gradient_64x48.jpg").to_string_lossy().into_owned(),
            dir.join("missing.png").to_string_lossy().into_owned(),
        ];
        let settings = BatchSettings {
            adjustments: Adjustments { exposure: 1.0, lut: crate::document::LutChoice { id: "builtin:mono".into(), amount: 100.0 }, ..Default::default() },
            auto_enhance: false,
            export: ExportOptions { format: ExportFormat::Png, quality: 90, long_side: Some(32), upscale: 1 },
            folder: dir.join("out").to_string_lossy().into_owned(),
            suffix: "-b".into(),
        };
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let jobs = Jobs::new(move |u| tx.lock().unwrap().send(u).unwrap());
        let luts = Arc::new(LutLibrary::new(dir.join("luts")));
        let (matte, depth) = (Arc::new(ModelSlot::new(background::BACKGROUND_MODEL)), Arc::new(ModelSlot::new(background::DEPTH_MODEL)));
        let result: Arc<Mutex<Vec<BatchItem>>> = Arc::new(Mutex::new(Vec::new()));
        let keep = result.clone();
        jobs.start(move |ctx| {
            let registry = Registry::load(&repo_models()).map_err(|e| JobError::new(e.code(), e.to_string()))?;
            let models = BatchModels { registry: &registry, detector: None, matte: &matte, depth: &depth };
            *keep.lock().unwrap() = run(&photos, &settings, &models, &luts, ctx)?;
            Ok(serde_json::Value::Null)
        });
        loop {
            match rx.recv_timeout(Duration::from_secs(300)).unwrap() {
                JobUpdate::Finished { .. } => break,
                JobUpdate::Failed { code, detail, .. } => panic!("{code}: {detail}"),
                JobUpdate::Cancelled { .. } => panic!("cancelled"),
                JobUpdate::Progress { .. } => {}
            }
        }
        let items = result.lock().unwrap().clone();
        assert_eq!(items.len(), 3);
        assert_eq!(items[2].code.as_deref(), Some("not_found"));
        for item in &items[..2] {
            let output = item.output.as_ref().unwrap();
            let image = engine_sys::decode_rgba8(&std::fs::read(output).unwrap(), 0).unwrap();
            assert_eq!((image.width, image.height), (32, 24));
            // Black and white (the mono LUT): no colour left.
            assert!(image.pixels.chunks_exact(4).all(|p| p[0].abs_diff(p[1]) <= 2 && p[1].abs_diff(p[2]) <= 2));
        }
        // Two photos of the same name do not overwrite each other.
        assert!(items[0].output.as_ref().unwrap().ends_with("gradient_64x48-b.png"));
        assert!(items[1].output.as_ref().unwrap().ends_with("gradient_64x48-b (2).png"));
    }
}

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
const AIBS_ERR_MODEL_INVALID: i32 = 10;
const AIBS_ERR_INFERENCE_FAILED: i32 = 11;
const AIBS_PROVIDER_CPU: i32 = 0;
const AIBS_PROVIDER_DIRECTML: i32 = 1;

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
        portrait: *const PortraitRaw,
        adjustments: *const Adjustments,
        background: *const BackgroundRaw,
        light: *const LightRaw,
        layers: *const LayerRaw,
        layer_count: u32,
        original_columns: u32,
        out: *mut AibsImage,
    ) -> i32;
    fn aibs_detect_faces(
        source: *const AibsSourceRaw,
        detector: *mut AiSessionRaw,
        min_score: f32,
        out: *mut Face,
        capacity: u32,
        count: *mut u32,
    ) -> i32;
    fn aibs_segment(source: *const AibsSourceRaw, model: *mut AiSessionRaw, out: *mut *mut MatteRaw) -> i32;
    fn aibs_matte_create(
        values: *const u8,
        width: u32,
        height: u32,
        source_width: u32,
        source_height: u32,
        out: *mut *mut MatteRaw,
    ) -> i32;
    fn aibs_matte_coverage(matte: *const MatteRaw) -> f32;
    fn aibs_matte_free(matte: *mut MatteRaw);
    fn aibs_estimate_shape(source: *const AibsSourceRaw, model: *mut AiSessionRaw, out: *mut *mut ShapeRaw) -> i32;
    fn aibs_shape_create(
        depth: *const f32,
        width: u32,
        height: u32,
        source_width: u32,
        source_height: u32,
        out: *mut *mut ShapeRaw,
    ) -> i32;
    fn aibs_shape_free(shape: *mut ShapeRaw);
    fn aibs_encode(
        pixels: *const u8,
        width: u32,
        height: u32,
        channels: u32,
        format: u32,
        quality: u32,
        out: *mut BufferRaw,
    ) -> i32;
    fn aibs_buffer_free(buffer: *mut BufferRaw);
    fn aibs_matte_values(
        matte: *const MatteRaw,
        width: *mut u32,
        height: *mut u32,
        source_width: *mut u32,
        source_height: *mut u32,
        values: *mut *const u8,
    ) -> i32;
    fn aibs_shape_normals(
        shape: *const ShapeRaw,
        width: *mut u32,
        height: *mut u32,
        source_width: *mut u32,
        source_height: *mut u32,
        normals: *mut *const f32,
    ) -> i32;
    fn aibs_shape_from_normals(
        normals: *const f32,
        width: u32,
        height: u32,
        source_width: u32,
        source_height: u32,
        out: *mut *mut ShapeRaw,
    ) -> i32;
    fn aibs_auto_adjust(source: *const AibsSourceRaw, faces: *const Face, face_count: u32, out: *mut Adjustments) -> i32;
    fn aibs_gpu_list(out: *mut GpuInfoRaw, capacity: u32) -> u32;
    fn aibs_ai_runtime_version() -> *const c_char;
    fn aibs_ai_open(
        model: *const u8,
        len: usize,
        preferred: i32,
        out: *mut *mut AiSessionRaw,
        used_provider: *mut i32,
        note: *mut c_char,
        note_len: usize,
    ) -> i32;
    fn aibs_ai_run(
        session: *mut AiSessionRaw,
        input: *const f32,
        shape: *const i64,
        rank: usize,
        output: *mut *mut f32,
        out_shape: *mut i64,
        out_rank: *mut usize,
    ) -> i32;
    fn aibs_ai_free(data: *mut f32);
    fn aibs_ai_close(session: *mut AiSessionRaw);
    fn aibs_adjust_rgb8(adjustments: *const Adjustments, input: *const u8, output: *mut u8, pixel_count: usize) -> i32;
    fn aibs_source_free(source: *mut AibsSourceRaw);
}

/// Number of HSL colour bands: red, orange, yellow, green, aqua, blue,
/// purple, magenta.
pub const HSL_BANDS: usize = 8;

/// Light and colour adjustments, matching `aibs_adjustments`. All zero
/// leaves the image unchanged. Exposure is -5..5 EV; everything else
/// -100..100. The engine clamps values outside those ranges.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Adjustments {
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub temperature: f32,
    pub tint: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub hsl_hue: [f32; HSL_BANDS],
    pub hsl_saturation: [f32; HSL_BANDS],
    pub hsl_luminance: [f32; HSL_BANDS],
}

/// A detected face, in source pixels. Landmarks are (x, y) pairs: right
/// eye, left eye, nose tip, right and left mouth corners (from the
/// viewer's side: the person's right eye is on the left of the picture).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Face {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub landmarks: [f32; 10],
    pub score: f32,
    /// Skin colour model: mean Cb, mean Cr, var Cb, var Cr, cov Cb/Cr.
    pub skin: [f32; 5],
}

/// Most healing spots used in one render.
pub const MAX_SPOTS: usize = 500;

/// A healing spot in source pixels: the round area of `radius` around
/// (x, y) is replaced by matching texture from nearby.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spot {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

/// Portrait retouching applied while rendering. Every strength is 0 (off)
/// to 100; only colours change, nothing moves.
#[derive(Debug, Clone, Copy, Default)]
pub struct Portrait<'a> {
    pub faces: &'a [Face],
    pub smoothing: f32,
    /// Small dark or red spots on the skin fade.
    pub blemishes: f32,
    /// Shadows under the eyes lift.
    pub dark_circles: f32,
    /// Eyes brighter and clearer.
    pub eyes: f32,
    /// Teeth whiter.
    pub teeth: f32,
    /// Healing spots (work without faces too).
    pub spots: &'a [Spot],
}

#[repr(C)]
struct PortraitRaw {
    faces: *const Face,
    face_count: u32,
    smoothing: f32,
    blemishes: f32,
    dark_circles: f32,
    eyes: f32,
    teeth: f32,
    spots: *const Spot,
    spot_count: u32,
}

#[repr(C)]
struct MatteRaw {
    _private: [u8; 0],
}

/// Which subject pixels belong to the subject, for a whole source; made by
/// [`Source::segment`] or [`Matte::from_values`].
pub struct Matte {
    raw: *mut MatteRaw,
}

// SAFETY: a matte is read-only after creation; the engine reads it from
// any thread.
unsafe impl Send for Matte {}
unsafe impl Sync for Matte {}

impl Matte {
    /// A matte from coverage values (0 background .. 255 subject), `width`
    /// x `height`, stretched over a source of the given size.
    pub fn from_values(
        values: &[u8],
        width: u32,
        height: u32,
        source_width: u32,
        source_height: u32,
    ) -> Result<Matte, EngineError> {
        if values.len() != width as usize * height as usize {
            return Err(EngineError::InvalidArgument);
        }
        let mut raw: *mut MatteRaw = std::ptr::null_mut();
        // SAFETY: `values` holds width * height bytes; `raw` is writable.
        status_to_result(unsafe {
            aibs_matte_create(values.as_ptr(), width, height, source_width, source_height, &mut raw)
        })?;
        Ok(Matte { raw })
    }

    /// Fraction of the image that is subject, 0..1.
    pub fn coverage(&self) -> f32 {
        // SAFETY: `raw` is a valid matte.
        unsafe { aibs_matte_coverage(self.raw) }
    }
}

impl Drop for Matte {
    fn drop(&mut self) {
        // SAFETY: `raw` came from the engine and is freed once.
        unsafe { aibs_matte_free(self.raw) };
    }
}

impl fmt::Debug for Matte {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Matte").field("coverage", &self.coverage()).finish()
    }
}

/// What to do with the background (see [`Background`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundMode {
    /// Keep it; brightness and saturation still apply.
    Original,
    /// Remove it: transparent.
    Transparent,
    /// Replace it with a colour (R, G, B).
    Color([u8; 3]),
    /// Blur it.
    Blur,
    /// Replace it with another image, scaled to cover.
    Image,
}

/// Background treatment applied while rendering, after all other edits.
#[derive(Debug, Clone, Copy)]
pub struct Background<'a> {
    pub matte: &'a Matte,
    pub mode: BackgroundMode,
    /// 0..100, for [`BackgroundMode::Blur`].
    pub blur: f32,
    /// -100..100, background only.
    pub brightness: f32,
    /// -100..100, background only.
    pub saturation: f32,
    /// -100 (pull the subject's edge in) .. 100 (push it out).
    pub edge: f32,
    /// The replacement for [`BackgroundMode::Image`].
    pub image: Option<&'a Source>,
    /// 0..100: the edge follows fine detail such as hair more closely.
    pub detail: f32,
}

#[repr(C)]
struct BackgroundRaw {
    matte: *const MatteRaw,
    mode: u32,
    color: [u8; 3],
    reserved: u8,
    blur: f32,
    brightness: f32,
    saturation: f32,
    edge: f32,
    image: *const AibsSourceRaw,
    detail: f32,
}

impl Background<'_> {
    fn raw(&self) -> BackgroundRaw {
        let (mode, color) = match self.mode {
            BackgroundMode::Original => (0, [0; 3]),
            BackgroundMode::Transparent => (1, [0; 3]),
            BackgroundMode::Color(c) => (2, c),
            BackgroundMode::Blur => (3, [0; 3]),
            BackgroundMode::Image => (4, [0; 3]),
        };
        BackgroundRaw {
            matte: self.matte.raw,
            mode,
            color,
            reserved: 0,
            blur: self.blur,
            brightness: self.brightness,
            saturation: self.saturation,
            edge: self.edge,
            image: self.image.map_or(std::ptr::null(), |s| s.raw as *const AibsSourceRaw),
            detail: self.detail,
        }
    }
}

#[repr(C)]
struct ShapeRaw {
    _private: [u8; 0],
}

/// The 3D shape of a photo (surface directions from AI depth), for
/// lighting; made by [`Source::estimate_shape`] or [`Shape::from_depth`].
pub struct Shape {
    raw: *mut ShapeRaw,
}

// SAFETY: read-only after creation.
unsafe impl Send for Shape {}
unsafe impl Sync for Shape {}

impl Shape {
    /// A shape from relative inverse depth (larger is closer), `width` x
    /// `height`, stretched over a source of the given size.
    pub fn from_depth(
        depth: &[f32],
        width: u32,
        height: u32,
        source_width: u32,
        source_height: u32,
    ) -> Result<Shape, EngineError> {
        if depth.len() != width as usize * height as usize {
            return Err(EngineError::InvalidArgument);
        }
        let mut raw: *mut ShapeRaw = std::ptr::null_mut();
        // SAFETY: `depth` holds width * height values; `raw` is writable.
        status_to_result(unsafe {
            aibs_shape_create(depth.as_ptr(), width, height, source_width, source_height, &mut raw)
        })?;
        Ok(Shape { raw })
    }
}

impl Drop for Shape {
    fn drop(&mut self) {
        // SAFETY: `raw` came from the engine and is freed once.
        unsafe { aibs_shape_free(self.raw) };
    }
}

impl fmt::Debug for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Shape")
    }
}

/// Kinds of light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightKind {
    Natural,
    Diffuse,
    Reflector,
    Softbox,
}

/// A light added to the photo, shaded by its 3D shape when there is one.
#[derive(Debug, Clone, Copy)]
pub struct Light<'a> {
    pub kind: LightKind,
    /// Position as a fraction of the width and height.
    pub x: f32,
    pub y: f32,
    /// 0..100.
    pub intensity: f32,
    /// 0..100: reach.
    pub size: f32,
    /// -100 (cool) .. 100 (warm).
    pub warmth: f32,
    /// 0 (from the side) .. 100 (from the camera).
    pub height: f32,
    /// Light only the subject.
    pub subject: Option<&'a Matte>,
    pub shape: Option<&'a Shape>,
}

#[repr(C)]
struct LightRaw {
    kind: u32,
    x: f32,
    y: f32,
    intensity: f32,
    size: f32,
    warmth: f32,
    height: f32,
    subject: *const MatteRaw,
    shape: *const ShapeRaw,
}

impl Light<'_> {
    fn raw(&self) -> LightRaw {
        LightRaw {
            kind: match self.kind {
                LightKind::Natural => 1,
                LightKind::Diffuse => 2,
                LightKind::Reflector => 3,
                LightKind::Softbox => 4,
            },
            x: self.x,
            y: self.y,
            intensity: self.intensity,
            size: self.size,
            warmth: self.warmth,
            height: self.height,
            subject: self.subject.map_or(std::ptr::null(), |m| m.raw as *const MatteRaw),
            shape: self.shape.map_or(std::ptr::null(), |s| s.raw as *const ShapeRaw),
        }
    }
}

#[repr(C)]
struct BufferRaw {
    data: *mut u8,
    size: usize,
}

/// File formats for [`encode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// `quality` 1..=100; 1 or 3 channels.
    Jpeg { quality: u32 },
    Png,
}

/// Encode packed 8-bit pixels (1 grey, 3 RGB or 4 RGBA channels).
pub fn encode(pixels: &[u8], width: u32, height: u32, channels: u32, format: ImageFormat) -> Result<Vec<u8>, EngineError> {
    if pixels.len() != width as usize * height as usize * channels as usize {
        return Err(EngineError::InvalidArgument);
    }
    let (code, quality) = match format {
        ImageFormat::Jpeg { quality } => (1, quality),
        ImageFormat::Png => (2, 0),
    };
    let mut raw = BufferRaw { data: std::ptr::null_mut(), size: 0 };
    // SAFETY: `pixels` holds width * height * channels bytes; `raw` is writable.
    status_to_result(unsafe { aibs_encode(pixels.as_ptr(), width, height, channels, code, quality, &mut raw) })?;
    // SAFETY: on success `raw` holds `size` engine-owned bytes, freed once below.
    let bytes = unsafe { std::slice::from_raw_parts(raw.data, raw.size).to_vec() };
    // SAFETY: as above.
    unsafe { aibs_buffer_free(&mut raw) };
    Ok(bytes)
}

/// Saved form of a matte or shape: its own size, the source size it
/// stretches over, and its values.
#[derive(Debug, Clone, PartialEq)]
pub struct Plane<T> {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub values: Vec<T>,
}

impl Matte {
    /// The coverage values, for saving.
    pub fn values(&self) -> Result<Plane<u8>, EngineError> {
        let (mut w, mut h, mut sw, mut sh) = (0, 0, 0, 0);
        let mut data: *const u8 = std::ptr::null();
        // SAFETY: valid matte; outputs are writable.
        status_to_result(unsafe { aibs_matte_values(self.raw, &mut w, &mut h, &mut sw, &mut sh, &mut data) })?;
        // SAFETY: the engine returned w * h bytes valid while the matte lives.
        let values = unsafe { std::slice::from_raw_parts(data, w as usize * h as usize).to_vec() };
        Ok(Plane { width: w, height: h, source_width: sw, source_height: sh, values })
    }
}

impl Shape {
    /// The surface directions (x, y, z per pixel), for saving.
    pub fn normals(&self) -> Result<Plane<f32>, EngineError> {
        let (mut w, mut h, mut sw, mut sh) = (0, 0, 0, 0);
        let mut data: *const f32 = std::ptr::null();
        // SAFETY: valid shape; outputs are writable.
        status_to_result(unsafe { aibs_shape_normals(self.raw, &mut w, &mut h, &mut sw, &mut sh, &mut data) })?;
        // SAFETY: the engine returned w * h * 3 floats valid while the shape lives.
        let values = unsafe { std::slice::from_raw_parts(data, w as usize * h as usize * 3).to_vec() };
        Ok(Plane { width: w, height: h, source_width: sw, source_height: sh, values })
    }

    /// A shape from saved surface directions.
    pub fn from_normals(plane: &Plane<f32>) -> Result<Shape, EngineError> {
        if plane.values.len() != plane.width as usize * plane.height as usize * 3 {
            return Err(EngineError::InvalidArgument);
        }
        let mut raw: *mut ShapeRaw = std::ptr::null_mut();
        // SAFETY: `values` has width * height * 3 floats; `raw` is writable.
        status_to_result(unsafe {
            aibs_shape_from_normals(
                plane.values.as_ptr(),
                plane.width,
                plane.height,
                plane.source_width,
                plane.source_height,
                &mut raw,
            )
        })?;
        Ok(Shape { raw })
    }
}

/// How a layer's colours combine with what is below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    Darken,
    Lighten,
    Difference,
}

/// What a layer's mask starts as, before strokes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskBase {
    All,
    None,
    Subject,
    Background,
}

/// A brush stroke on a layer mask.
#[derive(Debug, Clone, Copy)]
pub struct Stroke<'a> {
    /// x, y pairs in source pixels.
    pub points: &'a [f32],
    /// Brush diameter in source pixels.
    pub size: f32,
    /// 0 (soft) .. 1 (hard).
    pub hardness: f32,
    /// 0..1.
    pub opacity: f32,
    pub erase: bool,
}

/// What a layer puts on top.
#[derive(Debug, Clone, Copy)]
pub enum LayerContent<'a> {
    /// Local light and colour adjustments.
    Adjustment(&'a Adjustments),
    /// Another image, scaled to cover the photo.
    Image(&'a Source),
}

/// One layer, applied over everything below it.
#[derive(Debug, Clone, Copy)]
pub struct Layer<'a> {
    pub content: LayerContent<'a>,
    pub blend: BlendMode,
    /// 0..1.
    pub opacity: f32,
    pub mask_base: MaskBase,
    pub mask_invert: bool,
    /// For the subject and background mask bases.
    pub matte: Option<&'a Matte>,
    pub strokes: &'a [Stroke<'a>],
}

#[repr(C)]
struct StrokeRaw {
    points: *const f32,
    point_count: u32,
    size: f32,
    hardness: f32,
    opacity: f32,
    erase: u32,
}

#[repr(C)]
struct LayerRaw {
    kind: u32,
    blend: u32,
    opacity: f32,
    mask_base: u32,
    mask_invert: u32,
    matte: *const MatteRaw,
    adjustments: *const Adjustments,
    image: *const AibsSourceRaw,
    strokes: *const StrokeRaw,
    stroke_count: u32,
}

/// Raw layers and the stroke arrays they point into; kept alive together
/// for the duration of a render.
struct RawLayers {
    layers: Vec<LayerRaw>,
    _strokes: Vec<Vec<StrokeRaw>>,
}

fn raw_layers(layers: &[Layer<'_>]) -> RawLayers {
    let strokes: Vec<Vec<StrokeRaw>> = layers
        .iter()
        .map(|layer| {
            layer
                .strokes
                .iter()
                .map(|s| StrokeRaw {
                    points: s.points.as_ptr(),
                    point_count: (s.points.len() / 2) as u32,
                    size: s.size,
                    hardness: s.hardness,
                    opacity: s.opacity,
                    erase: s.erase as u32,
                })
                .collect()
        })
        .collect();
    let raw = layers
        .iter()
        .zip(&strokes)
        .map(|(layer, strokes)| {
            let (kind, adjustments, image) = match layer.content {
                LayerContent::Adjustment(a) => (0, a as *const Adjustments, std::ptr::null()),
                LayerContent::Image(source) => (1, std::ptr::null(), source.raw as *const AibsSourceRaw),
            };
            LayerRaw {
                kind,
                blend: layer.blend as u32,
                opacity: layer.opacity,
                mask_base: layer.mask_base as u32,
                mask_invert: layer.mask_invert as u32,
                matte: layer.matte.map_or(std::ptr::null(), |m| m.raw as *const MatteRaw),
                adjustments,
                image,
                strokes: strokes.as_ptr(),
                stroke_count: strokes.len() as u32,
            }
        })
        .collect();
    RawLayers { layers: raw, _strokes: strokes }
}

/// Most faces reported for one photo.
pub const MAX_FACES: usize = 32;

/// Apply adjustments to packed 8-bit RGB pixels exactly as rendering does.
pub fn adjust_rgb8(adjustments: &Adjustments, rgb: &mut [u8]) {
    let count = rgb.len() / 3;
    // SAFETY: `rgb` holds at least `count * 3` bytes; the engine reads and
    // writes exactly that many and allows input and output to be the same.
    let status = unsafe { aibs_adjust_rgb8(adjustments, rgb.as_ptr(), rgb.as_mut_ptr(), count) };
    debug_assert_eq!(status, AIBS_OK);
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
        AIBS_ERR_MODEL_INVALID => Err(EngineError::ModelInvalid),
        AIBS_ERR_INFERENCE_FAILED => Err(EngineError::InferenceFailed),
        _ => Err(EngineError::Internal),
    }
}

#[repr(C)]
struct GpuInfoRaw {
    name: [c_char; 256],
    vendor_id: u32,
    is_software: u32,
    dedicated_memory: u64,
    shared_memory: u64,
}

/// A display adapter reported by the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    pub name: String,
    pub vendor_id: u32,
    /// A software renderer, not a real GPU.
    pub is_software: bool,
    pub dedicated_memory: u64,
    pub shared_memory: u64,
}

/// The system's display adapters, in the order the system prefers them.
pub fn gpu_list() -> Vec<GpuInfo> {
    // SAFETY: a null buffer with capacity 0 only returns the count.
    let count = unsafe { aibs_gpu_list(std::ptr::null_mut(), 0) };
    let mut raw: Vec<GpuInfoRaw> = (0..count)
        .map(|_| GpuInfoRaw {
            name: [0; 256],
            vendor_id: 0,
            is_software: 0,
            dedicated_memory: 0,
            shared_memory: 0,
        })
        .collect();
    // SAFETY: `raw` has room for `count` entries.
    let filled = unsafe { aibs_gpu_list(raw.as_mut_ptr(), count) }.min(count) as usize;
    raw.truncate(filled);
    raw.iter()
        .map(|g| GpuInfo {
            // SAFETY: the engine NUL-terminates the name within the array.
            name: unsafe { CStr::from_ptr(g.name.as_ptr()) }.to_string_lossy().into_owned(),
            vendor_id: g.vendor_id,
            is_software: g.is_software != 0,
            dedicated_memory: g.dedicated_memory,
            shared_memory: g.shared_memory,
        })
        .collect()
}

/// Where a model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Cpu,
    DirectMl,
}

/// ONNX Runtime version linked into the engine.
pub fn ai_runtime_version() -> String {
    // SAFETY: the engine returns a pointer to a static NUL-terminated string.
    unsafe { CStr::from_ptr(aibs_ai_runtime_version()) }.to_string_lossy().into_owned()
}

#[repr(C)]
struct AiSessionRaw {
    _private: [u8; 0],
}

/// A loaded model with one float input and one float output.
pub struct AiSession {
    raw: *mut AiSessionRaw,
    provider: Provider,
    /// Why DirectML is not in use, when it was wanted.
    fallback_reason: Option<String>,
}

// SAFETY: the engine allows runs from several threads; callers that need
// ordering add their own locking.
unsafe impl Send for AiSession {}
unsafe impl Sync for AiSession {}

impl AiSession {
    /// Load a model, preferring `preferred` and falling back to the CPU.
    pub fn open(model: &[u8], preferred: Provider) -> Result<AiSession, (EngineError, String)> {
        let mut raw: *mut AiSessionRaw = std::ptr::null_mut();
        let mut used: i32 = AIBS_PROVIDER_CPU;
        let mut note = [0 as c_char; 512];
        let wanted = match preferred {
            Provider::Cpu => AIBS_PROVIDER_CPU,
            Provider::DirectMl => AIBS_PROVIDER_DIRECTML,
        };
        // SAFETY: all pointers are valid for the call; the note buffer's
        // length is passed so the engine never writes past it.
        let status = unsafe {
            aibs_ai_open(model.as_ptr(), model.len(), wanted, &mut raw, &mut used, note.as_mut_ptr(), note.len())
        };
        // SAFETY: the engine always NUL-terminates the note.
        let note = unsafe { CStr::from_ptr(note.as_ptr()) }.to_string_lossy().into_owned();
        status_to_result(status).map_err(|e| (e, note.clone()))?;
        let provider = if used == AIBS_PROVIDER_DIRECTML { Provider::DirectMl } else { Provider::Cpu };
        Ok(AiSession {
            raw,
            provider,
            fallback_reason: if preferred == Provider::DirectMl && provider == Provider::Cpu {
                Some(note)
            } else {
                None
            },
        })
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn fallback_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }

    /// Run the model. Returns the output values and their shape.
    pub fn run(&self, input: &[f32], shape: &[i64]) -> Result<(Vec<f32>, Vec<i64>), EngineError> {
        let expected: i64 = shape.iter().product();
        if shape.is_empty() || shape.len() > 8 || expected <= 0 || expected as usize != input.len() {
            return Err(EngineError::InvalidArgument);
        }
        let mut output: *mut f32 = std::ptr::null_mut();
        let mut out_shape = [0i64; 8];
        let mut out_rank = 0usize;
        // SAFETY: input and shape are valid for their lengths; outputs are writable.
        status_to_result(unsafe {
            aibs_ai_run(
                self.raw,
                input.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &mut output,
                out_shape.as_mut_ptr(),
                &mut out_rank,
            )
        })?;
        let dims = out_shape[..out_rank].to_vec();
        let count = dims.iter().product::<i64>().max(0) as usize;
        // SAFETY: on success the engine returns `count` floats, freed below.
        let values = unsafe { std::slice::from_raw_parts(output, count) }.to_vec();
        // SAFETY: `output` came from the engine and is freed once.
        unsafe { aibs_ai_free(output) };
        Ok((values, dims))
    }
}

impl Drop for AiSession {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `aibs_ai_open` and is closed once.
        unsafe { aibs_ai_close(self.raw) };
    }
}

impl fmt::Debug for AiSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiSession").field("provider", &self.provider).finish()
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
    /// an `out_width` x `out_height` image, unadjusted. Parts outside the
    /// source are transparent.
    pub fn render(
        &self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        out_width: u32,
        out_height: u32,
    ) -> Result<RgbaImage, EngineError> {
        self.render_adjusted(x, y, w, h, out_width, out_height, None, None, None, None, &[], 0)
    }

    /// Like [`Source::render`], applying portrait retouching, then
    /// `adjustments`, then the background treatment to every output column from `original_columns` on (the
    /// columns left of it show the original).
    #[allow(clippy::too_many_arguments)]
    pub fn render_adjusted(
        &self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        out_width: u32,
        out_height: u32,
        portrait: Option<Portrait<'_>>,
        adjustments: Option<&Adjustments>,
        background: Option<Background<'_>>,
        light: Option<Light<'_>>,
        layers: &[Layer<'_>],
        original_columns: u32,
    ) -> Result<RgbaImage, EngineError> {
        let raw_layers = raw_layers(layers);
        let light_raw = light.map(|l| l.raw());
        let light_ptr = light_raw.as_ref().map_or(std::ptr::null(), |l| l as *const LightRaw);
        let mut raw = empty_image();
        let background_raw = background.map(|b| b.raw());
        let background_ptr = background_raw.as_ref().map_or(std::ptr::null(), |b| b as *const BackgroundRaw);
        let portrait_raw = portrait.map(|p| PortraitRaw {
            faces: p.faces.as_ptr(),
            face_count: p.faces.len().min(MAX_FACES) as u32,
            smoothing: p.smoothing,
            blemishes: p.blemishes,
            dark_circles: p.dark_circles,
            eyes: p.eyes,
            teeth: p.teeth,
            spots: p.spots.as_ptr(),
            spot_count: p.spots.len().min(MAX_SPOTS) as u32,
        });
        let portrait_ptr = portrait_raw.as_ref().map_or(std::ptr::null(), |p| p as *const PortraitRaw);
        let adjustments_ptr = adjustments.map_or(std::ptr::null(), |a| a as *const Adjustments);
        // SAFETY: `self.raw` is valid for the lifetime of `self`; the
        // adjustments pointer is null or valid for the call; `raw` is writable.
        status_to_result(unsafe {
            aibs_source_render(
                self.raw,
                x,
                y,
                w,
                h,
                out_width,
                out_height,
                portrait_ptr,
                adjustments_ptr,
                background_ptr,
                light_ptr,
                raw_layers.layers.as_ptr(),
                raw_layers.layers.len() as u32,
                original_columns,
                &mut raw,
            )
        })?;
        // SAFETY: the render succeeded, so `raw` holds an engine image.
        Ok(unsafe { take_image(raw) })
    }

    /// Find faces with the YuNet detector session `detector`. Each face
    /// comes with its skin colour measured. Faces scoring below
    /// `min_score` (0..1) are left out.
    pub fn detect_faces(&self, detector: &AiSession, min_score: f32) -> Result<Vec<Face>, EngineError> {
        let mut faces = vec![Face::default(); MAX_FACES];
        let mut count = 0u32;
        // SAFETY: both handles are valid; `faces` has room for MAX_FACES.
        status_to_result(unsafe {
            aibs_detect_faces(self.raw, detector.raw, min_score, faces.as_mut_ptr(), MAX_FACES as u32, &mut count)
        })?;
        faces.truncate((count as usize).min(MAX_FACES));
        Ok(faces)
    }

    /// Separate subject and background with the BiRefNet session `model`.
    pub fn segment(&self, model: &AiSession) -> Result<Matte, EngineError> {
        let mut raw: *mut MatteRaw = std::ptr::null_mut();
        // SAFETY: both handles are valid; `raw` is writable.
        status_to_result(unsafe { aibs_segment(self.raw, model.raw, &mut raw) })?;
        Ok(Matte { raw })
    }

    /// Estimate the photo's 3D shape with the Depth Anything session `model`.
    pub fn estimate_shape(&self, model: &AiSession) -> Result<Shape, EngineError> {
        let mut raw: *mut ShapeRaw = std::ptr::null_mut();
        // SAFETY: both handles are valid; `raw` is writable.
        status_to_result(unsafe { aibs_estimate_shape(self.raw, model.raw, &mut raw) })?;
        Ok(Shape { raw })
    }

    /// Suggested adjustments for one-click enhancement.
    pub fn auto_adjust(&self, faces: &[Face]) -> Result<Adjustments, EngineError> {
        let mut out = Adjustments::default();
        // SAFETY: `faces` is valid for its length; `out` is writable.
        status_to_result(unsafe {
            aibs_auto_adjust(self.raw, faces.as_ptr(), faces.len().min(MAX_FACES) as u32, &mut out)
        })?;
        Ok(out)
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

impl fmt::Debug for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Source")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
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
    ModelInvalid,
    InferenceFailed,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            EngineError::InvalidArgument => "invalid argument passed to the engine",
            EngineError::DecodeFailed => "the data is not a readable image",
            EngineError::OutOfMemory => "not enough memory to decode the image",
            EngineError::Internal => "internal engine error",
            EngineError::ModelInvalid => "the model could not be loaded",
            EngineError::InferenceFailed => "the model could not be run on this input",
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
    fn adjustments_match_adjust_rgb8_and_respect_the_split() {
        let source = Source::open(PNG).unwrap();
        let original = source.render(0.0, 0.0, 64.0, 48.0, 64, 48).unwrap();
        let zero = Adjustments::default();
        let unchanged = source
            .render_adjusted(0.0, 0.0, 64.0, 48.0, 64, 48, None, Some(&zero), None, None, &[], 0)
            .unwrap();
        assert_eq!(unchanged.pixels, original.pixels);

        let mut look = Adjustments { exposure: -1.0, saturation: 40.0, ..Default::default() };
        look.hsl_hue[0] = 50.0;
        let split = source
            .render_adjusted(0.0, 0.0, 64.0, 48.0, 64, 48, None, Some(&look), None, None, &[], 32)
            .unwrap();
        for col in [0usize, 31, 32, 63] {
            let p = (10 * 64 + col) * 4;
            let mut expected = [original.pixels[p], original.pixels[p + 1], original.pixels[p + 2]];
            if col >= 32 {
                adjust_rgb8(&look, &mut expected);
            }
            assert_eq!(&split.pixels[p..p + 3], &expected, "column {col}");
        }
    }

    const TEST_MODEL: &[u8] = include_bytes!("../../../models/files/test-luma-v1.onnx");

    #[test]
    fn reports_gpus_and_runtime_version() {
        let gpus = gpu_list();
        if cfg!(windows) {
            assert!(!gpus.is_empty());
        }
        assert!(gpus.iter().all(|g| !g.name.is_empty()));
        assert!(ai_runtime_version().starts_with("1."));
    }

    #[test]
    fn runs_the_test_model_on_cpu_and_preferred_gpu() {
        for preferred in [Provider::Cpu, Provider::DirectMl] {
            let session = AiSession::open(TEST_MODEL, preferred).unwrap();
            if preferred == Provider::Cpu {
                assert_eq!(session.provider(), Provider::Cpu);
                assert!(session.fallback_reason().is_none());
            }
            // 2 x 2 image: pure red, green, blue and white.
            let input = [1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0];
            let (values, shape) = session.run(&input, &[1, 3, 2, 2]).unwrap();
            assert_eq!(shape, [1, 1, 2, 2]);
            for (got, want) in values.iter().zip([0.2126f32, 0.7152, 0.0722, 1.0]) {
                assert!((got - want).abs() < 1e-3, "{got} vs {want}");
            }
        }
    }

    #[test]
    fn rejects_broken_models_and_mismatched_input() {
        let (error, note) = AiSession::open(b"not a model", Provider::Cpu).unwrap_err();
        assert_eq!(error, EngineError::ModelInvalid);
        assert!(!note.is_empty());
        let session = AiSession::open(TEST_MODEL, Provider::Cpu).unwrap();
        assert_eq!(session.run(&[0.5; 3], &[1, 3, 2, 2]), Err(EngineError::InvalidArgument));
        assert_eq!(session.run(&[0.5; 4], &[1, 1, 2, 2]), Err(EngineError::InferenceFailed));
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

    #[test]
    fn portrait_without_faces_changes_nothing() {
        let source = Source::open(PNG).unwrap();
        let plain = source.render(0.0, 0.0, 64.0, 48.0, 64, 48).unwrap();
        let portrait = Portrait { faces: &[], smoothing: 100.0, ..Default::default() };
        let with = source.render_adjusted(0.0, 0.0, 64.0, 48.0, 64, 48, Some(portrait), None, None, None, &[], 0).unwrap();
        assert_eq!(plain.pixels, with.pixels);
    }

    #[test]
    fn auto_adjust_suggests_slider_values() {
        let source = Source::open(PNG).unwrap();
        let suggested = source.auto_adjust(&[]).unwrap();
        assert!(suggested.exposure.abs() <= 2.0);
        assert!(suggested.vibrance > 0.0);
        assert_eq!(suggested.hsl_hue, [0.0; HSL_BANDS]);
    }

    #[test]
    fn detect_faces_finds_none_in_a_gradient() {
        let model = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../models/files/face_detection_yunet_2023mar.onnx"
        ))
        .expect("face model missing: run scripts/fetch_models.py");
        let detector = AiSession::open(&model, Provider::Cpu).unwrap();
        let source = Source::open(PNG).unwrap();
        assert_eq!(source.detect_faces(&detector, 0.6).unwrap(), Vec::<Face>::new());
    }

    #[test]
    fn background_colour_replaces_only_the_background() {
        let source = Source::open(PNG).unwrap();
        // Left half subject.
        let values: Vec<u8> = (0..32 * 24).map(|i| if i % 32 < 16 { 255 } else { 0 }).collect();
        let matte = Matte::from_values(&values, 32, 24, 64, 48).unwrap();
        assert!((matte.coverage() - 0.5).abs() < 1e-3);
        let plain = source.render(0.0, 0.0, 64.0, 48.0, 64, 48).unwrap();
        let background = Background {
            matte: &matte,
            mode: BackgroundMode::Color([1, 2, 3]),
            blur: 0.0,
            brightness: 0.0,
            saturation: 0.0,
            edge: 0.0,
            image: None,
            detail: 0.0,
        };
        let out = source.render_adjusted(0.0, 0.0, 64.0, 48.0, 64, 48, None, None, Some(background), None, &[], 0).unwrap();
        let at = |image: &RgbaImage, x: usize, y: usize| image.pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4].to_vec();
        assert_eq!(at(&out, 5, 10), at(&plain, 5, 10));
        assert_eq!(at(&out, 60, 10), vec![1, 2, 3, 255]);
        assert_eq!(Matte::from_values(&values, 10, 10, 64, 48).unwrap_err(), EngineError::InvalidArgument);
    }

    #[test]
    fn light_brightens_near_it_and_follows_a_shape() {
        let source = Source::open(PNG).unwrap();
        let plain = source.render(0.0, 0.0, 64.0, 48.0, 64, 48).unwrap();
        let depth: Vec<f32> = (0..64 * 48).map(|i| (i % 64) as f32 / 64.0).collect();
        let shape = Shape::from_depth(&depth, 64, 48, 64, 48).unwrap();
        let light = Light {
            kind: LightKind::Softbox,
            x: 0.1,
            y: 0.5,
            intensity: 80.0,
            size: 40.0,
            warmth: 0.0,
            height: 50.0,
            subject: None,
            shape: Some(&shape),
        };
        let lit = source.render_adjusted(0.0, 0.0, 64.0, 48.0, 64, 48, None, None, None, Some(light), &[], 0).unwrap();
        let green = |image: &RgbaImage, x: usize, y: usize| image.pixels[(y * 64 + x) * 4 + 1];
        assert!(green(&lit, 6, 24) > green(&plain, 6, 24));
        assert_eq!(Shape::from_depth(&depth, 10, 10, 64, 48).unwrap_err(), EngineError::InvalidArgument);
    }

    #[test]
    fn encodes_and_round_trips_analysis_results() {
        let rgb: Vec<u8> = (0..16 * 8 * 3).map(|i| (i % 251) as u8).collect();
        let jpeg = encode(&rgb, 16, 8, 3, ImageFormat::Jpeg { quality: 90 }).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        let png = encode(&rgb, 16, 8, 3, ImageFormat::Png).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert_eq!(decode_rgba8(&png, 0).unwrap().width, 16);
        assert!(encode(&rgb, 16, 9, 3, ImageFormat::Png).is_err());

        let matte = Matte::from_values(&[0, 64, 128, 255], 2, 2, 20, 20).unwrap();
        let plane = matte.values().unwrap();
        assert_eq!((plane.width, plane.source_width), (2, 20));
        assert_eq!(plane.values, vec![0, 64, 128, 255]);

        let depth: Vec<f32> = (0..16 * 12).map(|i| (i % 16) as f32).collect();
        let shape = Shape::from_depth(&depth, 16, 12, 32, 24).unwrap();
        let normals = shape.normals().unwrap();
        let again = Shape::from_normals(&normals).unwrap().normals().unwrap();
        assert_eq!(normals.width, again.width);
        assert!(normals.values.iter().zip(&again.values).all(|(a, b)| (a - b).abs() < 1e-5));
    }

    #[test]
    fn layers_with_strokes_render() {
        let source = Source::open(PNG).unwrap();
        let plain = source.render(0.0, 0.0, 64.0, 48.0, 64, 48).unwrap();
        let brighter = Adjustments { exposure: 1.0, ..Default::default() };
        let points = [4.0, 24.0, 60.0, 24.0];
        let strokes = [Stroke { points: &points, size: 8.0, hardness: 1.0, opacity: 1.0, erase: false }];
        let layer = Layer {
            content: LayerContent::Adjustment(&brighter),
            blend: BlendMode::Normal,
            opacity: 1.0,
            mask_base: MaskBase::None,
            mask_invert: false,
            matte: None,
            strokes: &strokes,
        };
        let out = source.render_adjusted(0.0, 0.0, 64.0, 48.0, 64, 48, None, None, None, None, &[layer], 0).unwrap();
        let green = |image: &RgbaImage, x: usize, y: usize| image.pixels[(y * 64 + x) * 4 + 1];
        assert!(green(&out, 30, 24) > green(&plain, 30, 24));
        assert_eq!(green(&out, 30, 5), green(&plain, 30, 5));
    }
}

//! The .aibs project format.
//!
//! A project is a ZIP container (stored entries; images inside are already
//! compressed) holding:
//!
//! - `project.json`: format name and version, the original image's
//!   reference (path, path relative to the project, size, SHA-256), the
//!   undo history of adjustments, detected faces, replacement background
//!   images and the names of the entries below;
//! - `masks/subject.png`: subject/background separation (8-bit grey);
//! - `shape/normals.png`: 3D shape for lighting (surface directions as RGB);
//! - `preview.jpg`: a small preview of the edited image.
//!
//! The original image is referenced, never copied or changed. Older format
//! versions are migrated when read; newer ones are refused with a clear
//! error rather than misread.

use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

use engine_sys::{Face, ImageFormat, Matte, Plane, RgbaImage, Shape};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::document::{Adjustments, History};
use crate::files::{self, FileRef, Located};
use crate::inpaint::PatchData;
use crate::models::ModelRef;

/// File name extension of projects.
pub const EXTENSION: &str = "aibs";
/// Identifies the file as an AI Beauty Studio project.
pub const FORMAT_NAME: &str = "ai-beauty-studio-project";
/// The version this build writes. Readers accept this and older versions.
///
/// - 1: first release (1.0).
/// - 2: records which model (id and version) produced each AI result;
///   `backgroundImages` renamed `images` (they also serve image layers).
pub const FORMAT_VERSION: u32 = 2;

const PROJECT_ENTRY: &str = "project.json";
const MATTE_ENTRY: &str = "masks/subject.png";
const SHAPE_ENTRY: &str = "shape/normals.png";
const PREVIEW_ENTRY: &str = "preview.jpg";

/// True when `path` names a project file (by extension).
pub fn is_project_path(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}

/// Why a project could not be read or written. `code()` is the stable
/// identifier the UI maps to a message.
#[derive(Debug)]
pub enum ProjectError {
    NotFound,
    /// Not a project file at all.
    NotAProject,
    /// Written by a newer version of the app.
    NewerVersion(u32),
    /// A project, but parts of it are damaged.
    Damaged(String),
    /// The original photo is not where the project says.
    OriginalMissing(String),
    /// A different file is where the original photo should be.
    OriginalChanged(String),
    Io(io::Error),
}

impl ProjectError {
    pub fn code(&self) -> &'static str {
        match self {
            ProjectError::NotFound => "not_found",
            ProjectError::NotAProject => "not_a_project",
            ProjectError::NewerVersion(_) => "project_newer_version",
            ProjectError::Damaged(_) => "project_damaged",
            ProjectError::OriginalMissing(_) => "original_missing",
            ProjectError::OriginalChanged(_) => "original_changed",
            ProjectError::Io(_) => "project_io_failed",
        }
    }
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::NotFound => f.write_str("the project file does not exist"),
            ProjectError::NotAProject => f.write_str("not an AI Beauty Studio project"),
            ProjectError::NewerVersion(v) => write!(f, "project format version {v} is newer than this app"),
            ProjectError::Damaged(what) => write!(f, "the project is damaged: {what}"),
            ProjectError::OriginalMissing(path) => write!(f, "the original photo is missing: {path}"),
            ProjectError::OriginalChanged(path) => write!(f, "a different file is at {path}"),
            ProjectError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ProjectError {}

impl From<io::Error> for ProjectError {
    fn from(e: io::Error) -> Self {
        ProjectError::Io(e)
    }
}

/// A face as saved.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceRecord {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub landmarks: [f32; 10],
    pub score: f32,
    pub skin: [f32; 5],
}

impl From<&Face> for FaceRecord {
    fn from(f: &Face) -> Self {
        FaceRecord {
            x: f.x,
            y: f.y,
            width: f.width,
            height: f.height,
            landmarks: f.landmarks,
            score: f.score,
            skin: f.skin,
        }
    }
}

impl From<&FaceRecord> for Face {
    fn from(f: &FaceRecord) -> Self {
        Face {
            x: f.x,
            y: f.y,
            width: f.width,
            height: f.height,
            landmarks: f.landmarks,
            score: f.score,
            skin: f.skin,
        }
    }
}

/// An image entry inside the container and the plane it holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaneEntry {
    pub entry: String,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    /// The model that produced it (unknown for projects from format 1).
    #[serde(default)]
    pub model: Option<ModelRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedHistory {
    pub states: Vec<Adjustments>,
    pub index: usize,
}

/// `project.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub format: String,
    pub version: u32,
    pub app_version: String,
    /// Seconds since the Unix epoch.
    pub saved_at: u64,
    pub original: FileRef,
    pub image_width: u32,
    pub image_height: u32,
    pub history: SavedHistory,
    /// None when detection had not finished.
    #[serde(default)]
    pub faces: Option<Vec<FaceRecord>>,
    /// The model that found the faces.
    #[serde(default)]
    pub faces_model: Option<ModelRef>,
    #[serde(default)]
    pub matte: Option<PlaneEntry>,
    #[serde(default)]
    pub shape: Option<PlaneEntry>,
    /// Images the edits use (replacement backgrounds, image layers);
    /// number n in the adjustments is element n - 1.
    #[serde(default)]
    pub images: Vec<FileRef>,
    #[serde(default)]
    pub preview: Option<String>,
    /// Object-removal and repair patches; their pixels are PNG entries.
    #[serde(default)]
    pub patches: Vec<PatchEntry>,
}

/// A patch inside the container: where it goes, and the entry with its
/// RGBA pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchEntry {
    #[serde(flatten)]
    pub patch: PatchData,
    pub entry: String,
}

/// Which model produced each AI result.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnalysisModels<'a> {
    pub faces: Option<&'a ModelRef>,
    pub matte: Option<&'a ModelRef>,
    pub shape: Option<&'a ModelRef>,
}

/// What to save: the open document's state.
pub struct Snapshot<'a> {
    pub original: &'a FileRef,
    pub image_size: (u32, u32),
    pub history: &'a History,
    pub faces: Option<&'a [Face]>,
    pub matte: Option<&'a Matte>,
    pub shape: Option<&'a Shape>,
    /// The models that produced the faces, matte and shape.
    pub models: AnalysisModels<'a>,
    pub images: &'a [FileRef],
    /// Object-removal and repair patches.
    pub patches: &'a [PatchData],
    /// A small rendering of the edited image (RGBA).
    pub preview: Option<&'a RgbaImage>,
    pub saved_at: u64,
}

fn engine_error(what: &str) -> impl Fn(engine_sys::EngineError) -> ProjectError + '_ {
    move |e| ProjectError::Damaged(format!("{what}: {e}"))
}

/// The entry holding a patch's coverage, next to its colour entry.
fn alpha_entry(colour: &str) -> String {
    format!("{}-alpha.png", colour.trim_end_matches(".png"))
}

fn rgb_of(image: &RgbaImage) -> Vec<u8> {
    image.pixels.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect()
}

/// Write `snapshot` to `path` as a project. The file is replaced only once
/// the new one is complete.
pub fn save(path: &Path, snapshot: &Snapshot) -> Result<(), ProjectError> {
    let project_dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    let relative = |r: &FileRef| project_dir.map_or_else(|| r.clone(), |d| r.relative_to(d));

    let mut entries: Vec<(&str, Vec<u8>)> = Vec::new();
    let matte = match snapshot.matte {
        Some(m) => {
            let plane = m.values().map_err(engine_error("subject mask"))?;
            let png = engine_sys::encode(&plane.values, plane.width, plane.height, 1, ImageFormat::Png)
                .map_err(engine_error("subject mask"))?;
            entries.push((MATTE_ENTRY, png));
            Some(plane_entry(MATTE_ENTRY, &plane, snapshot.models.matte))
        }
        None => None,
    };
    let shape = match snapshot.shape {
        Some(s) => {
            let plane = s.normals().map_err(engine_error("3D shape"))?;
            // Directions -1..1 as 0..255 per channel.
            let bytes: Vec<u8> = plane.values.iter().map(|v| ((v.clamp(-1.0, 1.0) + 1.0) * 127.5).round() as u8).collect();
            let png = engine_sys::encode(&bytes, plane.width, plane.height, 3, ImageFormat::Png)
                .map_err(engine_error("3D shape"))?;
            entries.push((SHAPE_ENTRY, png));
            Some(plane_entry(SHAPE_ENTRY, &plane, snapshot.models.shape))
        }
        None => None,
    };
    let preview = match snapshot.preview {
        Some(image) => {
            let jpeg = engine_sys::encode(
                &rgb_of(image),
                image.width,
                image.height,
                3,
                ImageFormat::Jpeg { quality: 85 },
            )
            .map_err(engine_error("preview"))?;
            entries.push((PREVIEW_ENTRY, jpeg));
            Some(PREVIEW_ENTRY.to_string())
        }
        None => None,
    };
    let mut patch_entries = Vec::new();
    let mut patch_names = Vec::new();
    for patch in snapshot.patches {
        // Colour and coverage as two images (images are read back without
        // their own transparency).
        let rgb: Vec<u8> = patch.rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        let alpha: Vec<u8> = patch.rgba.chunks_exact(4).map(|p| p[3]).collect();
        let (w, h) = (patch.pixel_width, patch.pixel_height);
        let colour = engine_sys::encode(&rgb, w, h, 3, ImageFormat::Png).map_err(engine_error("patch"))?;
        let coverage = engine_sys::encode(&alpha, w, h, 1, ImageFormat::Png).map_err(engine_error("patch"))?;
        let name = format!("patches/{}.png", patch.id);
        patch_entries.push(PatchEntry { patch: patch.clone(), entry: name.clone() });
        patch_names.push((alpha_entry(&name), coverage));
        patch_names.push((name, colour));
    }
    let (states, index) = snapshot.history.parts();
    let file = ProjectFile {
        format: FORMAT_NAME.to_string(),
        version: FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at: snapshot.saved_at,
        original: relative(snapshot.original),
        image_width: snapshot.image_size.0,
        image_height: snapshot.image_size.1,
        history: SavedHistory { states: states.to_vec(), index },
        faces: snapshot.faces.map(|faces| faces.iter().map(FaceRecord::from).collect()),
        faces_model: snapshot.faces.and(snapshot.models.faces).cloned(),
        matte,
        shape,
        images: snapshot.images.iter().map(relative).collect(),
        preview,
        patches: patch_entries,
    };
    let json = serde_json::to_vec_pretty(&file).map_err(io::Error::other)?;

    files::write_atomically_with(path, |out| {
        let mut zip = ZipWriter::new(out);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file(PROJECT_ENTRY, options).map_err(io::Error::other)?;
        zip.write_all(&json)?;
        for (name, bytes) in entries.iter().map(|(n, b)| (*n, b)).chain(patch_names.iter().map(|(n, b)| (n.as_str(), b))) {
            zip.start_file(name, options).map_err(io::Error::other)?;
            zip.write_all(bytes)?;
        }
        zip.finish().map_err(io::Error::other)?;
        Ok(())
    })?;
    Ok(())
}

fn plane_entry<T>(entry: &str, plane: &Plane<T>, model: Option<&ModelRef>) -> PlaneEntry {
    PlaneEntry {
        entry: entry.to_string(),
        width: plane.width,
        height: plane.height,
        source_width: plane.source_width,
        source_height: plane.source_height,
        model: model.cloned(),
    }
}

fn open_archive(path: &Path) -> Result<ZipArchive<File>, ProjectError> {
    let file = File::open(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => ProjectError::NotFound,
        _ => ProjectError::Io(e),
    })?;
    ZipArchive::new(file).map_err(|_| ProjectError::NotAProject)
}

fn read_entry<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Result<Vec<u8>, ProjectError> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| ProjectError::Damaged(format!("{name} is missing")))?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| ProjectError::Damaged(format!("{name}: {e}")))?;
    Ok(bytes)
}

/// Turn any supported version's `project.json` into the current form.
pub fn migrate(value: serde_json::Value) -> Result<ProjectFile, ProjectError> {
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT_NAME) {
        return Err(ProjectError::NotAProject);
    }
    let version = value
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ProjectError::Damaged("no format version".into()))? as u32;
    if version > FORMAT_VERSION {
        return Err(ProjectError::NewerVersion(version));
    }
    // Each step rewrites one version's JSON into the next.
    let mut value = value;
    if version < 2 {
        // 1 -> 2: "backgroundImages" became "images"; model records are
        // new and stay unknown for results saved by version 1.
        if let Some(object) = value.as_object_mut() {
            let images = object.remove("backgroundImages").unwrap_or_else(|| serde_json::json!([]));
            object.insert("images".into(), images);
        }
    }
    let mut file: ProjectFile =
        serde_json::from_value(value).map_err(|e| ProjectError::Damaged(format!("project.json: {e}")))?;
    file.version = FORMAT_VERSION;
    Ok(file)
}

/// Read only `project.json` (for showing what a project refers to).
pub fn inspect(path: &Path) -> Result<ProjectFile, ProjectError> {
    let mut archive = open_archive(path)?;
    let json = read_entry(&mut archive, PROJECT_ENTRY).map_err(|_| ProjectError::NotAProject)?;
    let value: serde_json::Value = serde_json::from_slice(&json).map_err(|_| ProjectError::NotAProject)?;
    migrate(value)
}

/// The project's preview image (JPEG bytes), if it has one.
pub fn read_preview(path: &Path) -> Result<Vec<u8>, ProjectError> {
    let mut archive = open_archive(path)?;
    read_entry(&mut archive, PREVIEW_ENTRY)
}

/// A replacement background image the project refers to, and where it was
/// found (None when it is missing; edits using it then show the original
/// background).
#[derive(Debug, Clone)]
pub struct BackgroundRef {
    pub file: FileRef,
    pub found: Option<PathBuf>,
}

/// A project read back.
pub struct Loaded {
    pub file: ProjectFile,
    /// Where the original photo was found.
    pub original_path: PathBuf,
    pub history: History,
    pub faces: Option<Vec<Face>>,
    pub matte: Option<Matte>,
    pub shape: Option<Shape>,
    pub images: Vec<BackgroundRef>,
    pub patches: Vec<PatchData>,
}

/// Read the project at `path`. The original photo is looked for where the
/// project says (absolute, then relative to the project), or at
/// `original_override` when the user has pointed to it; in every case it
/// must be the same file (size and SHA-256).
pub fn load(path: &Path, original_override: Option<&Path>) -> Result<Loaded, ProjectError> {
    let mut archive = open_archive(path)?;
    let json = read_entry(&mut archive, PROJECT_ENTRY).map_err(|_| ProjectError::NotAProject)?;
    let value: serde_json::Value = serde_json::from_slice(&json).map_err(|_| ProjectError::NotAProject)?;
    let file = migrate(value)?;
    let project_dir = path.parent();

    let original_path = match original_override {
        Some(chosen) => match file.original.matches(chosen) {
            Ok(true) => chosen.to_path_buf(),
            Ok(false) => return Err(ProjectError::OriginalChanged(chosen.to_string_lossy().into_owned())),
            Err(_) => return Err(ProjectError::OriginalMissing(chosen.to_string_lossy().into_owned())),
        },
        None => match files::locate(&file.original, project_dir) {
            Located::Found(found) => found,
            Located::Changed(at) => return Err(ProjectError::OriginalChanged(at.to_string_lossy().into_owned())),
            Located::Missing => return Err(ProjectError::OriginalMissing(file.original.path.clone())),
        },
    };

    let matte = match &file.matte {
        Some(entry) => {
            let png = read_entry(&mut archive, &entry.entry)?;
            let image = engine_sys::decode_rgba8(&png, 0).map_err(engine_error("subject mask"))?;
            check_size(entry, &image)?;
            let values: Vec<u8> = image.pixels.chunks_exact(4).map(|p| p[0]).collect();
            Some(
                Matte::from_values(&values, entry.width, entry.height, entry.source_width, entry.source_height)
                    .map_err(engine_error("subject mask"))?,
            )
        }
        None => None,
    };
    let shape = match &file.shape {
        Some(entry) => {
            let png = read_entry(&mut archive, &entry.entry)?;
            let image = engine_sys::decode_rgba8(&png, 0).map_err(engine_error("3D shape"))?;
            check_size(entry, &image)?;
            let values: Vec<f32> = image
                .pixels
                .chunks_exact(4)
                .flat_map(|p| [p[0], p[1], p[2]].map(|v| v as f32 / 127.5 - 1.0))
                .collect();
            Some(
                Shape::from_normals(&Plane {
                    width: entry.width,
                    height: entry.height,
                    source_width: entry.source_width,
                    source_height: entry.source_height,
                    values,
                })
                .map_err(engine_error("3D shape"))?,
            )
        }
        None => None,
    };
    let images = file
        .images
        .iter()
        .map(|reference| BackgroundRef {
            file: reference.clone(),
            found: match files::locate(reference, project_dir) {
                Located::Found(p) => Some(p),
                _ => None,
            },
        })
        .collect();
    let mut patches = Vec::new();
    for entry in &file.patches {
        let colour = engine_sys::decode_rgba8(&read_entry(&mut archive, &entry.entry)?, 0).map_err(engine_error("patch"))?;
        let coverage = engine_sys::decode_rgba8(&read_entry(&mut archive, &alpha_entry(&entry.entry))?, 0)
            .map_err(engine_error("patch"))?;
        let size = (entry.patch.pixel_width, entry.patch.pixel_height);
        if (colour.width, colour.height) != size || (coverage.width, coverage.height) != size {
            return Err(ProjectError::Damaged(format!("patch {} has the wrong size", entry.patch.id)));
        }
        let rgba = colour
            .pixels
            .chunks_exact(4)
            .zip(coverage.pixels.chunks_exact(4))
            .flat_map(|(c, a)| [c[0], c[1], c[2], a[0]])
            .collect();
        patches.push(PatchData { rgba, ..entry.patch.clone() });
    }
    Ok(Loaded {
        patches,
        history: History::from_parts(file.history.states.clone(), file.history.index),
        faces: file.faces.as_ref().map(|faces| faces.iter().map(Face::from).collect()),
        original_path,
        matte,
        shape,
        images,
        file,
    })
}

fn check_size(entry: &PlaneEntry, image: &RgbaImage) -> Result<(), ProjectError> {
    if image.width != entry.width || image.height != entry.height {
        return Err(ProjectError::Damaged(format!("{} has the wrong size", entry.entry)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::tests::scratch;
    use std::fs;

    const PNG: &[u8] = include_bytes!("../../../tests/fixtures/gradient_64x48.png");

    struct Fixture {
        dir: PathBuf,
        original: PathBuf,
        reference: FileRef,
    }

    fn fixture(name: &str) -> Fixture {
        let dir = scratch(name);
        let original = dir.join("photos").join("相片 1.png");
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(&original, PNG).unwrap();
        let reference = FileRef::of(&original).unwrap();
        Fixture { dir, original, reference }
    }

    fn edited_history() -> History {
        let mut history = History::default();
        history.commit(Adjustments { exposure: 0.5, ..Default::default() });
        history.commit(Adjustments { exposure: 0.5, skin_smoothing: 40.0, ..Default::default() });
        history.undo();
        history
    }

    fn snapshot<'a>(f: &'a Fixture, history: &'a History, preview: Option<&'a RgbaImage>) -> Snapshot<'a> {
        Snapshot {
            original: &f.reference,
            image_size: (64, 48),
            history,
            faces: None,
            matte: None,
            shape: None,
            models: AnalysisModels::default(),
            images: &[],
            patches: &[],
            preview,
            saved_at: 1_700_000_000,
        }
    }

    #[test]
    fn saves_and_restores_everything() {
        let f = fixture("project-roundtrip");
        let history = edited_history();
        let face = Face { x: 1.0, y: 2.0, width: 30.0, height: 40.0, score: 0.9, ..Default::default() };
        let matte = Matte::from_values(&(0..=255u8).collect::<Vec<_>>(), 16, 16, 64, 48).unwrap();
        let depth: Vec<f32> = (0..32 * 24).map(|i| (i % 32) as f32).collect();
        let shape = Shape::from_depth(&depth, 32, 24, 64, 48).unwrap();
        let backdrop = f.dir.join("photos").join("back.png");
        fs::write(&backdrop, PNG).unwrap();
        let backgrounds = [FileRef::of(&backdrop).unwrap()];
        let preview = engine_sys::Source::open(PNG).unwrap().render_fit(32).unwrap();
        let patches = vec![PatchData {
            id: 3,
            x: 4.0,
            y: 5.0,
            width: 10.0,
            height: 6.0,
            pixel_width: 5,
            pixel_height: 3,
            rgba: (0..60).map(|i| (i * 4) as u8).collect(),
            model: Some(ModelRef { id: "inpaint-lama".into(), version: "carve-lama-fp32".into() }),
        }];
        let path = f.dir.join("projects").join("我的專案.aibs");
        let yunet = ModelRef { id: "face-yunet".into(), version: "2023mar".into() };
        let birefnet = ModelRef { id: "background-birefnet-lite".into(), version: "v1".into() };
        let snap = Snapshot {
            faces: Some(std::slice::from_ref(&face)),
            matte: Some(&matte),
            shape: Some(&shape),
            models: AnalysisModels { faces: Some(&yunet), matte: Some(&birefnet), shape: None },
            images: &backgrounds,
            patches: &patches,
            ..snapshot(&f, &history, Some(&preview))
        };
        save(&path, &snap).unwrap();
        assert!(is_project_path(&path));

        let loaded = load(&path, None).unwrap();
        assert_eq!(loaded.patches, patches);
        assert_eq!(loaded.original_path, f.original);
        assert_eq!(loaded.history.state(), history.state());
        assert_eq!(loaded.history.parts().0.len(), 3);
        assert_eq!(loaded.faces.as_deref(), Some(&[face][..]));
        assert_eq!(loaded.matte.unwrap().values().unwrap(), matte.values().unwrap());
        let normals = shape.normals().unwrap();
        let restored = loaded.shape.unwrap().normals().unwrap();
        assert_eq!((restored.width, restored.source_width), (normals.width, normals.source_width));
        let worst = normals.values.iter().zip(&restored.values).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(worst < 0.02, "normals within 8-bit precision, worst {worst}");
        assert_eq!(loaded.images[0].found.as_deref(), Some(backdrop.as_path()));
        assert_eq!(loaded.file.faces_model.as_ref(), Some(&yunet));
        assert_eq!(loaded.file.matte.as_ref().unwrap().model.as_ref(), Some(&birefnet));
        assert_eq!(loaded.file.shape.as_ref().unwrap().model, None);
        assert_eq!(loaded.file.version, FORMAT_VERSION);
        assert_eq!(loaded.file.original.relative_path.as_deref().map(Path::new), Some(Path::new("../photos/相片 1.png")));
        assert_eq!(&read_preview(&path).unwrap()[..2], &[0xFF, 0xD8]);
        assert_eq!(inspect(&path).unwrap().saved_at, 1_700_000_000);
        // The original photo is untouched.
        assert_eq!(fs::read(&f.original).unwrap(), PNG);
    }

    #[test]
    fn finds_a_moved_original_or_asks_for_it() {
        let f = fixture("project-moved");
        let history = History::default();
        let path = f.dir.join("p.aibs");
        save(&path, &snapshot(&f, &history, None)).unwrap();

        let elsewhere = f.dir.join("elsewhere.png");
        fs::rename(&f.original, &elsewhere).unwrap();
        match load(&path, None) {
            Err(ProjectError::OriginalMissing(p)) => assert!(p.ends_with("相片 1.png")),
            other => panic!("expected original_missing, got {:?}", other.err().map(|e| e.code())),
        }
        // The user points to it.
        assert_eq!(load(&path, Some(&elsewhere)).unwrap().original_path, elsewhere);
        // A different photo is refused.
        let other = f.dir.join("other.png");
        fs::write(&other, b"not the same photo").unwrap();
        assert_eq!(load(&path, Some(&other)).err().unwrap().code(), "original_changed");
        fs::write(&f.original, b"replaced").unwrap();
        assert_eq!(load(&path, None).err().unwrap().code(), "original_changed");
    }

    #[test]
    fn refuses_other_files_newer_versions_and_damage() {
        let f = fixture("project-bad");
        let history = History::default();
        let not_zip = f.dir.join("photo.aibs");
        fs::write(&not_zip, PNG).unwrap();
        assert_eq!(load(&not_zip, None).err().unwrap().code(), "not_a_project");
        assert_eq!(load(&f.dir.join("none.aibs"), None).err().unwrap().code(), "not_found");

        let mut value = serde_json::to_value(ProjectFile {
            format: FORMAT_NAME.into(),
            version: FORMAT_VERSION + 1,
            app_version: "9".into(),
            saved_at: 0,
            original: f.reference.clone(),
            image_width: 64,
            image_height: 48,
            history: SavedHistory { states: vec![Adjustments::default()], index: 0 },
            faces: None,
            faces_model: None,
            matte: None,
            shape: None,
            images: vec![],
            preview: None,
            patches: vec![],
        })
        .unwrap();
        assert_eq!(migrate(value.clone()).err().unwrap().code(), "project_newer_version");
        value["format"] = "something-else".into();
        assert_eq!(migrate(value).err().unwrap().code(), "not_a_project");

        // A project whose mask entry is missing is reported as damaged.
        let path = f.dir.join("damaged.aibs");
        let matte = Matte::from_values(&[255; 4], 2, 2, 64, 48).unwrap();
        save(&path, &Snapshot { matte: Some(&matte), ..snapshot(&f, &history, None) }).unwrap();
        let mut archive = ZipArchive::new(File::open(&path).unwrap()).unwrap();
        let json = read_entry(&mut archive, PROJECT_ENTRY).unwrap();
        let broken = f.dir.join("broken.aibs");
        let mut zip = ZipWriter::new(File::create(&broken).unwrap());
        zip.start_file(PROJECT_ENTRY, SimpleFileOptions::default().compression_method(CompressionMethod::Stored))
            .unwrap();
        zip.write_all(&json).unwrap();
        zip.finish().unwrap();
        assert_eq!(load(&broken, None).err().unwrap().code(), "project_damaged");
    }

    #[test]
    fn missing_background_images_do_not_stop_loading() {
        let f = fixture("project-backdrop");
        let history = History::default();
        let backdrop = f.dir.join("back.png");
        fs::write(&backdrop, PNG).unwrap();
        let backgrounds = [FileRef::of(&backdrop).unwrap()];
        let path = f.dir.join("p.aibs");
        save(&path, &Snapshot { images: &backgrounds, ..snapshot(&f, &history, None) }).unwrap();
        fs::remove_file(&backdrop).unwrap();
        let loaded = load(&path, None).unwrap();
        assert_eq!(loaded.images.len(), 1);
        assert!(loaded.images[0].found.is_none());
    }

    /// A project written by version 1.0 (format 1) opens in this version.
    #[test]
    fn format_1_projects_are_migrated() {
        let f = fixture("project-v1");
        let backdrop = f.dir.join("back.png");
        fs::write(&backdrop, PNG).unwrap();
        let v1 = serde_json::json!({
            "format": FORMAT_NAME,
            "version": 1,
            "appVersion": "1.0.0",
            "savedAt": 5,
            "original": f.reference,
            "imageWidth": 64,
            "imageHeight": 48,
            "history": { "states": [{ "exposure": 0.5 }], "index": 0 },
            "faces": [],
            "matte": null,
            "shape": null,
            "backgroundImages": [FileRef::of(&backdrop).unwrap()],
            "preview": null
        });
        let path = f.dir.join("old.aibs");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file(PROJECT_ENTRY, SimpleFileOptions::default().compression_method(CompressionMethod::Stored))
            .unwrap();
        zip.write_all(&serde_json::to_vec(&v1).unwrap()).unwrap();
        zip.finish().unwrap();

        let loaded = load(&path, None).unwrap();
        assert_eq!(loaded.file.version, FORMAT_VERSION);
        assert_eq!(loaded.images.len(), 1);
        assert_eq!(loaded.images[0].found.as_deref(), Some(backdrop.as_path()));
        assert_eq!(loaded.faces, Some(vec![]));
        assert_eq!(loaded.file.faces_model, None);
        assert_eq!(loaded.history.current().exposure, 0.5);
    }
}

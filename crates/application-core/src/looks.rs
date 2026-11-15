//! Looks: LUT presets (built in, or imported .cube files kept in the app's
//! LUT folder) and the .cube reader.
//!
//! The built-in presets are made here from simple colour formulas, so no
//! third-party LUT files are shipped. A LUT is chosen by id:
//! `builtin:<name>` or `user:<file stem>`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use engine_sys::Lut;
use serde::Serialize;

/// Size of the built-in LUTs (points per side).
const BUILTIN_SIZE: usize = 33;
/// Largest .cube file read (a 65-point LUT is about 8 MB of text).
const MAX_CUBE_BYTES: u64 = 32 * 1024 * 1024;

/// The built-in presets, in the order shown.
pub const BUILTIN: &[&str] = &["warm", "cool", "film", "fade", "teal-orange", "vivid", "mono", "sepia"];

/// LUT values: size^3 RGB triples (0..1), red changing fastest.
#[derive(Debug, Clone, PartialEq)]
pub struct LutData {
    pub size: u32,
    pub rgb: Vec<f32>,
}

impl LutData {
    /// Build a LUT by applying `f` to every grid colour.
    fn from_fn(size: usize, f: impl Fn([f32; 3]) -> [f32; 3]) -> LutData {
        let d = (size - 1) as f32;
        let mut rgb = Vec::with_capacity(size * size * size * 3);
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    let out = f([r as f32 / d, g as f32 / d, b as f32 / d]);
                    rgb.extend(out.iter().map(|v| v.clamp(0.0, 1.0)));
                }
            }
        }
        LutData { size: size as u32, rgb }
    }

    pub fn to_engine(&self) -> Result<Lut, engine_sys::EngineError> {
        Lut::new(&self.rgb, self.size)
    }
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn saturate(c: [f32; 3], amount: f32) -> [f32; 3] {
    let l = luma(c);
    c.map(|v| l + (v - l) * amount)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A gentle S curve around middle grey.
fn s_curve(v: f32, strength: f32) -> f32 {
    let s = v + strength * (v - 0.5) * (1.0 - (2.0 * v - 1.0).abs());
    s.clamp(0.0, 1.0)
}

/// A built-in preset by name.
pub fn builtin_lut(name: &str) -> Option<LutData> {
    let f: Box<dyn Fn([f32; 3]) -> [f32; 3]> = match name {
        "warm" => Box::new(|[r, g, b]| [r * 1.06 + 0.02, g * 1.01 + 0.01, b * 0.9]),
        "cool" => Box::new(|[r, g, b]| [r * 0.92, g * 0.99 + 0.01, b * 1.07 + 0.02]),
        "film" => Box::new(|c| {
            let c = saturate(c, 0.85);
            let l = luma(c);
            // Lifted blacks, softened highlights, warm highlights, cool shadows.
            let lift = |v: f32| 0.05 + v * 0.9;
            let shade = 1.0 - smoothstep(0.0, 0.5, l);
            let light = smoothstep(0.5, 1.0, l);
            [
                s_curve(lift(c[0]), 0.25) + 0.03 * light - 0.01 * shade,
                s_curve(lift(c[1]), 0.25) + 0.01 * light,
                s_curve(lift(c[2]), 0.25) - 0.03 * light + 0.03 * shade,
            ]
        }),
        "fade" => Box::new(|c| {
            let c = saturate(c, 0.75);
            c.map(|v| 0.1 + v * 0.82)
        }),
        "teal-orange" => Box::new(|c| {
            let l = luma(c);
            let shade = 1.0 - smoothstep(0.1, 0.6, l);
            let light = smoothstep(0.4, 0.9, l);
            let c = [
                c[0] - 0.06 * shade + 0.08 * light,
                c[1] + 0.02 * shade + 0.02 * light,
                c[2] + 0.07 * shade - 0.08 * light,
            ];
            c.map(|v| s_curve(v, 0.2))
        }),
        "vivid" => Box::new(|c| saturate(c, 1.35).map(|v| s_curve(v, 0.3))),
        "mono" => Box::new(|c| {
            let l = s_curve(luma(c), 0.2);
            [l, l, l]
        }),
        "sepia" => Box::new(|c| {
            let l = luma(c);
            [l * 1.07 + 0.03, l * 0.95 + 0.02, l * 0.76]
        }),
        _ => return None,
    };
    Some(LutData::from_fn(BUILTIN_SIZE, f))
}

/// Why a .cube file could not be used.
#[derive(Debug, PartialEq, Eq)]
pub enum CubeError {
    /// Not a 3D .cube LUT (missing LUT_3D_SIZE, a 1D LUT, bad numbers).
    Invalid,
    /// Size outside 2..=65, or the number of values does not match it.
    WrongSize,
    TooLarge,
}

impl CubeError {
    pub fn code(&self) -> &'static str {
        match self {
            CubeError::Invalid => "lut_invalid",
            CubeError::WrongSize => "lut_invalid",
            CubeError::TooLarge => "lut_too_large",
        }
    }
}

/// Read a .cube 3D LUT (Adobe/Resolve format): `LUT_3D_SIZE n`, optional
/// `DOMAIN_MIN`/`DOMAIN_MAX` and `TITLE`, `#` comments, then n^3 lines of
/// R G B with red changing fastest.
pub fn parse_cube(text: &str) -> Result<LutData, CubeError> {
    let mut size: Option<usize> = None;
    let mut min = [0.0f32; 3];
    let mut max = [1.0f32; 3];
    let mut values: Vec<f32> = Vec::new();
    let numbers = |rest: &str| -> Result<[f32; 3], CubeError> {
        let parts: Vec<f32> = rest
            .split_whitespace()
            .map(|p| p.parse::<f32>().map_err(|_| CubeError::Invalid))
            .collect::<Result<_, _>>()?;
        if parts.len() != 3 || parts.iter().any(|v| !v.is_finite()) {
            return Err(CubeError::Invalid);
        }
        Ok([parts[0], parts[1], parts[2]])
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let keyword = line.split_whitespace().next().unwrap_or("");
        let rest = line[keyword.len()..].trim();
        match keyword {
            "TITLE" => {}
            "LUT_1D_SIZE" | "LUT_1D_INPUT_RANGE" => return Err(CubeError::Invalid),
            "LUT_3D_SIZE" => {
                let n: usize = rest.parse().map_err(|_| CubeError::Invalid)?;
                if !(2..=65).contains(&n) {
                    return Err(CubeError::WrongSize);
                }
                size = Some(n);
                values.reserve(n * n * n * 3);
            }
            "DOMAIN_MIN" => min = numbers(rest)?,
            "DOMAIN_MAX" => max = numbers(rest)?,
            "LUT_3D_INPUT_RANGE" => {
                let parts: Vec<f32> = rest.split_whitespace().filter_map(|p| p.parse().ok()).collect();
                if parts.len() != 2 {
                    return Err(CubeError::Invalid);
                }
                min = [parts[0]; 3];
                max = [parts[1]; 3];
            }
            _ => {
                if keyword.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
                    // Other keywords (some tools add their own) are skipped.
                    continue;
                }
                let triple = numbers(line)?;
                values.extend_from_slice(&triple);
            }
        }
    }
    let n = size.ok_or(CubeError::Invalid)?;
    if values.len() != n * n * n * 3 {
        return Err(CubeError::WrongSize);
    }
    if (0..3).any(|i| !(max[i] > min[i])) {
        return Err(CubeError::Invalid);
    }
    // Output values are in the same domain; bring them to 0..1.
    for (i, v) in values.iter_mut().enumerate() {
        let c = i % 3;
        *v = ((*v - min[c]) / (max[c] - min[c])).clamp(0.0, 1.0);
    }
    Ok(LutData { size: n as u32, rgb: values })
}

/// A LUT the user can choose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LutEntry {
    pub id: String,
    /// The preset name (built-in, translated by the UI) or the file's name.
    pub name: String,
    pub builtin: bool,
}

/// The built-in presets and the user's imported LUTs, with loaded LUTs kept
/// for reuse.
pub struct LutLibrary {
    dir: PathBuf,
    loaded: Mutex<HashMap<String, Arc<Lut>>>,
}

impl LutLibrary {
    /// A library keeping imported LUTs in `dir` (created when needed).
    pub fn new(dir: PathBuf) -> LutLibrary {
        LutLibrary { dir, loaded: Mutex::new(HashMap::new()) }
    }

    pub fn list(&self) -> Vec<LutEntry> {
        let mut entries: Vec<LutEntry> = BUILTIN
            .iter()
            .map(|name| LutEntry { id: format!("builtin:{name}"), name: name.to_string(), builtin: true })
            .collect();
        let mut user: Vec<LutEntry> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let is_cube = path.extension().is_some_and(|x| x.eq_ignore_ascii_case("cube"));
                let stem = path.file_stem()?.to_string_lossy().into_owned();
                is_cube.then(|| LutEntry { id: format!("user:{stem}"), name: stem, builtin: false })
            })
            .collect();
        user.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        entries.extend(user);
        entries
    }

    /// Copy a .cube file into the library (after checking it reads) and
    /// return its id. A name already taken gets a number added.
    pub fn import(&self, path: &Path) -> Result<LutEntry, String> {
        let size = fs::metadata(path).map_err(|_| "read_failed".to_string())?.len();
        if size > MAX_CUBE_BYTES {
            return Err(CubeError::TooLarge.code().to_string());
        }
        let text = fs::read_to_string(path).map_err(|_| "lut_invalid".to_string())?;
        parse_cube(&text).map_err(|e| e.code().to_string())?;
        let stem: String = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
            .chars()
            .map(|c| if c.is_alphanumeric() || " -_().".contains(c) { c } else { '_' })
            .collect::<String>()
            .trim()
            .to_string();
        let stem = if stem.is_empty() { "LUT".to_string() } else { stem };
        fs::create_dir_all(&self.dir).map_err(|_| "lut_write_failed".to_string())?;
        let mut name = stem.clone();
        let mut n = 2;
        while self.dir.join(format!("{name}.cube")).exists() {
            name = format!("{stem} ({n})");
            n += 1;
        }
        fs::write(self.dir.join(format!("{name}.cube")), text).map_err(|_| "lut_write_failed".to_string())?;
        Ok(LutEntry { id: format!("user:{name}"), name, builtin: false })
    }

    /// The LUT with this id, loaded once; None when it does not exist (a
    /// removed user LUT) or cannot be read.
    pub fn get(&self, id: &str) -> Option<Arc<Lut>> {
        if id.is_empty() {
            return None;
        }
        if let Some(lut) = self.loaded.lock().ok()?.get(id) {
            return Some(lut.clone());
        }
        let data = if let Some(name) = id.strip_prefix("builtin:") {
            builtin_lut(name)?
        } else if let Some(name) = id.strip_prefix("user:") {
            if name.contains(['/', '\\']) || name.contains("..") {
                return None;
            }
            let path = self.dir.join(format!("{name}.cube"));
            if fs::metadata(&path).ok()?.len() > MAX_CUBE_BYTES {
                return None;
            }
            parse_cube(&fs::read_to_string(path).ok()?).ok()?
        } else {
            return None;
        };
        let lut = Arc::new(data.to_engine().ok()?);
        self.loaded.lock().ok()?.insert(id.to_string(), lut.clone());
        Some(lut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::tests::scratch;

    #[test]
    fn builtin_presets_are_valid_luts() {
        for name in BUILTIN {
            let lut = builtin_lut(name).unwrap();
            assert_eq!(lut.rgb.len(), BUILTIN_SIZE.pow(3) * 3, "{name}");
            assert!(lut.rgb.iter().all(|v| (0.0..=1.0).contains(v)), "{name}");
            lut.to_engine().unwrap();
        }
        assert!(builtin_lut("nope").is_none());
        // Mono has no colour; warm makes white-ish grey warmer.
        let mono = builtin_lut("mono").unwrap();
        assert!(mono.rgb.chunks(3).all(|c| c[0] == c[1] && c[1] == c[2]));
        let warm = builtin_lut("warm").unwrap();
        let mid = (BUILTIN_SIZE / 2) * (1 + BUILTIN_SIZE + BUILTIN_SIZE * BUILTIN_SIZE) * 3;
        assert!(warm.rgb[mid] > warm.rgb[mid + 2]);
    }

    #[test]
    fn cube_files_are_read_with_their_domain() {
        let text = "# a comment\nTITLE \"swap\"\nLUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 2 2 2\n\
                    0 0 0\n0 0 2\n0 2 0\n0 2 2\n2 0 0\n2 0 2\n2 2 0\n2 2 2\n";
        let lut = parse_cube(text).unwrap();
        assert_eq!(lut.size, 2);
        assert_eq!(&lut.rgb[3..6], &[0.0, 0.0, 1.0]);
        assert_eq!(parse_cube("LUT_1D_SIZE 4\n0 0 0\n"), Err(CubeError::Invalid));
        assert_eq!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n"), Err(CubeError::WrongSize));
        assert_eq!(parse_cube("LUT_3D_SIZE 99\n"), Err(CubeError::WrongSize));
        assert_eq!(parse_cube("0 0 0\n"), Err(CubeError::Invalid));
        assert_eq!(parse_cube("LUT_3D_SIZE 2\n0 0 x\n"), Err(CubeError::Invalid));
    }

    #[test]
    fn the_library_imports_lists_and_loads() {
        let dir = scratch("luts");
        let library = LutLibrary::new(dir.join("luts"));
        assert_eq!(library.list().len(), BUILTIN.len());
        let source = dir.join("My Look.cube");
        let mut text = String::from("LUT_3D_SIZE 2\n");
        for i in 0..8 {
            text += &format!("{} {} {}\n", i & 1, (i >> 1) & 1, (i >> 2) & 1);
        }
        fs::write(&source, &text).unwrap();
        let first = library.import(&source).unwrap();
        let second = library.import(&source).unwrap();
        assert_eq!(first.id, "user:My Look");
        assert_eq!(second.id, "user:My Look (2)");
        let ids: Vec<String> = library.list().into_iter().map(|e| e.id).collect();
        assert!(ids.contains(&"user:My Look".to_string()) && ids.contains(&"builtin:film".to_string()));
        assert_eq!(library.get("user:My Look").unwrap().size(), 2);
        assert!(library.get("builtin:warm").is_some());
        assert!(library.get("user:../secret").is_none());
        assert!(library.get("user:missing").is_none());
        assert!(library.get("").is_none());
        let bad = dir.join("bad.cube");
        fs::write(&bad, "LUT_3D_SIZE 2\n1 2\n").unwrap();
        assert_eq!(library.import(&bad).unwrap_err(), "lut_invalid");
    }
}

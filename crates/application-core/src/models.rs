//! The model registry: which AI models the app may load.
//!
//! A model loads only when its file matches the SHA-256 recorded in
//! `registry.json` and its license record exists with `"approved": true`.
//! This turns "every model needs a verified license" into a check the app
//! performs every time, not a promise in a document.

use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub id: String,
    pub version: String,
    /// Model file, relative to the registry folder.
    pub file: String,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    pub purpose: String,
    /// License record, relative to the registry folder.
    pub license: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    format_version: u32,
    models: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LicenseRecord {
    model: String,
    license: String,
    approved: bool,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    commercial_use: Option<bool>,
    #[serde(default)]
    checked_on: Option<String>,
}

/// Which model (and version) produced a result, as recorded in projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    pub id: String,
    pub version: String,
}

/// A model's license record, as shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseSummary {
    pub license: String,
    pub approved: bool,
    pub source: Option<String>,
    pub commercial_use: Option<bool>,
    pub checked_on: Option<String>,
}

/// What the app knows about one registered model without reading all of
/// its file (see [`Registry::verify`] for the full check).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub version: String,
    pub purpose: String,
    /// Size of the file on disk, when it is there.
    pub size: Option<u64>,
    pub sha256: String,
    pub license: Option<LicenseSummary>,
    /// Why the model cannot be used (an error code), if it cannot.
    pub problem: Option<String>,
}

/// Why a model may not be used. `code()` is the stable identifier the UI
/// maps to a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    RegistryUnreadable(String),
    UnknownModel(String),
    FileMissing(String),
    HashMismatch(String),
    LicenseMissing(String),
    LicenseNotApproved(String),
}

impl ModelError {
    pub fn code(&self) -> &'static str {
        match self {
            ModelError::RegistryUnreadable(_) => "model_registry_unreadable",
            ModelError::UnknownModel(_) => "model_unknown",
            ModelError::FileMissing(_) => "model_missing",
            ModelError::HashMismatch(_) => "model_damaged",
            ModelError::LicenseMissing(_) | ModelError::LicenseNotApproved(_) => "model_license_unapproved",
        }
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::RegistryUnreadable(why) => write!(f, "the model registry could not be read: {why}"),
            ModelError::UnknownModel(id) => write!(f, "model {id} is not in the registry"),
            ModelError::FileMissing(id) => write!(f, "the file of model {id} is missing"),
            ModelError::HashMismatch(id) => write!(f, "the file of model {id} does not match its recorded hash"),
            ModelError::LicenseMissing(id) => write!(f, "model {id} has no license record"),
            ModelError::LicenseNotApproved(id) => write!(f, "the license of model {id} is not approved"),
        }
    }
}

impl std::error::Error for ModelError {}

pub struct Registry {
    root: PathBuf,
    models: Vec<ModelEntry>,
}

/// Paths in the registry must stay inside its folder.
fn inside(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute() || path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(root.join(path))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

impl Registry {
    /// Read `registry.json` in `root`.
    pub fn load(root: &Path) -> Result<Registry, ModelError> {
        let text = fs::read_to_string(root.join("registry.json"))
            .map_err(|e| ModelError::RegistryUnreadable(e.to_string()))?;
        let file: RegistryFile =
            serde_json::from_str(&text).map_err(|e| ModelError::RegistryUnreadable(e.to_string()))?;
        if file.format_version != 1 {
            return Err(ModelError::RegistryUnreadable(format!(
                "unsupported format version {}",
                file.format_version
            )));
        }
        Ok(Registry {
            root: root.to_path_buf(),
            models: file.models,
        })
    }

    pub fn models(&self) -> &[ModelEntry] {
        &self.models
    }

    pub fn entry(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|m| m.id == id)
    }

    /// The model's id and version, for recording what produced a result.
    pub fn model_ref(&self, id: &str) -> Option<ModelRef> {
        self.entry(id).map(|m| ModelRef { id: m.id.clone(), version: m.version.clone() })
    }

    fn license_record(&self, entry: &ModelEntry) -> Option<LicenseRecord> {
        let path = inside(&self.root, &entry.license)?;
        let text = fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn check_license(&self, entry: &ModelEntry) -> Result<LicenseRecord, ModelError> {
        let record = self.license_record(entry).ok_or_else(|| ModelError::LicenseMissing(entry.id.clone()))?;
        if record.model != entry.id || !record.approved || record.license.trim().is_empty() {
            return Err(ModelError::LicenseNotApproved(entry.id.clone()));
        }
        Ok(record)
    }

    /// Every registered model with its license and whether its file is
    /// there. Quick: file sizes only, no hashing.
    pub fn inspect(&self) -> Vec<ModelInfo> {
        self.models
            .iter()
            .map(|entry| {
                let license = self.license_record(entry).map(|r| LicenseSummary {
                    license: r.license.clone(),
                    approved: r.approved && r.model == entry.id,
                    source: r.source.clone(),
                    commercial_use: r.commercial_use,
                    checked_on: r.checked_on.clone(),
                });
                let size = inside(&self.root, &entry.file).and_then(|p| fs::metadata(p).ok()).map(|m| m.len());
                let problem = match (self.check_license(entry), size) {
                    (Err(e), _) => Some(e.code().to_string()),
                    (Ok(_), None) => Some(ModelError::FileMissing(entry.id.clone()).code().to_string()),
                    (Ok(_), Some(_)) => None,
                };
                ModelInfo {
                    id: entry.id.clone(),
                    version: entry.version.clone(),
                    purpose: entry.purpose.clone(),
                    size,
                    sha256: entry.sha256.clone(),
                    license,
                    problem,
                }
            })
            .collect()
    }

    /// Full check of one model: license record and the file's SHA-256
    /// (read in pieces, so large models do not need to fit in memory).
    pub fn verify(&self, id: &str) -> Result<(), ModelError> {
        let entry = self.entry(id).ok_or_else(|| ModelError::UnknownModel(id.to_string()))?;
        self.check_license(entry)?;
        let path = inside(&self.root, &entry.file).ok_or_else(|| ModelError::FileMissing(id.into()))?;
        let hash = crate::files::sha256_file(&path).map_err(|_| ModelError::FileMissing(id.into()))?;
        if hash != entry.sha256.to_ascii_lowercase() {
            return Err(ModelError::HashMismatch(id.into()));
        }
        Ok(())
    }

    /// The model's bytes, after checking its license record and hash.
    pub fn load_model(&self, id: &str) -> Result<(ModelEntry, Vec<u8>), ModelError> {
        let entry = self
            .models
            .iter()
            .find(|m| m.id == id)
            .ok_or_else(|| ModelError::UnknownModel(id.to_string()))?
            .clone();

        self.check_license(&entry)?;

        let file_path = inside(&self.root, &entry.file).ok_or_else(|| ModelError::FileMissing(id.into()))?;
        let bytes = fs::read(file_path).map_err(|_| ModelError::FileMissing(id.into()))?;
        if sha256_hex(&bytes) != entry.sha256.to_ascii_lowercase() {
            return Err(ModelError::HashMismatch(id.into()));
        }
        Ok((entry, bytes))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn repo_models() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models")
    }

    /// A copy of the repository's models folder that a test may damage.
    struct Copy(PathBuf);

    impl Copy {
        fn new(name: &str) -> Copy {
            let dir = std::env::temp_dir().join(format!("aibs-models-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            for sub in ["files", "licenses"] {
                fs::create_dir_all(dir.join(sub)).unwrap();
                for entry in fs::read_dir(repo_models().join(sub)).unwrap() {
                    let entry = entry.unwrap();
                    fs::copy(entry.path(), dir.join(sub).join(entry.file_name())).unwrap();
                }
            }
            fs::copy(repo_models().join("registry.json"), dir.join("registry.json")).unwrap();
            Copy(dir)
        }
    }

    impl Drop for Copy {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_shipped_registry_loads_and_its_models_verify() {
        let registry = Registry::load(&repo_models()).unwrap();
        assert!(!registry.models().is_empty());
        for model in registry.models() {
            let (entry, bytes) = registry.load_model(&model.id).unwrap();
            assert_eq!(entry.id, model.id);
            assert!(!bytes.is_empty());
        }
    }

    #[test]
    fn a_changed_model_file_is_refused() {
        let copy = Copy::new("hash");
        let path = copy.0.join("files/test-luma-v1.onnx");
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, bytes).unwrap();
        let error = Registry::load(&copy.0).unwrap().load_model("test-luma").unwrap_err();
        assert_eq!(error.code(), "model_damaged");
    }

    #[test]
    fn a_model_without_an_approved_license_is_refused() {
        let copy = Copy::new("license");
        let record = copy.0.join("licenses/test-luma.json");
        let text = fs::read_to_string(&record).unwrap().replace("\"approved\": true", "\"approved\": false");
        fs::write(&record, text).unwrap();
        let registry = Registry::load(&copy.0).unwrap();
        assert_eq!(
            registry.load_model("test-luma").unwrap_err(),
            ModelError::LicenseNotApproved("test-luma".into())
        );
        fs::remove_file(&record).unwrap();
        assert_eq!(
            registry.load_model("test-luma").unwrap_err(),
            ModelError::LicenseMissing("test-luma".into())
        );
    }

    #[test]
    fn missing_files_unknown_ids_and_escaping_paths_are_refused() {
        let copy = Copy::new("missing");
        fs::remove_file(copy.0.join("files/test-luma-v1.onnx")).unwrap();
        let registry = Registry::load(&copy.0).unwrap();
        assert_eq!(registry.load_model("test-luma").unwrap_err().code(), "model_missing");
        assert_eq!(registry.load_model("nope").unwrap_err().code(), "model_unknown");

        let text = fs::read_to_string(copy.0.join("registry.json"))
            .unwrap()
            .replace("files/test-luma-v1.onnx", "../outside.onnx");
        fs::write(copy.0.join("registry.json"), text).unwrap();
        let registry = Registry::load(&copy.0).unwrap();
        assert_eq!(registry.load_model("test-luma").unwrap_err().code(), "model_missing");
    }

    #[test]
    fn an_unreadable_registry_is_reported() {
        let copy = Copy::new("broken");
        fs::write(copy.0.join("registry.json"), "{ not json").unwrap();
        assert_eq!(Registry::load(&copy.0).err().unwrap().code(), "model_registry_unreadable");
    }

    #[test]
    fn hashes_are_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn inspection_lists_licenses_and_problems_and_verify_checks_hashes() {
        let registry = Registry::load(&repo_models()).unwrap();
        let infos = registry.inspect();
        assert_eq!(infos.len(), registry.models().len());
        for info in &infos {
            assert_eq!(info.problem, None, "{} should be usable", info.id);
            assert!(info.size.unwrap() > 0);
            let license = info.license.as_ref().unwrap();
            assert!(license.approved && !license.license.is_empty());
            registry.verify(&info.id).unwrap();
        }
        assert_eq!(
            registry.model_ref("face-yunet"),
            Some(ModelRef { id: "face-yunet".into(), version: "2023mar".into() })
        );

        let copy = Copy::new("inspect");
        let path = copy.0.join("files/test-luma-v1.onnx");
        let mut bytes = fs::read(&path).unwrap();
        bytes[0] ^= 1;
        fs::write(&path, &bytes).unwrap();
        let damaged = Registry::load(&copy.0).unwrap();
        // A quick look cannot see a changed byte; the full check can.
        assert_eq!(damaged.inspect().iter().find(|m| m.id == "test-luma").unwrap().problem, None);
        assert_eq!(damaged.verify("test-luma").unwrap_err().code(), "model_damaged");
        fs::remove_file(&path).unwrap();
        let info = Registry::load(&copy.0).unwrap().inspect().into_iter().find(|m| m.id == "test-luma").unwrap();
        assert_eq!(info.problem.as_deref(), Some("model_missing"));
        assert_eq!(info.size, None);
    }
}

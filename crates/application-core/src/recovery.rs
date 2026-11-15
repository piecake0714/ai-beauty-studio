//! Automatic recovery: the open document is saved in the background to a
//! recovery project, and offered back after the app ends unexpectedly.
//!
//! A clean save, closing the image or quitting normally discards it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::files;
use crate::project::{self, ProjectError, Snapshot};

/// What the UI shows when offering to restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInfo {
    /// Seconds since the Unix epoch.
    pub saved_at: u64,
    /// The project the work belonged to, if it had been saved as one.
    pub project_path: Option<String>,
    /// The original photo.
    pub original_path: String,
}

/// The recovery slot in the app's data folder.
#[derive(Debug, Clone)]
pub struct Recovery {
    dir: PathBuf,
}

impl Recovery {
    pub fn new(dir: impl Into<PathBuf>) -> Recovery {
        Recovery { dir: dir.into() }
    }

    /// The recovery project file.
    pub fn project_path(&self) -> PathBuf {
        self.dir.join("autosave.aibs")
    }

    fn info_path(&self) -> PathBuf {
        self.dir.join("autosave.json")
    }

    /// Save the document. The description is written last, so a crash in
    /// the middle leaves either the previous recovery or none, never a
    /// description pointing at a half-written project.
    pub fn save(&self, snapshot: &Snapshot, project_path: Option<&Path>) -> Result<(), ProjectError> {
        project::save(&self.project_path(), snapshot)?;
        let info = RecoveryInfo {
            saved_at: snapshot.saved_at,
            project_path: project_path.map(|p| p.to_string_lossy().into_owned()),
            original_path: snapshot.original.path.clone(),
        };
        let json = serde_json::to_vec_pretty(&info).map_err(std::io::Error::other)?;
        files::write_atomically(&self.info_path(), &json)?;
        Ok(())
    }

    /// The recovery waiting to be offered, if any.
    pub fn pending(&self) -> Option<RecoveryInfo> {
        if !self.project_path().exists() {
            return None;
        }
        let text = fs::read(self.info_path()).ok()?;
        serde_json::from_slice(&text).ok()
    }

    /// Forget the recovery.
    pub fn discard(&self) {
        let _ = fs::remove_file(self.info_path());
        let _ = fs::remove_file(self.project_path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Adjustments, History};
    use crate::files::tests::scratch;
    use crate::files::FileRef;

    #[test]
    fn saves_offers_and_discards() {
        let dir = scratch("recovery");
        let original = dir.join("a.png");
        fs::write(&original, include_bytes!("../../../tests/fixtures/gradient_64x48.png")).unwrap();
        let reference = FileRef::of(&original).unwrap();
        let mut history = History::default();
        history.commit(Adjustments { contrast: 30.0, ..Default::default() });
        let recovery = Recovery::new(dir.join("recovery"));
        assert!(recovery.pending().is_none());

        let snapshot = Snapshot {
            original: &reference,
            image_size: (64, 48),
            history: &history,
            faces: None,
            matte: None,
            shape: None,
            models: Default::default(),
            images: &[],
            preview: None,
            saved_at: 42,
        };
        recovery.save(&snapshot, Some(&dir.join("work.aibs"))).unwrap();
        let info = recovery.pending().unwrap();
        assert_eq!(info.saved_at, 42);
        assert!(info.project_path.unwrap().ends_with("work.aibs"));
        let restored = project::load(&recovery.project_path(), None).unwrap();
        assert_eq!(restored.history.current().contrast, 30.0);

        recovery.discard();
        assert!(recovery.pending().is_none());
        assert!(!recovery.project_path().exists());
    }
}

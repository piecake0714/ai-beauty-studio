//! Face detection for portrait retouching.
//!
//! Faces are found once when an image opens. Their positions and skin
//! colour models are kept with the document and passed to every render, so
//! skin smoothing follows the faces at any zoom level.

use engine_sys::{AiSession, Face, Provider, Source};
use serde::Serialize;

use crate::models::Registry;

/// The face detection model (YuNet).
pub const FACE_MODEL: &str = "face-yunet";

/// Faces scoring below this are ignored (0..1).
pub const MIN_FACE_SCORE: f32 = 0.6;

/// Load the face detector through the registry checks (license and hash).
/// It runs on the CPU: it is small, runs once per image, and this keeps
/// the GPU free for rendering.
pub fn open_detector(registry: &Registry) -> Result<AiSession, String> {
    let (_, bytes) = registry.load_model(FACE_MODEL).map_err(|e| e.code().to_string())?;
    AiSession::open(&bytes, Provider::Cpu).map_err(|_| "model_load_failed".to_string())
}

pub fn detect(source: &Source, detector: &AiSession) -> Result<Vec<Face>, String> {
    source
        .detect_faces(detector, MIN_FACE_SCORE)
        .map_err(|_| "inference_failed".to_string())
}

/// What the UI shows about face detection for the open image.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum FaceStatus {
    Detecting,
    Ready { count: usize },
    /// Detection could not run; `code` says why (e.g. model missing).
    Unavailable { code: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::tests::repo_models;

    #[test]
    fn the_shipped_face_model_loads_and_finds_no_faces_in_a_gradient() {
        let registry = Registry::load(&repo_models()).unwrap();
        let detector = open_detector(&registry).unwrap();
        let png = include_bytes!("../../../tests/fixtures/gradient_64x48.png");
        let source = Source::open(png).unwrap();
        assert!(detect(&source, &detector).unwrap().is_empty());
    }

    #[test]
    fn status_json_matches_the_ui() {
        assert_eq!(
            serde_json::to_value(FaceStatus::Ready { count: 2 }).unwrap(),
            serde_json::json!({"state": "ready", "count": 2})
        );
        assert_eq!(
            serde_json::to_value(FaceStatus::Unavailable { code: "x".into() }).unwrap(),
            serde_json::json!({"state": "unavailable", "code": "x"})
        );
    }
}

//! AI analyses that run once per image as background jobs: separating
//! subject and background, and estimating the 3D shape for lighting.
//!
//! Models are loaded through the registry checks (preferring the GPU) and
//! kept for later images. If the GPU fails while running, the job retries
//! on the CPU.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use engine_sys::{AiSession, EngineError, Matte, Provider, Shape, Source};
use serde::Serialize;

use crate::jobs::{JobContext, JobError};
use crate::models::{ModelRef, Registry};

/// The subject/background separation model (BiRefNet lite).
pub const BACKGROUND_MODEL: &str = "background-birefnet-lite";
/// The depth model for 3D lighting (Depth Anything V2 Small).
pub const DEPTH_MODEL: &str = "depth-anything-v2-small";

/// One loaded model, shared between jobs.
pub struct ModelSlot {
    id: &'static str,
    session: Mutex<Option<(Arc<AiSession>, ModelRef)>>,
    /// Never use the GPU for this model.
    cpu_only: bool,
}

impl ModelSlot {
    pub fn new(id: &'static str) -> ModelSlot {
        ModelSlot { id, session: Mutex::new(None), cpu_only: false }
    }

    /// A slot whose model always runs on the CPU: for models the GPU path
    /// runs without an error but gives wrong results (LaMa's Fourier
    /// layers under DirectML return the picture unchanged).
    pub fn cpu_only(id: &'static str) -> ModelSlot {
        ModelSlot { id, session: Mutex::new(None), cpu_only: true }
    }

    pub fn id(&self) -> &'static str {
        self.id
    }

    fn get(&self, registry: &Registry, provider: Provider) -> Result<(Arc<AiSession>, ModelRef), JobError> {
        let mut slot = self.session.lock().map_err(|_| JobError::new("internal", "model lock"))?;
        if let Some((session, model)) = slot.as_ref() {
            if provider == Provider::DirectMl || session.provider() == Provider::Cpu {
                return Ok((session.clone(), model.clone()));
            }
        }
        let (entry, bytes) = registry.load_model(self.id).map_err(|e| JobError::new(e.code(), e.to_string()))?;
        let session = AiSession::open(&bytes, provider).map_err(|(_, note)| JobError::new("model_load_failed", note))?;
        let loaded = (Arc::new(session), ModelRef { id: entry.id, version: entry.version });
        *slot = Some(loaded.clone());
        Ok(loaded)
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded_provider().is_some()
    }

    /// The provider the loaded model runs on, if it is loaded.
    pub fn loaded_provider(&self) -> Option<Provider> {
        self.session.lock().ok()?.as_ref().map(|(s, _)| s.provider())
    }

    /// Release the model (it is loaded again when next needed). Returns
    /// whether one was loaded.
    pub fn unload(&self) -> bool {
        self.session.lock().map(|mut s| s.take().is_some()).unwrap_or(false)
    }
}

/// What a finished analysis reports to the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisReport {
    /// "directml" or "cpu".
    pub provider: String,
    /// Why the GPU was not used, when it was wanted.
    pub fallback_reason: Option<String>,
    pub ms: f64,
    /// The model that produced the result.
    pub model: ModelRef,
}

/// Run `analyse` with the slot's model, GPU first, CPU if the GPU fails.
pub(crate) fn run<T>(
    registry: &Registry,
    slot: &ModelSlot,
    ctx: &JobContext,
    analyse: impl Fn(&AiSession) -> Result<T, EngineError>,
) -> Result<(T, AnalysisReport), JobError> {
    run_reporting(registry, slot, ctx, true, analyse)
}

/// Like [`run`], reporting progress only when `report` is true (callers
/// that run a model several times report their own).
pub(crate) fn run_reporting<T>(
    registry: &Registry,
    slot: &ModelSlot,
    ctx: &JobContext,
    report: bool,
    analyse: impl Fn(&AiSession) -> Result<T, EngineError>,
) -> Result<(T, AnalysisReport), JobError> {
    if report {
        ctx.progress(0.05, "load");
    }
    let first = if slot.cpu_only { Provider::Cpu } else { Provider::DirectMl };
    let (session, model) = slot.get(registry, first)?;
    ctx.check()?;
    if report {
        ctx.progress(0.3, "run");
    }
    let started = Instant::now();
    let mut fallback_reason = session.fallback_reason().map(str::to_string);
    let (value, used) = match analyse(&session) {
        Ok(value) => (value, session.provider()),
        Err(error) if session.provider() == Provider::DirectMl => {
            // The GPU could not run it; the CPU can.
            ctx.check()?;
            fallback_reason = Some(format!("DirectML failed while running: {error}"));
            let (cpu, _) = slot.get(registry, Provider::Cpu)?;
            let value = analyse(&cpu).map_err(|e| JobError::new("inference_failed", e.to_string()))?;
            (value, Provider::Cpu)
        }
        Err(error) => return Err(JobError::new("inference_failed", error.to_string())),
    };
    ctx.check()?;
    if report {
        ctx.progress(1.0, "done");
    }
    Ok((
        value,
        AnalysisReport {
            provider: match used {
                Provider::DirectMl => "directml",
                Provider::Cpu => "cpu",
            }
            .to_string(),
            fallback_reason,
            ms: started.elapsed().as_secs_f64() * 1000.0,
            model,
        },
    ))
}

/// Separate subject and background of `source` (slot: [`BACKGROUND_MODEL`]).
pub fn separate(
    registry: &Registry,
    slot: &ModelSlot,
    source: &Source,
    ctx: &JobContext,
) -> Result<(Matte, AnalysisReport), JobError> {
    run(registry, slot, ctx, |session| source.segment(session))
}

/// Estimate the 3D shape of `source` (slot: [`DEPTH_MODEL`]).
pub fn estimate_shape(
    registry: &Registry,
    slot: &ModelSlot,
    source: &Source,
    ctx: &JobContext,
) -> Result<(Shape, AnalysisReport), JobError> {
    run(registry, slot, ctx, |session| source.estimate_shape(session))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{JobUpdate, Jobs};
    use crate::models::tests::repo_models;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn separation_runs_as_a_job_through_the_registry_and_reuses_the_model() {
        let png = include_bytes!("../../../tests/fixtures/gradient_64x48.png");
        let source = Arc::new(Source::open(png).unwrap());
        let slot = Arc::new(ModelSlot::new(BACKGROUND_MODEL));
        let depth_slot = Arc::new(ModelSlot::new(DEPTH_MODEL));
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let jobs = Jobs::new(move |u| tx.lock().unwrap().send(u).unwrap());
        for _ in 0..2 {
            let (source, slot) = (source.clone(), slot.clone());
            jobs.start(move |ctx| {
                let registry = Registry::load(&repo_models()).map_err(|e| JobError::new(e.code(), e.to_string()))?;
                let (matte, report) = separate(&registry, &slot, &source, ctx)?;
                assert!((0.0..=1.0).contains(&matte.coverage()));
                Ok(serde_json::to_value(report).unwrap())
            });
            let result = loop {
                match rx.recv_timeout(Duration::from_secs(300)).unwrap() {
                    JobUpdate::Finished { result, .. } => break result,
                    JobUpdate::Failed { code, detail, .. } => panic!("{code}: {detail}"),
                    JobUpdate::Cancelled { .. } => panic!("cancelled"),
                    JobUpdate::Progress { .. } => {}
                }
            };
            assert!(["directml", "cpu"].contains(&result["provider"].as_str().unwrap()));
            assert_eq!(result["model"]["id"], BACKGROUND_MODEL);
        }
        assert!(slot.is_loaded());
        assert!(slot.unload());
        assert!(!slot.is_loaded() && !slot.unload());

        // The depth model the same way.
        let (source2, slot2) = (source.clone(), depth_slot.clone());
        jobs.start(move |ctx| {
            let registry = Registry::load(&repo_models()).map_err(|e| JobError::new(e.code(), e.to_string()))?;
            let (_shape, report) = estimate_shape(&registry, &slot2, &source2, ctx)?;
            Ok(serde_json::to_value(report).unwrap())
        });
        loop {
            match rx.recv_timeout(Duration::from_secs(300)).unwrap() {
                JobUpdate::Finished { .. } => break,
                JobUpdate::Failed { code, detail, .. } => panic!("{code}: {detail}"),
                JobUpdate::Cancelled { .. } => panic!("cancelled"),
                JobUpdate::Progress { .. } => {}
            }
        }
        assert!(depth_slot.is_loaded());
    }
}

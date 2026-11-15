//! Subject/background separation for background edits.
//!
//! Separation runs once per image as a background job: the model is loaded
//! through the registry checks (preferring the GPU) and kept for later
//! images. If the GPU fails while running it, the job retries on the CPU.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use engine_sys::{AiSession, Matte, Provider, Source};
use serde::Serialize;

use crate::jobs::{JobContext, JobError};
use crate::models::Registry;

/// The subject/background separation model (BiRefNet lite).
pub const BACKGROUND_MODEL: &str = "background-birefnet-lite";

/// The loaded model, shared between jobs.
#[derive(Default)]
pub struct ModelSlot(Mutex<Option<Arc<AiSession>>>);

impl ModelSlot {
    fn get(&self, registry: &Registry, provider: Provider) -> Result<Arc<AiSession>, JobError> {
        let mut slot = self.0.lock().map_err(|_| JobError::new("internal", "model lock"))?;
        if let Some(session) = slot.as_ref() {
            if provider == Provider::DirectMl || session.provider() == Provider::Cpu {
                return Ok(session.clone());
            }
        }
        let (_, bytes) = registry
            .load_model(BACKGROUND_MODEL)
            .map_err(|e| JobError::new(e.code(), e.to_string()))?;
        let session = AiSession::open(&bytes, provider).map_err(|(_, note)| JobError::new("model_load_failed", note))?;
        let session = Arc::new(session);
        *slot = Some(session.clone());
        Ok(session)
    }
}

/// What a finished separation reports to the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeparationReport {
    /// "directml" or "cpu".
    pub provider: String,
    /// Why the GPU was not used, when it was wanted.
    pub fallback_reason: Option<String>,
    /// Fraction of the image that is subject.
    pub coverage: f32,
    pub ms: f64,
}

/// Separate subject and background of `source`.
pub fn separate(
    registry: &Registry,
    slot: &ModelSlot,
    source: &Source,
    ctx: &JobContext,
) -> Result<(Matte, SeparationReport), JobError> {
    ctx.progress(0.05, "load");
    let session = slot.get(registry, Provider::DirectMl)?;
    ctx.check()?;
    ctx.progress(0.3, "run");
    let started = Instant::now();
    let mut fallback_reason = session.fallback_reason().map(str::to_string);
    let (matte, used) = match source.segment(&session) {
        Ok(matte) => (matte, session.provider()),
        Err(error) if session.provider() == Provider::DirectMl => {
            // The GPU could not run it; the CPU can.
            ctx.check()?;
            fallback_reason = Some(format!("DirectML failed while running: {error}"));
            let cpu = slot.get(registry, Provider::Cpu)?;
            let matte = source
                .segment(&cpu)
                .map_err(|e| JobError::new("inference_failed", e.to_string()))?;
            (matte, Provider::Cpu)
        }
        Err(error) => return Err(JobError::new("inference_failed", error.to_string())),
    };
    ctx.check()?;
    ctx.progress(1.0, "done");
    let report = SeparationReport {
        provider: match used {
            Provider::DirectMl => "directml",
            Provider::Cpu => "cpu",
        }
        .to_string(),
        fallback_reason,
        coverage: matte.coverage(),
        ms: started.elapsed().as_secs_f64() * 1000.0,
    };
    Ok((matte, report))
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
        let slot = Arc::new(ModelSlot::default());
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let jobs = Jobs::new(move |u| tx.lock().unwrap().send(u).unwrap());
        for _ in 0..2 {
            let (source, slot) = (source.clone(), slot.clone());
            jobs.start(move |ctx| {
                let registry = Registry::load(&repo_models()).map_err(|e| JobError::new(e.code(), e.to_string()))?;
                let (matte, report) = separate(&registry, &slot, &source, ctx)?;
                assert_eq!(matte.coverage(), report.coverage);
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
            let coverage = result["coverage"].as_f64().unwrap();
            assert!((0.0..=1.0).contains(&coverage));
            assert!(["directml", "cpu"].contains(&result["provider"].as_str().unwrap()));
        }
        assert!(slot.0.lock().unwrap().is_some());
    }
}

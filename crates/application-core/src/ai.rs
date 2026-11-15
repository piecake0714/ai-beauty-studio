//! AI runtime status and the self-test the user can run from Settings.

use std::time::Instant;

use engine_sys::{AiSession, Provider};
use serde::Serialize;

use crate::jobs::{JobContext, JobError};
use crate::models::Registry;

/// The model used by the self-test.
pub const SELF_TEST_MODEL: &str = "test-luma";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub name: String,
    pub vendor: String,
    pub is_software: bool,
    pub dedicated_memory_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    pub gpus: Vec<Gpu>,
    pub runtime_version: String,
}

fn vendor_name(id: u32) -> &'static str {
    match id {
        0x10DE => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x8086 => "Intel",
        0x5143 => "Qualcomm",
        0x1414 => "Microsoft",
        _ => "",
    }
}

pub fn hardware() -> Hardware {
    Hardware {
        gpus: engine_sys::gpu_list()
            .into_iter()
            .map(|g| Gpu {
                vendor: vendor_name(g.vendor_id).to_string(),
                name: g.name,
                is_software: g.is_software,
                dedicated_memory_mb: g.dedicated_memory / (1024 * 1024),
            })
            .collect(),
        runtime_version: engine_sys::ai_runtime_version(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTestReport {
    /// "directml" or "cpu".
    pub provider: String,
    /// Why the GPU is not in use, when it was wanted.
    pub fallback_reason: Option<String>,
    pub model: String,
    pub width: u32,
    pub height: u32,
    /// First run includes one-time preparation on the GPU.
    pub first_run_ms: f64,
    /// A second run, as later AI work will see it.
    pub run_ms: f64,
    /// Largest difference from the exact answer (0..1 scale).
    pub max_error: f64,
    pub passed: bool,
}

/// Load the self-test model through the registry checks, run it on a
/// synthetic image (GPU preferred), and compare with the exact answer.
pub fn self_test(registry: &Registry, ctx: &JobContext, size: u32) -> Result<SelfTestReport, JobError> {
    ctx.progress(0.05, "verify");
    let (entry, bytes) = registry
        .load_model(SELF_TEST_MODEL)
        .map_err(|e| JobError::new(e.code(), e.to_string()))?;
    ctx.check()?;

    ctx.progress(0.2, "load");
    let session = AiSession::open(&bytes, Provider::DirectMl)
        .map_err(|(_, note)| JobError::new("model_load_failed", note))?;
    ctx.check()?;

    ctx.progress(0.35, "prepare");
    let plane = (size * size) as usize;
    let input: Vec<f32> = (0..plane * 3)
        .map(|i| ((i as u64 * 2_654_435_761) % 1000) as f32 / 999.0)
        .collect();
    let shape = [1, 3, size as i64, size as i64];
    ctx.check()?;

    ctx.progress(0.5, "run");
    let started = Instant::now();
    session
        .run(&input, &shape)
        .map_err(|e| JobError::new("inference_failed", e.to_string()))?;
    let first_run_ms = started.elapsed().as_secs_f64() * 1000.0;
    ctx.check()?;

    ctx.progress(0.75, "run");
    let started = Instant::now();
    let (output, out_shape) = session
        .run(&input, &shape)
        .map_err(|e| JobError::new("inference_failed", e.to_string()))?;
    let run_ms = started.elapsed().as_secs_f64() * 1000.0;
    ctx.check()?;

    ctx.progress(0.9, "check");
    let mut max_error = if out_shape == [1, 1, size as i64, size as i64] && output.len() == plane {
        0.0f64
    } else {
        f64::INFINITY
    };
    if max_error == 0.0 {
        for i in 0..plane {
            let expected = 0.2126 * input[i] as f64 + 0.7152 * input[plane + i] as f64 + 0.0722 * input[2 * plane + i] as f64;
            max_error = max_error.max((expected - output[i] as f64).abs());
        }
    }
    ctx.progress(1.0, "check");

    Ok(SelfTestReport {
        provider: match session.provider() {
            Provider::DirectMl => "directml",
            Provider::Cpu => "cpu",
        }
        .to_string(),
        fallback_reason: session.fallback_reason().map(str::to_string),
        model: format!("{} v{}", entry.id, entry.version),
        width: size,
        height: size,
        first_run_ms,
        run_ms,
        max_error,
        // Well under one 8-bit level, allowing for reduced GPU precision.
        passed: max_error < 1e-3,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{JobUpdate, Jobs};
    use crate::models::tests::repo_models;
    use std::sync::{mpsc, Mutex};
    use std::time::Duration;

    #[test]
    fn hardware_lists_gpus_on_windows() {
        let info = hardware();
        if cfg!(windows) {
            assert!(!info.gpus.is_empty());
        }
        assert!(info.runtime_version.starts_with("1."));
    }

    #[test]
    fn self_test_passes_and_reports_progress() {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let jobs = Jobs::new(move |u| tx.lock().unwrap().send(u).unwrap());
        let id = jobs.start(|ctx| {
            let registry = Registry::load(&repo_models()).map_err(|e| JobError::new(e.code(), e.to_string()))?;
            let report = self_test(&registry, ctx, 256)?;
            Ok(serde_json::to_value(report).unwrap())
        });
        let mut stages = Vec::new();
        let result = loop {
            match rx.recv_timeout(Duration::from_secs(60)).unwrap() {
                JobUpdate::Progress { stage, .. } => stages.push(stage),
                JobUpdate::Finished { id: done, result } => {
                    assert_eq!(done, id);
                    break result;
                }
                other => panic!("unexpected {other:?}"),
            }
        };
        assert_eq!(stages.first().map(String::as_str), Some("verify"));
        assert_eq!(result["passed"], true);
        assert_eq!(result["model"], "test-luma v1");
        let provider = result["provider"].as_str().unwrap();
        assert!(provider == "directml" || (provider == "cpu" && result["fallbackReason"].is_string()));
        println!("self-test: {result}");
    }
}

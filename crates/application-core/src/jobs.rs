//! Background jobs with progress and cancellation.
//!
//! Long work (AI, export) runs on its own thread so the UI never waits.
//! A job reports progress through a callback and checks its cancel token
//! between steps; cancelling is cooperative and takes effect at the next
//! check.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::Serialize;

/// Updates sent while a job runs. Every job ends with exactly one of
/// `Finished`, `Failed` or `Cancelled`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum JobUpdate {
    #[serde(rename_all = "camelCase")]
    Progress { id: u64, fraction: f32, stage: String },
    #[serde(rename_all = "camelCase")]
    Finished { id: u64, result: serde_json::Value },
    #[serde(rename_all = "camelCase")]
    Failed { id: u64, code: String, detail: String },
    #[serde(rename_all = "camelCase")]
    Cancelled { id: u64 },
}

/// Error returned by a job: a stable code and a detail for logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    pub code: String,
    pub detail: String,
}

impl JobError {
    pub fn new(code: &str, detail: impl Into<String>) -> JobError {
        JobError {
            code: code.to_string(),
            detail: detail.into(),
        }
    }

    fn cancelled() -> JobError {
        JobError::new("cancelled", "")
    }
}

type Notify = Arc<dyn Fn(JobUpdate) + Send + Sync>;

/// Handed to a running job.
pub struct JobContext {
    id: u64,
    cancelled: Arc<AtomicBool>,
    notify: Notify,
}

impl JobContext {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Report progress from 0 to 1 with a short stage name.
    pub fn progress(&self, fraction: f32, stage: &str) {
        (self.notify)(JobUpdate::Progress {
            id: self.id,
            fraction: fraction.clamp(0.0, 1.0),
            stage: stage.to_string(),
        });
    }

    /// Call between steps: returns an error once the job was cancelled.
    pub fn check(&self) -> Result<(), JobError> {
        if self.cancelled.load(Ordering::SeqCst) {
            Err(JobError::cancelled())
        } else {
            Ok(())
        }
    }
}

pub struct Jobs {
    next_id: AtomicU64,
    running: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    notify: Notify,
}

impl Jobs {
    pub fn new(notify: impl Fn(JobUpdate) + Send + Sync + 'static) -> Arc<Jobs> {
        Arc::new(Jobs {
            next_id: AtomicU64::new(1),
            running: Mutex::new(HashMap::new()),
            notify: Arc::new(notify),
        })
    }

    /// Start `work` on its own thread and return its id at once.
    pub fn start<F>(self: &Arc<Self>, work: F) -> u64
    where
        F: FnOnce(&JobContext) -> Result<serde_json::Value, JobError> + Send + 'static,
    {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.running.lock().unwrap().insert(id, cancelled.clone());
        let jobs = Arc::clone(self);
        thread::spawn(move || {
            let context = JobContext {
                id,
                cancelled: cancelled.clone(),
                notify: jobs.notify.clone(),
            };
            let outcome = work(&context);
            jobs.running.lock().unwrap().remove(&id);
            let update = match outcome {
                Ok(result) => JobUpdate::Finished { id, result },
                Err(error) if error.code == "cancelled" || cancelled.load(Ordering::SeqCst) => {
                    JobUpdate::Cancelled { id }
                }
                Err(error) => JobUpdate::Failed {
                    id,
                    code: error.code,
                    detail: error.detail,
                },
            };
            (jobs.notify)(update);
        });
        id
    }

    /// Ask a running job to stop. False when no such job is running.
    pub fn cancel(&self, id: u64) -> bool {
        match self.running.lock().unwrap().get(&id) {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn jobs_with_channel() -> (Arc<Jobs>, mpsc::Receiver<JobUpdate>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        (Jobs::new(move |update| tx.lock().unwrap().send(update).unwrap()), rx)
    }

    fn until_done(rx: &mpsc::Receiver<JobUpdate>) -> Vec<JobUpdate> {
        let mut updates = Vec::new();
        loop {
            let update = rx.recv_timeout(Duration::from_secs(10)).expect("job did not finish");
            let done = !matches!(update, JobUpdate::Progress { .. });
            updates.push(update);
            if done {
                return updates;
            }
        }
    }

    #[test]
    fn reports_progress_then_the_result() {
        let (jobs, rx) = jobs_with_channel();
        let id = jobs.start(|ctx| {
            ctx.progress(0.5, "half");
            ctx.progress(1.0, "done");
            Ok(serde_json::json!({ "answer": 42 }))
        });
        let updates = until_done(&rx);
        assert_eq!(
            updates,
            vec![
                JobUpdate::Progress { id, fraction: 0.5, stage: "half".into() },
                JobUpdate::Progress { id, fraction: 1.0, stage: "done".into() },
                JobUpdate::Finished { id, result: serde_json::json!({ "answer": 42 }) },
            ]
        );
        assert!(!jobs.cancel(id), "a finished job is no longer running");
    }

    #[test]
    fn a_cancelled_job_stops_at_its_next_check() {
        let (jobs, rx) = jobs_with_channel();
        let (started_tx, started_rx) = mpsc::channel();
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let id = jobs.start(move |ctx| {
            started_tx.send(()).unwrap();
            go_rx.recv().unwrap();
            ctx.check()?;
            Ok(serde_json::Value::Null)
        });
        started_rx.recv().unwrap();
        assert!(jobs.cancel(id));
        go_tx.send(()).unwrap();
        assert_eq!(until_done(&rx), vec![JobUpdate::Cancelled { id }]);
    }

    #[test]
    fn failures_carry_their_code() {
        let (jobs, rx) = jobs_with_channel();
        let id = jobs.start(|_| Err(JobError::new("model_damaged", "hash differs")));
        assert_eq!(
            until_done(&rx),
            vec![JobUpdate::Failed { id, code: "model_damaged".into(), detail: "hash differs".into() }]
        );
    }

    #[test]
    fn ids_are_unique_and_unknown_ids_cannot_be_cancelled() {
        let (jobs, rx) = jobs_with_channel();
        let a = jobs.start(|_| Ok(serde_json::Value::Null));
        until_done(&rx);
        let b = jobs.start(|_| Ok(serde_json::Value::Null));
        until_done(&rx);
        assert_ne!(a, b);
        assert!(!jobs.cancel(9999));
    }

    #[test]
    fn updates_serialize_for_the_ui() {
        let json = serde_json::to_value(JobUpdate::Progress { id: 3, fraction: 0.25, stage: "run".into() }).unwrap();
        assert_eq!(json, serde_json::json!({ "state": "progress", "id": 3, "fraction": 0.25, "stage": "run" }));
        let json = serde_json::to_value(JobUpdate::Cancelled { id: 3 }).unwrap();
        assert_eq!(json, serde_json::json!({ "state": "cancelled", "id": 3 }));
    }
}

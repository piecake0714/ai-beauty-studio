import { useEffect, useRef, useState } from "react";
import { errorCode } from "../api";
import { useApp, useI18n } from "../context";
import { errorMessage, type MessageKey } from "../i18n";
import { applyJobUpdate, type Hardware, type JobUpdate, type JobView, type SelfTestReport } from "../jobs";

const STAGES = ["verify", "load", "prepare", "run", "check"];

/** Settings section: detected graphics adapters and the AI runtime self-test. */
export function AiSection() {
  const { t, language } = useI18n();
  const api = useApp((s) => s.api);
  const [hardware, setHardware] = useState<Hardware | null>(null);
  const [hardwareError, setHardwareError] = useState<string | null>(null);
  const [job, setJob] = useState<JobView>({ kind: "idle" });
  const jobRef = useRef(job);
  jobRef.current = job;
  // Updates can arrive before start() learns its job id; keep recent ones
  // so they can be replayed.
  const recentRef = useRef<JobUpdate[]>([]);

  useEffect(() => {
    let active = true;
    api
      .hardwareInfo()
      .then((info) => active && setHardware(info))
      .catch((reason) => active && setHardwareError(errorCode(reason)));
    return () => {
      active = false;
    };
  }, [api]);

  useEffect(() => {
    let stop: (() => void) | undefined;
    let disposed = false;
    api
      .onJobUpdate((update) => {
        recentRef.current = [...recentRef.current.slice(-50), update];
        setJob((view) => applyJobUpdate(view, update));
      })
      .then((unlisten) => (disposed ? unlisten() : (stop = unlisten)))
      .catch(() => {
        // Without updates the test simply shows no progress.
      });
    return () => {
      disposed = true;
      stop?.();
      // Closing Settings stops a test that is still running.
      const current = jobRef.current;
      if (current.kind === "running") void api.cancelJob(current.id);
    };
  }, [api]);

  async function start() {
    try {
      const id = await api.startAiSelfTest();
      const started: JobView = { kind: "running", id, fraction: 0, stage: "verify", cancelling: false };
      setJob(recentRef.current.filter((u) => u.id === id).reduce(applyJobUpdate, started));
    } catch (reason) {
      setJob({ kind: "failed", code: errorCode(reason) });
    }
  }

  function cancel() {
    if (job.kind !== "running") return;
    setJob({ ...job, cancelling: true });
    void api.cancelJob(job.id);
  }

  const report = job.kind === "finished" ? (job.result as SelfTestReport) : null;

  return (
    <section className="ai-section" aria-labelledby="ai-title">
      <h2 id="ai-title">{t("ai.title")}</h2>

      {hardwareError && <p className="error-text">{errorMessage(language, hardwareError)}</p>}
      {hardware && (
        <>
          <div className="gpu-list">
            <span className="muted">{t("ai.gpus")}</span>
            {hardware.gpus.length === 0 && <span>{t("ai.noGpu")}</span>}
            <ul>
              {hardware.gpus.map((gpu, index) => (
                <li key={`${gpu.name}-${index}`}>
                  {gpu.name}
                  {gpu.isSoftware && <span className="muted">（{t("ai.software")}）</span>}
                  {gpu.dedicatedMemoryMb > 0 && <span className="muted"> · {t("ai.memory", { mb: gpu.dedicatedMemoryMb })}</span>}
                </li>
              ))}
            </ul>
          </div>
          <p className="muted">{t("ai.runtime", { version: hardware.runtimeVersion })}</p>
        </>
      )}

      <p className="muted small-text">{t("ai.testHint")}</p>
      {job.kind === "running" ? (
        <div className="job-row">
          <progress max={1} value={job.fraction} aria-label={t("ai.test")} />
          <span className="muted">
            {STAGES.includes(job.stage) ? t(`ai.stage.${job.stage}` as MessageKey) : ""}
          </span>
          <button type="button" className="ghost small" onClick={cancel} disabled={job.cancelling}>
            {t("ai.cancel")}
          </button>
        </div>
      ) : (
        <button type="button" className="ghost small" onClick={() => void start()}>
          {t("ai.test")}
        </button>
      )}

      {report && (
        <div className={report.passed ? "result ok" : "result bad"} role="status">
          <p>
            {report.passed
              ? t(report.provider === "directml" ? "ai.passedGpu" : "ai.passedCpu")
              : t("ai.notPassed", { error: report.maxError.toExponential(1) })}
          </p>
          {report.fallbackReason && <p className="small-text">{t("ai.fallback", { reason: report.fallbackReason })}</p>}
          <p className="small-text">
            {t("ai.timing", {
              width: report.width,
              height: report.height,
              first: report.firstRunMs.toFixed(1),
              run: report.runMs.toFixed(1),
            })}
          </p>
        </div>
      )}
      {job.kind === "failed" && (
        <p className="error-text" role="alert">
          {errorMessage(language, job.code)}
        </p>
      )}
      {job.kind === "cancelled" && <p className="muted">{t("ai.cancelled")}</p>}
    </section>
  );
}

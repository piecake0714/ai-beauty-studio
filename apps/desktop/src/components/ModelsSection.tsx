import { useCallback, useEffect, useRef, useState } from "react";
import { errorCode } from "../api";
import { useApp, useI18n } from "../context";
import { formatBytes } from "../format";
import { errorMessage, type MessageKey } from "../i18n";
import type { JobUpdate } from "../jobs";
import type { ModelOverview } from "../types";

type Verify =
  | { state: "idle" }
  | { state: "running"; id: number; fraction: number }
  | { state: "done"; problems: { id: string; problem: string }[] }
  | { state: "failed"; code: string };

/** Settings section: every AI model with its version, license and state. */
export function ModelsSection() {
  const { t, language } = useI18n();
  const api = useApp((s) => s.api);
  const [models, setModels] = useState<ModelOverview[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [verify, setVerify] = useState<Verify>({ state: "idle" });
  const [released, setReleased] = useState<number | null>(null);
  const verifyRef = useRef(verify);
  verifyRef.current = verify;
  const early = useRef<JobUpdate[]>([]);

  const refresh = useCallback(() => {
    api
      .modelOverview()
      .then((list) => {
        setModels(list);
        setError(null);
      })
      .catch((reason) => setError(errorCode(reason)));
  }, [api]);

  useEffect(refresh, [refresh]);

  const apply = useCallback(
    (update: JobUpdate) => {
      const current = verifyRef.current;
      if (current.state !== "running" || current.id !== update.id) return;
      if (update.state === "progress") setVerify({ ...current, fraction: update.fraction });
      else if (update.state === "finished") {
        const result = update.result as { models?: { id: string; problem: string | null }[] } | null;
        const problems = (result?.models ?? [])
          .filter((m): m is { id: string; problem: string } => m.problem !== null)
          .map((m) => ({ id: m.id, problem: m.problem }));
        setVerify({ state: "done", problems });
        refresh();
      } else if (update.state === "failed") setVerify({ state: "failed", code: update.code });
      else setVerify({ state: "idle" });
    },
    [refresh],
  );

  useEffect(() => {
    let stop: (() => void) | undefined;
    let disposed = false;
    void api
      .onJobUpdate((update) => {
        early.current = [...early.current.slice(-20), update];
        apply(update);
      })
      .then((unlisten) => (disposed ? unlisten() : (stop = unlisten)));
    return () => {
      disposed = true;
      stop?.();
    };
  }, [api, apply]);

  async function startVerify() {
    try {
      const id = await api.verifyModels();
      const running: Verify = { state: "running", id, fraction: 0 };
      verifyRef.current = running;
      setVerify(running);
      early.current.filter((u) => u.id === id).forEach(apply);
    } catch (reason) {
      setVerify({ state: "failed", code: errorCode(reason) });
    }
  }

  async function release() {
    setReleased(await api.unloadModels().catch(() => 0));
    refresh();
  }

  return (
    <section className="models" aria-labelledby="models-title">
      <h2 id="models-title">{t("models.title")}</h2>
      {error && <p className="error-text">{errorMessage(language, error)}</p>}
      {models && (
        <ul className="model-list">
          {models.map((model) => (
            <li key={model.id} className="model-card">
              <div className="model-head">
                <strong>{model.usedFor ? t(`models.usedFor.${model.usedFor}` as MessageKey) : model.id}</strong>
                <span className={model.problem ? "badge bad" : "badge ok"}>
                  {model.problem ? errorMessage(language, model.problem) : t("models.ok")}
                </span>
              </div>
              <p className="note">{model.purpose}</p>
              <dl className="model-facts">
                <dt>{t("models.version")}</dt>
                <dd>
                  {model.id} · {model.version}
                </dd>
                <dt>{t("models.size")}</dt>
                <dd>{model.size === null ? t("models.missing") : formatBytes(model.size, language)}</dd>
                <dt>{t("models.license")}</dt>
                <dd>
                  {model.license
                    ? `${model.license.license}${
                        model.license.commercialUse === null
                          ? ""
                          : ` · ${t(model.license.commercialUse ? "models.commercialYes" : "models.commercialNo")}`
                      }`
                    : t("models.noLicense")}
                </dd>
                {model.license?.source && (
                  <>
                    <dt>{t("models.source")}</dt>
                    <dd className="path-text">{model.license.source}</dd>
                  </>
                )}
                {model.license?.checkedOn && (
                  <>
                    <dt>{t("models.checked")}</dt>
                    <dd>{model.license.checkedOn}</dd>
                  </>
                )}
                <dt>{t("models.sha")}</dt>
                <dd className="path-text">{model.sha256.slice(0, 16)}…</dd>
                <dt>{t("models.state")}</dt>
                <dd>
                  {model.loaded
                    ? t("models.loaded", { provider: model.loaded === "directml" ? "GPU (DirectML)" : "CPU" })
                    : t("models.notLoaded")}
                </dd>
              </dl>
            </li>
          ))}
        </ul>
      )}
      <div className="dialog-actions start">
        <button type="button" className="ghost small" onClick={() => void startVerify()} disabled={verify.state === "running"}>
          {verify.state === "running"
            ? t("models.verifying", { percent: Math.round(verify.fraction * 100) })
            : t("models.verify")}
        </button>
        <button type="button" className="ghost small" onClick={() => void release()}>
          {t("models.unload")}
        </button>
      </div>
      {verify.state === "done" && (
        <p className={verify.problems.length ? "error-text" : "note"} role="status">
          {verify.problems.length
            ? verify.problems.map((p) => `${p.id}: ${errorMessage(language, p.problem)}`).join("；")
            : t("models.verifiedOk")}
        </p>
      )}
      {verify.state === "failed" && <p className="error-text">{errorMessage(language, verify.code)}</p>}
      {released !== null && (
        <p className="note" role="status">
          {t("models.released", { count: released })}
        </p>
      )}
    </section>
  );
}

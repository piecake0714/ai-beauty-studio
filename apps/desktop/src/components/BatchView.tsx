import { useState } from "react";
import { useApp, useI18n } from "../context";
import { fileStem } from "../store";
import { errorMessage } from "../i18n";
import { NO_ADJUSTMENTS, type BatchSettings } from "../types";

/** Batch editing: the same edits on many photos, exported to a folder. */
export function BatchView() {
  const { t, language } = useI18n();
  const api = useApp((s) => s.api);
  const committed = useApp((s) => s.edit?.adjustments ?? null);
  const batch = useApp((s) => s.batch);
  const startBatch = useApp((s) => s.startBatch);
  const cancelBatch = useApp((s) => s.cancelBatch);
  const dismissBatch = useApp((s) => s.dismissBatch);
  const [photos, setPhotos] = useState<string[]>([]);
  const [folder, setFolder] = useState<string | null>(null);
  const [useCurrent, setUseCurrent] = useState(committed !== null);
  const [autoEnhance, setAutoEnhance] = useState(committed === null);
  const [format, setFormat] = useState<"jpeg" | "png">("jpeg");
  const [quality, setQuality] = useState(92);
  const [suffix, setSuffix] = useState("-edited");

  const addPhotos = async () => {
    const picked = await api.pickImages(t("imageFiles"));
    setPhotos((current) => Array.from(new Set([...current, ...picked])).slice(0, 500));
  };
  const chooseFolder = async () => {
    const chosen = await api.pickFolder();
    if (chosen) setFolder(chosen);
  };
  const ready = photos.length > 0 && folder !== null && (useCurrent || autoEnhance) && batch?.state !== "running";
  const run = () => {
    if (!folder) return;
    const settings: BatchSettings = {
      adjustments: useCurrent && committed ? committed : NO_ADJUSTMENTS,
      autoEnhance,
      export: { format, quality, longSide: null, upscale: 1 },
      folder,
      suffix,
    };
    void startBatch(photos, settings);
  };

  if (batch?.state === "running") {
    return (
      <main className="batch">
        <h1>{t("batch.title")}</h1>
        <div className="separation" role="status">
          <p className="note">
            {t("batch.running", { done: Math.min(batch.count, Math.floor(batch.fraction * batch.count)), count: batch.count })}
          </p>
          <progress max={1} value={batch.fraction} />
          <button type="button" className="ghost small" onClick={() => void cancelBatch()} disabled={batch.cancelling}>
            {t("ai.cancel")}
          </button>
        </div>
      </main>
    );
  }
  if (batch?.state === "done") {
    const failed = batch.items.filter((item) => item.code);
    return (
      <main className="batch">
        <h1>{t("batch.title")}</h1>
        <p role="status">{t("batch.done", { ok: batch.items.length - failed.length, failed: failed.length })}</p>
        <ul className="batch-list">
          {batch.items.map((item) => (
            <li key={item.path} className={item.code ? "failed" : ""}>
              <span>{fileStem(item.path)}</span>
              <span className="note">{item.code ? errorMessage(language, item.code) : item.output}</span>
            </li>
          ))}
        </ul>
        <button type="button" className="primary" onClick={dismissBatch}>
          {t("batch.again")}
        </button>
      </main>
    );
  }

  return (
    <main className="batch">
      <h1>{t("batch.title")}</h1>
      <p className="note">{t("batch.intro")}</p>
      {batch?.state === "failed" && <p className="error-text">{errorMessage(language, batch.code)}</p>}
      <section aria-labelledby="batch-photos">
        <h2 id="batch-photos">{t("batch.photos", { count: photos.length })}</h2>
        <div className="button-row">
          <button type="button" className="ghost small" onClick={() => void addPhotos()}>
            {t("batch.add")}
          </button>
          <button type="button" className="ghost small" disabled={photos.length === 0} onClick={() => setPhotos([])}>
            {t("batch.clear")}
          </button>
        </div>
        {photos.length > 0 && (
          <ul className="batch-list">
            {photos.map((path) => (
              <li key={path}>
                <span>{fileStem(path)}</span>
                <button
                  type="button"
                  className="icon"
                  aria-label={t("layers.delete")}
                  onClick={() => setPhotos((list) => list.filter((p) => p !== path))}
                >
                  ×
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
      <section aria-labelledby="batch-edits">
        <h2 id="batch-edits">{t("batch.edits")}</h2>
        <label className="check-row">
          <input type="checkbox" checked={useCurrent && committed !== null} disabled={committed === null} onChange={(e) => setUseCurrent(e.target.checked)} />
          {t("batch.useCurrent")}
        </label>
        {committed === null && <p className="note">{t("batch.noCurrent")}</p>}
        <p className="note">{t("batch.leftOut")}</p>
        <label className="check-row">
          <input type="checkbox" checked={autoEnhance} onChange={(e) => setAutoEnhance(e.target.checked)} />
          {t("batch.autoEnhance")}
        </label>
      </section>
      <section aria-labelledby="batch-output">
        <h2 id="batch-output">{t("batch.output")}</h2>
        <div className="button-row">
          <button type="button" className="ghost small" onClick={() => void chooseFolder()}>
            {t("batch.folder")}
          </button>
          <span className="note path-text">{folder ?? t("batch.noFolder")}</span>
        </div>
        <div className="segmented small" role="radiogroup" aria-label={t("export.format")}>
          {(["jpeg", "png"] as const).map((f) => (
            <button key={f} type="button" role="radio" aria-checked={format === f} className={format === f ? "active" : ""} onClick={() => setFormat(f)}>
              {f === "jpeg" ? "JPG" : "PNG"}
            </button>
          ))}
        </div>
        {format === "jpeg" && (
          <label className="field">
            <span>{t("export.quality", { quality })}</span>
            <input type="range" min={50} max={100} step={1} value={quality} onChange={(e) => setQuality(Number(e.target.value))} />
          </label>
        )}
        <label className="field">
          {t("batch.suffix")}
          <input type="text" value={suffix} maxLength={40} onChange={(e) => setSuffix(e.target.value.replace(/[\\/:*?"<>|]/g, ""))} />
        </label>
      </section>
      <button type="button" className="primary" disabled={!ready} onClick={run}>
        {t("batch.start", { count: photos.length })}
      </button>
    </main>
  );
}

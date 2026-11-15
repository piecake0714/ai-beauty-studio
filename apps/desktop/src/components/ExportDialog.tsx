import { useEffect, useState } from "react";
import { useApp, useFileLabels, useI18n } from "../context";
import { errorMessage } from "../i18n";
import type { ExportOptions } from "../types";

const SIZES: (number | null)[] = [null, 4096, 2048, 1080];

/** Export settings, then progress and the result of the export. */
export function ExportDialog({ onClose }: { onClose: () => void }) {
  const { t, language } = useI18n();
  const labels = useFileLabels();
  const document = useApp((s) => s.document);
  const transparent = useApp((s) => s.live.backgroundMode === "transparent" && s.background?.state === "ready");
  const exporting = useApp((s) => s.exporting);
  const exportImage = useApp((s) => s.exportImage);
  const cancelExport = useApp((s) => s.cancelExport);
  const dismissExport = useApp((s) => s.dismissExport);
  const [options, setOptions] = useState<ExportOptions>({
    format: transparent ? "png" : "jpeg",
    quality: 92,
    longSide: null,
  });

  const close = () => {
    if (exporting?.state === "running") return;
    dismissExport();
    onClose();
  };
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const longer = document ? Math.max(document.size.width, document.size.height) : 0;

  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && close()}>
      <div className="dialog small-dialog" role="dialog" aria-modal="true" aria-labelledby="export-title">
        <div className="dialog-head">
          <h1 id="export-title">{t("export.title")}</h1>
        </div>
        {exporting?.state === "running" ? (
          <div className="separation" role="status">
            <p className="note">{t("export.running", { percent: Math.round(exporting.fraction * 100) })}</p>
            <progress max={1} value={exporting.fraction} />
            <button type="button" className="ghost small" onClick={() => void cancelExport()} disabled={exporting.cancelling}>
              {t("ai.cancel")}
            </button>
          </div>
        ) : exporting?.state === "done" ? (
          <div className="separation" role="status">
            <p>{t("export.done", { width: exporting.width, height: exporting.height })}</p>
            <p className="note path-text">{exporting.path}</p>
            <button type="button" className="primary" onClick={close}>
              {t("close")}
            </button>
          </div>
        ) : (
          <>
            {exporting?.state === "failed" && <p className="error-text">{errorMessage(language, exporting.code)}</p>}
            <fieldset className="field">
              <legend>{t("export.format")}</legend>
              <div className="segmented small" role="radiogroup">
                {(["jpeg", "png"] as const).map((format) => (
                  <button
                    key={format}
                    type="button"
                    role="radio"
                    aria-checked={options.format === format}
                    className={options.format === format ? "active" : ""}
                    onClick={() => setOptions({ ...options, format })}
                  >
                    {format === "jpeg" ? "JPG" : "PNG"}
                  </button>
                ))}
              </div>
              <p className="note">{t(options.format === "jpeg" ? "export.jpegNote" : "export.pngNote")}</p>
              {options.format === "jpeg" && transparent && <p className="note warning">{t("export.flattenNote")}</p>}
            </fieldset>
            {options.format === "jpeg" && (
              <label className="field">
                <span>{t("export.quality", { quality: options.quality })}</span>
                <input
                  type="range"
                  min={50}
                  max={100}
                  step={1}
                  value={options.quality}
                  onChange={(event) => setOptions({ ...options, quality: Number(event.target.value) })}
                />
              </label>
            )}
            <label className="field">
              <span>{t("export.size")}</span>
              <select
                value={options.longSide ?? ""}
                onChange={(event) => setOptions({ ...options, longSide: event.target.value ? Number(event.target.value) : null })}
              >
                {SIZES.filter((size) => size === null || size < longer).map((size) => (
                  <option key={size ?? "full"} value={size ?? ""}>
                    {size === null ? t("export.fullSize", { size: longer }) : t("export.longSide", { size })}
                  </option>
                ))}
              </select>
            </label>
            <div className="dialog-actions">
              <button type="button" className="ghost" onClick={close}>
                {t("discard.cancel")}
              </button>
              <button type="button" className="primary" onClick={() => void exportImage(options, labels)}>
                {t("export.choose")}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

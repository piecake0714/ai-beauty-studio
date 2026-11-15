import { useApp, useI18n } from "../context";
import { folderOf, formatBytes } from "../format";
import { AdjustPanel } from "./AdjustPanel";
import { ImageCanvas } from "./ImageCanvas";

function EditToolbar() {
  const { t } = useI18n();
  const canUndo = useApp((s) => s.edit?.canUndo ?? false);
  const canRedo = useApp((s) => s.edit?.canRedo ?? false);
  const undo = useApp((s) => s.undo);
  const redo = useApp((s) => s.redo);
  const splitView = useApp((s) => s.splitView);
  const setSplitView = useApp((s) => s.setSplitView);
  const setHoldOriginal = useApp((s) => s.setHoldOriginal);

  return (
    <div className="edit-toolbar" role="toolbar" aria-label={t("nav.editor")}>
      <button type="button" className="ghost small" onClick={() => void undo()} disabled={!canUndo} title={`${t("edit.undo")} (Ctrl+Z)`}>
        ↶ {t("edit.undo")}
      </button>
      <button type="button" className="ghost small" onClick={() => void redo()} disabled={!canRedo} title={`${t("edit.redo")} (Ctrl+Y)`}>
        ↷ {t("edit.redo")}
      </button>
      <div className="spacer" />
      <button
        type="button"
        className={splitView ? "ghost small pressed" : "ghost small"}
        aria-pressed={splitView}
        onClick={() => setSplitView(!splitView)}
      >
        {t("compare.split")}
      </button>
      <button
        type="button"
        className="ghost small"
        title={`${t("compare.hold")} (\\)`}
        onPointerDown={() => setHoldOriginal(true)}
        onPointerUp={() => setHoldOriginal(false)}
        onPointerLeave={() => setHoldOriginal(false)}
        onKeyDown={(event) => (event.key === " " || event.key === "Enter") && setHoldOriginal(true)}
        onKeyUp={() => setHoldOriginal(false)}
      >
        {t("compare.hold")}
      </button>
    </div>
  );
}

function ZoomControls() {
  const { t } = useI18n();
  const zoom = useApp((s) => s.camera?.zoom ?? null);
  const zoomStep = useApp((s) => s.zoomStep);
  const zoomTo = useApp((s) => s.zoomTo);
  const fitToView = useApp((s) => s.fitToView);
  const disabled = zoom === null;

  return (
    <div className="zoom-controls" role="group" aria-label={t("zoom.label")}>
      <button type="button" className="ghost small" onClick={() => zoomStep(-1)} disabled={disabled} title={`${t("zoom.out")} (Ctrl+-)`} aria-label={t("zoom.out")}>
        −
      </button>
      <span className="zoom-value" aria-live="polite">
        {zoom === null ? "—" : t("zoom.percent", { percent: Math.round(zoom * 100) })}
      </span>
      <button type="button" className="ghost small" onClick={() => zoomStep(1)} disabled={disabled} title={`${t("zoom.in")} (Ctrl++)`} aria-label={t("zoom.in")}>
        +
      </button>
      <button type="button" className="ghost small" onClick={fitToView} disabled={disabled} title="Ctrl+0">
        {t("zoom.fit")}
      </button>
      <button type="button" className="ghost small" onClick={() => zoomTo(1)} disabled={disabled} title="Ctrl+1">
        {t("zoom.actual")}
      </button>
    </div>
  );
}

export function EditorView() {
  const { t, language } = useI18n();
  const document = useApp((s) => s.document);
  const opening = useApp((s) => s.opening);
  const rendering = useApp((s) => s.rendering);

  if (!document) {
    return (
      <main className="editor">
        <p className="muted center">{t("editor.empty")}</p>
      </main>
    );
  }

  const { size, info, path } = document;
  const unknown = t("info.unknown");
  const status = opening ? t("status.opening") : rendering ? t("status.rendering") : t("status.ready");

  return (
    <main className="editor">
      <EditToolbar />
      <section className="canvas-area">
        <ImageCanvas />
      </section>

      <aside className="side-panel">
        <AdjustPanel />
        <h2 className="info-title">{t("info.title")}</h2>
        <dl className="info-list">
          <dt>{t("info.name")}</dt>
          <dd>{info?.name ?? unknown}</dd>
          <dt>{t("info.format")}</dt>
          <dd>{info?.format ?? unknown}</dd>
          <dt>{t("info.dimensions")}</dt>
          <dd>
            {size.width} × {size.height}
          </dd>
          <dt>{t("info.size")}</dt>
          <dd>{info ? formatBytes(info.sizeBytes, language) : unknown}</dd>
          <dt>{t("info.folder")}</dt>
          <dd className="path">{folderOf(path) || unknown}</dd>
        </dl>
      </aside>

      <footer className="statusbar">
        <span role="status">{status}</span>
        <div className="spacer" />
        <ZoomControls />
      </footer>
    </main>
  );
}

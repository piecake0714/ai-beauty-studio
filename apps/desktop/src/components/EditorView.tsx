import { useApp, useI18n } from "../context";
import { folderOf, formatBytes } from "../format";
import { FrameCanvas } from "./FrameCanvas";

export function EditorView() {
  const { t, language } = useI18n();
  const document = useApp((s) => s.document);
  const opening = useApp((s) => s.opening);

  if (!document) {
    return (
      <main className="editor">
        <p className="muted center">{t("editor.empty")}</p>
      </main>
    );
  }

  const { frame, info, path } = document;
  const unknown = t("info.unknown");

  return (
    <main className="editor">
      <section className="canvas-area">
        <FrameCanvas frame={frame} className="main-canvas" label={info?.name ?? undefined} />
      </section>

      <aside className="side-panel" aria-label={t("info.title")}>
        <h2>{t("info.title")}</h2>
        <dl className="info-list">
          <dt>{t("info.name")}</dt>
          <dd>{info?.name ?? unknown}</dd>
          <dt>{t("info.format")}</dt>
          <dd>{info?.format ?? unknown}</dd>
          <dt>{t("info.dimensions")}</dt>
          <dd>
            {frame.sourceWidth} × {frame.sourceHeight}
          </dd>
          <dt>{t("info.size")}</dt>
          <dd>{info ? formatBytes(info.sizeBytes, language) : unknown}</dd>
          <dt>{t("info.folder")}</dt>
          <dd className="path">{folderOf(path) || unknown}</dd>
        </dl>
      </aside>

      <footer className="statusbar" role="status">
        <span>{opening ? t("status.opening") : t("status.ready")}</span>
        <span className="muted">{t("status.preview", { width: frame.width, height: frame.height })}</span>
      </footer>
    </main>
  );
}


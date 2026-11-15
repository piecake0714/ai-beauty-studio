import { useEffect, useState } from "react";
import { errorCode } from "../api";
import { isProjectPath } from "../api";
import { useApp, useDiscardPrompt, useFileLabels, useI18n } from "../context";
import { fileName, folderOf, formatOpenedAt } from "../format";
import { parseFrame, type Frame } from "../frame";
import type { RecentFile } from "../types";
import { FrameCanvas } from "./FrameCanvas";

type ThumbState = { kind: "loading" } | { kind: "ready"; frame: Frame } | { kind: "missing" } | { kind: "failed" };

function Thumbnail({ path }: { path: string }) {
  const { t } = useI18n();
  const api = useApp((s) => s.api);
  const [state, setState] = useState<ThumbState>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    setState({ kind: "loading" });
    api
      .thumbnail(path)
      .then((buffer) => !cancelled && setState({ kind: "ready", frame: parseFrame(buffer) }))
      .catch((reason) => {
        if (cancelled) return;
        setState({ kind: errorCode(reason) === "not_found" ? "missing" : "failed" });
      });
    return () => {
      cancelled = true;
    };
  }, [api, path]);

  switch (state.kind) {
    case "ready":
      return <FrameCanvas frame={state.frame} className="thumb-image" />;
    case "missing":
      return <span className="thumb-note warn">{t("home.missing")}</span>;
    case "failed":
      return <span className="thumb-note">{t("home.noPreview")}</span>;
    default:
      return <span className="thumb-note" aria-busy="true" />;
  }
}

function RecentCard({ entry }: { entry: RecentFile }) {
  const { t, language } = useI18n();
  const openPath = useApp((s) => s.openPath);
  const removeRecent = useApp((s) => s.removeRecent);
  const opening = useApp((s) => s.opening);
  const discard = useDiscardPrompt("open");
  const name = fileName(entry.path);

  return (
    <li className="recent-card">
      <button type="button" className="recent-open" onClick={() => openPath(entry.path, discard)} disabled={opening} title={entry.path}>
        <span className="thumb">
          <Thumbnail path={entry.path} />
        </span>
        <span className="recent-name">
          {isProjectPath(entry.path) && <span className="badge">{t("files.projectBadge")}</span>}
          {name}
        </span>
        <span className="recent-meta">{folderOf(entry.path)}</span>
        <span className="recent-meta">{formatOpenedAt(entry.openedAt, language)}</span>
      </button>
      <button
        type="button"
        className="recent-remove"
        onClick={() => removeRecent(entry.path)}
        aria-label={t("home.removeNamed", { name })}
        title={t("home.remove")}
      >
        ×
      </button>
    </li>
  );
}

export function HomeView() {
  const { t, language } = useI18n();
  const recent = useApp((s) => s.settings.recent);
  const opening = useApp((s) => s.opening);
  const pickToOpen = useApp((s) => s.pickToOpen);
  const labels = useFileLabels();
  const recovery = useApp((s) => s.recovery);
  const restoreRecovery = useApp((s) => s.restoreRecovery);
  const discardRecovery = useApp((s) => s.discardRecovery);
  const clearRecent = useApp((s) => s.clearRecent);
  const discard = useDiscardPrompt("open");

  return (
    <main className="home">
      {recovery && (
        <section className="recovery-card" role="alert">
          <div>
            <h2>{t("recovery.title")}</h2>
            <p>
              {t("recovery.message", {
                name: fileName(recovery.projectPath ?? recovery.originalPath),
                time: formatOpenedAt(recovery.savedAt, language),
              })}
            </p>
          </div>
          <div className="dialog-actions">
            <button type="button" className="ghost" onClick={() => void discardRecovery()} disabled={opening}>
              {t("recovery.discard")}
            </button>
            <button type="button" className="primary" onClick={() => void restoreRecovery()} disabled={opening}>
              {t("recovery.restore")}
            </button>
          </div>
        </section>
      )}
      <section className="hero">
        <h1>{t("home.title")}</h1>
        <p>{t("home.subtitle")}</p>
        <button type="button" className="primary large" onClick={() => void pickToOpen(labels, discard)} disabled={opening}>
          {opening ? t("opening") : t("file.openLong")}
        </button>
      </section>

      <section className="recent">
        <div className="section-head">
          <h2>{t("home.recent")}</h2>
          {recent.length > 0 && (
            <button type="button" className="ghost small" onClick={clearRecent}>
              {t("home.clearRecent")}
            </button>
          )}
        </div>
        {recent.length === 0 ? (
          <p className="muted">{t("home.noRecent")}</p>
        ) : (
          <ul className="recent-grid">
            {recent.map((entry) => (
              <RecentCard key={entry.path} entry={entry} />
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}

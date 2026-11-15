import { useApp, useDiscardPrompt, useFileLabels, useI18n } from "../context";
import { documentKey, fileStem } from "../store";
import type { View } from "../store";

export function TopBar({ onOpenSettings, onExport }: { onOpenSettings: () => void; onExport: () => void }) {
  const { t } = useI18n();
  const labels = useFileLabels();
  const saving = useApp((s) => s.saving);
  const saveProject = useApp((s) => s.saveProject);
  const pickToOpen = useApp((s) => s.pickToOpen);
  const title = useApp((s) => (s.document ? fileStem(s.projectPath ?? s.document.path) : null));
  const dirty = useApp((s) => s.document !== null && s.savedKey !== documentKey(s));
  const view = useApp((s) => s.view);
  const hasDocument = useApp((s) => s.document !== null);
  const opening = useApp((s) => s.opening);
  const showView = useApp((s) => s.showView);
  const discard = useDiscardPrompt("open");

  const tab = (target: View, label: string, disabled = false) => (
    <button
      type="button"
      role="tab"
      aria-selected={view === target}
      className={view === target ? "tab active" : "tab"}
      disabled={disabled}
      onClick={() => showView(target)}
    >
      {label}
    </button>
  );

  return (
    <header className="topbar">
      <span className="brand">{t("appName")}</span>
      <nav role="tablist" className="tabs">
        {tab("home", t("nav.home"))}
        {tab("editor", t("nav.editor"), !hasDocument)}
      </nav>
      {title && (
        <span className="doc-title" title={dirty ? t("file.unsaved") : undefined}>
          {title}
          {dirty && <span className="dirty-dot" aria-label={t("file.unsaved")}> ●</span>}
        </span>
      )}
      <div className="spacer" />
      <button type="button" className="ghost" onClick={() => void pickToOpen(labels, discard)} disabled={opening} title="Ctrl+O">
        {opening ? t("opening") : t("file.open")}
      </button>
      {hasDocument && (
        <>
          <button type="button" className="ghost" onClick={() => void saveProject(labels)} disabled={saving} title="Ctrl+S">
            {saving ? t("file.saving") : t("file.save")}
          </button>
          <button type="button" className="primary" onClick={onExport} title="Ctrl+E">
            {t("file.export")}
          </button>
        </>
      )}
      <button type="button" className="ghost" onClick={onOpenSettings}>
        {t("settings")}
      </button>
    </header>
  );
}

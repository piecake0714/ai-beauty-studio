import { useApp, useDiscardPrompt, useI18n } from "../context";
import type { View } from "../store";

export function TopBar({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { t } = useI18n();
  const view = useApp((s) => s.view);
  const hasDocument = useApp((s) => s.document !== null);
  const opening = useApp((s) => s.opening);
  const showView = useApp((s) => s.showView);
  const pickAndOpen = useApp((s) => s.pickAndOpen);
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
      <div className="spacer" />
      <button type="button" className="primary" onClick={() => pickAndOpen(t("imageFiles"), discard)} disabled={opening}>
        {opening ? t("opening") : t("openImage")}
      </button>
      <button type="button" className="ghost" onClick={onOpenSettings}>
        {t("settings")}
      </button>
    </header>
  );
}

import { useCallback, useEffect, useState } from "react";
import { EditorView } from "./components/EditorView";
import { HomeView } from "./components/HomeView";
import { SettingsDialog } from "./components/SettingsDialog";
import { TopBar } from "./components/TopBar";
import { useApp, useI18n } from "./context";
import { resolveTheme } from "./format";
import { errorMessage } from "./i18n";

const darkQuery = "(prefers-color-scheme: dark)";

/** Applies the theme setting, following the system when asked to. */
function useThemeAttribute() {
  const preference = useApp((s) => s.settings.theme);
  const [systemDark, setSystemDark] = useState(() => window.matchMedia(darkQuery).matches);

  useEffect(() => {
    const media = window.matchMedia(darkQuery);
    const onChange = () => setSystemDark(media.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = resolveTheme(preference, systemDark);
  }, [preference, systemDark]);
}

export function App() {
  const { t, language } = useI18n();
  const ready = useApp((s) => s.ready);
  const view = useApp((s) => s.view);
  const error = useApp((s) => s.error);
  const settingsRecovered = useApp((s) => s.settingsRecovered);
  const init = useApp((s) => s.init);
  const pickAndOpen = useApp((s) => s.pickAndOpen);
  const dismissError = useApp((s) => s.dismissError);
  const dismissRecovered = useApp((s) => s.dismissRecovered);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const closeSettings = useCallback(() => setSettingsOpen(false), []);

  useThemeAttribute();

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  // Ctrl+O opens an image from anywhere.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey && !event.altKey && !event.shiftKey && event.key.toLowerCase() === "o") {
        event.preventDefault();
        void pickAndOpen(t("imageFiles"));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pickAndOpen, t]);

  if (!ready) return <div className="app" aria-busy="true" />;

  return (
    <div className="app">
      <TopBar onOpenSettings={() => setSettingsOpen(true)} />
      {settingsRecovered && (
        <div className="banner" role="status">
          <span>{t("settings.recovered")}</span>
          <button type="button" className="ghost small" onClick={dismissRecovered} aria-label={t("dismiss")}>
            ×
          </button>
        </div>
      )}
      {error && (
        <div className="banner error" role="alert">
          <span>{errorMessage(language, error)}</span>
          <button type="button" className="ghost small" onClick={dismissError} aria-label={t("dismiss")}>
            ×
          </button>
        </div>
      )}
      {view === "editor" ? <EditorView /> : <HomeView />}
      {settingsOpen && <SettingsDialog onClose={closeSettings} />}
    </div>
  );
}

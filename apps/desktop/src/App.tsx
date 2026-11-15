import { useCallback, useEffect, useRef, useState } from "react";
import { EditorView } from "./components/EditorView";
import { HomeView } from "./components/HomeView";
import { SettingsDialog } from "./components/SettingsDialog";
import { TopBar } from "./components/TopBar";
import { useApp, useDiscardPrompt, useI18n } from "./context";
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
  const zoomStep = useApp((s) => s.zoomStep);
  const zoomTo = useApp((s) => s.zoomTo);
  const fitToView = useApp((s) => s.fitToView);
  const undo = useApp((s) => s.undo);
  const redo = useApp((s) => s.redo);
  const setHoldOriginal = useApp((s) => s.setHoldOriginal);
  const confirmDiscard = useApp((s) => s.confirmDiscard);
  const api = useApp((s) => s.api);
  const openPrompt = useDiscardPrompt("open");
  const closePrompt = useDiscardPrompt("close");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const closeSettings = useCallback(() => setSettingsOpen(false), []);

  useThemeAttribute();

  useEffect(() => {
    void init();
  }, [init]);

  const receiveFaces = useApp((s) => s.receiveFaces);
  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    void api.onFaces(receiveFaces).then((unlisten) => {
      if (cancelled) unlisten();
      else stop = unlisten;
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [api, receiveFaces]);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  // Keyboard shortcuts. Ctrl+O works everywhere; zoom, undo and the
  // hold-to-compare key in the editor. preventDefault also stops the web
  // view's own handling of these keys (such as page zoom).
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (settingsOpen) return;
      if (view === "editor" && event.code === "Backslash" && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        setHoldOriginal(true);
        return;
      }
      if (!event.ctrlKey || event.altKey) return;
      const key = event.key.toLowerCase();
      if (key === "o" && !event.shiftKey) {
        event.preventDefault();
        void pickAndOpen(t("imageFiles"), openPrompt);
        return;
      }
      if (view !== "editor") return;
      const actions: Record<string, () => void> = {
        "0": fitToView,
        "1": () => zoomTo(1),
        "=": () => zoomStep(1),
        "+": () => zoomStep(1),
        "-": () => zoomStep(-1),
        z: () => void (event.shiftKey ? redo() : undo()),
        y: () => void redo(),
      };
      const action = actions[key];
      if (action) {
        event.preventDefault();
        action();
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.code === "Backslash") setHoldOriginal(false);
    };
    const onBlur = () => setHoldOriginal(false);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", onBlur);
    };
  }, [pickAndOpen, t, view, settingsOpen, fitToView, zoomTo, zoomStep, undo, redo, setHoldOriginal, openPrompt]);

  // Ask before closing the window would discard adjustments.
  const closePromptRef = useRef(closePrompt);
  closePromptRef.current = closePrompt;
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    api
      .onCloseRequested(() => confirmDiscard(closePromptRef.current))
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // Without the listener the window simply closes as before.
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [api, confirmDiscard]);

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

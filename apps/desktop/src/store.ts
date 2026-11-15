import { createStore } from "zustand/vanilla";
import { errorCode, type Api } from "./api";
import { parseFrame, type Frame } from "./frame";
import type { FileInfo, LanguagePreference, Settings, ThemePreference } from "./types";

export type View = "home" | "editor";

export interface OpenDocument {
  path: string;
  frame: Frame;
  /** Null when the file's details could not be read after opening. */
  info: FileInfo | null;
}

export interface AppState {
  api: Api;
  ready: boolean;
  version: string;
  settings: Settings;
  settingsRecovered: boolean;
  view: View;
  document: OpenDocument | null;
  opening: boolean;
  /** Error code of the last failed action, shown until dismissed. */
  error: string | null;

  init(): Promise<void>;
  openPath(path: string): Promise<void>;
  pickAndOpen(filterName: string): Promise<void>;
  showView(view: View): void;
  setTheme(theme: ThemePreference): Promise<void>;
  setLanguage(language: LanguagePreference): Promise<void>;
  removeRecent(path: string): Promise<void>;
  clearRecent(): Promise<void>;
  dismissError(): void;
  dismissRecovered(): void;
}

export const defaultSettings: Settings = { theme: "system", language: "system", recent: [] };

export function createAppStore(api: Api) {
  return createStore<AppState>()((set, get) => {
    /** Run a settings change in the core and adopt the settings it returns. */
    async function changeSettings(change: () => Promise<Settings>, optimistic?: Partial<Settings>) {
      if (optimistic) set({ settings: { ...get().settings, ...optimistic } });
      try {
        set({ settings: await change() });
      } catch (reason) {
        set({ error: errorCode(reason) });
        // The core may still have applied the change in memory; show its state.
        try {
          set({ settings: await api.getSettings() });
        } catch {
          // Keep what is shown.
        }
      }
    }

    return {
      api,
      ready: false,
      version: "",
      settings: defaultSettings,
      settingsRecovered: false,
      view: "home",
      document: null,
      opening: false,
      error: null,

      async init() {
        try {
          const startup = await api.startup();
          set({
            ready: true,
            version: startup.version,
            settings: startup.settings,
            settingsRecovered: startup.settingsRecovered,
          });
        } catch (reason) {
          set({ ready: true, error: errorCode(reason) });
        }
      },

      async openPath(path) {
        if (get().opening) return;
        set({ opening: true, error: null });
        try {
          const frame = parseFrame(await api.openImage(path));
          const info = await api.fileInfo(path).catch(() => null);
          set({ document: { path, frame, info }, view: "editor" });
        } catch (reason) {
          set({ error: errorCode(reason) });
        } finally {
          set({ opening: false });
        }
        // The core updated the recent list when the image opened.
        try {
          set({ settings: await api.getSettings() });
        } catch {
          // Keep the list shown; it refreshes on the next change.
        }
      },

      async pickAndOpen(filterName) {
        if (get().opening) return;
        const path = await api.pickImage(filterName);
        if (path) await get().openPath(path);
      },

      showView(view) {
        if (view === "editor" && !get().document) return;
        set({ view });
      },

      setTheme: (theme) => changeSettings(() => api.setTheme(theme), { theme }),
      setLanguage: (language) => changeSettings(() => api.setLanguage(language), { language }),
      removeRecent: (path) => changeSettings(() => api.removeRecent(path)),
      clearRecent: () => changeSettings(() => api.clearRecent()),
      dismissError: () => set({ error: null }),
      dismissRecovered: () => set({ settingsRecovered: false }),
    };
  });
}

export type AppStore = ReturnType<typeof createAppStore>;

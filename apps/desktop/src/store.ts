import { createStore } from "zustand/vanilla";
import { errorCode, type Api } from "./api";
import { parseFrame, type Frame } from "./frame";
import type { FileInfo, LanguagePreference, Settings, ThemePreference } from "./types";
import {
  clampCamera,
  fitCamera,
  panBy,
  stepZoom,
  zoomAt,
  type Camera,
  type Point,
  type Size,
} from "./view";

export type View = "home" | "editor";

export interface OpenDocument {
  path: string;
  /** Whole image at reduced size, shown at once and under sharper renders. */
  overview: Frame;
  /** Full-resolution size. */
  size: Size;
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
  /** Canvas size in device pixels; null until the editor is laid out. */
  viewport: Size | null;
  /** Null until both an image and a viewport exist. */
  camera: Camera | null;
  /** A sharper render of the current view is on its way. */
  rendering: boolean;

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
  setViewport(viewport: Size): void;
  /** Multiply the zoom, keeping `anchor` (viewport pixels) fixed; default is the centre. */
  zoomBy(factor: number, anchor?: Point): void;
  zoomStep(direction: 1 | -1): void;
  /** Set an exact zoom, keeping `anchor` fixed; default is the centre. */
  zoomTo(zoom: number, anchor?: Point): void;
  fitToView(): void;
  pan(dx: number, dy: number): void;
  setRendering(rendering: boolean): void;
  reportError(code: string): void;
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
          const overview = parseFrame(await api.openImage(path));
          const info = await api.fileInfo(path).catch(() => null);
          const size = { width: overview.sourceWidth, height: overview.sourceHeight };
          const viewport = get().viewport;
          set({
            document: { path, overview, size, info },
            view: "editor",
            camera: viewport ? fitCamera(size, viewport) : null,
          });
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
      viewport: null,
      camera: null,
      rendering: false,

      setViewport(viewport) {
        const { document, camera } = get();
        if (!document) {
          set({ viewport });
        } else if (!camera) {
          set({ viewport, camera: fitCamera(document.size, viewport) });
        } else {
          set({ viewport, camera: clampCamera(camera, document.size, viewport) });
        }
      },

      zoomTo(zoom, anchor) {
        const { document, camera, viewport } = get();
        if (!document || !camera || !viewport) return;
        const point = anchor ?? { x: viewport.width / 2, y: viewport.height / 2 };
        set({ camera: zoomAt(camera, zoom, point, document.size, viewport) });
      },

      zoomBy(factor, anchor) {
        const camera = get().camera;
        if (camera) get().zoomTo(camera.zoom * factor, anchor);
      },

      zoomStep(direction) {
        const camera = get().camera;
        if (camera) get().zoomTo(stepZoom(camera.zoom, direction));
      },

      fitToView() {
        const { document, viewport } = get();
        if (document && viewport) set({ camera: fitCamera(document.size, viewport) });
      },

      pan(dx, dy) {
        const { document, camera, viewport } = get();
        if (!document || !camera || !viewport) return;
        set({ camera: panBy(camera, dx, dy, document.size, viewport) });
      },

      setRendering: (rendering) => set({ rendering }),
      reportError: (code) => set({ error: code }),
    };
  });
}

export type AppStore = ReturnType<typeof createAppStore>;

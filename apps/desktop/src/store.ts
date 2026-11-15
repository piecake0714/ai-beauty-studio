import { createStore } from "zustand/vanilla";
import { errorCode, type Api } from "./api";
import { withBackground } from "./adjustments";
import type { JobUpdate } from "./jobs";
import { parseFrame, type Frame } from "./frame";
import {
  NO_ADJUSTMENTS,
  type Adjustments,
  type EditorMode,
  type EditState,
  type FaceStatus,
  type FacesReport,
  type FileInfo,
  type LanguagePreference,
  type Settings,
  type ThemePreference,
} from "./types";
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

/** Texts for the "discard unsaved adjustments?" question, in the UI language. */
export interface DiscardPrompt {
  title: string;
  message: string;
  yes: string;
  no: string;
}

export interface OpenDocument {
  /** Different for every opened image, even the same file opened again. */
  id: number;
  path: string;
  /** Whole image at reduced size, shown at once and under sharper renders. */
  overview: Frame;
  /** Full-resolution size. */
  size: Size;
  /** Null when the file's details could not be read after opening. */
  info: FileInfo | null;
}

/** Subject/background separation for the open image. */
export type BackgroundView =
  | { state: "none" }
  | { state: "running"; jobId: number; fraction: number; cancelling: boolean }
  | { state: "ready"; provider: string }
  | { state: "failed"; code: string };

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
  /** Committed edit state of the open image, from the core. */
  edit: EditState | null;
  /** Adjustments on screen; differ from the committed ones while dragging. */
  live: Adjustments;
  /** Show the split before/after view. */
  splitView: boolean;
  /** Divider position as a fraction of the canvas width. */
  split: number;
  /** Show the original while a key or button is held. */
  holdOriginal: boolean;
  /** Face detection for the open image; null when no image is open. */
  faces: FaceStatus | null;
  /** Core serial of the image `faces` belongs to. */
  facesSerial: number;
  /** One-click enhancement is running. */
  enhancing: boolean;
  /** Subject/background separation; null when no image is open. */
  background: BackgroundView | null;

  init(): Promise<void>;
  /** Open an image. With `discard`, asks first when adjustments would be lost. */
  openPath(path: string, discard?: DiscardPrompt): Promise<void>;
  pickAndOpen(filterName: string, discard?: DiscardPrompt): Promise<void>;
  /** True when it is fine to lose the current adjustments (none, or the user agreed). */
  confirmDiscard(discard?: DiscardPrompt): Promise<boolean>;
  showView(view: View): void;
  setTheme(theme: ThemePreference): Promise<void>;
  setLanguage(language: LanguagePreference): Promise<void>;
  setEditorMode(mode: EditorMode): Promise<void>;
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
  /** Show adjustments without recording them (while dragging a slider). */
  setLive(adjustments: Adjustments): void;
  /** Record the adjustments on screen as one undoable step. */
  commitLive(): Promise<void>;
  resetAdjustments(): Promise<void>;
  undo(): Promise<void>;
  redo(): Promise<void>;
  setSplitView(on: boolean): void;
  setSplit(fraction: number): void;
  setHoldOriginal(on: boolean): void;
  /** Adopt a face detection report from the core (newer images win). */
  receiveFaces(report: FacesReport): void;
  /** One-click enhancement as one undoable step. */
  autoEnhance(): Promise<void>;
  startSeparation(): Promise<void>;
  cancelSeparation(): Promise<void>;
  /** Adopt a background job update (others are ignored). */
  receiveJobUpdate(update: JobUpdate): void;
  /** Change the background treatment as one undoable step. */
  chooseBackground(change: Partial<Pick<Adjustments, "backgroundMode" | "backgroundColor" | "backgroundImage">>): Promise<void>;
  /** Pick an image file and use it as the background. */
  pickBackgroundImage(filterName: string): Promise<void>;
  /** Output columns to leave unadjusted for a render `outWidth` wide. */
  originalColumns(outWidth: number): number;
}

export const defaultSettings: Settings = { theme: "system", language: "system", editorMode: "beginner", recent: [] };

export function createAppStore(api: Api) {
  let nextDocumentId = 1;
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

    /** Render the adjusted overview again (after anything that changes it). */
    async function refreshOverview(adjustments: Adjustments) {
      const document = get().document;
      if (!document) return;
      const overview = parseFrame(await api.renderOverview(adjustments));
      // Ignore if another image was opened meanwhile.
      if (get().document === document) set({ document: { ...document, overview } });
    }

    /** Adopt an edit state from the core and refresh the adjusted overview. */
    async function adoptEdit(change: () => Promise<EditState>) {
      try {
        const edit = await change();
        set({ edit, live: edit.adjustments });
        await refreshOverview(edit.adjustments);
      } catch (reason) {
        const code = errorCode(reason);
        if (code !== "no_document") set({ error: code });
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

      async confirmDiscard(discard) {
        if (!discard || !get().edit?.edited) return true;
        return api.confirm(discard.title, discard.message, discard.yes, discard.no);
      },

      async openPath(path, discard) {
        if (get().opening) return;
        if (!(await get().confirmDiscard(discard))) return;
        set({ opening: true, error: null });
        try {
          const overview = parseFrame(await api.openImage(path));
          const info = await api.fileInfo(path).catch(() => null);
          const size = { width: overview.sourceWidth, height: overview.sourceHeight };
          const edit = await api.editState();
          const viewport = get().viewport;
          set({
            document: { id: nextDocumentId++, path, overview, size, info },
            view: "editor",
            camera: viewport ? fitCamera(size, viewport) : null,
            edit,
            live: edit.adjustments,
            holdOriginal: false,
            background: { state: "none" },
          });
          get().receiveFaces(await api.facesStatus().catch(() => ({ serial: get().facesSerial + 1, status: { state: "unavailable" as const, code: "internal" } })));
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

      async pickAndOpen(filterName, discard) {
        if (get().opening) return;
        const path = await api.pickImage(filterName);
        if (path) await get().openPath(path, discard);
      },

      showView(view) {
        if (view === "editor" && !get().document) return;
        set({ view });
      },

      setTheme: (theme) => changeSettings(() => api.setTheme(theme), { theme }),
      setLanguage: (language) => changeSettings(() => api.setLanguage(language), { language }),
      setEditorMode: (editorMode) => changeSettings(() => api.setEditorMode(editorMode), { editorMode }),
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

      edit: null,
      live: NO_ADJUSTMENTS,
      splitView: false,
      split: 0.5,
      holdOriginal: false,

      setLive(adjustments) {
        if (get().document) set({ live: adjustments });
      },

      async commitLive() {
        if (!get().document) return;
        const live = get().live;
        await adoptEdit(() => api.commitAdjustments(live));
      },

      async resetAdjustments() {
        if (!get().document) return;
        set({ live: NO_ADJUSTMENTS });
        await adoptEdit(() => api.commitAdjustments(NO_ADJUSTMENTS));
      },

      async undo() {
        if (get().edit?.canUndo) await adoptEdit(() => api.undo());
      },

      async redo() {
        if (get().edit?.canRedo) await adoptEdit(() => api.redo());
      },

      setSplitView: (on) => set({ splitView: on }),
      setSplit: (fraction) => set({ split: Math.min(1, Math.max(0, fraction)) }),
      setHoldOriginal: (on) => set({ holdOriginal: on }),

      faces: null,
      facesSerial: 0,
      enhancing: false,
      background: null,

      async startSeparation() {
        const background = get().background;
        if (!get().document || !background || background.state === "running") return;
        try {
          const jobId = await api.startBackgroundSeparation();
          set({ background: { state: "running", jobId, fraction: 0, cancelling: false } });
        } catch (reason) {
          set({ background: { state: "failed", code: errorCode(reason) } });
        }
      },

      async cancelSeparation() {
        const background = get().background;
        if (background?.state !== "running") return;
        set({ background: { ...background, cancelling: true } });
        await api.cancelJob(background.jobId).catch(() => false);
      },

      receiveJobUpdate(update) {
        const background = get().background;
        if (background?.state !== "running" || background.jobId !== update.id) return;
        switch (update.state) {
          case "progress":
            set({ background: { ...background, fraction: Math.max(background.fraction, update.fraction) } });
            break;
          case "finished": {
            const result = update.result as { provider?: string } | null;
            set({ background: { state: "ready", provider: result?.provider ?? "cpu" } });
            void refreshOverview(get().live).catch(() => undefined);
            break;
          }
          case "failed":
            set({ background: { state: "failed", code: update.code } });
            break;
          case "cancelled":
            set({ background: { state: "none" } });
            break;
        }
      },

      async chooseBackground(change) {
        if (!get().document) return;
        const next = withBackground(get().live, change);
        set({ live: next });
        await adoptEdit(() => api.commitAdjustments(next));
      },

      async pickBackgroundImage(filterName) {
        if (!get().document) return;
        const path = await api.pickImage(filterName);
        if (!path) return;
        try {
          const id = await api.addBackgroundImage(path);
          await get().chooseBackground({ backgroundMode: "image", backgroundImage: id });
        } catch (reason) {
          set({ error: errorCode(reason) });
        }
      },

      receiveFaces(report) {
        const { facesSerial, faces } = get();
        // A report for a newer image always wins; for the same image, only
        // the move from "detecting" to a result counts (a status reply can
        // arrive after the result event).
        if (report.serial > facesSerial || (report.serial === facesSerial && faces?.state === "detecting")) {
          set({ faces: report.status, facesSerial: report.serial });
        }
      },

      async autoEnhance() {
        if (!get().document || get().enhancing) return;
        set({ enhancing: true });
        try {
          await adoptEdit(() => api.autoEnhance());
        } finally {
          set({ enhancing: false });
        }
      },

      originalColumns(outWidth) {
        const { holdOriginal, splitView, split } = get();
        if (holdOriginal) return outWidth;
        return splitView ? Math.round(split * outWidth) : 0;
      },
    };
  });
}

export type AppStore = ReturnType<typeof createAppStore>;

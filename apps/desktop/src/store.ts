import { createStore } from "zustand/vanilla";
import { errorCode, isProjectPath, type Api } from "./api";
import { adjustmentsKey, withBackground, withLightKind } from "./adjustments";
import type { JobUpdate } from "./jobs";
import { parseFrame, type Frame } from "./frame";
import {
  NO_ADJUSTMENTS,
  type Adjustments,
  type EditorMode,
  type EditState,
  type ExportOptions,
  type FaceStatus,
  type FacesReport,
  type FileInfo,
  type LanguagePreference,
  type RecoveryInfo,
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

/** Labels for file dialogs, in the UI language. */
export interface FileLabels {
  supported: string;
  images: string;
  projects: string;
  jpeg: string;
  png: string;
}

/** A project or recovery whose original photo must be pointed to. */
export interface PendingLocate {
  /** The project file, or null for the recovery. */
  projectPath: string | null;
  /** Where the project expected the photo. */
  expected: string;
  /** "original_missing" or "original_changed". */
  reason: string;
}

/** An export in progress or just finished. */
export type ExportView =
  | { state: "running"; jobId: number; fraction: number; cancelling: boolean; path: string }
  | { state: "done"; path: string; width: number; height: number }
  | { state: "failed"; code: string };

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

/** An AI analysis of the open image (subject/background, 3D shape). */
export type AnalysisView =
  | { state: "none" }
  | { state: "running"; jobId: number; fraction: number; cancelling: boolean }
  | { state: "ready"; provider: string }
  | { state: "failed"; code: string };

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
  /** 3D shape estimation for lighting; null when no image is open. */
  shape: AnalysisView | null;
  /** The project file of the open document, once saved or opened as one. */
  projectPath: string | null;
  /** What was saved (see documentKey); null when never saved. */
  savedKey: string | null;
  saving: boolean;
  exporting: ExportView | null;
  /** Unsaved work from a session that ended unexpectedly. */
  recovery: RecoveryInfo | null;
  /** The original photo of a project must be pointed to before it opens. */
  pendingLocate: PendingLocate | null;
  /** Background images a project refers to that could not be found. */
  missingBackgrounds: number;
  /** AI results made by a different model version than the installed one. */
  outdated: string[];

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
  startShapeEstimation(): Promise<void>;
  cancelShapeEstimation(): Promise<void>;
  /** Change the kind of light as one undoable step; a light also starts
   * the 3D shape estimation if it has not run. */
  chooseLight(kind: Adjustments["lightKind"]): Promise<void>;
  /** Light only the subject, or everything. */
  setLightSubjectOnly(on: boolean): Promise<void>;
  /** Output columns to leave unadjusted for a render `outWidth` wide. */
  originalColumns(outWidth: number): number;
  /** True when the open document has changes not saved in a project. */
  isDirty(): boolean;
  /** Open an image or project chosen in a dialog. */
  pickToOpen(labels: FileLabels, discard?: DiscardPrompt): Promise<void>;
  /** Save as a project; `asNew` (or a never-saved document) asks where. */
  saveProject(labels: FileLabels, asNew?: boolean): Promise<boolean>;
  /** Export the edited image; asks where. */
  exportImage(options: ExportOptions, labels: FileLabels): Promise<void>;
  cancelExport(): Promise<void>;
  dismissExport(): void;
  /** Point to the original photo of the pending project and open it. */
  locateOriginal(filterName: string): Promise<void>;
  cancelLocate(): void;
  restoreRecovery(): Promise<void>;
  discardRecovery(): Promise<void>;
  /** Save to the recovery slot if there are unsaved changes. */
  autosave(): Promise<void>;
}

/** What makes up the saved state of a document, as text: adjustments and
 * which analyses have been run. Equal keys mean nothing to save. */
export function documentKey(state: Pick<AppState, "edit" | "background" | "shape">): string {
  const adjustments = state.edit?.adjustments ?? NO_ADJUSTMENTS;
  return `${adjustmentsKey(adjustments)}|${state.background?.state === "ready"}|${state.shape?.state === "ready"}`;
}

/** File name without folders or extension. */
export function fileStem(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? path;
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/** Folder of `path`, with its trailing separator ("" when none). */
function folderOf(path: string): string {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut >= 0 ? path.slice(0, cut + 1) : "";
}

/** The state of a running analysis after a job update for it. */
function nextAnalysis(view: AnalysisView & { state: "running" }, update: JobUpdate): AnalysisView {
  switch (update.state) {
    case "progress":
      return { ...view, fraction: Math.max(view.fraction, update.fraction) };
    case "finished": {
      const result = update.result as { provider?: string } | null;
      return { state: "ready", provider: result?.provider ?? "cpu" };
    }
    case "failed":
      return { state: "failed", code: update.code };
    case "cancelled":
      return { state: "none" };
  }
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

    /** Show the document the core just opened (an image, project or
     * recovery); `frame` is its overview. `saved` means it matches its file. */
    async function adoptOpened(frame: ArrayBuffer, saved: boolean) {
      const overview = parseFrame(frame);
      const status = await api.documentStatus();
      const info = await api.fileInfo(status.originalPath).catch(() => null);
      const size = { width: overview.sourceWidth, height: overview.sourceHeight };
      const edit = await api.editState();
      const analyses = await api.backgroundStatus().catch(() => ({ ready: false, shapeReady: false }));
      const viewport = get().viewport;
      set({
        document: { id: nextDocumentId++, path: status.originalPath, overview, size, info },
        view: "editor",
        camera: viewport ? fitCamera(size, viewport) : null,
        edit,
        live: edit.adjustments,
        holdOriginal: false,
        background: analyses.ready ? { state: "ready", provider: "saved" } : { state: "none" },
        shape: analyses.shapeReady ? { state: "ready", provider: "saved" } : { state: "none" },
        projectPath: status.projectPath,
        missingBackgrounds: status.missingBackgrounds,
        outdated: status.outdated ?? [],
        exporting: null,
        // The core discards the recovery whenever a document opens.
        recovery: null,
      });
      set({ savedKey: saved ? documentKey(get()) : null });
      get().receiveFaces(
        await api
          .facesStatus()
          .catch(() => ({ serial: get().facesSerial + 1, status: { state: "unavailable" as const, code: "internal" } })),
      );
      if (edit.edited || analyses.ready) await refreshOverview(edit.adjustments).catch(() => undefined);
    }

    async function openProjectFile(path: string, original: string | null) {
      await adoptOpened(await api.openProject(path, original), true);
    }

    /** Restored work is unsaved until the user saves it. */
    async function restoreFrom(original: string | null) {
      await adoptOpened(await api.restoreRecovery(original), false);
    }

    /** A project whose photo is missing or changed asks the user to point
     * to it; other failures are shown as errors. */
    async function handleOpenFailure(reason: unknown, projectPath: string | null) {
      const code = errorCode(reason);
      if (code === "original_missing" || code === "original_changed") {
        const expected = projectPath
          ? await api
              .inspectProject(projectPath)
              .then((p) => p.originalPath)
              .catch(() => "")
          : (get().recovery?.originalPath ?? "");
        set({ pendingLocate: { projectPath, expected, reason: code } });
      } else {
        set({ error: code });
      }
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
            recovery: await api.recoveryStatus().catch(() => null),
          });
          // Started by double-clicking a project or image: open it.
          if (startup.launchPath) await get().openPath(startup.launchPath);
        } catch (reason) {
          set({ ready: true, error: errorCode(reason) });
        }
      },

      async confirmDiscard(discard) {
        if (!discard || !get().isDirty()) return true;
        return api.confirm(discard.title, discard.message, discard.yes, discard.no);
      },

      isDirty() {
        const state = get();
        if (!state.document) return false;
        return state.savedKey !== documentKey(state);
      },

      async openPath(path, discard) {
        if (get().opening) return;
        if (!(await get().confirmDiscard(discard))) return;
        set({ opening: true, error: null });
        try {
          if (isProjectPath(path)) {
            await openProjectFile(path, null);
          } else {
            await adoptOpened(await api.openImage(path), true);
          }
        } catch (reason) {
          await handleOpenFailure(reason, isProjectPath(path) ? path : null);
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

      async pickToOpen(labels, discard) {
        if (get().opening) return;
        const path = await api.pickToOpen(labels);
        if (path) await get().openPath(path, discard);
      },

      async locateOriginal(filterName) {
        const pending = get().pendingLocate;
        if (!pending) return;
        const original = await api.pickImage(filterName);
        if (!original) return;
        set({ pendingLocate: null, opening: true, error: null });
        try {
          if (pending.projectPath) await openProjectFile(pending.projectPath, original);
          else await restoreFrom(original);
        } catch (reason) {
          await handleOpenFailure(reason, pending.projectPath);
        } finally {
          set({ opening: false });
        }
      },

      cancelLocate: () => set({ pendingLocate: null }),

      async restoreRecovery() {
        if (get().opening || !get().recovery) return;
        set({ opening: true, error: null });
        try {
          await restoreFrom(null);
        } catch (reason) {
          await handleOpenFailure(reason, null);
        } finally {
          set({ opening: false });
        }
      },

      async discardRecovery() {
        await api.discardRecovery().catch(() => undefined);
        set({ recovery: null });
      },

      async autosave() {
        if (!get().isDirty()) return;
        await api.autosave().catch(() => undefined);
      },

      async saveProject(labels, asNew = false) {
        const state = get();
        if (!state.document || state.saving) return false;
        let target: string | null = null;
        if (asNew || !state.projectPath) {
          const base = state.projectPath ?? `${folderOf(state.document.path)}${fileStem(state.document.path)}.aibs`;
          target = await api.pickSavePath(labels.projects, "aibs", base);
          if (!target) return false;
        }
        set({ saving: true });
        try {
          const keyAtSave = documentKey(get());
          const saved = await api.saveProject(target);
          set({ projectPath: saved, savedKey: keyAtSave });
          set({ settings: await api.getSettings().catch(() => get().settings) });
          return true;
        } catch (reason) {
          set({ error: errorCode(reason) });
          return false;
        } finally {
          set({ saving: false });
        }
      },

      async exportImage(options, labels) {
        const document = get().document;
        if (!document || get().exporting?.state === "running") return;
        const extension = options.format === "png" ? "png" : "jpg";
        const suggested = `${folderOf(document.path)}${fileStem(document.path)}-edited.${extension}`;
        const path = await api.pickSavePath(options.format === "png" ? labels.png : labels.jpeg, extension, suggested);
        if (!path) return;
        try {
          const jobId = await api.exportImage(path, options);
          set({ exporting: { state: "running", jobId, fraction: 0, cancelling: false, path } });
        } catch (reason) {
          set({ exporting: { state: "failed", code: errorCode(reason) } });
        }
      },

      async cancelExport() {
        const exporting = get().exporting;
        if (exporting?.state !== "running") return;
        set({ exporting: { ...exporting, cancelling: true } });
        await api.cancelJob(exporting.jobId).catch(() => false);
      },

      dismissExport: () => set({ exporting: null }),

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

      projectPath: null,
      savedKey: null,
      saving: false,
      exporting: null,
      recovery: null,
      pendingLocate: null,
      missingBackgrounds: 0,
      outdated: [],
      faces: null,
      facesSerial: 0,
      enhancing: false,
      background: null,
      shape: null,

      async startShapeEstimation() {
        const shape = get().shape;
        if (!get().document || !shape || shape.state === "running") return;
        try {
          const jobId = await api.startShapeEstimation();
          set({ shape: { state: "running", jobId, fraction: 0, cancelling: false } });
        } catch (reason) {
          set({ shape: { state: "failed", code: errorCode(reason) } });
        }
      },

      async cancelShapeEstimation() {
        const shape = get().shape;
        if (shape?.state !== "running") return;
        set({ shape: { ...shape, cancelling: true } });
        await api.cancelJob(shape.jobId).catch(() => false);
      },

      async chooseLight(kind) {
        if (!get().document) return;
        const next = withLightKind(get().live, kind);
        set({ live: next });
        if (kind !== "none" && (get().shape?.state === "none" || get().shape?.state === "failed")) {
          void get().startShapeEstimation();
        }
        await adoptEdit(() => api.commitAdjustments(next));
      },

      async setLightSubjectOnly(on) {
        if (!get().document) return;
        const next = { ...get().live, lightSubjectOnly: on };
        set({ live: next });
        await adoptEdit(() => api.commitAdjustments(next));
      },

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
        const exporting = get().exporting;
        if (exporting?.state === "running" && exporting.jobId === update.id) {
          switch (update.state) {
            case "progress":
              set({ exporting: { ...exporting, fraction: Math.max(exporting.fraction, update.fraction) } });
              break;
            case "finished": {
              const result = update.result as { path?: string; width?: number; height?: number } | null;
              set({
                exporting: {
                  state: "done",
                  path: result?.path ?? exporting.path,
                  width: result?.width ?? 0,
                  height: result?.height ?? 0,
                },
              });
              break;
            }
            case "failed":
              set({ exporting: { state: "failed", code: update.code } });
              break;
            case "cancelled":
              set({ exporting: null });
              break;
          }
          return;
        }
        const shape = get().shape;
        if (shape?.state === "running" && shape.jobId === update.id) {
          const next = nextAnalysis(shape, update);
          set({ shape: next });
          if (next.state === "ready") set({ outdated: get().outdated.filter((name) => name !== "shape") });
          if (next.state === "ready") void refreshOverview(get().live).catch(() => undefined);
          return;
        }
        const background = get().background;
        if (background?.state !== "running" || background.jobId !== update.id) return;
        switch (update.state) {
          case "progress":
            set({ background: { ...background, fraction: Math.max(background.fraction, update.fraction) } });
            break;
          case "finished": {
            const result = update.result as { provider?: string } | null;
            set({
              background: { state: "ready", provider: result?.provider ?? "cpu" },
              outdated: get().outdated.filter((name) => name !== "matte"),
            });
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

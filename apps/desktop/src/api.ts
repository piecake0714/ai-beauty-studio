import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import type {
  Adjustments,
  DocumentStatus,
  EditorMode,
  EditState,
  ExportOptions,
  FacesReport,
  FileInfo,
  ModelOverview,
  LanguagePreference,
  RecoveryInfo,
  Settings,
  Startup,
  ThemePreference,
} from "./types";
import type { Hardware, JobUpdate } from "./jobs";

/**
 * Everything the UI asks of the Rust core. The store depends on this
 * interface, so tests can supply a fake instead of the real app.
 * Failed calls reject with a stable error code string.
 */
export interface Api {
  startup(): Promise<Startup>;
  getSettings(): Promise<Settings>;
  /** Show the file picker; resolves to null when cancelled. */
  pickImage(filterName: string): Promise<string | null>;
  /** Pick an image or a project to open. */
  pickToOpen(labels: { supported: string; images: string; projects: string }): Promise<string | null>;
  /** Pick where to save; `extension` without the dot. */
  pickSavePath(filterName: string, extension: string, defaultPath: string): Promise<string | null>;
  /** Open a project; `original` is where the user says the photo is. */
  openProject(path: string, original?: string | null): Promise<ArrayBuffer>;
  inspectProject(path: string): Promise<{ originalPath: string; savedAt: number }>;
  documentStatus(): Promise<DocumentStatus>;
  /** Which AI analyses of the open image are available. */
  backgroundStatus(): Promise<{ serial: number; ready: boolean; shapeReady: boolean }>;
  /** Save as a project, to `path` or to the open project; resolves to the path saved to. */
  saveProject(path: string | null): Promise<string>;
  autosave(): Promise<void>;
  modelOverview(): Promise<ModelOverview[]>;
  /** Check every model file's hash; resolves to the job id. */
  verifyModels(): Promise<number>;
  /** Release loaded models; resolves to how many were loaded. */
  unloadModels(): Promise<number>;
  /** Set the window title. */
  setTitle(title: string): Promise<void>;
  recoveryStatus(): Promise<RecoveryInfo | null>;
  restoreRecovery(original?: string | null): Promise<ArrayBuffer>;
  discardRecovery(): Promise<void>;
  /** Start exporting; resolves to the job id. */
  exportImage(path: string, options: ExportOptions): Promise<number>;
  /** Open an image for editing; resolves to an overview frame of the whole image. */
  openImage(path: string): Promise<ArrayBuffer>;
  /** Render part of the open image. Rejects with "superseded" when a newer request arrived. */
  renderView(request: RenderRequest): Promise<ArrayBuffer>;
  /** The whole open image with adjustments, at overview size. */
  renderOverview(adjustments: Adjustments): Promise<ArrayBuffer>;
  editState(): Promise<EditState>;
  /** Record adjustments as one undoable step. */
  commitAdjustments(adjustments: Adjustments): Promise<EditState>;
  undo(): Promise<EditState>;
  /** Face detection state of the open image. */
  facesStatus(): Promise<FacesReport>;
  /** Receive face detection results; resolves to a function that stops listening. */
  onFaces(handler: (report: FacesReport) => void): Promise<() => void>;
  /** One-click enhancement, recorded as one undoable step. */
  autoEnhance(): Promise<EditState>;
  /** Start separating subject and background; resolves to the job id. */
  startBackgroundSeparation(): Promise<number>;
  /** Start estimating the 3D shape for lighting; resolves to the job id. */
  startShapeEstimation(): Promise<number>;
  /** Open an image as a replacement background; resolves to its number. */
  addBackgroundImage(path: string): Promise<number>;
  redo(): Promise<EditState>;
  hardwareInfo(): Promise<Hardware>;
  /** Start the AI self-test; resolves to its job id. */
  startAiSelfTest(): Promise<number>;
  cancelJob(id: number): Promise<boolean>;
  /** Receive background job updates; resolves to a function that stops listening. */
  onJobUpdate(handler: (update: JobUpdate) => void): Promise<() => void>;
  /** Ask a yes/no question; resolves true for yes. */
  confirm(title: string, message: string, yes: string, no: string): Promise<boolean>;
  /** Call `allowClose` when the user closes the window; closing is cancelled when it resolves false. */
  onCloseRequested(allowClose: () => Promise<boolean>): Promise<() => void>;
  thumbnail(path: string): Promise<ArrayBuffer>;
  fileInfo(path: string): Promise<FileInfo>;
  setTheme(theme: ThemePreference): Promise<Settings>;
  setLanguage(language: LanguagePreference): Promise<Settings>;
  setEditorMode(mode: EditorMode): Promise<Settings>;
  removeRecent(path: string): Promise<Settings>;
  clearRecent(): Promise<Settings>;
  thirdPartyNotices(): Promise<string>;
}

export interface RenderRequest {
  /** Increasing per request; the core drops requests older than the newest. */
  id: number;
  x: number;
  y: number;
  width: number;
  height: number;
  outWidth: number;
  outHeight: number;
  /** What to show; may be uncommitted while a slider is being dragged. */
  adjustments: Adjustments;
  /** Leftmost output columns shown without adjustments (before/after). */
  originalColumns: number;
}

export const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "tif", "tiff", "webp"];
export const PROJECT_EXTENSION = "aibs";

/** True when `path` names a project file. */
export function isProjectPath(path: string): boolean {
  return path.toLowerCase().endsWith(`.${PROJECT_EXTENSION}`);
}

export const tauriApi: Api = {
  startup: () => invoke("startup"),
  getSettings: () => invoke("get_settings"),
  async pickImage(filterName) {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: filterName, extensions: IMAGE_EXTENSIONS }],
    });
    return typeof path === "string" ? path : null;
  },
  async pickToOpen(labels) {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [
        { name: labels.supported, extensions: [...IMAGE_EXTENSIONS, PROJECT_EXTENSION] },
        { name: labels.images, extensions: IMAGE_EXTENSIONS },
        { name: labels.projects, extensions: [PROJECT_EXTENSION] },
      ],
    });
    return typeof path === "string" ? path : null;
  },
  async pickSavePath(filterName, extension, defaultPath) {
    const path = await save({ defaultPath, filters: [{ name: filterName, extensions: [extension] }] });
    return path ?? null;
  },
  openProject: (path, original) => invoke("open_project", { path, original: original ?? null }),
  inspectProject: (path) => invoke("inspect_project", { path }),
  documentStatus: () => invoke("document_status"),
  backgroundStatus: () => invoke("background_status"),
  saveProject: (path) => invoke("save_project", { path }),
  autosave: () => invoke("autosave"),
  modelOverview: () => invoke("model_overview"),
  verifyModels: () => invoke("verify_models"),
  unloadModels: () => invoke("unload_models"),
  setTitle: (title) => getCurrentWindow().setTitle(title),
  recoveryStatus: () => invoke("recovery_status"),
  restoreRecovery: (original) => invoke("restore_recovery", { original: original ?? null }),
  discardRecovery: () => invoke("discard_recovery"),
  exportImage: (path, options) => invoke("export_image", { path, options }),
  openImage: (path) => invoke("open_image", { path }),
  renderView: (request) => invoke("render_view", { request }),
  renderOverview: (adjustments) => invoke("render_overview", { adjustments }),
  editState: () => invoke("edit_state"),
  commitAdjustments: (adjustments) => invoke("commit_adjustments", { adjustments }),
  undo: () => invoke("undo"),
  facesStatus: () => invoke("faces_status"),
  onFaces: (handler) => listen<FacesReport>("faces", (event) => handler(event.payload)),
  autoEnhance: () => invoke("auto_enhance"),
  startBackgroundSeparation: () => invoke("start_background_separation"),
  startShapeEstimation: () => invoke("start_shape_estimation"),
  addBackgroundImage: (path) => invoke("add_background_image", { path }),
  redo: () => invoke("redo"),
  hardwareInfo: () => invoke("hardware_info"),
  startAiSelfTest: () => invoke("start_ai_self_test"),
  cancelJob: (id) => invoke("cancel_job", { id }),
  onJobUpdate: (handler) => listen<JobUpdate>("job-update", (event) => handler(event.payload)),
  confirm: (title, message, yes, no) => ask(message, { title, kind: "warning", okLabel: yes, cancelLabel: no }),
  async onCloseRequested(allowClose) {
    return getCurrentWindow().onCloseRequested(async (event) => {
      if (!(await allowClose())) event.preventDefault();
    });
  },
  thumbnail: (path) => invoke("thumbnail", { path }),
  fileInfo: (path) => invoke("file_info", { path }),
  setTheme: (theme) => invoke("set_theme", { theme }),
  setLanguage: (language) => invoke("set_language", { language }),
  setEditorMode: (mode) => invoke("set_editor_mode", { mode }),
  removeRecent: (path) => invoke("remove_recent", { path }),
  clearRecent: () => invoke("clear_recent"),
  thirdPartyNotices: () => invoke("third_party_notices"),
};

/** Turn a rejected call into an error code; anything unexpected is "internal". */
export function errorCode(reason: unknown): string {
  return typeof reason === "string" && /^[a-z_]+$/.test(reason) ? reason : "internal";
}

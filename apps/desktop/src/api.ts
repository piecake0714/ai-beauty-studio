import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, open } from "@tauri-apps/plugin-dialog";
import type { Adjustments, EditState, FileInfo, LanguagePreference, Settings, Startup, ThemePreference } from "./types";

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
  redo(): Promise<EditState>;
  /** Ask a yes/no question; resolves true for yes. */
  confirm(title: string, message: string, yes: string, no: string): Promise<boolean>;
  /** Call `allowClose` when the user closes the window; closing is cancelled when it resolves false. */
  onCloseRequested(allowClose: () => Promise<boolean>): Promise<() => void>;
  thumbnail(path: string): Promise<ArrayBuffer>;
  fileInfo(path: string): Promise<FileInfo>;
  setTheme(theme: ThemePreference): Promise<Settings>;
  setLanguage(language: LanguagePreference): Promise<Settings>;
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
  openImage: (path) => invoke("open_image", { path }),
  renderView: (request) => invoke("render_view", { request }),
  renderOverview: (adjustments) => invoke("render_overview", { adjustments }),
  editState: () => invoke("edit_state"),
  commitAdjustments: (adjustments) => invoke("commit_adjustments", { adjustments }),
  undo: () => invoke("undo"),
  redo: () => invoke("redo"),
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
  removeRecent: (path) => invoke("remove_recent", { path }),
  clearRecent: () => invoke("clear_recent"),
  thirdPartyNotices: () => invoke("third_party_notices"),
};

/** Turn a rejected call into an error code; anything unexpected is "internal". */
export function errorCode(reason: unknown): string {
  return typeof reason === "string" && /^[a-z_]+$/.test(reason) ? reason : "internal";
}

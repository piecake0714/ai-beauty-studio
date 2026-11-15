import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { FileInfo, LanguagePreference, Settings, Startup, ThemePreference } from "./types";

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

/** Shapes shared with the Rust core. Field names match its JSON. */

export type ThemePreference = "system" | "light" | "dark";
export type LanguagePreference = "system" | "zh-Hant" | "en";

export interface RecentFile {
  path: string;
  /** Seconds since the Unix epoch. */
  openedAt: number;
}

export interface Settings {
  theme: ThemePreference;
  language: LanguagePreference;
  recent: RecentFile[];
}

export interface FileInfo {
  name: string;
  sizeBytes: number;
  format: "JPEG" | "PNG" | "TIFF" | "WebP" | null;
}

export interface Startup {
  settings: Settings;
  /** The settings file was damaged at startup and has been reset. */
  settingsRecovered: boolean;
  version: string;
}

/** Shapes shared with the Rust core. Field names match its JSON. */

export type ThemePreference = "system" | "light" | "dark";
export type LanguagePreference = "system" | "zh-Hant" | "en";

export interface RecentFile {
  path: string;
  /** Seconds since the Unix epoch. */
  openedAt: number;
}

/** Beginner shows the most-used adjustments; advanced shows all. */
export type EditorMode = "beginner" | "advanced";

export interface Settings {
  theme: ThemePreference;
  language: LanguagePreference;
  editorMode: EditorMode;
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

/** Number of HSL colour bands: red, orange, yellow, green, aqua, blue, purple, magenta. */
export const HSL_BANDS = 8;

/**
 * Light and colour adjustments; all zero leaves the image unchanged.
 * Exposure is in stops (EV), -5 to +5; every other value -100 to +100.
 */
export interface Adjustments {
  exposure: number;
  brightness: number;
  contrast: number;
  highlights: number;
  shadows: number;
  whites: number;
  blacks: number;
  temperature: number;
  tint: number;
  vibrance: number;
  saturation: number;
  /** One value per colour band. */
  hslHue: number[];
  hslSaturation: number[];
  hslLuminance: number[];
  /** Portrait skin smoothing on detected faces, 0 (off) to 100. */
  skinSmoothing: number;
  /** Background treatment; applies once subject and background are separated. */
  backgroundMode: BackgroundMode;
  /** R, G, B (0–255) for the "color" mode. */
  backgroundColor: [number, number, number];
  /** 0–100, for the "blur" mode. */
  backgroundBlur: number;
  /** -100 to +100, background only. */
  backgroundBrightness: number;
  backgroundSaturation: number;
  /** Number of the replacement image added to the document; 0 for none. */
  backgroundImage: number;
}

export type BackgroundMode = "original" | "transparent" | "color" | "blur" | "image";

export type ScalarAdjustment = Exclude<
  keyof Adjustments,
  "hslHue" | "hslSaturation" | "hslLuminance" | "backgroundMode" | "backgroundColor" | "backgroundImage"
>;
export type BandAdjustment = "hslHue" | "hslSaturation" | "hslLuminance";

export const NO_ADJUSTMENTS: Adjustments = Object.freeze({
  exposure: 0,
  brightness: 0,
  contrast: 0,
  highlights: 0,
  shadows: 0,
  whites: 0,
  blacks: 0,
  temperature: 0,
  tint: 0,
  vibrance: 0,
  saturation: 0,
  hslHue: Object.freeze(new Array(HSL_BANDS).fill(0)) as number[],
  hslSaturation: Object.freeze(new Array(HSL_BANDS).fill(0)) as number[],
  hslLuminance: Object.freeze(new Array(HSL_BANDS).fill(0)) as number[],
  skinSmoothing: 0,
  backgroundMode: "original",
  backgroundColor: Object.freeze([0, 0, 0]) as unknown as [number, number, number],
  backgroundBlur: 0,
  backgroundBrightness: 0,
  backgroundSaturation: 0,
  backgroundImage: 0,
}) as Adjustments;

/** Face detection for the open image. */
export type FaceStatus =
  | { state: "detecting" }
  | { state: "ready"; count: number }
  /** Detection could not run; `code` is an error code. */
  | { state: "unavailable"; code: string };

/** Face status of the image opened as number `serial` in the core. */
export interface FacesReport {
  serial: number;
  status: FaceStatus;
}

export interface EditState {
  adjustments: Adjustments;
  canUndo: boolean;
  canRedo: boolean;
  /** The image differs from the original. */
  edited: boolean;
}

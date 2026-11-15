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

/** Unsaved work kept from a session that ended unexpectedly. */
export interface RecoveryInfo {
  /** Seconds since the Unix epoch. */
  savedAt: number;
  projectPath: string | null;
  originalPath: string;
}

export interface ExportOptions {
  format: "jpeg" | "png";
  /** JPEG quality 1–100. */
  quality: number;
  /** Longest side in pixels; null keeps the original size. */
  longSide: number | null;
  /** AI enlargement: 1 (none), 2 or 4; when enlarging, longSide is not used. */
  upscale: 1 | 2 | 4;
}

export interface DocumentStatus {
  serial: number;
  /** Where the original photo is. */
  originalPath: string;
  projectPath: string | null;
  /** Background images a project refers to that could not be found. */
  missingBackgrounds: number;
  /** AI results ("faces", "matte", "shape") made by an older or newer model version. */
  outdated: string[];
}

/** One registered AI model (Settings → AI models). */
export interface ModelOverview {
  id: string;
  version: string;
  purpose: string;
  /** File size in bytes; null when the file is missing. */
  size: number | null;
  sha256: string;
  license: {
    license: string;
    approved: boolean;
    source: string | null;
    commercialUse: boolean | null;
    checkedOn: string | null;
  } | null;
  /** Why it cannot be used (an error code); null when usable. */
  problem: string | null;
  /** "directml" or "cpu" when loaded now. */
  loaded: string | null;
  /** "portrait", "background", "lighting" or "selfTest". */
  usedFor: string | null;
}

export interface FileInfo {
  name: string;
  sizeBytes: number;
  format: "JPEG" | "PNG" | "TIFF" | "WebP" | "RAW" | null;
}

export interface Startup {
  settings: Settings;
  /** The settings file was damaged at startup and has been reset. */
  settingsRecovered: boolean;
  version: string;
  /** A file the app was started with (for example a double-clicked project). */
  launchPath?: string | null;
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
  /** Portrait tools on detected faces, 0 (off) to 100. */
  blemishRemoval: number;
  darkCircles: number;
  eyeEnhance: number;
  teethWhitening: number;
  /** Healing spots, in source pixels. */
  spots: HealSpot[];
  /** Noise reduction, 0–100: brightness grain and colour blotches. */
  noiseLuminance: number;
  noiseColor: number;
  /** Object removals in effect (ids of the document's patches), in order. */
  removals: number[];
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
  /** -100 (pull the subject's edge in) to +100 (push it out). */
  backgroundEdge: number;
  /** 0–100: the subject's edge follows fine detail such as hair. */
  backgroundDetail: number;
  lightKind: LightKind;
  /** Light position as a fraction of the image width / height. */
  lightX: number;
  lightY: number;
  /** 0–100. */
  lightIntensity: number;
  /** 0–100: how far the light reaches. */
  lightSize: number;
  /** -100 (cool) to +100 (warm). */
  lightWarmth: number;
  /** 0 (from the side) to 100 (from the camera). */
  lightHeight: number;
  /** Light only the subject (needs subject/background separation). */
  lightSubjectOnly: boolean;
  /** Layers over the photo, bottom first. */
  layers: Layer[];
  /** The kept part of the photo, as fractions of its width and height. */
  crop: Crop;
  /** RGB curves, after the colour adjustments. */
  curves: Curves;
  /** Three-way colour grading. */
  grading: Grading;
  /** The chosen LUT (empty id for none). */
  lut: LutChoice;
}

/** What batch editing does with every photo. */
export interface BatchSettings {
  adjustments: Adjustments;
  /** Run the one-click enhancement on each photo first. */
  autoEnhance: boolean;
  export: ExportOptions;
  folder: string;
  /** Added to each name, e.g. "-edited". */
  suffix: string;
}

/** How one photo of a batch went. */
export interface BatchItem {
  path: string;
  output: string | null;
  code: string | null;
}

/** How far to extend each side, as fractions of the width / height. */
export interface Expansion {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** A curve point: x (input) and y (output), 0–1. */
export type CurvePoint = [number, number];

export interface Curves {
  master: CurvePoint[];
  red: CurvePoint[];
  green: CurvePoint[];
  blue: CurvePoint[];
}

export type CurveChannel = keyof Curves;

export interface GradeRange {
  /** 0–360 degrees. */
  hue: number;
  /** 0–100. */
  saturation: number;
  /** -100 to +100. */
  luminance: number;
}

export interface Grading {
  shadows: GradeRange;
  midtones: GradeRange;
  highlights: GradeRange;
  /** -100 (more counts as shadows) to +100 (more as highlights). */
  balance: number;
}

export type GradeRangeName = "shadows" | "midtones" | "highlights";

export interface LutChoice {
  /** "builtin:<name>", "user:<name>", or "" for none. */
  id: string;
  /** 0–100. */
  amount: number;
}

/** A LUT that can be chosen. */
export interface LutEntry {
  id: string;
  name: string;
  builtin: boolean;
}

/** A healing spot: the round area of `radius` source pixels around (x, y)
 * takes matching texture from nearby. */
export interface HealSpot {
  x: number;
  y: number;
  radius: number;
}

export interface Crop {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type LayerKind = "adjustment" | "image";
export type BlendMode = "normal" | "multiply" | "screen" | "overlay" | "soft-light" | "darken" | "lighten" | "difference";
export type MaskStart = "all" | "none" | "subject" | "background";

/** A brush stroke on a layer mask. */
export interface Stroke {
  /** x, y pairs in source pixels. */
  points: number[];
  /** Brush diameter in source pixels. */
  size: number;
  /** 0 (soft) to 100 (hard). */
  hardness: number;
  /** 0 to 100. */
  opacity: number;
  erase: boolean;
}

/** What an adjustment layer changes (same ranges as the global sliders). */
export interface LayerAdjustments {
  exposure: number;
  contrast: number;
  highlights: number;
  shadows: number;
  temperature: number;
  tint: number;
  saturation: number;
  vibrance: number;
}

export interface Layer {
  /** Stable within the document. */
  id: number;
  name: string;
  kind: LayerKind;
  visible: boolean;
  /** 0 to 100. */
  opacity: number;
  blend: BlendMode;
  mask: { start: MaskStart; invert: boolean; strokes: Stroke[] };
  adjustments: LayerAdjustments;
  /** For image layers: number of the document image. */
  image: number;
}

export type LightKind = "none" | "natural" | "diffuse" | "reflector" | "softbox";

export type BackgroundMode = "original" | "transparent" | "color" | "blur" | "image";

export type ScalarAdjustment = Exclude<
  keyof Adjustments,
  | "hslHue"
  | "hslSaturation"
  | "hslLuminance"
  | "backgroundMode"
  | "backgroundColor"
  | "backgroundImage"
  | "lightKind"
  | "lightSubjectOnly"
  | "layers"
  | "crop"
  | "spots"
  | "curves"
  | "grading"
  | "lut"
  | "removals"
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
  blemishRemoval: 0,
  darkCircles: 0,
  eyeEnhance: 0,
  teethWhitening: 0,
  spots: Object.freeze([]) as unknown as HealSpot[],
  noiseLuminance: 0,
  noiseColor: 0,
  removals: Object.freeze([]) as unknown as number[],
  backgroundMode: "original",
  backgroundColor: Object.freeze([0, 0, 0]) as unknown as [number, number, number],
  backgroundBlur: 0,
  backgroundBrightness: 0,
  backgroundSaturation: 0,
  backgroundImage: 0,
  backgroundEdge: 0,
  backgroundDetail: 0,
  lightKind: "none",
  lightX: 0,
  lightY: 0,
  lightIntensity: 0,
  lightSize: 0,
  lightWarmth: 0,
  lightHeight: 0,
  lightSubjectOnly: false,
  layers: Object.freeze([]) as unknown as Layer[],
  crop: Object.freeze({ x: 0, y: 0, width: 1, height: 1 }),
  curves: Object.freeze({ master: [], red: [], green: [], blue: [] }) as unknown as Curves,
  grading: Object.freeze({
    shadows: Object.freeze({ hue: 0, saturation: 0, luminance: 0 }),
    midtones: Object.freeze({ hue: 0, saturation: 0, luminance: 0 }),
    highlights: Object.freeze({ hue: 0, saturation: 0, luminance: 0 }),
    balance: 0,
  }) as Grading,
  lut: Object.freeze({ id: "", amount: 0 }),
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

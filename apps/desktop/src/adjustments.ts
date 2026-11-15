import { HSL_BANDS, NO_ADJUSTMENTS, type Adjustments, type BandAdjustment, type ScalarAdjustment } from "./types";

/** Adjustments shown in beginner mode. */
export const BEGINNER: ScalarAdjustment[] = ["exposure", "contrast", "highlights", "shadows", "temperature", "saturation", "skinSmoothing"];

export const LIGHT: ScalarAdjustment[] = ["exposure", "contrast", "highlights", "shadows", "whites", "blacks", "brightness"];
export const COLOUR: ScalarAdjustment[] = ["temperature", "tint", "vibrance", "saturation"];
export const PORTRAIT: ScalarAdjustment[] = ["skinSmoothing"];
export const BACKGROUND: ScalarAdjustment[] = ["backgroundBlur", "backgroundBrightness", "backgroundSaturation"];
export const BANDS: BandAdjustment[] = ["hslHue", "hslSaturation", "hslLuminance"];

/** A copy with one value changed. */
export function withScalar(adjustments: Adjustments, key: ScalarAdjustment, value: number): Adjustments {
  return { ...adjustments, [key]: value };
}

/** A copy with one band of one HSL array changed. */
export function withBand(adjustments: Adjustments, key: BandAdjustment, band: number, value: number): Adjustments {
  const values = [...adjustments[key]];
  values[band] = value;
  return { ...adjustments, [key]: values };
}

export function isIdentity(adjustments: Adjustments): boolean {
  return sameAdjustments(adjustments, NO_ADJUSTMENTS);
}

export function sameAdjustments(a: Adjustments, b: Adjustments): boolean {
  return adjustmentsKey(a) === adjustmentsKey(b);
}

/** True when an adjustment not shown in beginner mode is not zero. */
export function hasHiddenInBeginner(adjustments: Adjustments): boolean {
  const hiddenScalars = [...LIGHT, ...COLOUR, ...PORTRAIT].filter((key) => !BEGINNER.includes(key));
  return hiddenScalars.some((key) => adjustments[key] !== 0) || BANDS.some((key) => adjustments[key].some((v) => v !== 0));
}

/** Stable text key of adjustments, for detecting what a render was made with. */
export function adjustmentsKey(adjustments: Adjustments): string {
  const scalars = [...LIGHT, ...COLOUR, ...PORTRAIT, ...BACKGROUND].map((key) => adjustments[key]);
  const bands = BANDS.map((key) => Array.from({ length: HSL_BANDS }, (_, i) => adjustments[key][i] ?? 0));
  const background = [adjustments.backgroundMode, ...adjustments.backgroundColor, adjustments.backgroundImage];
  return JSON.stringify([scalars, bands, background]);
}

/** A copy with a different background treatment. */
export function withBackground(adjustments: Adjustments, change: Partial<Pick<Adjustments, "backgroundMode" | "backgroundColor" | "backgroundImage">>): Adjustments {
  return { ...adjustments, ...change };
}

/** "#rrggbb" for an R, G, B triple, and back. */
export function toHex([r, g, b]: [number, number, number]): string {
  return `#${[r, g, b].map((v) => Math.round(v).toString(16).padStart(2, "0")).join("")}`;
}

export function fromHex(hex: string): [number, number, number] {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex.trim());
  return m ? [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)] : [0, 0, 0];
}

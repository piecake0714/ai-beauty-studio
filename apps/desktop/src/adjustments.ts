import { HSL_BANDS, NO_ADJUSTMENTS, type Adjustments, type BandAdjustment, type ScalarAdjustment } from "./types";

/** Adjustments shown in beginner mode. */
export const BEGINNER: ScalarAdjustment[] = ["exposure", "contrast", "highlights", "shadows", "temperature", "saturation"];

export const LIGHT: ScalarAdjustment[] = ["exposure", "contrast", "highlights", "shadows", "whites", "blacks", "brightness"];
export const COLOUR: ScalarAdjustment[] = ["temperature", "tint", "vibrance", "saturation"];
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
  const scalars = [...LIGHT, ...COLOUR];
  if (scalars.some((key) => a[key] !== b[key])) return false;
  return BANDS.every((key) => {
    for (let i = 0; i < HSL_BANDS; i++) if ((a[key][i] ?? 0) !== (b[key][i] ?? 0)) return false;
    return true;
  });
}

/** True when an adjustment not shown in beginner mode is not zero. */
export function hasHiddenInBeginner(adjustments: Adjustments): boolean {
  const hiddenScalars = [...LIGHT, ...COLOUR].filter((key) => !BEGINNER.includes(key));
  return hiddenScalars.some((key) => adjustments[key] !== 0) || BANDS.some((key) => adjustments[key].some((v) => v !== 0));
}

/** Stable text key of adjustments, for detecting what a render was made with. */
export function adjustmentsKey(adjustments: Adjustments): string {
  return JSON.stringify([...LIGHT, ...COLOUR].map((key) => adjustments[key]).concat(...BANDS.map((key) => adjustments[key])));
}

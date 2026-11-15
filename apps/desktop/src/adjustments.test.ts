import { describe, expect, it } from "vitest";
import { adjustmentsKey, fromHex, hasHiddenInBeginner, isIdentity, sameAdjustments, toHex, withBackground, withBand, withScalar } from "./adjustments";
import { NO_ADJUSTMENTS } from "./types";

describe("adjustment helpers", () => {
  it("changes one value without touching the original", () => {
    const changed = withScalar(NO_ADJUSTMENTS, "shadows", 30);
    expect(changed.shadows).toBe(30);
    expect(NO_ADJUSTMENTS.shadows).toBe(0);
    const band = withBand(changed, "hslSaturation", 2, -40);
    expect(band.hslSaturation).toEqual([0, 0, -40, 0, 0, 0, 0, 0]);
    expect(changed.hslSaturation).toEqual([0, 0, 0, 0, 0, 0, 0, 0]);
  });

  it("recognises unchanged adjustments", () => {
    expect(isIdentity(NO_ADJUSTMENTS)).toBe(true);
    expect(isIdentity(withScalar(NO_ADJUSTMENTS, "tint", 1))).toBe(false);
    expect(isIdentity(withBand(NO_ADJUSTMENTS, "hslHue", 7, 1))).toBe(false);
    expect(sameAdjustments(withBand(NO_ADJUSTMENTS, "hslHue", 7, 1), withBand(NO_ADJUSTMENTS, "hslHue", 7, 1))).toBe(true);
  });

  it("knows which adjustments beginner mode hides", () => {
    expect(hasHiddenInBeginner(withScalar(NO_ADJUSTMENTS, "exposure", 1))).toBe(false);
    expect(hasHiddenInBeginner(withScalar(NO_ADJUSTMENTS, "saturation", 10))).toBe(false);
    expect(hasHiddenInBeginner(withScalar(NO_ADJUSTMENTS, "vibrance", 10))).toBe(true);
    expect(hasHiddenInBeginner(withScalar(NO_ADJUSTMENTS, "brightness", 10))).toBe(true);
    expect(hasHiddenInBeginner(withBand(NO_ADJUSTMENTS, "hslLuminance", 0, 5))).toBe(true);
  });

  it("gives different keys for different adjustments", () => {
    const a = adjustmentsKey(withScalar(NO_ADJUSTMENTS, "whites", 10));
    const b = adjustmentsKey(withBand(NO_ADJUSTMENTS, "hslHue", 0, 10));
    expect(a).not.toBe(b);
    expect(adjustmentsKey(NO_ADJUSTMENTS)).toBe(adjustmentsKey({ ...NO_ADJUSTMENTS }));
  });

  it("treats skin smoothing as an adjustment shown in beginner mode", () => {
    const smoothed = withScalar(NO_ADJUSTMENTS, "skinSmoothing", 40);
    expect(isIdentity(smoothed)).toBe(false);
    expect(hasHiddenInBeginner(smoothed)).toBe(false);
    expect(adjustmentsKey(smoothed)).not.toBe(adjustmentsKey(NO_ADJUSTMENTS));
  });

  it("treats background settings as adjustments", () => {
    const coloured = withBackground(NO_ADJUSTMENTS, { backgroundMode: "color", backgroundColor: [1, 2, 3] });
    expect(isIdentity(coloured)).toBe(false);
    expect(sameAdjustments(coloured, { ...coloured, backgroundColor: [1, 2, 3] })).toBe(true);
    expect(sameAdjustments(coloured, { ...coloured, backgroundColor: [1, 2, 4] })).toBe(false);
    expect(isIdentity(withScalar(NO_ADJUSTMENTS, "backgroundBlur", 10))).toBe(false);
    expect(hasHiddenInBeginner(coloured)).toBe(false);
  });

  it("converts colours to and from hex", () => {
    expect(toHex([255, 0, 16])).toBe("#ff0010");
    expect(fromHex("#FF0010")).toEqual([255, 0, 16]);
    expect(fromHex("nonsense")).toEqual([0, 0, 0]);
  });
});

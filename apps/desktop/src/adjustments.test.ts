import { describe, expect, it } from "vitest";
import { adjustmentsKey, cropRect, dragCrop, fitCropToRatio, fromHex, withLayer, withLayerMoved, withNewLayer, withoutLayer, hasHiddenInBeginner, isIdentity, sameAdjustments, toHex, withBackground, withBand, withLightKind, withScalar } from "./adjustments";
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

  it("starts a new light from the defaults and keeps a light's settings when switching kind", () => {
    const on = withLightKind(NO_ADJUSTMENTS, "natural");
    expect(on).toMatchObject({ lightKind: "natural", lightIntensity: 50, lightHeight: 50 });
    expect(isIdentity(on)).toBe(false);
    const moved = { ...on, lightX: 0.9 };
    expect(withLightKind(moved, "softbox").lightX).toBe(0.9);
    expect(sameAdjustments(moved, on)).toBe(false);
    expect(withLightKind(on, "none").lightKind).toBe("none");
  });

  it("turns a crop into whole source pixels inside the photo", () => {
    const size = { width: 40, height: 30 };
    expect(cropRect(undefined, size)).toEqual({ x: 0, y: 0, width: 40, height: 30 });
    expect(cropRect({ x: 0.26, y: 0.25, width: 0.5, height: 0.5 }, size)).toEqual({ x: 10, y: 7, width: 21, height: 16 });
    expect(cropRect({ x: 1, y: 1, width: 0, height: 0 }, size)).toEqual({ x: 39, y: 29, width: 1, height: 1 });
  });

  it("fits a crop to an aspect ratio inside the current crop", () => {
    const size = { width: 400, height: 300 };
    const square = fitCropToRatio({ x: 0, y: 0, width: 1, height: 1 }, 1, size);
    expect(square.width * 400).toBeCloseTo(300);
    expect(square.height * 300).toBeCloseTo(300);
    expect(square.x * 400).toBeCloseTo(50);
    // "Original" on a portrait photo keeps the photo's own shape.
    const tall = { width: 300, height: 400 };
    const same = fitCropToRatio({ x: 0, y: 0, width: 1, height: 1 }, 300 / 400, tall);
    expect(same.width).toBeCloseTo(1);
    expect(same.height).toBeCloseTo(1);
    // 4:3 on a portrait crop becomes 3:4.
    const portrait = fitCropToRatio({ x: 0, y: 0, width: 1, height: 1 }, 4 / 3, tall);
    expect((portrait.width * 300) / (portrait.height * 400)).toBeCloseTo(3 / 4);
  });

  it("drags crop edges and corners inside the photo, keeping a ratio when asked", () => {
    const size = { width: 400, height: 300 };
    const start = { x: 100, y: 100, width: 100, height: 100 };
    expect(dragCrop(start, "move", 500, -500, null, size)).toEqual({ x: 300, y: 0, width: 100, height: 100 });
    expect(dragCrop(start, "e", 50, 0, null, size)).toEqual({ x: 100, y: 100, width: 150, height: 100 });
    expect(dragCrop(start, "w", 500, 0, null, size).width).toBe(16);
    expect(dragCrop(start, "nw", -500, -500, null, size)).toEqual({ x: 0, y: 0, width: 200, height: 200 });
    const wide = dragCrop(start, "se", 100, 0, 2, size);
    expect(wide.width / wide.height).toBeCloseTo(2);
    expect(wide.x).toBe(100);
    const capped = dragCrop(start, "se", 1000, 0, 2, size);
    expect(capped.x + capped.width).toBeLessThanOrEqual(400);
    expect(capped.width / capped.height).toBeCloseTo(2);
  });

  it("adds, changes, moves and removes layers", () => {
    let a = withNewLayer(NO_ADJUSTMENTS, "adjustment", "A");
    a = withNewLayer(a, "image", "B", 2);
    expect(a.layers.map((l) => [l.id, l.name, l.image])).toEqual([
      [1, "A", 0],
      [2, "B", 2],
    ]);
    a = withLayer(a, 1, (l) => ({ ...l, opacity: 30 }));
    expect(a.layers[0].opacity).toBe(30);
    expect(withLayerMoved(a, 2, 1)).toBe(a);
    a = withLayerMoved(a, 1, 1);
    expect(a.layers.map((l) => l.id)).toEqual([2, 1]);
    a = withoutLayer(a, 2);
    expect(withNewLayer(a, "adjustment", "C").layers[1].id).toBe(2);
    expect(adjustmentsKey(a)).not.toBe(adjustmentsKey(NO_ADJUSTMENTS));
    expect(NO_ADJUSTMENTS.layers).toEqual([]);
  });
});

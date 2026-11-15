import {
  HSL_BANDS,
  NO_ADJUSTMENTS,
  type Adjustments,
  type BandAdjustment,
  type ScalarAdjustment,
  type Crop,
  type Layer,
  type LayerAdjustments,
} from "./types";

/** Adjustments shown in beginner mode. */
export const BEGINNER: ScalarAdjustment[] = [
  "exposure",
  "contrast",
  "highlights",
  "shadows",
  "temperature",
  "saturation",
  "skinSmoothing",
  "blemishRemoval",
  "darkCircles",
  "eyeEnhance",
  "teethWhitening",
];

export const LIGHT: ScalarAdjustment[] = ["exposure", "contrast", "highlights", "shadows", "whites", "blacks", "brightness"];
export const COLOUR: ScalarAdjustment[] = ["temperature", "tint", "vibrance", "saturation"];
export const PORTRAIT: ScalarAdjustment[] = ["skinSmoothing", "blemishRemoval", "darkCircles", "eyeEnhance", "teethWhitening"];
export const BACKGROUND: ScalarAdjustment[] = [
  "backgroundBlur",
  "backgroundBrightness",
  "backgroundSaturation",
  "backgroundEdge",
  "backgroundDetail",
];
export const LIGHTING: ScalarAdjustment[] = ["lightIntensity", "lightSize", "lightHeight", "lightWarmth"];
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
  const scalars = [...LIGHT, ...COLOUR, ...PORTRAIT, ...BACKGROUND, ...LIGHTING].map((key) => adjustments[key]);
  const bands = BANDS.map((key) => Array.from({ length: HSL_BANDS }, (_, i) => adjustments[key][i] ?? 0));
  const background = [adjustments.backgroundMode, ...adjustments.backgroundColor, adjustments.backgroundImage];
  const light = [adjustments.lightKind, adjustments.lightX, adjustments.lightY, adjustments.lightSubjectOnly];
  return JSON.stringify([
    scalars,
    bands,
    background,
    light,
    adjustments.layers ?? [],
    adjustments.crop ?? FULL_CROP,
    adjustments.spots ?? [],
  ]);
}

/** A copy with a different background treatment. */
export function withBackground(adjustments: Adjustments, change: Partial<Pick<Adjustments, "backgroundMode" | "backgroundColor" | "backgroundImage">>): Adjustments {
  return { ...adjustments, ...change };
}

export const FULL_CROP: Crop = Object.freeze({ x: 0, y: 0, width: 1, height: 1 });

/** True when the crop keeps the whole photo. */
export function isFullCrop(crop: Crop | undefined): boolean {
  return !crop || (crop.x <= 0 && crop.y <= 0 && crop.width >= 1 && crop.height >= 1);
}

/** The crop in source pixels (whole pixels, at least 1 x 1). */
export function cropRect(crop: Crop | undefined, size: { width: number; height: number }): {
  x: number;
  y: number;
  width: number;
  height: number;
} {
  const c = crop ?? FULL_CROP;
  // As the core: edges within a hundredth of a pixel snap to it.
  const down = (v: number, n: number) => Math.floor(v * n + 0.01);
  const up = (v: number, n: number) => Math.max(0, Math.ceil(v * n - 0.01));
  const x0 = Math.max(0, Math.min(down(c.x, size.width), size.width - 1));
  const y0 = Math.max(0, Math.min(down(c.y, size.height), size.height - 1));
  const x1 = Math.min(Math.max(up(c.x + c.width, size.width), x0 + 1), size.width);
  const y1 = Math.min(Math.max(up(c.y + c.height, size.height), y0 + 1), size.height);
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/** Aspect presets for cropping: width / height, or null for free. */
export const CROP_RATIOS: { key: string; ratio: number | null }[] = [
  { key: "free", ratio: null },
  { key: "original", ratio: -1 },
  { key: "1:1", ratio: 1 },
  { key: "4:3", ratio: 4 / 3 },
  { key: "3:2", ratio: 3 / 2 },
  { key: "16:9", ratio: 16 / 9 },
];

/** `ratio` (width / height) or its inverse, whichever is landscape or
 * portrait like a `width` x `height` box (a square box takes it as given). */
export function orientRatio(ratio: number, width: number, height: number): number {
  if (width === height || ratio === 1) return ratio;
  return width > height === ratio > 1 ? ratio : 1 / ratio;
}

/** The largest crop of pixel aspect `ratio` (width / height) centred in
 * `crop`, for a photo of `size`. Orientation follows the current crop. */
export function fitCropToRatio(crop: Crop, ratio: number, size: { width: number; height: number }): Crop {
  const w = crop.width * size.width;
  const h = crop.height * size.height;
  const target = orientRatio(ratio, w, h);
  let width = w;
  let height = w / target;
  if (height > h) {
    height = h;
    width = h * target;
  }
  return {
    x: crop.x + (w - width) / 2 / size.width,
    y: crop.y + (h - height) / 2 / size.height,
    width: width / size.width,
    height: height / size.height,
  };
}

/** Which part of the crop frame is dragged. */
export type CropHandle = "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

/** The crop after dragging `handle` by (dx, dy) source pixels from
 * `start`, kept inside the photo, at least `min` pixels on each side, and
 * at pixel aspect `ratio` (width / height) when given. All in source
 * pixels. */
export function dragCrop(
  start: { x: number; y: number; width: number; height: number },
  handle: CropHandle,
  dx: number,
  dy: number,
  ratio: number | null,
  size: { width: number; height: number },
  min = 16,
): { x: number; y: number; width: number; height: number } {
  if (handle === "move") {
    const x = Math.min(Math.max(start.x + dx, 0), size.width - start.width);
    const y = Math.min(Math.max(start.y + dy, 0), size.height - start.height);
    return { ...start, x, y };
  }
  let left = start.x;
  let top = start.y;
  let right = start.x + start.width;
  let bottom = start.y + start.height;
  if (handle.includes("w")) left = Math.min(Math.max(left + dx, 0), right - min);
  if (handle.includes("e")) right = Math.max(Math.min(right + dx, size.width), left + min);
  if (handle.includes("n")) top = Math.min(Math.max(top + dy, 0), bottom - min);
  if (handle.includes("s")) bottom = Math.max(Math.min(bottom + dy, size.height), top + min);
  if (ratio) {
    // Keep the aspect: the width decides, the height follows (edge handles
    // on top or bottom let the height decide), anchored at the far side.
    let width = right - left;
    let height = bottom - top;
    if (handle === "n" || handle === "s") width = height * ratio;
    else height = width / ratio;
    const maxW = handle.includes("w") ? right : size.width - left;
    const maxH = handle.includes("n") ? bottom : size.height - top;
    const shrink = Math.min(1, maxW / width, maxH / height);
    width *= shrink;
    height *= shrink;
    if (handle.includes("w")) left = right - width;
    else right = left + width;
    if (handle.includes("n")) top = bottom - height;
    else bottom = top + height;
  }
  return { x: left, y: top, width: right - left, height: bottom - top };
}

/** Most healing spots a document may have (as the core). */
export const MAX_SPOTS = 500;
/** Healing brush diameter limits, as fractions of the photo's longer side
 * (the largest spot radius is 2% of it). */
export const MIN_HEAL_SIZE = 0.003;
export const MAX_HEAL_SIZE = 0.04;

/** Most layers a document may have (as the engine side). */
export const MAX_LAYERS = 16;
/** Most strokes on one layer mask. */
export const MAX_STROKES = 5000;
/** Most coordinates (x and y each count) in one stroke. */
export const MAX_STROKE_VALUES = 20_000;

/** Adjustments of a new adjustment layer. */
export const NO_LAYER_ADJUSTMENTS: LayerAdjustments = Object.freeze({
  exposure: 0,
  contrast: 0,
  highlights: 0,
  shadows: 0,
  temperature: 0,
  tint: 0,
  saturation: 0,
  vibrance: 0,
});

/** The sliders of an adjustment layer. */
export const LAYER_SLIDERS: (keyof LayerAdjustments)[] = [
  "exposure",
  "contrast",
  "highlights",
  "shadows",
  "temperature",
  "tint",
  "saturation",
  "vibrance",
];

/** A copy with a new layer on top. */
export function withNewLayer(adjustments: Adjustments, kind: Layer["kind"], name: string, image = 0): Adjustments {
  const id = Math.max(0, ...adjustments.layers.map((l) => l.id)) + 1;
  const layer: Layer = {
    id,
    name,
    kind,
    visible: true,
    opacity: 100,
    blend: kind === "image" ? "soft-light" : "normal",
    // An adjustment layer starts hidden so the brush paints where it applies.
    mask: { start: kind === "image" ? "all" : "none", invert: false, strokes: [] },
    adjustments: { ...NO_LAYER_ADJUSTMENTS },
    image,
  };
  return { ...adjustments, layers: [...adjustments.layers, layer] };
}

/** A copy with layer `id` changed. */
export function withLayer(adjustments: Adjustments, id: number, change: (layer: Layer) => Layer): Adjustments {
  return { ...adjustments, layers: adjustments.layers.map((l) => (l.id === id ? change(l) : l)) };
}

/** A copy without layer `id`. */
export function withoutLayer(adjustments: Adjustments, id: number): Adjustments {
  return { ...adjustments, layers: adjustments.layers.filter((l) => l.id !== id) };
}

/** A copy with layer `id` moved one step up (+1) or down (-1). */
export function withLayerMoved(adjustments: Adjustments, id: number, step: 1 | -1): Adjustments {
  const layers = [...adjustments.layers];
  const from = layers.findIndex((l) => l.id === id);
  const to = from + step;
  if (from < 0 || to < 0 || to >= layers.length) return adjustments;
  [layers[from], layers[to]] = [layers[to], layers[from]];
  return { ...adjustments, layers };
}

/** Starting values when a light is first switched on. */
export const LIGHT_DEFAULTS = {
  lightX: 0.3,
  lightY: 0.3,
  lightIntensity: 50,
  lightSize: 50,
  lightWarmth: 0,
  lightHeight: 50,
} as const;

/** A copy with a different light kind; turning a light on from "none"
 * starts from the defaults. */
export function withLightKind(adjustments: Adjustments, kind: Adjustments["lightKind"]): Adjustments {
  if (adjustments.lightKind === "none" && kind !== "none") {
    return { ...adjustments, ...LIGHT_DEFAULTS, lightKind: kind };
  }
  return { ...adjustments, lightKind: kind };
}

/** "#rrggbb" for an R, G, B triple, and back. */
export function toHex([r, g, b]: [number, number, number]): string {
  return `#${[r, g, b].map((v) => Math.round(v).toString(16).padStart(2, "0")).join("")}`;
}

export function fromHex(hex: string): [number, number, number] {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex.trim());
  return m ? [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)] : [0, 0, 0];
}
